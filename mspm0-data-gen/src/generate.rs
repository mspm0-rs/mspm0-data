use std::{borrow::Cow, cmp::Ordering, collections::BTreeMap, fs, sync::LazyLock};

use anyhow::Context;
use mspm0_data_types::{
    Chip, DmaChannel, Interrupt, Package, PackagePin, Peripheral, PeripheralPin, PeripheralType,
};
use regex::Regex;

use crate::{
    header::{Header, Headers},
    int_group::Groups,
    sysconfig::{PartPeripheralWrapper, Sysconfig, SysconfigFile},
};

const SKIP_CHIPS: &[&str] = &[
    // FIXME: This is not a duplicate of C110x due to pinout differences
    "MSPS003FX",
    // Likely a duplicate of C110x
    "MSPM0C1105_C1106",
    // Unreleased
    "MSPM0L111X",
    "MSPM0H321X",
];

pub fn generate(
    headers: &Headers,
    sysconfig: &Sysconfig,
    int_groups: &BTreeMap<String, Groups>,
) -> anyhow::Result<()> {
    for (name, sysconfig_entry) in sysconfig.files.iter() {
        let packages = generate_packages(&name, &sysconfig_entry)?;

        if SKIP_CHIPS.iter().any(|&chip| chip == name) {
            continue;
        }

        // TODO: Remove _POCIx suffix from e.x. CS1_POCI1
        let iomux = generate_pincm(&name, &sysconfig_entry)?;

        let peripherals = generate_peripherals2(
            &name,
            headers
                .headers
                .get(&name.to_lowercase())
                .context(format!("Could not lookup header for {}", name))?,
            &sysconfig_entry,
        )?;

        let interrupts = generate_irqs(
            &name,
            headers.headers.get(&name.to_lowercase()).unwrap(),
            int_groups,
        )?;

        let dma_channels = generate_dma_channels(&name, &sysconfig_entry)?;

        fs::create_dir_all("./build/data/").unwrap();

        let chip = Chip {
            packages,
            iomux,
            peripherals,
            interrupts,
            dma_channels,
        };

        let _ = fs::write(
            format!("./build/data/{name}.json"),
            serde_json::to_string_pretty(&chip).unwrap(),
        );
    }

    Ok(())
}

fn generate_packages(chip_name: &str, sysconfig: &SysconfigFile) -> anyhow::Result<Vec<Package>> {
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?<name>[A-Za-z0-9-]+)\((?<package>[^)]+)\)").unwrap());

    let mut packages = Vec::with_capacity(sysconfig.packages.len());

    for package in sysconfig.packages.values() {
        let raw_name = &package.name;

        let captures = PATTERN.captures(&raw_name).unwrap();
        let name = &captures["name"];
        let package_name = &captures["package"];

        let mut pins = Vec::with_capacity(package.package_pins.len());

        for pin in package.package_pins.iter() {
            // Why TI has pins refer to a pin ID in sysconfig I do not know...
            let device_pin = sysconfig
                .device_pins
                .get(&pin.device_pin_id)
                .context(format!(
                    "{chip_name}: looked up non-existent pin with id: {}",
                    pin.device_pin_id
                ))?;

            pins.push(PackagePin {
                position: pin.ball.clone(),
                // Create a signal for both bonded pins. An example of this is the bonded PA1/NRST on the C110X devices.
                signals: device_pin.name.split("/").map(String::from).collect(),
            });
        }

        pins.sort_by(|a, b| a.position.cmp(&b.position));

        packages.push(Package {
            name: name.to_string(),
            chip: chip_name.to_string(),
            package: package_name.to_string(),
            pins,
        });
    }

    Ok(packages)
}

fn generate_pincm(
    chip_name: &str,
    sysconfig: &SysconfigFile,
) -> anyhow::Result<BTreeMap<String, u32>> {
    let mut pins = BTreeMap::new();

    // TODO: Remove this hack as we have replaced it.
    for device_pin in sysconfig.device_pins.values() {
        // TODO: Does this cause any problems?
        if device_pin.name.contains('/') {
            continue;
        }

        // "None" if the pin is not usable as I/O (GND or VCC for example).
        if let Ok(iomux_cm) = device_pin.attributes.iomux_pincm.parse::<u32>() {
            pins.insert(device_pin.name.to_string(), iomux_cm);
        };
    }

    Ok(pins)
}

