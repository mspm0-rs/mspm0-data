mod util;

use std::{
    borrow::Cow,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    pin,
    sync::LazyLock,
};

use anyhow::{Context as _, bail};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use regex::Regex;
use serde_json::Value;
use sysconfig_data_gen::{Package, PackagePin};

use crate::util::RegexMap;

/// SysConfig chip groups that we intentionally ignore.
const NOPE_SYSCONFIG_GROUPS: &[&str] = &[
    // Unreleased parts
    "CC2341R10",
    "CC2341R7-Q1",
    "CC2653P10",
    "CC2745R10-Q1",
    // Broken
    "CC3500",
];

static ALLOW_SYSCONFIG_GROUPS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(MSPM(.*))|CC(.*)"#).unwrap());

/// Mapping from chip to chip family for CCxxxx devices.
pub static CC_CHIP_TO_FAMILY: RegexMap<&str> = RegexMap::new(&[
    // CC13xx
    ("CC13[15]0.*", "cc13x0_26x0"),
    ("CC1311.*", "cc13x1_26x1"),
    ("CC13[15]2.*", "cc13x2_26x2"),
    ("CC13[15]4.*", "cc13x4_26x4"),
    // CC23xx
    ("CC2340.*", "cc23xx"),
    // CC26xx
    ("CC26[2345]0.*", "cc13x0_26x0"),
    ("CC2651.*", "cc13x1_26x1"),
    ("CC26[456]2.*", "cc13x2_26x2"),
    ("CC2674.*", "cc13x4_26x4"),
    // CC27xx
    ("CC27.*", "cc27xx"),
    // CC35xx
    ("CC3551.*", "cc35xx"),
]);

fn main() -> anyhow::Result<()> {
    let output = PathBuf::from("build/data2");
    let sources = PathBuf::from("sources");
    let sysconfig_dir = sources.join("sysconfig");

    if !fs::exists(&sysconfig_dir)? {
        bail!("sysconfig directory does not exist in sources. help: clone the git submodules?");
    }

    let gpns = get_gpns(&sysconfig_dir).context("getting gpns from sysconfig")?;

    // Load sysconfig data for each gpn
    let sysconfig_datas: Vec<(&GpnDevice, anyhow::Result<Value>)> = gpns
        .par_iter()
        .map(|gpn| (gpn, load_sysconfig(&sysconfig_dir, gpn)))
        .collect();

    if sysconfig_datas.iter().any(|(_, result)| result.is_err()) {
        println!("Error reading sysconfig data");

        for (gpn, result) in sysconfig_datas.into_iter() {
            if let Err(err) = result {
                println!("For {}: {}", gpn.device_name, err);
            }
        }

        bail!("Error reading sysconfig data (see above)");
    }

    // Get the package data
    let packages: Vec<(&GpnDevice, anyhow::Result<Package>)> = sysconfig_datas
        .par_iter()
        .map(|(gpn, sysconfig_data)| {
            let sysconfig_data = sysconfig_data
                .as_ref()
                .expect("already proved this is not Err");
            let name = &gpn.package;

            let Some(packages_object) = sysconfig_data.get("packages").and_then(Value::as_object)
            else {
                return (*gpn, Err(anyhow::anyhow!("packages is not an object")));
            };

            let Some(device_pins_object) =
                sysconfig_data.get("devicePins").and_then(Value::as_object)
            else {
                return (*gpn, Err(anyhow::anyhow!("devicePins is not an object")));
            };

            let Some(package_object) = packages_object.values().find(|object| {
                let Some(object) = object.as_object() else {
                    return false;
                };

                let Some(package_type) = object.get("name").and_then(Value::as_str) else {
                    return false;
                };

                package_type.contains(name)
            }) else {
                return (*gpn, Err(anyhow::anyhow!("cannot find package \"{name}\"")));
            };

            let Some(package_type) = package_object.get("name").and_then(Value::as_str) else {
                return (*gpn, Err(anyhow::anyhow!("package name is not a str")));
            };

            let package = if gpn.device_name.starts_with("MSPM") {
                let with_trailing_brace = package_type.split_once('(').unwrap().1;
                let package = with_trailing_brace.split_once(')').unwrap().0;
                package.into()
            } else {
                package_type.into()
            };

            let Some(package_pins_objects) =
                package_object.get("packagePin").and_then(Value::as_array)
            else {
                return (
                    *gpn,
                    Err(anyhow::anyhow!("package object has no packagePin array")),
                );
            };

            let mut pins = Vec::with_capacity(package_pins_objects.len());

            for package_pin_object in package_pins_objects {
                let Some(position) = package_pin_object.get("ball").and_then(Value::as_str) else {
                    return (
                        *gpn,
                        Err(anyhow::anyhow!("a package pin object has no ball value")),
                    );
                };

                let Some(device_pin_id) = package_pin_object
                    .get("devicePinID")
                    .and_then(Value::as_str)
                else {
                    return (
                        *gpn,
                        Err(anyhow::anyhow!("a package pin object has no device pin id")),
                    );
                };

                let Some(device_pin_object) = device_pins_object.get(device_pin_id) else {
                    return (
                        *gpn,
                        Err(anyhow::anyhow!(
                            "device does not have device pin id {device_pin_id}"
                        )),
                    );
                };

                let Some(pin_name) = device_pin_object.get("name").and_then(Value::as_str) else {
                    return (
                        *gpn,
                        Err(anyhow::anyhow!(
                            "device pin id {device_pin_id} has no name field"
                        )),
                    );
                };

                pins.push(PackagePin {
                    position: position.into(),
                    signals: pin_signals(pin_name),
                });
            }

            (
                *gpn,
                Ok::<_, anyhow::Error>(Package {
                    name: name.into(),
                    package,
                    pins,
                }),
            )
        })
        .collect();

    for (gpn, p) in packages.iter() {
        println!("{} for {}: {:?}", gpn.device_name, gpn.package, p);
    }

    // let sysconfig_datas = sysconfig_datas
    //     .into_iter()
    //     .map(|(gpn, result)| (gpn, result.unwrap()))
    //     .collect::<Vec<_>>();

    Ok(())
}

