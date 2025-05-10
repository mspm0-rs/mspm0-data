use std::{borrow::Cow, cmp::Ordering, collections::BTreeMap, fs, sync::LazyLock};

use anyhow::{bail, Context};
use mspm0_data_types::{
    Chip, DmaChannel, Interrupt, Memory, Package, PackagePin, Peripheral, PeripheralPin,
    PeripheralType, PowerDomain,
};
use regex::Regex;

use crate::{
    header::{Header, Headers},
    int_group::Groups,
    parts::{PartFamily, PartMemory, PartsFile},
    sysconfig::{self, PartPeripheralWrapper, Sysconfig, SysconfigFile},
    verify,
};

pub fn generate(
    parts: &PartsFile,
    headers: &Headers,
    sysconfig: &Sysconfig,
    int_groups: &BTreeMap<String, Groups>,
) -> anyhow::Result<()> {
    fs::create_dir_all("./build/data/").unwrap();

    for family in parts.families.iter() {
        let sysconfig = sysconfig
            .files
            .get(&family.family.to_uppercase())
            .context(format!(
                "No sysconfig data available for {}",
                &family.family
            ))?;

        // MSPS003FX is the same as C110X except for package options and some pins.
        let header_name = if family.family == "msps003fx" {
            "mspm0c110x"
        } else {
            &family.family
        };

        let header = headers
            .headers
            .get(&header_name.to_lowercase())
            .context(format!("Could not lookup header for {}", header_name))?;

        generate_family(family, header, sysconfig, int_groups)
            .context(format!("Error when generating family: {}", family.family))?;
    }

    Ok(())
}

fn generate_family(
    family: &PartFamily,
    header: &Header,
    sysconfig: &SysconfigFile,
    int_groups: &BTreeMap<String, Groups>,
) -> anyhow::Result<()> {
    // Data shared across all chips in a family.
    let packages = get_packages(&family.family, &sysconfig)?;
    let iomux = generate_pincm(&family.family, &sysconfig)?;
    let peripherals = generate_peripherals2(&family.family, header, &sysconfig)?;
    let interrupts = generate_irqs(&family.family, header, int_groups)?;
    let dma_channels = generate_dma_channels(&family.family, &sysconfig)?;

    for part_number in family.part_numbers.iter() {
        // Filter for package types available on the part number.
        let packages = packages
            .iter()
            .filter(|package| part_number.packages.contains(&package.package))
            .cloned();

        let chip = Chip {
            name: part_number.name.clone(),
            family: family.family.clone(),
            datasheet_url: family.datasheet_url.clone(),
            reference_manual_url: family.reference_manual_url.clone(),
            errata_url: family.errata_url.clone(),
            memory: part_number.memory.iter().map(convert_memory).collect(),
            packages: packages.collect(),
            iomux: iomux.clone(),
            peripherals: peripherals.clone(),
            interrupts: interrupts.clone(),
            dma_channels: dma_channels.clone(),
        };

        if let Err(err) = verify::verify(&chip, &part_number.name) {
            eprintln!("{err}");
        };

        let data = serde_json::to_string_pretty(&chip)
            .context(format!("Serializing chip {}", part_number.name))?;

        fs::write(format!("./build/data/{}.json", &part_number.name), data)
            .context(format!("Error writing data for {}", part_number.name))?;
    }

    Ok(())
}

fn get_packages(family: &str, sysconfig: &SysconfigFile) -> anyhow::Result<Vec<Package>> {
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
                    "{family}: looked up non-existent pin with id: {}",
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
            chip: family.to_string(),
            package: package_name.to_string(),
            pins,
        });
    }

    Ok(packages)
}

