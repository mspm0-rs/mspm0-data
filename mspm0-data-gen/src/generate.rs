use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::LazyLock,
};

use anyhow::{anyhow, bail, ensure, Context};
use mspm0_data_types::{
    Adc, Chip, DmaChannel, Interrupt, Memory, MemoryKind, Package, PackagePin, Peripheral,
    PeripheralInterrupt, PeripheralPin, PeripheralType, PowerDomain, PowerMode, Timer, WakeTimes,
};
use regex::Regex;

use crate::{
    clock_tree::{ClockTreeFile, ClockTrees},
    errata::Errata,
    header::{Header, Headers},
    int_group::Groups,
    operating_modes::OperatingModes,
    parts::{PartFamily, PartMemory, PartsFile},
    perimap::PERIMAP,
    svd::{Svd, Svds},
    sysconfig::{self, PartPeripheralWrapper, Sysconfig, SysconfigFile},
    timers::Timers,
    verify,
};

pub fn generate(
    parts: &PartsFile,
    headers: &Headers,
    sysconfig: &Sysconfig,
    svds: &Svds,
    operating_modes: &BTreeMap<String, OperatingModes>,
    int_groups: &BTreeMap<String, Groups>,
    timers: &BTreeMap<String, Timers>,
    clock_trees: &ClockTrees,
    errata: &BTreeMap<String, Errata>,
    wake: &BTreeMap<String, WakeTimes>,
) -> anyhow::Result<()> {
    fs::create_dir_all("./build/data/").unwrap();

    for family in parts.families.iter() {
        let sysconfig = sysconfig
            .files
            .get(&family.family.to_uppercase())
            .context(format!("No sysconfig data available for {}", family.family))?;

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

        let svd = svds.files.get(&family.family);

        generate_family(
            family,
            header,
            sysconfig,
            svd,
            operating_modes.get(&family.family),
            int_groups,
            timers.get(&family.family),
            clock_trees.files.get(&family.family),
            errata.get(&family.family),
            wake.get(&family.family).copied(),
        )
        .context(format!("Error when generating family: {}", family.family))?;
    }

    Ok(())
}

