use std::{
    convert::identity,
    fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use anyhow::{Context, bail};
use data_gen::{Package, PackagePin};
use natural_sort_rs::NaturalSort;
use regex::Regex;
use serde_json::{Map, Value};

use crate::serde_helper::{map_get_and_parse_str, map_get_array, map_get_bool, map_get_object, map_get_string};

/// These parts do not technically exist or are broken.
const SKIP: &[&str] = &[
    // Broken
    "CC3500",
];

#[derive(Debug, Clone)]
pub struct Part {
    pub part_group: String,
    pub package_name: String,
    pub device_family: String,
}

pub fn get_gpns(sources: &Path) -> anyhow::Result<Vec<Part>> {
    let sysconfig = sources.join("sysconfig");
    let gpns_json = sysconfig.join("gpns.json");
    let gpns_reader =
        fs::File::open(&gpns_json).with_context(|| format!("opening {}", gpns_json.display()))?;
    let data: Value = serde_json::from_reader(gpns_reader)
        .with_context(|| format!("deserializing {}", gpns_json.display()))?;
    let data = data.as_object().context("gpns.json is not an object")?;

    let mut parts = Vec::new();

    // Part group because it isn't the final name.
    for (part_group, object) in data.iter() {
        let object = object
            .as_object()
            .context("part group in gpns.json is not an object")?;
        let include = part_group.starts_with("MSPM") || part_group.starts_with("CC");

        if !include {
            continue;
        }

        if SKIP.iter().any(|skip| skip == part_group) {
            continue;
        }

        let public = map_get_bool(object, "isPublic")?;

        // Nothing appears to use this, but note it anyways.
        if !public {
            println!("Ignoring non-public {part_group}");
            continue;
        }

        let packages = map_get_object(object, "packages")?;

        for (package_name, package) in packages.iter() {
            let package_datas = package.as_object().context("package is not an object")?;

            for (package_data_name, package_data) in package_datas.iter() {
                let package_data = package_data.as_object().with_context(|| {
                    format!(
                        "package data for {} with name {} is not an object",
                        part_group, package_data_name
                    )
                })?;
                let public = map_get_bool(package_data, "isPublic")?;

                if !public {
                    println!("Ignoring non-public {package_name} package from {part_group}");
                    continue;
                }

                // More precisely this is the folder which sysconfig metadata for this part comes from.
                let legacy_device = map_get_string(package_data, "legacyDevice")?;
                let target_db_mapping = map_get_string(package_data, "targetDBMapping")?;

                // If this is not an MSP part group, the target db mapping is the human readable part group
                let part = if legacy_device.starts_with("MSP") {
                    Part {
                        part_group: target_db_mapping,
                        package_name: package_name.clone(),
                        device_family: legacy_device,
                    }
                } else {
                    Part {
                        part_group: legacy_device.clone(),
                        package_name: package_name.clone(),
                        device_family: legacy_device,
                    }
                };

                parts.push(part);
            }
        }
    }

    Ok(parts)
}

pub fn read_data(path: &PathBuf) -> anyhow::Result<Value> {
    let name = path
        .file_prefix()
        .context("file prefix is not valid")?
        .to_string_lossy();
    let data = path.join(&*name).with_extension("json");

    if !fs::exists(&data).with_context(|| format!("checking if {} exists", data.display()))? {
        bail!("{} does not exist", data.display());
    }

    let data_reader =
        fs::File::open(&data).with_context(|| format!("opening {}", data.display()))?;
    let data: Value = serde_json::from_reader(data_reader)
        .with_context(|| format!("deserializing {}", data.display()))?;

    Ok(data)
}

pub fn get_packages(
    sysconfig_name: &str,
    sysconfig: &Value,
) -> anyhow::Result<Vec<Package>> {
    let packages_json = sysconfig
        .get("packages")
        .context("has no packages field")?
        .as_object()
        .context("\"packages\" is not an object")?;

    packages_json
        .values()
        .map(|object| parse_package(sysconfig_name, &sysconfig, object))
        .map(Result::transpose)
        .filter_map(identity)
        .collect()
}

fn parse_package(
    sysconfig_name: &str,
    sysconfig_data: &Value,
    package_object: &Value,
) -> anyhow::Result<Option<Package>> {
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?<name>[A-Za-z0-9-]+)\((?<package>[^)]+)\)").unwrap());

    let package_object = package_object
        .as_object()
        .context("package entry is not object")?;
    let raw_name = map_get_string(package_object, "name")?;

    // FIXME: This package should not exist on CC3551, but we should generate info for MOD package...
    if raw_name == "56VQFN_MOD" {
        return Ok(None);
    }

    let (name, package) = match PATTERN.captures(&raw_name) {
        // MSPM0/33 packages always contain the human name, and package code in same string, must split.
        Some(captures) => {
            let name: String = captures["name"].to_string();
            let package: String = captures["package"].to_string();

            (name, package)
        }

        // All CCxxxx parts does not contain human name anywhere. Only contains type like "RGZ".
        // Reconstruct the human readable name.
        None => {
            let pin_count = map_get_array(package_object, "packagePin")
                .with_context(|| format!("in package entry {raw_name}"))?
                .len();

            let name = sysconfig_package_name_to_human_type(&raw_name, pin_count)?;
            let package = if sysconfig_name == "CC3551E" {
                String::from("RSH")
            } else {
                raw_name
            };

            (name, package)
        }
    };

    let mut pins = read_package_pins(sysconfig_data, &package_object)?;

    if sysconfig_name == "CC3551E" {
        missing_cc3551_pins(&mut pins);

        if pins.len() != 56 {
            bail!("CC3551E has wrong number of pins: {len}", len = pins.len());
        }
    }

    // Clone is easier than the alternative.
    pins.natural_sort_by_key::<str, _, _>(|p| p.position.to_string());

    Ok(Some(Package {
        name,
        package,
        pins,
    }))
}