#[derive(Debug)]
struct GpnDevice {
    /// The device name.
    ///
    /// This is a human readable device name minus any package specific info.
    pub device_name: String,

    /// The package key that is used to look up package data for this device.
    pub package: String,

    /// The device family.
    pub family: String,

    /// The name of the directory at which data for this device is available.
    pub dir_name: String,
}

fn get_gpns(sysconfig_dir: &Path) -> anyhow::Result<Vec<GpnDevice>> {
    let mut devices = Vec::new();
    let json = {
        let gpns_path = sysconfig_dir.join("gpns.json");

        if !fs::exists(&sysconfig_dir)? {
            bail!("gpns.json does not exist in sysconfig. help: reclone the git submodules?");
        }

        let reader = fs::File::open(&gpns_path).context("could not open gpns.json")?;
        let value: Value = serde_json::from_reader(reader).context("error parsing gpns.json")?;

        let Value::Object(object) = value else {
            bail!("gpns.json is not an object");
        };

        object
    };

    for (part_group, object) in json.iter() {
        if !ALLOW_SYSCONFIG_GROUPS.is_match(&part_group) {
            continue;
        }

        if NOPE_SYSCONFIG_GROUPS.iter().any(|skip| skip == part_group) {
            continue;
        }

        let group_devices = get_gpn_devices(part_group, object)
            .with_context(|| format!("getting group info for part group \"{part_group}\""))?;

        if group_devices.is_empty() {
            println!("No public devices in {part_group}");
            continue;
        }

        devices.extend(group_devices);
    }

    Ok(devices)
}