fn generate_peripherals2(
    chip_name: &str,
    header: &Header,
    sysconfig: &SysconfigFile,
) -> anyhow::Result<BTreeMap<String, Peripheral>> {
    static GPIO_PIN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)^P(?<bank>[A-Z])\d+").unwrap());
    static DMA_CHANNEL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"DMA_CH(?<channel>\d+)").unwrap());

    let mut peripherals = BTreeMap::new();

    assert_eq!(
        sysconfig.parts.len(),
        1,
        "Assumption that a single part is present was broken"
    );

    // We rely on the part definition to pick out the true peripherals (except for missing)
    // since the metadata has multiple instances of some peripherals which have missing addresses.
    for part in sysconfig.parts.values() {
        for PartPeripheralWrapper { peripheral_id } in part.peripheral_wrapper.iter() {
            let peripheral = sysconfig.peripherals.get(peripheral_id).unwrap();

            let name = &peripheral.name;
            // make names consistent sometimes
            let name = maybe_rename(name);
            let id = &peripheral.id;

            // GPIO pins are handled later by manually being added to their parent GPIO peripherals.
            if GPIO_PIN.is_match(&name) {
                continue;
            }

            // DMA channels have additional metadata that we need to declare separately.
            // The DMA peripheral itself is handled here.
            if DMA_CHANNEL.is_match(&name) {
                continue;
            }

            // SYSMEM in sysconfig metadata is not entirely clear. Either way we have better ways to get this info.
            if name == "SYSMEM" {
                continue;
            }

            // Already have FLASHCTL, but FLASH still has some useful data.
            if name == "FLASH" {
                continue;
            }

            let (ty, version) = get_peripheral_type_version(chip_name, &name);
            let address = get_peripheral_addresses(chip_name, &name, header, sysconfig)?;

            let mut peri = Peripheral {
                name: name.clone(),
                ty,
                version,
                address,
                pins: vec![],
            };

            // Lookup the pins
            for peri_pin in peripheral.peripheral_pin_wrapper.iter() {
                let pin_id = &peri_pin.peripheral_pin_id;
                let pin = sysconfig.peripheral_pins.get(pin_id).context(format!(
                    "Failed to lookup peripheral pin with id, `{pin_id}`, from {name} (id: {id}"
                ))?;

                // The name is `<peripheral>.<signal>`
                let pin_name_and_signal = &pin.name;
                let signal = pin_name_and_signal
                    .split_once('.')
                    .context(format!(
                        "Pin {pin_name_and_signal} from {name} did not match pattern `<peripheral>.<signal>`"
                    ))?
                    .1;

                // It makes more sense to use `reverseMuxes` from the sysconfig metadata.
                //
                // However it seems that TI forgot some pin ids in the reverse muxes. So we get to do O(n^2)
                // search using the forward mux.
                for mux in sysconfig.muxes.iter() {
                    for setting in mux.mux_setting.iter() {
                        if &setting.peripheral_pin_id == pin_id {
                            let device_pin_id = &mux.device_pin_id;
                            let device_pin = sysconfig
                                .device_pins
                                .get(device_pin_id)
                                .context(format!("Device pin with id {device_pin_id}, used by {pin_name_and_signal} (id: {pin_id}) is not present"))?;
                            let device_pin_name = &device_pin.name;

                            // Remove pin entries with a `/` as these represent multi-bonded pins.
                            //
                            // TODO: Does this cause any problems?
                            if device_pin_name.contains('/') {
                                continue;
                            }

                            let pf = setting.mode.parse::<u8>().context(format!(
                                "PF was not valid integer for {device_pin_name}, {pin_name_and_signal}"
                            ))?;

                            peri.pins.push(PeripheralPin {
                                pin: device_pin_name.clone(),
                                signal: String::from(signal),
                                pf: Some(pf),
                            });
                        }
                    }
                }

                // dedup pins as the metadata contains some duplicate pins.
                peri.pins.dedup();
                peri.pins.sort_by(|a, b| {
                    let signal = a.signal.cmp(&b.signal);

                    if signal == Ordering::Equal {
                        let pf = a.pf.cmp(&b.pf);

                        if pf == Ordering::Equal {
                            return a.pin.cmp(&b.pin);
                        }

                        return pf;
                    }

                    signal
                });
            }

            peripherals.insert(name.to_string(), peri);
        }
    }

    Ok(peripherals)
}

fn maybe_rename(name: &str) -> String {
    if name == "EVENTLP" {
        return "EVENT".to_string();
    }

    name.to_string()
}