fn generate_family(
    family: &PartFamily,
    header: &Header,
    sysconfig: &SysconfigFile,
    svd: Option<&Svd>,
    operating_modes: Option<&OperatingModes>,
    int_groups: &BTreeMap<String, Groups>,
    timers: Option<&Timers>,
    clock_tree: Option<&ClockTreeFile>,
    errata: Option<&Errata>,
    wake: Option<WakeTimes>,
) -> anyhow::Result<()> {
    // Data shared across all chips in a family.
    let packages = get_packages(&family.family, sysconfig)?;
    let iomux = generate_pincm(&family.family, sysconfig)?;
    let wakeup_pins = generate_wakeup_pins(sysconfig);
    let mut peripherals = generate_peripherals2(&family.family, header, sysconfig)?;
    let interrupts = generate_irqs(&family.family, header, int_groups)?;
    let dma_channels = generate_dma_channels(&family.family, sysconfig)?;
    let backup_domain = has_backup_domain(&family.family, sysconfig, &peripherals)?;
    let clock_tree = clock_tree
        .context(format!("{}: no clocktree.json", family.family))?
        .clock_tree(family);

    // Low power facts which are easier to attach once every peripheral is known.
    apply_peripheral_interrupts(&mut peripherals, &interrupts);
    apply_block_async(&mut peripherals, svd);
    apply_operating_modes(operating_modes, &mut peripherals);
    apply_standby1_timers(family, &mut peripherals)?;
    apply_timers(family, sysconfig, timers, &mut peripherals)?;
    apply_clock_ranges(family, &mut peripherals);
    apply_adc(family, sysconfig, &mut peripherals)?;

    for part_number in family.part_numbers.iter() {
        // Filter for package types available on the part number.
        let packages = packages
            .iter()
            .filter(|package| part_number.packages.contains(&package.package))
            .cloned()
            .map(|package| {
                // We need to build the actual chip name, including package.
                //
                // e.g. mspm0c1104dgs20
                //
                // however this really should be something like mspm0c1104**s**dgs20 or mspm0c1104**q**dgs20
                let mut chip = part_number.name.clone();
                chip.push_str(&package.package.to_lowercase());

                Package {
                    name: package.name,
                    chip,
                    package: package.package,
                    pins: package.pins,
                }
            });

        let chip = Chip {
            name: part_number.name.clone(),
            family: family.family.clone(),
            datasheet_url: family.datasheet_url.clone(),
            reference_manual_url: family.reference_manual_url.clone(),
            errata_url: family.errata_url.clone(),
            memory: part_number
                .memory
                .iter()
                .map(convert_memory)
                .collect::<anyhow::Result<_>>()?,
            packages: packages.collect(),
            iomux: iomux.clone(),
            wakeup_pins: wakeup_pins.clone(),
            peripherals: peripherals.clone(),
            interrupts: interrupts.clone(),
            dma_channels: dma_channels.clone(),
            nvic_priority_bits: header.nvic_priority_bits,
            max_mclk_hz: family.max_mclk_hz,
            max_ulpclk_hz: family.max_ulpclk_hz,
            sysosc_base_hz: family.sysosc_base_hz,
            flash_wait_hz: family.flash_wait_hz.clone(),
            backup_domain,
            clock_tree,
            errata: errata.map(|e| e.errata.clone()).unwrap_or_default(),
            wake_ns: wake.unwrap_or_default(),
        };

        for err in verify::verify(&chip, &part_number.name) {
            eprintln!("{err}");
        }

        let data = serde_json::to_string_pretty(&chip)
            .context(format!("Serializing chip {}", part_number.name))?;

        fs::write(format!("./build/data/{}.json", part_number.name), data)
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

        let captures = PATTERN.captures(raw_name).unwrap();
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
    static USB_EP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"USBFS(\d+)_EP(\w+)").unwrap());

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
            if name == "GPAMP"
                && (chip_name == "mspm0c110x"
                    || chip_name == "mspm0c1105_c1106"
                    || chip_name == "mspm0g151x")
            {
                continue;
            }

            // CANFD does not exist on G151x
            if name.starts_with("CANFD") && chip_name == "mspm0g151x" {
                continue;
            }

            // IWDT technically exists on G151x and G351x, but the SDK and datasheets do not define the address for IWDT.
            //
            // To prevent issues, we will only consider IWDT to exist on chips which define an address.
            if name == "IWDT" && !header.peripheral_addresses.contains_key(&name) {
                continue;
            }

            // Sysconfig creates USB EP peripherals which don't actually exist.
            if USB_EP.is_match(&name) {
                continue;
            }

            let (ty, version) = get_peripheral_type_version(chip_name, &name);
            let address = get_peripheral_addresses(chip_name, &name, header, sysconfig)?;
            let power_domain = get_power_domain(peripheral, ty, chip_name)?;
            let sys_fentries = get_sys_fentries(peripheral, chip_name)?;

            let mut peri = Peripheral {
                name: name.clone(),
                ty,
                version,
                address,
                power_domain,
                pins: vec![],
                sys_fentries,
                // Filled in by the `apply_*` passes once every peripheral is known.
                interrupts: Vec::new(),
                block_async: None,
                retained_through: None,
                usable_through: None,
                clocked_in_standby1: None,
                timer: None,
                clock_range_hz: None,
                adc: None,
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
                                .unwrap_or_else(|| device_pin_name)
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

fn get_sys_fentries(
    peripheral: &sysconfig::Peripheral,
    chip_name: &str,
) -> anyhow::Result<Option<usize>> {
    if !(peripheral.name.starts_with("SPI")
        || peripheral.name.starts_with("UART")
        || peripheral.name.starts_with("I2C"))
    {
        return Ok(None);
    }

    let Some(sys_fentries) = peripheral.attributes.get("SYS_FENTRIES") else {
        bail!("{chip_name}: {} has no SYS_FENTRIES field", peripheral.name)
    };

    let Some(sys_fentries) = sys_fentries.as_str() else {
        bail!(
            "{chip_name}: {} SYS_FENTRIES field is not a string value",
            peripheral.name
        )
    };

    Ok(Some(sys_fentries.parse::<usize>().unwrap()))
}

fn get_power_domain(
    peripheral: &sysconfig::Peripheral,
    ty: PeripheralType,
    chip_name: &str,
) -> anyhow::Result<PowerDomain> {
    let Some(power_domain) = peripheral
        .attributes
        .get("power_domain")
        // G151x uses all caps power domain while other chips use lowercase.
        .or_else(|| peripheral.attributes.get("POWER_DOMAIN"))
    else {
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
                || chip_name == "mspm0c1105_c1106"
                || chip_name == "mspm0h321x"
                || chip_name == "mspm0l110x"
                || chip_name == "mspm0l122x"
                || chip_name == "mspm0l130x"
                || chip_name == "mspm0l134x"
                || chip_name == "mspm0l222x"
                || chip_name == "mspm0l112x"
                || chip_name == "mspm0l211x")
                && ty == PeripheralType::Cpuss =>
        {
            PowerDomain::Pd1
        }
        "PD_ULP_AON"
            if (chip_name == "mspm0l122x"
                || chip_name == "mspm0l222x"
                || chip_name == "mspm0l112x"
                || chip_name == "mspm0l211x")
                && ty == PeripheralType::AesAdv =>
        {
            PowerDomain::Pd1
        }
        "PD_ULP_AON"
            if (chip_name == "msps003fx"
                || chip_name == "mspm0c110x"
                || chip_name == "mspm0c1105_c1106"
                || chip_name == "mspm0h321x"
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
                || chip_name == "mspm0c1105_c1106"
                || chip_name == "mspm0h321x"
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

    let version = PERIMAP
        .get(&format!("{}:{}", chip_name, PeripheralType::Dma))
        .map(|s| s.to_string());
    peripherals.insert(
        "DMA".to_string(),
        Peripheral {
            name: "DMA".to_string(),
            ty: PeripheralType::Dma,
            version,
            address: Some(0x4042A000),
            // DMA always lives in PD1
            power_domain: PowerDomain::Pd1,
            pins: vec![],
            sys_fentries: None,
            interrupts: Vec::new(),
            block_async: None,
            retained_through: None,
            usable_through: None,
            clocked_in_standby1: None,
            timer: None,
            clock_range_hz: None,
            adc: None,
        },
    );

    // FACTORYREGION is not described in sysconfig, but every SDK header defines its address.
    let version = PERIMAP
        .get(&format!("{}:{}", chip_name, PeripheralType::FactoryRegion))
        .map(|s| s.to_string());
    let address = header
        .peripheral_addresses
        .get("FACTORYREGION")
        .copied()
        .context(format!("{chip_name}: FACTORYREGION must have address"))?;
    peripherals.insert(
        "FACTORYREGION".to_string(),
        Peripheral {
            name: "FACTORYREGION".to_string(),
            ty: PeripheralType::FactoryRegion,
            version,
            address: Some(address),
            // FACTORYREGION is read-only flash which is always available.
            power_domain: PowerDomain::Pd0,
            pins: vec![],
            sys_fentries: None,
            interrupts: Vec::new(),
            block_async: None,
            retained_through: None,
            usable_through: None,
            clocked_in_standby1: None,
            timer: None,
            clock_range_hz: None,
            adc: None,
        },
    );

    // Some devices duplicate the pins multiple times (such as C110x with PA1 and NRST sharing the same physical pin).
    let mut device_pins = BTreeSet::new();

    // GPIO peripherals are not described in sysconfig.
    for device_pin in sysconfig.device_pins.values() {
        if let Some(captures) = GPIO_PIN.captures(&device_pin.name) {
            let bank = &captures["bank"];

            // Resolving the address always is unfortunately required because or_insert_with_key cannot handle
            // fallible closures.
            let bank = format!("GPIO{bank}");
            let address = get_peripheral_addresses(chip_name, &bank, header, sysconfig)?
                .context(format!("{bank} must have address"))?;

            let version = PERIMAP
                .get(&format!("{}:{}", chip_name, PeripheralType::Gpio))
                .map(|s| s.to_string());
            let gpio = peripherals
                .entry(bank)
                .or_insert_with_key(|name| Peripheral {
                    name: name.clone(),
                    ty: PeripheralType::Gpio,
                    version,
                    address: Some(address),
                    // GPIO always lives in PD0
                    power_domain: PowerDomain::Pd0,
                    pins: vec![],
                    sys_fentries: None,
                    interrupts: Vec::new(),
                    block_async: None,
                    retained_through: None,
                    usable_through: None,
                    clocked_in_standby1: None,
                    timer: None,
                    clock_range_hz: None,
                    adc: None,
                });

            let pin = device_pin
                .name
                .split_once('/')
                .map(|(a, _)| a)
                .unwrap_or_else(|| &device_pin.name)
                .to_string();

            if device_pins.insert(pin.clone()) {
                gpio.pins.push(PeripheralPin {
                    pin: pin.clone(),
                    signal: pin,
                    // GPIO always has a PF of 1
                    pf: Some(1),
                });
            }
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
        let version = PERIMAP
            .get(&format!("{}:{}", chip_name, PeripheralType::Sysctl))
            .map(|s| s.to_string());
        return (PeripheralType::Sysctl, version);
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
    } else if name.starts_with("I2S") {
        PeripheralType::I2s
    } else if name.starts_with("IOMUX") {
        PeripheralType::Iomux
    } else if name.starts_with("IWDT") {
        PeripheralType::Iwdt
    } else if name.starts_with("KEYSTORECTL") {
        PeripheralType::KeystoreCtl
    } else if name.starts_with("LCD") {
        PeripheralType::Lcd
    } else if name.starts_with("LFSS") {
        PeripheralType::Lfss
    } else if name.starts_with("MATHACL") {
        PeripheralType::Mathacl
    } else if name.starts_with("NPU") {
        PeripheralType::Npu
    } else if name.starts_with("OPA") {
        PeripheralType::Opa
    } else if name.starts_with("RTC") {
        PeripheralType::Rtc
    } else if name.starts_with("SPI") {
        PeripheralType::Spi
    } else if name.starts_with("TIMA") {
        PeripheralType::Tim
    } else if name.starts_with("TIMB") {
        PeripheralType::Tim
    } else if name.starts_with("TIMG") {
        PeripheralType::Tim
    } else if name.starts_with("TRNG") {
        PeripheralType::Trng
    } else if name.starts_with("UART") {
        PeripheralType::Uart
    } else if name.starts_with("UC") {
        PeripheralType::Unicomm
    } else if name.starts_with("USBFS") {
        PeripheralType::Usbfs
    } else if name.starts_with("VREF") {
        PeripheralType::Vref
    } else if name.starts_with("WUC") {
        PeripheralType::Wuc
    } else if name.starts_with("WWDT") {
        PeripheralType::Wwdt
    } else {
        PeripheralType::Unknown
    };

    // TIMB is a basic timer and has its own register block, so the key names the instance kind
    // rather than the peripheral type. TIMA and TIMG share one, and both keep the plain `tim` key.
    let key = if ty == PeripheralType::Tim && name.starts_with("TIMB") {
        "timb"
    } else {
        &ty.to_string()
    };
    let version = PERIMAP
        .get(&format!("{}:{}", chip_name, key))
        .map(|s| s.to_string());

    (ty, version)
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
            // G151x defines full channel in all caps
            .or_else(|| channel.attributes.get("FULL_CHANNEL"))
            .context(format!("{name} does not have a full_channel attribute"))?
            .as_bool()
            .context(format!("{name} full_channel attribute is not a bool"))?;

        channels.insert(channel_number, DmaChannel { full });
    }

    Ok(channels)
}

/// Record the facts about each ADC instance which the shared `adc_v1` register block cannot carry.
///
/// `MEMCTL` is per instance in sysconfig, so it is read per instance even though no part has two ADCs
/// which disagree. `VRSEL` is stated once per family.
fn apply_adc(
    family: &PartFamily,
    sysconfig: &SysconfigFile,
    peripherals: &mut BTreeMap<String, Peripheral>,
) -> anyhow::Result<()> {
    let chip_name = &family.family;
    let vrsel = adc_vrsel_mapping(&family.adc_vrsel)?;

    let mut memctl = BTreeMap::new();
    for peripheral in sysconfig
        .peripherals
        .values()
        .filter(|p| p.name.starts_with("ADC"))
    {
        let name = maybe_rename(&peripheral.name);

        let raw = peripheral
            .attributes
            .get("SYS_ADC_MEMCTL_DIM")
            .context(format!("{chip_name}: {name} has no SYS_ADC_MEMCTL_DIM"))?;
        let raw = raw.as_str().context(format!(
            "{chip_name}: {name} SYS_ADC_MEMCTL_DIM is not a string value"
        ))?;
        let dim = raw.parse::<u8>().context(format!(
            "{chip_name}: {name} SYS_ADC_MEMCTL_DIM `{raw}` is not a number"
        ))?;

        memctl.insert(name, dim);
    }

    for (name, peripheral) in peripherals.iter_mut() {
        if peripheral.ty != PeripheralType::Adc {
            continue;
        }

        let memctl = *memctl
            .get(name)
            .context(format!("{chip_name}: {name} has no MEMCTL count"))?;

        peripheral.adc = Some(Adc { memctl, vrsel });
    }

    Ok(())
}

fn adc_vrsel_mapping(vrsel: &String) -> anyhow::Result<u8> {
    match vrsel.as_str() {
        "VDD_INTREF" => Ok(3),
        "VDD_INTREF_EXTREF" => Ok(5),
        _ => Err(anyhow!("Invalid adc vrsel option {}", vrsel)),
    }
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

fn convert_memory(memory: &PartMemory) -> anyhow::Result<Memory> {
    let kind = match memory.name.as_str() {
        "FLASH" => MemoryKind::Flash,
        "RAM" | "RAM_BANK" => MemoryKind::Ram,
        name => bail!("Unknown memory partition `{name}`, cannot tell what kind of memory it is"),
    };

    // Flash is non-volatile, and SRAM survives everything short of SHUTDOWN. Only the parts with a
    // second RAM bank differ, and they say so in parts.yaml.
    let retained_through = memory.retained_through.unwrap_or(match kind {
        MemoryKind::Flash => PowerMode::Shutdown,
        MemoryKind::Ram => PowerMode::Standby,
    });

    Ok(Memory {
        name: memory.name.clone(),
        kind,
        length: memory.length,
        address: memory.address,
        retained_through,
    })
}

/// Device pins which have wakeup logic, and can therefore wake the device from SHUTDOWN.
///
/// `None` when the family's sysconfig omits `io_wakeup` entirely, which is a gap in the vendor data
/// rather than a device with no wake-capable pin.
fn generate_wakeup_pins(sysconfig: &SysconfigFile) -> Option<BTreeSet<String>> {
    let pins = sysconfig
        .device_pins
        .values()
        // Multi-bonded pins are excluded everywhere else too, see `generate_pincm`.
        .filter(|pin| !pin.name.contains('/'))
        .collect::<Vec<_>>();

    // Missing and `false` are different answers, and only GPIO pins carry the attribute at all.
    if pins.iter().all(|pin| pin.attributes.io_wakeup.is_none()) {
        return None;
    }

    Some(
        pins.iter()
            .filter(|pin| pin.attributes.io_wakeup.unwrap_or(false))
            .map(|pin| pin.name.clone())
            .collect(),
    )
}

/// Whether the chip has an independent `VBAT` supply, and therefore a real backup power domain.
///
/// The presence of a `VBAT` device pin is the authoritative answer, and is what TRM §30 uses to
/// distinguish the RTC variants.
fn has_backup_domain(
    chip_name: &str,
    sysconfig: &SysconfigFile,
    peripherals: &BTreeMap<String, Peripheral>,
) -> anyhow::Result<bool> {
    let vbat = sysconfig
        .device_pins
        .values()
        .any(|pin| pin.name.split('/').any(|signal| signal == "VBAT"));

    // Cross-check against the power domain sysconfig assigns to the peripherals. A chip with a VBAT
    // pin must place something in the backup domain and vice versa; if the two disagree then one of
    // the two sources has changed meaning and the flag cannot be trusted.
    let backup_peripherals = peripherals
        .values()
        .any(|peripheral| peripheral.power_domain == PowerDomain::Backup);

    ensure!(
        vbat == backup_peripherals,
        "{chip_name}: VBAT pin present is {vbat} but a peripheral in the backup power domain \
         present is {backup_peripherals}"
    );

    Ok(vbat)
}

/// Attach each peripheral to the interrupts it raises.
///
/// A peripheral either owns NVIC interrupts of its own or sits inside an `INT_GROUP` and shares the
/// group's interrupt, distinguished by an `IIDX` value. Both are matched by name, which is the only
/// thing tying them together in the vendor data.
///
/// Every match is collected rather than the first. No MSPM0 peripheral has more than one, but the
/// MSPM33 parts route a peripheral's interrupt outputs to several NVIC lines.
fn apply_peripheral_interrupts(
    peripherals: &mut BTreeMap<String, Peripheral>,
    interrupts: &BTreeMap<i32, Interrupt>,
) {
    for (name, peripheral) in peripherals.iter_mut() {
        let own = interrupts.values().filter_map(|interrupt| {
            (&interrupt.name == name).then(|| PeripheralInterrupt {
                name: interrupt.name.clone(),
                num: interrupt.num,
                group_iidx: None,
            })
        });

        let shared = interrupts.values().filter_map(|interrupt| {
            let (&iidx, _) = interrupt.group.iter().find(|(_, member)| *member == name)?;

            Some(PeripheralInterrupt {
                name: interrupt.name.clone(),
                num: interrupt.num,
                group_iidx: Some(iidx),
            })
        });

        peripheral.interrupts = own.chain(shared).collect();
    }
}

/// Mark the peripheral instances which have their own `CLKCFG.BLOCKASYNC` bit.
fn apply_block_async(peripherals: &mut BTreeMap<String, Peripheral>, svd: Option<&Svd>) {
    let Some(svd) = svd else {
        // No SVD for this family, so leave every instance unknown rather than claiming that none of
        // them can be masked. `verify` reports this.
        return;
    };

    for (name, peripheral) in peripherals.iter_mut() {
        peripheral.block_async = Some(svd.block_async.contains(name));
    }
}

/// Record what the datasheet's operating-mode table says about each peripheral.
///
/// Retention applies only to PD1, since nothing else is automatically disabled by SYSCTL. Usability
/// applies to any peripheral the table has a row for.
fn apply_operating_modes(
    modes: Option<&OperatingModes>,
    peripherals: &mut BTreeMap<String, Peripheral>,
) {
    let Some(modes) = modes else {
        return;
    };

    for (name, peripheral) in peripherals.iter_mut() {
        if peripheral.power_domain == PowerDomain::Pd1 {
            peripheral.retained_through = modes.retained_through.get(name).copied();
        }

        peripheral.usable_through = modes.usable_through.get(name).copied();
    }
}

/// Record the input clock range of the peripherals whose datasheet specifies one.
///
/// The ADC and the TRNG are the only two so far. Both are stated once per datasheet, so they apply to
/// every instance of the family: the parts which have two ADCs give one `fADCCLK` covering both.
fn apply_clock_ranges(family: &PartFamily, peripherals: &mut BTreeMap<String, Peripheral>) {
    for peripheral in peripherals.values_mut() {
        peripheral.clock_range_hz = match peripheral.ty {
            PeripheralType::Adc => Some(family.adc_clock_hz.into()),
            PeripheralType::Trng => family.trng_clock_hz.map(Into::into),
            _ => None,
        };
    }
}

/// Record what each timer instance can do.
///
/// Matched by instance name, which is what the datasheet table names its rows. That the same name
/// can mean different capabilities on another family is exactly why this is read per family rather
/// than from one table.
fn apply_timers(
    family: &PartFamily,
    sysconfig: &SysconfigFile,
    timers: Option<&Timers>,
    peripherals: &mut BTreeMap<String, Peripheral>,
) -> anyhow::Result<()> {
    let Some(timers) = timers else {
        return Ok(());
    };

    // Sysconfig's own count of capture/compare channels, where it has one. It agrees with the
    // datasheet on every instance which has both, so a disagreement means the table was misread
    // rather than that the two sources describe different things.
    //
    // Only the count is cross-checked. `SYS_FLAVOR` cannot stand in for the rest: `flavorC` with two
    // channels covers `TIMG6` both with and without shadow load, since mspm0g151x, mspm0g351x and
    // mspm0g518x drop it where the earlier G families keep it.
    let mut channels = BTreeMap::new();
    for peripheral in sysconfig.peripherals.values() {
        let Some(count) = peripheral.attributes.get("SYS_NUM_CC") else {
            continue;
        };
        let Some(count) = count.as_str().and_then(|count| count.parse::<u8>().ok()) else {
            bail!(
                "{}: {} SYS_NUM_CC is not a number: {count}",
                family.family,
                peripheral.name
            );
        };

        channels.insert(maybe_rename(&peripheral.name), count);
    }

    // How many of the counter array's counters an instance implements. Only the basic timers have
    // more than one, and sysconfig is the only per-instance source: the datasheets state it as a
    // feature of TIMBx in general, in the same words on the G-series parts which have four and the
    // L-series ones which have two.
    let mut counters = BTreeMap::new();
    for peripheral in sysconfig.peripherals.values() {
        let Some(count) = peripheral.attributes.get("SYS_NUM_COUNTERS") else {
            continue;
        };
        let Some(count) = count.as_str().and_then(|count| count.parse::<u8>().ok()) else {
            bail!(
                "{}: {} SYS_NUM_COUNTERS is not a number: {count}",
                family.family,
                peripheral.name
            );
        };

        counters.insert(maybe_rename(&peripheral.name), count);
    }

    for (name, peripheral) in peripherals.iter_mut() {
        if peripheral.ty != PeripheralType::Tim {
            continue;
        }

        let timer = timers.timers.get(name).copied().map(|timer| Timer {
            // Absent means the instance is not a counter array, so it has the one counter.
            counters: counters.get(name).copied().unwrap_or(1),
            ..timer
        });
        peripheral.timer = timer;

        if let (Some(timer), Some(&count)) = (timer, channels.get(name)) {
            ensure!(
                timer.ccp_channels == count,
                "{}, {name}: data/timers says {} capture/compare channels but sysconfig says {count}",
                family.family,
                timer.ccp_channels
            );
        }
    }

    Ok(())
}

/// Record which timers keep receiving a clock in STANDBY1.
fn apply_standby1_timers(
    family: &PartFamily,
    peripherals: &mut BTreeMap<String, Peripheral>,
) -> anyhow::Result<()> {
    for timer in family.standby1_timers.iter() {
        let peripheral = peripherals.get(timer).context(format!(
            "{}: standby1_timers names {timer}, which is not a peripheral of this family",
            family.family
        ))?;

        ensure!(
            peripheral.ty == PeripheralType::Tim,
            "{}: standby1_timers names {timer}, which is not a timer",
            family.family
        );
    }

    for (name, peripheral) in peripherals.iter_mut() {
        if peripheral.ty != PeripheralType::Tim {
            continue;
        }

        peripheral.clocked_in_standby1 = Some(family.standby1_timers.contains(name));
    }

    Ok(())
}