fn generate_pincm(
    _chip_name: &str,
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

            // GPAMP does not exist on these parts.
            if name == "GPAMP" && (chip_name == "mspm0c110x" || chip_name == "mspm0g151x") {
                continue;
            }

            let (ty, version) = get_peripheral_type_version(chip_name, &name);
            let address = get_peripheral_addresses(chip_name, &name, header, sysconfig)?;
            let power_domain = get_power_domain(peripheral, ty, chip_name)?;

            let mut peri = Peripheral {
                name: name.clone(),
                ty,
                version,
                address,
                power_domain,
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

                            let pin = device_pin_name
                                .split_once('/')
                                .map(|(a, _)| a)
                                .unwrap_or_else(|| &device_pin_name)
                                .to_string();

                            if skip_peripheral_pin(device_pin_name, chip_name) {
                                continue;
                            }

                            peri.pins.push(PeripheralPin {
                                pin,
                                signal: String::from(signal),
                                pf: Some(pf),
                            });
                        }
                    }
                }

                // dedup pins as the metadata contains some duplicate pins.
                peri.pins.dedup();
            }

            peripherals.insert(name.to_string(), peri);
        }
    }

    generate_missing(chip_name, header, sysconfig, &mut peripherals)?;

    peripherals.iter_mut().for_each(|(_, p)| {
        p.pins.sort_by(|a, b| {
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
    });

    Ok(peripherals)
}

fn get_power_domain(
    peripheral: &sysconfig::Peripheral,
    ty: PeripheralType,
    chip_name: &str,
) -> anyhow::Result<PowerDomain> {
    let Some(power_domain) = peripheral.attributes.get("power_domain") else {
        // GPAMP does not have a specified power domain from sysconfig. It is always in PD0.
        if peripheral.name == "GPAMP" {
            return Ok(PowerDomain::Pd0);
        }

        bail!("{chip_name}: {} has no power domain", peripheral.name)
    };

    let Some(power_domain) = power_domain.as_str() else {
        bail!(
            "{chip_name}: {} power domain is not a string value",
            peripheral.name
        )
    };

    // A few notes on exceptions:
    // - ADCx:
    //   The ADCs technically are in both PD0 and PD1 power domains. We pick PD0 since the
    //   ADC is in the more permissive power.
    //
    // - GPIOx:
    //   Same rationale as ADCs
    let domain = match power_domain {
        // Fix mistakes in SYSCTL
        "PD_ULP_AON"
            if (chip_name == "msps003fx"
                || chip_name == "mspm0c110x"
                || chip_name == "mspm0l110x"
                || chip_name == "mspm0l122x"
                || chip_name == "mspm0l130x"
                || chip_name == "mspm0l134x"
                || chip_name == "mspm0l222x")
                && ty == PeripheralType::Cpuss =>
        {
            PowerDomain::Pd1
        }
        "PD_ULP_AON"
            if (chip_name == "mspm0l122x" || chip_name == "mspm0l222x")
                && ty == PeripheralType::AesAdv =>
        {
            PowerDomain::Pd1
        }
        "PD_ULP_AON"
            if (chip_name == "msps003fx"
                || chip_name == "mspm0c110x"
                || chip_name == "mspm0l110x"
                || chip_name == "mspm0l122x"
                || chip_name == "mspm0l130x"
                || chip_name == "mspm0l134x"
                || chip_name == "mspm0l222x")
                && ty == PeripheralType::Crc =>
        {
            PowerDomain::Pd1
        }
        "PD_ULP_AON"
            if (chip_name == "msps003fx"
                || chip_name == "mspm0c110x"
                || chip_name == "mspm0l110x"
                || chip_name == "mspm0l122x"
                || chip_name == "mspm0l130x"
                || chip_name == "mspm0l134x"
                || chip_name == "mspm0l222x")
                && ty == PeripheralType::Spi =>
        {
            PowerDomain::Pd1
        }
        "PD_ULP_AON"
            if (chip_name == "mspm0l122x" || chip_name == "mspm0l222x")
                && ty == PeripheralType::Trng =>
        {
            PowerDomain::Pd1
        }

        // Q: GPAMP appears to be in PD0 but is None in most chips.

        // Normal
        "PD_ULP_AON" => PowerDomain::Pd0,
        "PD_ULP_AAON" => PowerDomain::Pd1,
        "PD_VRTC_AON" => PowerDomain::Backup,
        _ => anyhow::bail!("{chip_name}: Unknown power domain value: {}", power_domain),
    };

    Ok(domain)
}

fn generate_missing(
    chip_name: &str,
    header: &Header,
    sysconfig: &SysconfigFile,
    peripherals: &mut BTreeMap<String, Peripheral>,
) -> anyhow::Result<()> {
    static GPIO_PIN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)^P(?<bank>[A-Z])\d+").unwrap());

    peripherals.insert(
        "DMA".to_string(),
        Peripheral {
            name: "DMA".to_string(),
            ty: PeripheralType::Dma,
            version: None,
            address: Some(0x4042A000),
            // DMA always lives in PD1
            power_domain: PowerDomain::Pd1,
            pins: vec![],
        },
    );

    // GPIO peripherals are not described in sysconfig.
    for device_pin in sysconfig.device_pins.values() {
        if let Some(captures) = GPIO_PIN.captures(&device_pin.name) {
            let bank = &captures["bank"];

            // Resolving the address always is unfortunately required because or_insert_with_key cannot handle
            // fallible closures.
            let bank = format!("GPIO{bank}");
            let address = get_peripheral_addresses(chip_name, &bank, header, sysconfig)?
                .context(format!("{bank} must have address"))?;

            let gpio = peripherals
                .entry(bank)
                .or_insert_with_key(|name| Peripheral {
                    name: name.clone(),
                    ty: PeripheralType::Gpio,
                    version: None,
                    address: Some(address),
                    // GPIO always lives in PD0
                    power_domain: PowerDomain::Pd0,
                    pins: vec![],
                });

            let pin = device_pin
                .name
                .split_once('/')
                .map(|(a, _)| a)
                .unwrap_or_else(|| &device_pin.name)
                .to_string();

            gpio.pins.push(PeripheralPin {
                pin: pin.clone(),
                signal: pin,
                // GPIO always has a PF of 1
                pf: Some(1),
            });
        }
    }

    Ok(())
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

    let ty = if name.starts_with("ADC") {
        PeripheralType::Adc
    } else if name.starts_with("AESADV") {
        PeripheralType::AesAdv
    } else if name.starts_with("AES") {
        PeripheralType::Aes
    } else if name.starts_with("CANFD") {
        PeripheralType::Canfd
    } else if name.starts_with("COMP") {
        PeripheralType::Comp
    } else if name.starts_with("CPUSS") {
        PeripheralType::Cpuss
    } else if name.starts_with("CRC") {
        PeripheralType::Crc
    } else if name.starts_with("DAC") {
        PeripheralType::Dac
    } else if name.starts_with("DEBUGSS") {
        PeripheralType::Debugss
    } else if name.starts_with("DMA") {
        PeripheralType::Dma
    } else if name.starts_with("EVENT") {
        PeripheralType::Event
    } else if name.starts_with("FLASHCTL") {
        PeripheralType::FlashCtl
    } else if name.starts_with("GPAMP") {
        PeripheralType::GpAmp
    } else if name.starts_with("GPIO") {
        PeripheralType::Gpio
    } else if name.starts_with("I2C") {
        PeripheralType::I2c
    } else if name.starts_with("IOMUX") {
        PeripheralType::Iomux
    } else if name.starts_with("KEYSTORECTL") {
        PeripheralType::KeystoreCtl
    } else if name.starts_with("LCD") {
        PeripheralType::Lcd
    } else if name.starts_with("LFSS") {
        PeripheralType::Lfss
    } else if name.starts_with("MATHACL") {
        PeripheralType::Mathacl
    } else if name.starts_with("OPA") {
        PeripheralType::Opa
    } else if name.starts_with("RTC") {
        PeripheralType::Rtc
    } else if name.starts_with("SPI") {
        PeripheralType::Spi
    } else if name.starts_with("TIMA") {
        PeripheralType::Tim
    } else if name.starts_with("TIMG") {
        PeripheralType::Tim
    } else if name.starts_with("TRNG") {
        PeripheralType::Trng
    } else if name.starts_with("UART") {
        PeripheralType::Uart
    } else if name.starts_with("VREF") {
        PeripheralType::Vref
    } else if name.starts_with("WUC") {
        PeripheralType::Wuc
    } else if name.starts_with("WWDT") {
        PeripheralType::Wwdt
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
    let name = Cow::from(name);

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
        "msps003fx" | "mspm0c110x" => "c110x",
        "mspm0l110x" | "mspm0l130x" | "mspm0l134x" => "l110x_l130x_l134x",
        "mspm0l122x" | "mspm0l222x" => "l122x_l222x",
        "mspm0g110x" | "mspm0g150x" | "mspm0g310x" | "mspm0g350x" => "g350x_g310x_g150x_g110x",
        "mspm0g151x" | "mspm0g351x" => "g351x_g151x",
        "mspm0h321x" => "h321x",

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
        // If LFSS is present, then RTC belongs to LFSS interrupts.
        let is_lfss = num == 30 && entries.iter().any(|p| p == "LFSS");

        // Generate static entry
        //
        // But RTC and LFSS
        if entries.len() == 1 || is_lfss {
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
    _chip_name: &str,
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

fn skip_peripheral_pin(pin_name: &String, chip_name: &str) -> bool {
    // L130X and L134X defines some device pins that only contain `OPAx.IN0-`, which is one of the symbols. Not the pin
    // itself.
    if (chip_name == "mspm0l130x" || chip_name == "mspm0l134x")
        && (pin_name == "OPA0.IN0-" || pin_name == "OPA1.IN0-")
    {
        return true;
    }

    false
}

fn convert_memory(memory: &PartMemory) -> Memory {
    Memory {
        name: memory.name.clone(),
        length: memory.length,
        address: memory.address,
    }
}