fn get_peripheral_type_version(chip_name: &str, name: &str) -> (PeripheralType, Option<String>) {
    if name.starts_with("SYSCTL") {
        return (PeripheralType::Sysctl, Some(get_sysctl_version(chip_name)));
    }

    let ty = if name.starts_with("TIMA") {
        PeripheralType::Tim
    } else if name.starts_with("TIMG") {
        PeripheralType::Tim
    } else if name.starts_with("UART") {
        PeripheralType::Uart
    } else {
        PeripheralType::Unknown
    };

    (ty, None)
}

fn get_peripheral_addresses(
    chip_name: &str,
    name: &str,
    header: &Header,
    _sysconfig: &SysconfigFile,
) -> anyhow::Result<Option<u32>> {
    let mut name = Cow::from(name);

    // GPAMP lives in sysctl.
    if name == "GPAMP" {
        return Ok(None);
    }

    if name == "EVENT" {
        // Constant address
        return Ok(Some(0x400C9000));
    }

    let address = header
        .peripheral_addresses
        .get(name.as_ref())
        .copied()
        .context(format!(
            "{chip_name}: Could not resolve address for peripheral: {name}"
        ))?;

    Ok(Some(address))
}

fn get_sysctl_version(chip_name: &str) -> String {
    let s = match chip_name {
        "MSPM0C110X" => "c110x",
        "MSPM0L110X" | "MSPM0L130X" | "MSPM0L134X" => "l110x_l130x_l134x",
        "MSPM0L122X" | "MSPM0L222X" => "l122x_l222x",
        "MSPM0G110X" | "MSPM0G150X" | "MSPM0G310X" | "MSPM0G350X" => "g350x_g310x_g150x_g110x",
        "MSPM0G151X" | "MSPM0G351X" => "g351x_g151x",
        "MSPM0H321X" => "h321x",

        _ => unreachable!("Missing mapping from {chip_name} to sysctl version"),
    };

    String::from(s)
}

fn generate_irqs(
    chip_name: &str,
    header: &Header,
    int_groups: &BTreeMap<String, Groups>,
) -> anyhow::Result<BTreeMap<i32, Interrupt>> {
    let mut interrupts = BTreeMap::new();

    for (&num, entries) in header.irq_numbers.iter() {
        // Generate static entry
        if entries.len() == 1 {
            let entry = &entries[0];
            interrupts.insert(
                num,
                Interrupt {
                    name: entry.clone(),
                    num,
                    group: BTreeMap::new(),
                },
            );

            continue;
        }

        // FIXME: Why is GROUP30 produced here? Seems to be the presence of LFSS and RTC_A/B, but these are the same?

        // Interrupt group
        let interrupt = interrupts.entry(num).or_insert_with(|| Interrupt {
            name: format!("GROUP{num}"),
            num,
            group: BTreeMap::new(),
        });

        let Some(int_groups) = int_groups.get(&chip_name.to_lowercase()) else {
            println!("{chip_name}: Could not find INT_GROUP mapping file");
            continue;
        };

        let Some(group) = int_groups.groups.get(&interrupt.name) else {
            println!(
                "{chip_name}: Could not find mappings for {}",
                interrupt.name
            );
            continue;
        };

        for entry in entries {
            let Some(group_mapping) = group.iter().find(|i| &i.name == entry) else {
                println!(
                    "{chip_name}: missing group mapping for interrupt {entry} in {}",
                    interrupt.name
                );
                continue;
            };

            if interrupt
                .group
                .insert(group_mapping.iidx as u32, entry.clone())
                .is_some()
            {
                return Err(anyhow::anyhow!(
                    "{chip_name}, {}: IIDX {} already has a mapping",
                    interrupt.name,
                    group_mapping.iidx
                ));
            }
        }
    }

    Ok(interrupts)
}

fn generate_dma_channels(
    chip_name: &str,
    sysconfig: &SysconfigFile,
) -> anyhow::Result<BTreeMap<u32, DmaChannel>> {
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"DMA_CH(?<channel>\d+)").unwrap());

    let mut channels = BTreeMap::new();

    for channel in sysconfig
        .peripherals
        .values()
        .filter(|p| p.name.starts_with("DMA_CH"))
    {
        let name = &channel.name;
        let captures = PATTERN.captures(name).unwrap();
        let channel_number = captures["channel"]
            .parse::<u32>()
            .context("Could not parse DMA channel number")?;
        let full = channel
            .attributes
            .get("full_channel")
            .context(format!("{name} does not have a full_channel attribute"))?
            .as_bool()
            .context(format!("{name} full_channel attribute is not a bool"))?;

        channels.insert(channel_number, DmaChannel { full });
    }

    Ok(channels)
}
