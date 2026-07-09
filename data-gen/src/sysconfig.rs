use std::{
    collections::BTreeMap,
    convert::identity,
    fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use anyhow::{Context, bail};
use data_gen::{Package, PackagePin, Peripheral, PeripheralSignal, PinRouting, Routing};
use natural_sort_rs::NaturalSort;
use regex::Regex;
use serde_json::{Map, Value};

use crate::serde_helper::{
    map_get_and_parse_str, map_get_array, map_get_bool, map_get_object, map_get_string,
};

/// These parts do not technically exist or are broken.
const SKIP: &[&str] = &[
    // Broken
    "CC3500",
];

static MSP_GPIO_PIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^P(?<bank>[A-Z])\d+").unwrap());

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

pub fn get_packages(sysconfig_name: &str, sysconfig: &Value) -> anyhow::Result<Vec<Package>> {
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

pub fn get_peripherals(
    part: &str,
    family: &str,
    sysconfig: &Value,
    package: &Package,
) -> anyhow::Result<BTreeMap<String, Peripheral>> {
    let mut peripherals = BTreeMap::new();

    let sysconfig_object = sysconfig.as_object().context("sysconfig is not object")?;
    let peripherals_sys = map_get_object(sysconfig_object, "peripherals")?;

    for (id, object) in peripherals_sys {
        let peripheral_object = object
            .as_object()
            .with_context(|| format!("peripheral with id \"{id}\" is not object"))?;
        let name = map_get_string(peripheral_object, "name")?;

        let include = !is_useless_peripheral(part, &name) && does_peripheral_exist(family, &name);

        if include {
            let signals =
                get_peripheral_signals(part, sysconfig_object, peripheral_object, &name, package)
                    .with_context(|| format!("get peripheral signals for {name}"))?;

            peripherals.insert(name.clone(), Peripheral { name, signals });
        }
    }

    // For MSP, generate each GPIO peripheral and attach its signals manually.
    if part.starts_with("MSP") {
        let mut gpio_peripherals = BTreeMap::<String, Vec<PeripheralSignal>>::new();

        for pin in package.pins.iter() {
            for signal in pin.signals.iter() {
                if let Some(captures) = MSP_GPIO_PIN.captures(signal) {
                    let bank = &captures["bank"];
                    let peripheral_signals =
                        gpio_peripherals.entry(format!("GPIO{bank}")).or_default();

                    peripheral_signals.push(PeripheralSignal {
                        name: signal.clone(),
                        // GPIO on MSP always uses function 1
                        routing: Routing::Pins(vec![PinRouting {
                            pin: signal.clone(),
                            function: 1,
                        }]),
                    });
                }
            }
        }

        for (name, mut signals) in gpio_peripherals {
            // Sort signals from GPIO for neatness
            signals.natural_sort_by_key::<str, _, _>(|s| s.name.to_string());

            peripherals.insert(name.clone(), Peripheral { name, signals });
        }
    }

    // For CC13xx/26xx generate a single GPIO peripheral with each GPIO pin as a signal.
    if part.starts_with("CC13") || part.starts_with("CC26") {
        // TODO
    }

    // TODO: CC23xx/27xx/35xx?

    Ok(peripherals)
}

fn does_peripheral_exist(family: &str, name: &str) -> bool {
    // dbg!(family);

    // GPAMP does not exist on these parts
    if name == "GPAMP"
        && (family.eq_ignore_ascii_case("MSPS003FX")
            || family.starts_with("MSP32")
            || family.eq_ignore_ascii_case("MSPM0C110X")
            || family.eq_ignore_ascii_case("MSPM0C1105_C1106")
            || family.eq_ignore_ascii_case("MSPM0G151X"))
    {
        return false;
    }

    // CC1310 has no GPIO31
    if family.starts_with("CC1310") && name == "GPIO31" {
        return false;
    }

    true
}

fn is_useless_peripheral(part: &str, name: &str) -> bool {
    // CC3551, this is just a DMA mode
    name.starts_with("MEM2MEM")
        || name.starts_with("FASTCLK")
        || (part.starts_with("CC3551") && name.starts_with("ANT"))
        // USB EP singletons are not useful
        || name.contains("EP_IN")
        || name.contains("EP_OUT")
        // DMA channels are generated in a different step
        || name.starts_with("DMA_CH")
        || name.starts_with("DMA0_CH")
        || name.starts_with("DMA1_CH")
        // FLASHCTL exists on MSP parts, no need to generate a duplicate.
        || (part.starts_with("MSP") && name == "FLASH")
        // P<bank>x is useless on MSP, we will make the signals and peripheral manually.
        || (part.starts_with("MSP") && MSP_GPIO_PIN.is_match(&name))
        || (
            // CC13xx/26xx
            (part.starts_with("CC13") || part.starts_with("CC26"))
            // GPIOx is useless on CC13xx/25xx, we will make the signals and peripheral manually.
            && name.starts_with("GPIO")
        )
}

fn get_peripheral_signals(
    part: &str,
    sysconfig: &Map<String, Value>,
    peripheral: &Map<String, Value>,
    peripheral_name: &str,
    package: &Package,
) -> anyhow::Result<Vec<PeripheralSignal>> {
    let mut signals = Vec::new();
    let peripheral_pins = map_get_object(sysconfig, "peripheralPins")?;
    let interface_pins = map_get_object(sysconfig, "interfacePins")?;
    let muxes = map_get_array(sysconfig, "muxes")?;
    let device_pins = map_get_object(sysconfig, "devicePins")?;

    let peripheral_pin_ids = map_get_array(peripheral, "peripheralPinWrapper")?
        .iter()
        .map(|peripheral_pin_wrapper| {
            let object = peripheral_pin_wrapper
                .as_object()
                .context("peripheralPinWrapper entry is not an object")?;
            map_get_string(object, "peripheralPinID")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    for peripheral_pin_id in peripheral_pin_ids {
        let peripheral_pin = peripheral_pins
            .get(&peripheral_pin_id)
            .with_context(|| {
                format!("Peripheral pin with id \"{peripheral_pin_id}\" does not exist")
            })?
            .as_object()
            .with_context(|| {
                format!("Peripheral pin with id \"{peripheral_pin_id}\" is not an object")
            })?;

        // This tells us the name of the interface.
        let interface_pin_id = map_get_string(peripheral_pin, "interfacePinID")?;
        let interface_pin =
            map_get_object(interface_pins, &interface_pin_id).with_context(|| {
                format!("Interface pin with id \"{interface_pin_id}\" does not exist")
            })?;

        let signal_name = map_get_string(interface_pin, "name")?;

        // Nonsense signal in MSPM33C321
        if signal_name.starts_with("va_msp_comp") {
            continue;
        }

        // DMA channels for CC13xx/26xx are virtual.
        if peripheral_name.starts_with("DMA") {
            continue;
        }

        // CC13xx/26xx also defines signals on each peripheral for DMA. These separately.
        if signal_name.starts_with("DMA") {
            continue;
        }

        #[derive(Debug)]
        struct RawRouting {
            peripheral_pin_id: String,
            device_pin_name: String,
            mode: String,
        }
        let mut routings = Vec::new();

        // While the peripheral claims to have this pin, we can't be truly sure unless a mux option exists.
        for mux in muxes.iter() {
            let mux = mux.as_object().context("mux in muxes is not object")?;
            let device_pin_id = map_get_string(mux, "devicePinID")?;
            let mux_settings = map_get_array(mux, "muxSetting")?;

            for mux_setting in mux_settings {
                let mux_setting = mux_setting
                    .as_object()
                    .context("mux setting is not object")?;
                let mux_peripheral_pin_id = map_get_string(mux_setting, "peripheralPinID")?;
                let mode = map_get_string(mux_setting, "mode")?;

                if mux_peripheral_pin_id != peripheral_pin_id {
                    continue;
                }

                // We know the mux is valid. Check if this is a real pin per the package. Because not every package
                // contains every pin and some pins are virtual (why is a DMA channel a pin?) this must be done.
                let Some(device_pin) = device_pins.get(&device_pin_id) else {
                    println!("{part}: skipping non-existent device pin id, \"{device_pin_id}\"");
                    continue;
                };
                let device_pin = device_pin.as_object().context("")?;
                let device_pin_name = map_get_string(device_pin, "name")?;

                if package.pins.iter().any(|pin| {
                    pin.signals
                        .iter()
                        .any(|pin_signal| pin_signal == &device_pin_name)
                }) {
                    // The pin is confirmed to have a mux, now actually resolve the mode and pin.
                    routings.push(RawRouting {
                        peripheral_pin_id: peripheral_pin_id.clone(),
                        device_pin_name: device_pin_name.clone(),
                        mode,
                    });
                }
            }
        }

        routings.sort_by(|a, b| a.device_pin_name.cmp(&b.device_pin_name));

        // All routings must be same for a port id routing.
        //
        // On CC1354 GPTM signals are a special case. The signals are routed to the port
        // using EVENT routing. Because of the event indirection this will never be a valid
        // port id routing.
        let all_same = match routings.first() {
            Some(first) => routings.iter().all(|r| r.mode == first.mode),
            // A signal with no routes should not become a port id routing.
            None => false,
        };

        // Only CC13xx and CC26xx allow port id routing.
        //
        // If only 1 routing exists (e.g. cJTAG) then this should be a pin routing.
        let port_id_routing = (part.starts_with("CC13") || part.starts_with("CC26"))
            && routings.len() > 1
            && all_same;

        let routing = if port_id_routing {
            let id = routings.first().unwrap().mode.parse().unwrap();
            Routing::PortId(id)
        } else {
            let mut pin_routings = Vec::new();

            for RawRouting {
                device_pin_name,
                mode,
                ..
            } in routings
            {
                pin_routings.push(PinRouting {
                    pin: device_pin_name,
                    function: mode.parse().unwrap(),
                });
            }

            // Sometimes duplicate pin routings exist.
            pin_routings.dedup_by(|a, b| a.pin == b.pin && a.function == b.function);

            // Fixed routing on each pin.
            Routing::Pins(pin_routings)
        };

        signals.push(PeripheralSignal {
            name: signal_name,
            routing,
        });
    }

    Ok(signals)
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

    let mut pins = read_package_pins(sysconfig_name, sysconfig_data, &package_object, &package)?;

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
    sysconfig_name: &str,
    sysconfig_data: &Value,
    packages_object: &Map<String, Value>,
    package: &str,
) -> anyhow::Result<Vec<PackagePin>> {
    let package_pins = map_get_array(packages_object, "packagePin")?;
    let mut pins = Vec::with_capacity(package_pins.len());

    for package_pin in package_pins {
        pins.push(read_package_pin(
            sysconfig_name,
            sysconfig_data,
            package_pin,
            package,
        )?);
    }

    Ok(pins)
}

fn read_package_pin(
    sysconfig_name: &str,
    sysconfig_data: &Value,
    package_pin: &Value,
    package: &str,
) -> anyhow::Result<PackagePin> {
    let object = package_pin
        .as_object()
        .context("package pin is not an object")?;

    let device_pin_id = map_get_string(object, "devicePinID")?;
    let position = map_get_and_parse_str(object, "ball")?;

    Ok(PackagePin {
        position,
        signals: get_device_pin_signals(sysconfig_name, sysconfig_data, &device_pin_id, package)?,
    })
}

// TODO: Normalize for CCxxxx
fn get_device_pin_signals(
    sysconfig_name: &str,
    sysconfig: &Value,
    device_pin_id: &str,
    package: &str,
) -> anyhow::Result<Vec<String>> {
    let device_pins = sysconfig.get("devicePins").context("no devicePins")?;

    let device_pin = device_pins
        .get(device_pin_id)
        .with_context(|| format!("{} is not in devicePins", device_pin_id))?
        .as_object()
        .with_context(|| format!("{} is not an object", device_pin_id))?;

    let mut signals = map_get_string(device_pin, "name")?;

    // L130X in RTR package is missing some hardwired pin combinations (e.g. PA10/PA14).
    //
    // These need to be revised by hand.
    if sysconfig_name == "MSPM0L130X" && package == "RTR" {
        if signals == "PA10" {
            signals = String::from("PA10/PA14");
        }

        if signals == "PA23" {
            signals = String::from("PA23/PA25");
        }
    }

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