fn get_gpn_devices(part_group: &str, object: &Value) -> anyhow::Result<Vec<GpnDevice>> {
    let object = object.as_object().context("not an object")?;

    let public = object
        .get("isPublic")
        .and_then(Value::as_bool)
        .context("isPublic does not exist or is not a bool")?;

    if !public {
        return Ok(Vec::new());
    }

    let packages_object = object
        .get("packages")
        .and_then(Value::as_object)
        .context("packages is not an object")?;

    // At least 1 device will exist.
    let mut devices = Vec::with_capacity(1);

    for (package, object) in packages_object.iter() {
        let object = object
            .as_object()
            .with_context(|| format!("package entry \"{package}\" is not an object"))?;

        for (part, object) in object.iter() {
            let object = object
                .as_object()
                .with_context(|| format!("package entry \"{package}.{part}\" is not an object"))?;

            let is_public = object
                .get("isPublic")
                .and_then(Value::as_bool)
                .with_context(|| {
                    format!("package entry \"{package}.{part}.isPublic\" is not a bool")
                })?;

            if !is_public {
                println!("Skipping non-public \"{package}.{part}\" in group {part_group}");
                continue;
            }

            let legacy_device = object
                .get("legacyDevice")
                .and_then(Value::as_str)
                .with_context(|| {
                    format!("package entry \"{package}.{part}.legacyDevice\" is not a str")
                })?;

            let target_db_mapping = object
                .get("targetDBMapping")
                .and_then(Value::as_str)
                .with_context(|| {
                    format!("package entry \"{package}.{part}.targetDBMapping\" is not a str")
                })?;

            let legacy_package = object
                .get("legacyPackage")
                .and_then(Value::as_str)
                .with_context(|| {
                    format!("package entry \"{package}.{part}.legacyPackage\" is not a str")
                })?;

            let family = CC_CHIP_TO_FAMILY
                .get(part_group)
                .map(ToString::to_string)
                // MSP has the family from the GPN legacy device
                .unwrap_or_else(|| legacy_device.to_lowercase());

            devices.push(GpnDevice {
                device_name: target_db_mapping.to_uppercase(),
                package: legacy_package.into(),
                family,
                dir_name: legacy_device.into(),
            });
        }
    }

    // CC3551E has 3 duplicate GPN entries
    if part_group == "CC3551E" {
        while devices.len() != 1 {
            assert!(!devices.is_empty());
            devices.pop();
        }
    }

    Ok(devices)
}

fn load_sysconfig(sysconfig_dir: &Path, gpn: &GpnDevice) -> anyhow::Result<Value> {
    let json_path = sysconfig_dir
        .join(&gpn.dir_name)
        .join(&gpn.dir_name)
        .with_extension("json");

    println!("{}", json_path.display());

    let file = fs::File::open(&json_path)
        .with_context(|| format!("error opening {}", json_path.display()))?;
    let value = serde_json::from_reader(file)
        .with_context(|| format!("parsing {}", json_path.display()))?;

    Ok(value)
}

fn make_packages(sysconfig: &Value) -> anyhow::Result<Vec<Package>> {
    let packages_json = sysconfig
        .get("packages")
        .context("no packages entry")?
        .as_object()
        .context("packages is not an object")?;

    let device_pins = sysconfig
        .get("devicePins")
        .context("no devicePins entry")?
        .as_object()
        .context("devicePins is not an object")?;

    let mut packages = Vec::with_capacity(packages_json.len());

    for package in packages_json.values() {
        let name = package
            .get("name")
            .context("package has no name")?
            .as_str()
            .context("package name is not a string")?;

        let package_pins = package
            .get("packagePin")
            .context("package has no packagePin")?
            .as_array()
            .context("packagePin is not an array")?;

        let mut package = Package {
            name: name.into(),
            // TODO: Get a nice package name?
            package: name.into(),
            pins: Vec::with_capacity(package_pins.len()),
        };

        for pin in package_pins {
            let ball = pin
                .get("ball")
                .context("package pin has no ball field")?
                .as_str()
                .context("ball is not a str")?;

            let pin_id = pin
                .get("devicePinID")
                .context("package pin has no devicePinID")?
                .as_str()
                .context("devicePinID is not a str")?;

            let device_pin = device_pins
                .get(pin_id)
                .with_context(|| format!("{pin_id} does not exist"))?;

            let pin_name = device_pin
                .get("name")
                .with_context(|| format!("{pin_id} has no name"))?
                .as_str()
                .with_context(|| format!("{pin_id} name is not a str"))?;

            package.pins.push(PackagePin {
                position: ball.into(),
                signals: pin_signals(pin_name),
            });
        }

        package.pins.sort_by(|a, b| a.position.cmp(&b.position));
        packages.push(package);
    }

    Ok(packages)
}

fn pin_signals(pin_name: &str) -> Vec<String> {
    let mut pin_name = Cow::Borrowed(pin_name);

    // C1105/6 has a bug where PA30/VCORE is a pin. VCORE and a pin being bonded together does not make sense.
    //
    // The datasheet confirms the pin is just PA30.
    if pin_name.ends_with("/VCORE") {
        pin_name = Cow::Owned(pin_name.strip_suffix("/VCORE").unwrap().into());
    }

    // On MSPMxx OPAx is removed since the signals are resolved later.
    if pin_name.contains("/OPA") {
        pin_name = Cow::Owned(pin_name.split_once("/OPA").unwrap().0.into());
    }

    pin_name.split('/').map(String::from).collect()
}
