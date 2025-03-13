use std::{
    collections::{BTreeMap, HashSet},
    fs,
    sync::LazyLock,
};

use anyhow::{anyhow, Context};
use mspm0_data_types::{
    Chip, Interrupt, Package, PackagePin, Peripheral, PeripheralPin, PeripheralType, PinCm,
    PinFunction,
};
use regex::Regex;

use crate::{
    header::{Header, Headers},
    int_group::Groups,
    sysconfig::{Sysconfig, SysconfigFile},
};

const SKIP_CHIPS: &[&str] = &[
    // FIXME: This is not a duplicate of C110x due to pinout differences
    "MSPS003FX",

    // Likely a duplicate of C110x
    "MSPM0C1105_C1106",

    // Unreleased, need to verify if 
    "MSPM0L111X",
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
        let pin_cm = generate_pincm(&name, &sysconfig_entry)?;

        let peripherals = generate_peripherals(
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

        fs::create_dir_all("./data/chips/").unwrap();

        let chip = Chip {
            packages,
            pin_cm,
            peripherals,
            interrupts,
        };

        let _ = fs::write(
            format!("./data/chips/{name}.json"),
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
) -> anyhow::Result<BTreeMap<String, PinCm>> {
    let mut pins = BTreeMap::new();

    for device_pin in sysconfig.device_pins.values() {
        // FIXME: PA10/PA14 - split pins?
        let name = device_pin
            .name
            .split("/")
            .next()
            .unwrap_or(&device_pin.name);

        // "None" if the pin is not usable as I/O.
        if let Ok(iomux_cm) = device_pin.attributes.iomux_pincm.parse::<u32>() {
            pins.insert(
                name.to_string(),
                PinCm {
                    iomux_cm,
                    pfs: BTreeMap::new(),
                },
            );
        };
    }

    // Visit the muxes and get the pf for each CM.
    for mux in sysconfig.muxes.iter() {
        // Lookup the device pin name.
        let device_pin = sysconfig
            .device_pins
            .get(&mux.device_pin_id)
            .context(format!(
                "{chip_name}: looked up non-existent pin with id: {}",
                &mux.device_pin_id
            ))?;

        let pin_name = device_pin
            .name
            .split("/")
            .next()
            .unwrap_or(&device_pin.name);

        if let Some(pincm) = pins.get_mut(pin_name) {
            for setting in mux.mux_setting.iter() {
                let pf_num = setting.mode.parse::<u32>().context("Invalid pf")?;

                let peripheral_pin = sysconfig
                    .peripheral_pins
                    .get(&setting.peripheral_pin_id)
                    .context(format!(
                        "{chip_name}: looked up non-existent pin peripheral pin id: {}",
                        &setting.peripheral_pin_id
                    ))?;

                let pf = pincm.pfs.entry(pf_num).or_default();

                pf.push(PinFunction {
                    name: peripheral_pin.name.clone(),
                });
            }
        }
    }

    Ok(pins)
}

#[derive(Debug, Default, Clone)]
struct PeripheralRaw {
    pins: HashSet<String>,
}

fn generate_peripherals(
    chip_name: &str,
    header: &Header,
    sysconfig: &SysconfigFile,
) -> anyhow::Result<BTreeMap<String, Peripheral>> {
    let mut raw_peripherals = BTreeMap::<String, PeripheralRaw>::new();

    // Peripheral pins are described in one of two ways:
    // Either it is the `<peripheral>`.`<pin>` or a single name.
    for peripheral_pin in sysconfig.peripheral_pins.values() {
        let parts = peripheral_pin.name.split('.').collect::<Vec<&str>>();
        let (peri, pin) = match parts.len() {
            1 => (parts[0], None),
            2 => (parts[0], Some(parts[1])),
            _ => {
                return Err(anyhow!(
                    "{chip_name}: Peripheral \"{}\" has {} number of parts",
                    peripheral_pin.name,
                    parts.len()
                ))
            }
        };

        // Filter nonsense peripherals
        if peri == "VREF" || peri == "SYSMEM" {
            continue;
        }

        let entry = raw_peripherals.entry(peri.to_string()).or_default();

        // Rewrite pin names since the POCIx suffix doesn't do anything
        if let Some(pin) = pin {
            let pin = match pin {
                "CS1_POCI1" if peri.starts_with("SPI") => "CS1".to_string(),
                "CS2_POCI2" if peri.starts_with("SPI") => "CS2".to_string(),
                "CS3_CD_POCI3" if peri.starts_with("SPI") => "CS3_CD".to_string(),
                pin => pin.to_string(),
            };

            entry.pins.insert(pin);
        }
    }

    // TODO: DMA channels do not exist within peripheral pins, so resolve these from peripherals
    // for peri in sysconfig.peripherals.values() {}

    // TODO: Rename or mark ADC channels which are VREF, TEMP and VBAT (if applicable)
    // TODO: Attributes
    // - TEMP channel nums
    // - is a dma channel full?
    // - I2C fifo size
    // - ADC SVT

    static GPIO_PIN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)^P(?<bank>[A-Z])\d+").unwrap());

    let mut peripherals = BTreeMap::<String, Peripheral>::new();

    // Now we have every peripheral, resolve the addresses of each.
    for (name, raw) in raw_peripherals.iter() {
        // Defer GPIO to later, as PA1 is a nonsense peripheral name.
        // What we do instead is later go back and add PA1 to GPIOA
        if GPIO_PIN.is_match(name) {
            continue;
        }

        // TODO: GPAMP (available via SYSCTL, but is dynamically configurable?)
        if name == "GPAMP" {
            continue;
        }

        let address = header
            .peripheral_addresses
            .get(name)
            .context(format!("{chip_name}: Could not resolve address for {name}"))?;

        let irq_n = header
            .irq_numbers
            .iter()
            .find(|(_, peripherals)| peripherals.iter().any(|peri| peri == name));

        // OPA has no IRQs
        if !name.starts_with("OPA") && irq_n.is_none() {
            return Err(anyhow!("{name} has no IRQs"));
        }

        let mut pins = raw
            .pins
            .iter()
            .map(|name| PeripheralPin { name: name.clone() })
            .collect::<Vec<_>>();
        pins.sort_by(|a, b| a.name.cmp(&b.name));

        let ty = if name.starts_with("TIMA") {
            PeripheralType::Tim
        } else if name.starts_with("TIMG") {
            PeripheralType::Tim
        } else if name.starts_with("UART") {
            PeripheralType::Uart
        } else if name.starts_with("SYSCTL") {
            PeripheralType::Sysctl
        } else {
            PeripheralType::Unknown
        };

        let _peripheral = peripherals.entry(name.to_string()).or_insert_with(|| {
            let mut version = None;

            if name.starts_with("SYSCTL") {
                version = Some(get_sysctl_version(chip_name));
            }

            Peripheral {
                name: name.clone(),
                ty,
                version,
                address: *address,
                pins,
            }
        });
    }

    // Assign GPIOs to peripherals
    for (name, _) in raw_peripherals.iter() {
        let Some(captures) = GPIO_PIN.captures(name) else {
            continue;
        };

        let bank = captures
            .name("bank")
            .context("Could not match bank in gpio pin name")?
            .as_str();

        if bank.len() != 1 {
            return Err(anyhow!("GPIO bank was more or less than 1 character"));
        }

        let bank = format!("GPIO{bank}");

        let address = header.peripheral_addresses.get(&bank).context(format!(
            "{chip_name}: Could not resolve address for GPIO bank {bank}"
        ))?;

        let peripheral = peripherals
            .entry(bank.clone())
            .or_insert_with(|| Peripheral {
                name: bank,
                ty: PeripheralType::Gpio,
                version: None,
                address: *address,
                pins: Vec::new(),
            });

        peripheral.pins.push(PeripheralPin { name: name.clone() });
    }

    // TODO: Define fake "GPAMP" peripheral if present

    // IOMUX exists at a constant address on every part, but it is not defined in any data source.
    peripherals.insert(
        String::from("IOMUX"),
        Peripheral {
            name: String::from("IOMUX"),
            ty: PeripheralType::Iomux,
            version: None,
            address: 0x40428000,
            pins: vec![],
        },
    );

    // CPUSS exists at a constant address on every part, but is not defined in any data source.
    peripherals.insert(
        String::from("CPUSS"),
        Peripheral {
            name: String::from("CPUSS"),
            ty: PeripheralType::Cpuss,
            version: None,
            address: 0x40400000,
            pins: vec![],
        },
    );

    // Sort pins
    peripherals
        .values_mut()
        .for_each(|peripheral| peripheral.pins.sort_by(|a, b| a.name.cmp(&b.name)));

    Ok(peripherals)
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