fn sysconfig_package_name_to_human_type(ty: &str, count: usize) -> anyhow::Result<String> {
    // CC3551 does not define all non-IO pins, so we need to manually state the number of pins.
    if ty == "RSH" {
        return Ok(String::from("VQFN-56"));
    }

    if ty.starts_with("RGZ")
        || ty.starts_with("RHB")
        || ty.starts_with("RSM")
        || ty.starts_with("RKP")
        || ty.starts_with("RSK")
        || ty.starts_with("RGE")
        || ty.starts_with("RHA")
        || ty.starts_with("RSL")
        || ty.starts_with("RTQ")
    {
        return Ok(format!("VQFN-{}", count));
    }

    if ty.starts_with("RTC") {
        return Ok(format!("VQFNP-{}", count));
    }

    if ty.starts_with("YBG") || ty.starts_with("YCJ") {
        return Ok(format!("DSBGA-{}", count));
    }

    // FIXME: on CC2340R5MODA release, is MHA a MOT type package?
    if ty.starts_with("SIP") || ty.starts_with("MHA") {
        return Ok(String::from("MOT"));
    }

    bail!("unknown package type: {ty} with {count} pins")
}

fn read_package_pins(
    sysconfig_data: &Value,
    packages_object: &Map<String, Value>,
) -> anyhow::Result<Vec<PackagePin>> {
    let package_pins = map_get_array(packages_object, "packagePin")?;
    let mut pins = Vec::with_capacity(package_pins.len());

    for package_pin in package_pins {
        pins.push(read_package_pin(sysconfig_data, package_pin)?);
    }

    Ok(pins)
}

fn read_package_pin(sysconfig_data: &Value, package_pin: &Value) -> anyhow::Result<PackagePin> {
    let object = package_pin
        .as_object()
        .context("package pin is not an object")?;

    let device_pin_id = map_get_string(object, "devicePinID")?;
    let position = map_get_and_parse_str(object, "ball")?;

    Ok(PackagePin {
        position,
        signals: get_device_pin_signals(sysconfig_data, &device_pin_id)?,
    })
}

// TODO: Normalize for CCxxxx
fn get_device_pin_signals(sysconfig: &Value, device_pin_id: &str) -> anyhow::Result<Vec<String>> {
    let device_pins = sysconfig.get("devicePins").context("no devicePins")?;

    let device_pin = device_pins
        .get(device_pin_id)
        .with_context(|| format!("{} is not in devicePins", device_pin_id))?
        .as_object()
        .with_context(|| format!("{} is not an object", device_pin_id))?;

    let signals = map_get_string(device_pin, "name")?;

    Ok(signals.split('/').map(String::from).collect())
}

/// CC3551E is missing pins in sysconfig for anything that isn't GPIO
fn missing_cc3551_pins(pins: &mut Vec<PackagePin>) {
    fn make(position: &str, name: &str) -> PackagePin {
        PackagePin {
            position: position.into(),
            signals: vec![name.into()],
        }
    }

    pins.extend([
        make("1", "PA_LDO_OUT"),
        make("2", "RF_BG"),
        make("3", "GND"),
        make("4", "VDDANA_IN1"),
        make("5", "VDDANA_IN2"),
        make("6", "HFXT_P"),
        make("7", "HFXT_N"),
        make("10", "VDD_DIG_IN"),
        make("15", "VIO2"),
        make("23", "VDD_SF"),
        make("37", "VIO1"),
        make("38", "LOGGER"),
        make("39", "SWCLK"),
        make("40", "SWDIO"),
        make("45", "VPP_IN"),
        make("47", "DIG_LDO_OUT"),
        make("48", "VDD_MAIN_IN"),
        make("49", "N_RESET"),
        make("54", "RF_A"),
        make("55", "PA_LDO_IN2"),
        make("56", "PA_LDO_IN1"),
    ]);
}

