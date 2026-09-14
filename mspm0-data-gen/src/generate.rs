use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::LazyLock,
};

use anyhow::{anyhow, bail, ensure, Context};
use mspm0_data_types::{
    Adc, AdcInternalChannel, AdcInternalSource, Chip, Comp, Dma, DmaChannel, Flashctl, Interrupt,
    IoStructure, Memory, MemoryKind, Package, PackagePin, Peripheral, PeripheralInterrupt,
    PeripheralPin, PeripheralType, PowerDomain, PowerMode, Sysctl, Timer, Uart, Unicomm, Vref,
};
use regex::Regex;

use crate::comp::CompTiming;
use crate::{
    adc_channels::AdcChannels,
    adc_internal_sample::AdcInternalSample,
    adc_sample::AdcSample,
    adc_wakeup::AdcWakeup,
    header::Header,
    int_group::Groups,
    opa::Opas,
    operating_modes::OperatingModes,
    parts::{PartFamily, PartMemory},
    perimap::PERIMAP,
    sources::{FamilySources, Sources},
    svd::Svd,
    sysconfig::{self, PartPeripheralWrapper, SysconfigFile},
    timers::Timers,
    uart::Uarts,
    verify,
};

pub fn generate(sources: &Sources) -> anyhow::Result<()> {
    fs::create_dir_all("./build/data/").unwrap();

    for family in sources.parts.families.iter() {
        let family_sources = sources.family(&family.family)?;

        generate_family(family, &family_sources)
            .context(format!("Error when generating family: {}", family.family))?;
    }

    Ok(())
}

fn generate_family(family: &PartFamily, sources: &FamilySources) -> anyhow::Result<()> {
    let FamilySources {
        header,
        sysconfig,
        adc_channels,
        adc_internal_sample,
        adc_sample,
        adc_wakeup,
        svd,
        clock_tree,
        operating_modes,
        timers,
        uart,
        opa,
        errata,
        wake,
        vref,
        comp,
        temp_sensor,
        int_groups,
    } = *sources;

    // Data shared across all chips in a family.
    let packages = get_packages(&family.family, sysconfig)?;
    let iomux = generate_pincm(sysconfig)?;
    let io_structure = generate_io_structure(&family.family, sysconfig)?;
    let wakeup_pins = generate_wakeup_pins(sysconfig);
    let mut peripherals = generate_peripherals2(&family.family, header, sysconfig)?;
    let interrupts = generate_irqs(&family.family, header, int_groups)?;
    let dma_channels = generate_dma_channels(sysconfig)?;
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
    apply_adc(
        family,
        sysconfig,
        adc_channels,
        adc_internal_sample,
        adc_sample,
        adc_wakeup,
        &mut peripherals,
    )?;
    apply_unicomm(family, header, &mut peripherals)?;
    apply_uart(family, sysconfig, uart, &mut peripherals)?;
    apply_opa(opa, &mut peripherals);
    apply_vref(vref, &mut peripherals);
    apply_comp(sysconfig, comp, &mut peripherals);
    apply_flashctl(family, header, &mut peripherals);
    apply_sysctl(family, &mut peripherals);
    apply_dma(family, header, svd, &mut peripherals)?;

    for part_number in family.part_numbers.iter() {
        // A package sysconfig has no pinout for produces no chip at all, and the filter below
        // cannot tell that from a package this part is simply not offered in. The G150x parts
        // each list DGS, which their datasheet sells as MSPM0G150xSDGS28R and which the family's
        // sysconfig has no entry for, so three parts lost a package and nothing said so.
        let unknown: Vec<_> = part_number
            .packages
            .iter()
            .filter(|wanted| !packages.iter().any(|package| &package.package == *wanted))
            .map(String::as_str)
            .collect();

        // Reported rather than fatal: the source is incomplete, not self-contradictory, and
        // parts.yaml should keep saying what TI sells.
        if !unknown.is_empty() {
            eprintln!(
                "{}: parts.yaml lists package {}, which sysconfig has no pinout for, so no chip \
                 is generated for it",
                part_number.name,
                unknown.join(", ")
            );
        }

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
            io_structure: io_structure.clone(),
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
            temperature_sensor: temp_sensor,
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

/// Read each pin's IO structure from sysconfig's `io_type`.
///
/// An unrecognised value is an error rather than a skipped pin: the structure decides which PINCM
/// fields do anything, so a pin quietly missing from the map would read as a pin with no
/// restrictions.
fn generate_io_structure(
    family: &str,
    sysconfig: &SysconfigFile,
) -> anyhow::Result<BTreeMap<String, IoStructure>> {
    let mut structures = BTreeMap::new();

    for device_pin in sysconfig.device_pins.values() {
        // Multi-bonded pins are listed separately under each function, as in generate_pincm.
        if device_pin.name.contains('/') {
            continue;
        }

        // A pin with no PINCM is not usable as I/O, and sysconfig gives those no io_type either.
        if device_pin.attributes.iomux_pincm.parse::<u32>().is_err() {
            continue;
        }

        let io_type = device_pin.attributes.io_type.as_deref().context(format!(
            "{family}: {} has a PINCM but no io_type",
            device_pin.name
        ))?;

        let structure = match io_type {
            "SD" => IoStructure::Standard,
            "SDL" => IoStructure::StandardLowLeakage,
            "SDW" => IoStructure::StandardWithWake,
            "HD" => IoStructure::HighDrive,
            "HS" => IoStructure::HighSpeed,
            "OD" => IoStructure::OpenDrain,
            "USB" => IoStructure::Usb,
            other => bail!(
                "{family}: {} has the unknown io_type {other}",
                device_pin.name
            ),
        };

        structures.insert(device_pin.name.to_string(), structure);
    }

    Ok(structures)
}

fn generate_pincm(sysconfig: &SysconfigFile) -> anyhow::Result<BTreeMap<String, u32>> {
    let mut pins = BTreeMap::new();

    for device_pin in sysconfig.device_pins.values() {
        // Multi-bonded pins, as in generate_peripherals2: named for both functions, listed
        // separately under each.
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
            let address = get_peripheral_addresses(chip_name, &name, header)?;
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
                unicomm: None,
                uart: None,
                opa: None,
                vref: None,
                comp: None,
                flashctl: None,
                sysctl: None,
                dma: None,
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

                            // Multi-bonded pins, which sysconfig names "PA1/NRST". The bank and
                            // pin they alias are listed separately, so skipping them loses nothing.
                            if device_pin_name.contains('/') {
                                continue;
                            }

                            let pf = setting.mode.parse::<u8>().context(format!(
                                "PF was not valid integer for {device_pin_name}, {pin_name_and_signal}"
                            ))?;

                            let pin = device_pin_name.to_string();

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

/// The device series a family belongs to.
///
/// Only exists to keep [`POWER_DOMAIN_FIXES`] from being applied to parts it was never checked
/// against. Adding MSPM33 or a SimpleLink part means adding a variant and its own table, not
/// extending the MSPM0 one — their power domains are laid out differently.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Series {
    Mspm0,
}

impl Series {
    fn of(family: &str) -> anyhow::Result<Self> {
        if family.starts_with("mspm0") || family.starts_with("msps003") {
            return Ok(Series::Mspm0);
        }

        bail!(
            "{family}: unknown device series. Power domains are stated per series, so a new one \
             needs its own table rather than MSPM0's"
        )
    }

    /// The domain a peripheral is in whatever sysconfig claims, if it is one of the known-wrong ones.
    fn power_domain_fix(self, ty: PeripheralType) -> Option<PowerDomain> {
        let fixes = match self {
            Series::Mspm0 => POWER_DOMAIN_FIXES,
        };

        fixes
            .iter()
            .find(|(fixed, _)| *fixed == ty)
            .map(|(_, domain)| *domain)
    }
}

/// Peripherals whose power domain sysconfig states wrongly, or not at all, on MSPM0.
///
/// Each of these holds for every MSPM0 family, so they are applied unconditionally rather than
/// against a list of the families whose metadata happens to be wrong today — a new family got the
/// bug and not the fix. Applying them by construction is also what enforces the invariant; there
/// were six checks in `verify.rs` asserting exactly this list, and they could no longer fail.
///
/// GPAMP is here because sysconfig gives it no power domain at all, not because it gives a wrong
/// one.
const POWER_DOMAIN_FIXES: &[(PeripheralType, PowerDomain)] = &[
    (PeripheralType::AesAdv, PowerDomain::Pd1),
    (PeripheralType::Cpuss, PowerDomain::Pd1),
    (PeripheralType::Crc, PowerDomain::Pd1),
    (PeripheralType::GpAmp, PowerDomain::Pd0),
    (PeripheralType::Spi, PowerDomain::Pd1),
    (PeripheralType::Trng, PowerDomain::Pd1),
];

fn get_power_domain(
    peripheral: &sysconfig::Peripheral,
    ty: PeripheralType,
    chip_name: &str,
) -> anyhow::Result<PowerDomain> {
    if let Some(domain) = Series::of(chip_name)?.power_domain_fix(ty) {
        return Ok(domain);
    }

    let Some(power_domain) = peripheral
        .attributes
        .get("power_domain")
        // G151x uses all caps power domain while other chips use lowercase.
        .or_else(|| peripheral.attributes.get("POWER_DOMAIN"))
    else {
        bail!("{chip_name}: {} has no power domain", peripheral.name)
    };

    let Some(power_domain) = power_domain.as_str() else {
        bail!(
            "{chip_name}: {} power domain is not a string value",
            peripheral.name
        )
    };

    // The ADCs and GPIOs are in both PD0 and PD1; we take the more permissive of the two.
    let domain = match power_domain {
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
            unicomm: None,
            uart: None,
            opa: None,
            vref: None,
            comp: None,
            flashctl: None,
            sysctl: None,
            dma: None,
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
            unicomm: None,
            uart: None,
            opa: None,
            vref: None,
            comp: None,
            flashctl: None,
            sysctl: None,
            dma: None,
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
            let address = get_peripheral_addresses(chip_name, &bank, header)?
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
                    unicomm: None,
                    uart: None,
                    opa: None,
                    vref: None,
                    comp: None,
                    flashctl: None,
                    sysctl: None,
                    dma: None,
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

    // The beeper is documented as one SYSCTL register and addressed through it, so sysconfig does
    // not list it and its register block was carved out of the SYSCTL maps. Every family below
    // puts BEEPCFG at 0x1190 with the same two fields, per its hw_sysctl_*.h.
    //
    // Keyed on the family and not on the SYSCTL version: mspm0l112x and mspm0l211x have the beeper
    // and share `sysctl_l122x_l222x` with mspm0l122x and mspm0l222x, which do not.
    const BEEPER_FAMILIES: &[&str] = &[
        "mspm0c110x",
        "mspm0c1105_c1106",
        "msps003fx",
        "mspm0h321x",
        "mspm0l112x",
        "mspm0l211x",
    ];

    if BEEPER_FAMILIES.contains(&chip_name) {
        let address = get_peripheral_addresses(chip_name, "SYSCTL", header)?
            .context(format!("{chip_name}: BEEPER needs SYSCTL's address"))?;

        let version = PERIMAP
            .get(&format!("{}:{}", chip_name, PeripheralType::Beeper))
            .map(|s| s.to_string());

        peripherals.insert(
            "BEEPER".to_string(),
            Peripheral {
                name: "BEEPER".to_string(),
                ty: PeripheralType::Beeper,
                version,
                address: Some(address),
                // It is part of SYSCTL, which is in PD0 on every family that has a beeper.
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
                unicomm: None,
                uart: None,
                opa: None,
                vref: None,
                comp: None,
                flashctl: None,
                sysctl: None,
                dma: None,
            },
        );
    }

    Ok(())
}

fn maybe_rename(name: &str) -> String {
    if name == "EVENTLP" {
        return "EVENT".to_string();
    }

    name.to_string()
}

/// Peripheral instance name prefixes, and the type each names.
///
/// First match wins, so order matters where one prefix starts with another: `AESADV` has to come
/// before `AES`. TIMA, TIMB and TIMG are all `Tim` - which register block a timer instance uses is
/// a separate question, answered by [`register_block_key`].
const PERIPHERAL_PREFIXES: &[(&str, PeripheralType)] = &[
    ("ADC", PeripheralType::Adc),
    ("AESADV", PeripheralType::AesAdv),
    ("AES", PeripheralType::Aes),
    ("CANFD", PeripheralType::Canfd),
    ("COMP", PeripheralType::Comp),
    ("CPUSS", PeripheralType::Cpuss),
    ("CRC", PeripheralType::Crc),
    ("DAC", PeripheralType::Dac),
    ("DEBUGSS", PeripheralType::Debugss),
    ("DMA", PeripheralType::Dma),
    ("EVENT", PeripheralType::Event),
    ("FLASHCTL", PeripheralType::FlashCtl),
    ("GPAMP", PeripheralType::GpAmp),
    ("GPIO", PeripheralType::Gpio),
    ("I2C", PeripheralType::I2c),
    ("I2S", PeripheralType::I2s),
    ("IOMUX", PeripheralType::Iomux),
    ("IWDT", PeripheralType::Iwdt),
    ("KEYSTORECTL", PeripheralType::KeystoreCtl),
    ("LCD", PeripheralType::Lcd),
    ("LFSS", PeripheralType::Lfss),
    ("MATHACL", PeripheralType::Mathacl),
    ("NPU", PeripheralType::Npu),
    ("OPA", PeripheralType::Opa),
    ("RTC", PeripheralType::Rtc),
    ("SPG", PeripheralType::Spgss),
    ("SPI", PeripheralType::Spi),
    ("SYSCTL", PeripheralType::Sysctl),
    ("TIMA", PeripheralType::Tim),
    ("TIMB", PeripheralType::Tim),
    ("TIMG", PeripheralType::Tim),
    ("TRNG", PeripheralType::Trng),
    ("UART", PeripheralType::Uart),
    ("UC", PeripheralType::Unicomm),
    ("USBFS", PeripheralType::Usbfs),
    ("VREF", PeripheralType::Vref),
    ("WUC", PeripheralType::Wuc),
    ("WWDT", PeripheralType::Wwdt),
];

/// The type of a peripheral, from its instance name.
///
/// `Unknown` for a name no prefix covers. That is not inert - such a peripheral is dropped from the
/// generated metadata - so `verify::peripheral_types_known` reports it rather than letting a source
/// bump quietly lose one.
fn peripheral_type_from_name(name: &str) -> PeripheralType {
    PERIPHERAL_PREFIXES
        .iter()
        .find(|(prefix, _)| name.starts_with(prefix))
        .map(|(_, ty)| *ty)
        .unwrap_or(PeripheralType::Unknown)
}

/// The `data/registers` prefix a peripheral's register block is filed under.
///
/// Normally the peripheral type, since a chip has one block per type. TIMB is the exception: a
/// basic timer has its own block but the same `Tim` type as TIMA and TIMG, so it needs a key of its
/// own to reach `perimap`. The version it selects then gets a module name from `VARIANT_MODULES` in
/// the metapac generator.
fn register_block_key(name: &str, ty: PeripheralType) -> Cow<'static, str> {
    if ty == PeripheralType::Tim && name.starts_with("TIMB") {
        return Cow::Borrowed("timb");
    }

    Cow::Owned(ty.to_string())
}

fn get_peripheral_type_version(chip_name: &str, name: &str) -> (PeripheralType, Option<String>) {
    let ty = peripheral_type_from_name(name);
    let key = register_block_key(name, ty);

    let version = PERIMAP
        .get(&format!("{chip_name}:{key}"))
        .map(|s| s.to_string());

    (ty, version)
}

fn get_peripheral_addresses(
    chip_name: &str,
    name: &str,
    header: &Header,
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

        // Members the device header does not list at all. `IRQn` names peripherals, and the
        // wake-up controller's two event subscriber ports are not peripherals - no header names
        // them on any device, though every datasheet's interrupt table gives them an IIDX. Taking
        // the header as the membership list drops them, which leaves a consumer unable to tell
        // which group member woke it after arming `WUC.FSUB`.
        for mapping in group.iter() {
            if entries.iter().any(|entry| entry == &mapping.name) {
                continue;
            }

            if let Some(existing) = interrupt
                .group
                .insert(mapping.iidx as u32, mapping.name.clone())
            {
                return Err(anyhow::anyhow!(
                    "{chip_name}, {}: IIDX {} maps to both {existing} and {}",
                    interrupt.name,
                    mapping.iidx,
                    mapping.name
                ));
            }
        }
    }

    Ok(interrupts)
}

fn generate_dma_channels(sysconfig: &SysconfigFile) -> anyhow::Result<BTreeMap<u32, DmaChannel>> {
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
/// Offset of each UNICOMM register map below the instance's own address.
///
/// Fixed by the IP: `mspm0g518x.h` computes them with `UC_UART_BASE(UC0_BASE)` and friends over
/// `UC_*_OFFSET`, and the L-series headers write every base out literally. `apply_unicomm` checks
/// each literal against these.
const UNICOMM_MODE_OFFSETS: &[(&str, u32)] = &[
    ("UART", 0x80000),
    ("I2CC", 0x60000),
    ("I2CT", 0x40000),
    ("SPI", 0x20000),
];

/// The register map each UNICOMM mode is described by, and how [`Unicomm`] reports it.
/// A UNICOMM mode view: the suffix its instance name takes, the peripheral type it becomes, and
/// whether a given instance implements it.
type UnicommMode = (&'static str, PeripheralType, fn(&Unicomm) -> bool);

const UNICOMM_MODE_TYPES: &[UnicommMode] = &[
    ("UART", PeripheralType::UnicommUart, |m| m.uart),
    ("I2CC", PeripheralType::UnicommI2cc, |m| m.i2c_controller),
    ("I2CT", PeripheralType::UnicommI2ct, |m| m.i2c_target),
    ("SPI", PeripheralType::UnicommSpi, |m| m.spi),
];

/// Record which register maps each UNICOMM instance implements.
fn apply_unicomm(
    family: &PartFamily,
    header: &Header,
    peripherals: &mut BTreeMap<String, Peripheral>,
) -> anyhow::Result<()> {
    for (name, peripheral) in peripherals.iter_mut() {
        if peripheral.ty != PeripheralType::Unicomm {
            continue;
        }

        let modes = header.unicomm_modes.get(name).context(format!(
            "{}: {name} is not in the header's UNICOMM instance table",
            family.family
        ))?;

        ensure!(
            modes.uart || modes.i2c_controller || modes.i2c_target || modes.spi,
            "{}: {name} implements no UNICOMM register map",
            family.family
        );

        // Where the header states a map's address rather than computing it, hold it to the offset
        // a consumer is told to use. A part which moved one would otherwise be silently wrong.
        if let Some(address) = peripheral.address {
            for (mode, offset) in UNICOMM_MODE_OFFSETS {
                let Some(&stated) = header.peripheral_addresses.get(&format!("{name}_{mode}"))
                else {
                    continue;
                };

                ensure!(
                    stated == address - offset,
                    "{}: {name}_{mode} is at {stated:#x}, not {:#x} as the offset from {name} says",
                    family.family,
                    address - offset
                );
            }
        }

        peripheral.unicomm = Some(*modes);
    }

    // Each mode the instance implements becomes a peripheral of its own, so that a consumer gets
    // the right register block at the right address rather than having to subtract an offset from
    // the instance. TI's own SVDs model them this way too, as UC0_UART, UC0_I2CC and so on.
    //
    // The instance is what the device has: it owns the pins, the interrupt and the low power
    // facts, and the views repeat only what is true of them as a window onto the same silicon.
    let mut views = Vec::new();

    for peripheral in peripherals.values() {
        let (Some(modes), Some(address)) = (peripheral.unicomm, peripheral.address) else {
            continue;
        };

        for (mode, ty, implemented) in UNICOMM_MODE_TYPES {
            if !implemented(&modes) {
                continue;
            }

            let offset = UNICOMM_MODE_OFFSETS
                .iter()
                .find(|(name, _)| name == mode)
                .map(|(_, offset)| offset)
                .expect("every mode has an offset");

            let name = format!("{}_{mode}", peripheral.name);
            let version = PERIMAP
                .get(&format!("{}:{ty}", family.family))
                .map(|version| version.to_string());

            views.push(Peripheral {
                name: name.clone(),
                ty: *ty,
                version,
                address: Some(address - offset),
                power_domain: peripheral.power_domain,
                pins: vec![],
                sys_fentries: None,
                interrupts: Vec::new(),
                block_async: None,
                retained_through: peripheral.retained_through,
                usable_through: peripheral.usable_through,
                clocked_in_standby1: None,
                timer: None,
                clock_range_hz: None,
                adc: None,
                unicomm: None,
                uart: None,
                opa: None,
                vref: None,
                comp: None,
                flashctl: None,
                sysctl: None,
                dma: None,
            });
        }
    }

    for view in views {
        peripherals.insert(view.name.clone(), view);
    }

    Ok(())
}

/// Attach each OPA instance's input-mux maps from `data/opa`.
fn apply_opa(opas: Option<&Opas>, peripherals: &mut BTreeMap<String, Peripheral>) {
    for (name, peripheral) in peripherals.iter_mut() {
        if peripheral.ty != PeripheralType::Opa {
            continue;
        }

        // Absent data is a gap verify.rs reports, like the other curated per-instance facts.
        peripheral.opa = opas.and_then(|family| family.get(name)).cloned();
    }
}

/// Attach the family's VREF startup time to its VREF instance.
///
/// Left absent rather than defaulted when the family has no figure: a consumer which has to wait out
/// `VREF_ERR_01` needs to refuse rather than guess, and a guessed number is one that silently returns
/// an unsettled reference.
fn apply_vref(vref: Option<Vref>, peripherals: &mut BTreeMap<String, Peripheral>) {
    let Some(vref) = vref else {
        return;
    };

    for peripheral in peripherals
        .values_mut()
        .filter(|peripheral| peripheral.ty == PeripheralType::Vref)
    {
        peripheral.vref = Some(vref);
    }
}

/// Attach each COMP instance's facts: whether it implements the `CTL2.REFSRC` internal-reference
/// positions, and the family's timing figures from `data/comp`.
///
/// `SYS_COMP_INT_VREF` is present (as `"1"`) exactly on the instances which have them and absent
/// otherwise. It agrees with every SVD which enumerates `REFSRC` — the five newer-generation SVDs
/// list values 5 through 7 and the four older ones stop at 3 — and with the two datasheets which
/// state the feature in prose (the L1228's "dedicated internal reference", the G5187's "internal
/// VREF1"), so unlike `SYS_LIN_EN` it is safe to read directly. driverlib is *not* a cross-check:
/// its `DL_COMP_REF_SOURCE_INT_VREF` gate names only L122X_L222X, against both of the above.
fn apply_comp(
    sysconfig: &SysconfigFile,
    timing: Option<CompTiming>,
    peripherals: &mut BTreeMap<String, Peripheral>,
) {
    for peripheral in sysconfig.peripherals.values() {
        let name = maybe_rename(&peripheral.name);
        let Some(comp) = peripherals.get_mut(&name) else {
            continue;
        };
        if comp.ty != PeripheralType::Comp {
            continue;
        }

        let int_vref = peripheral
            .attributes
            .get("SYS_COMP_INT_VREF")
            .is_some_and(|value| value.as_str() == Some("1"));

        // Absent timing is a gap verify.rs reports, like the VREF startup figure.
        comp.comp = Some(Comp {
            int_vref,
            enable_fast_ns: timing.and_then(|t| t.enable_fast_ns),
            enable_ulp_ns: timing.and_then(|t| t.enable_ulp_ns),
            dac_settle_ns: timing.and_then(|t| t.dac_settle_ns),
            dac_settle_pin_ns: timing.and_then(|t| t.dac_settle_pin_ns),
        });
    }
}

/// Attach the flash geometry and protection layout to the FLASHCTL instance.
///
/// The widths and the ECC flag come from the per-device header's `FLASHCTL_SYS_*` constants and
/// `__MSPM0_HAS_ECC__`; the sector size is the datasheet's, curated in `parts.yaml`. Not from the
/// SVDs, which describe `CMDWEPROTA` on parts whose header gives it zero width, nor from
/// driverlib, whose `DL_FLASHCTL_SECTOR_SIZE` is one portfolio-wide constant.
fn apply_sysctl(family: &PartFamily, peripherals: &mut BTreeMap<String, Peripheral>) {
    for peripheral in peripherals
        .values_mut()
        .filter(|peripheral| peripheral.ty == PeripheralType::Sysctl)
    {
        peripheral.sysctl = Some(Sysctl {
            bor_warning_levels: family.bor_warning_levels,
        });
    }
}

/// Record the DMA-wide facts the register block cannot carry.
///
/// The width is a property of the DMA, not of a channel: where it exists the datasheet ticks it for
/// the basic channels as well as the full ones.
fn apply_dma(
    family: &PartFamily,
    header: &Header,
    svd: Option<&Svd>,
    peripherals: &mut BTreeMap<String, Peripheral>,
) -> anyhow::Result<()> {
    // The SVD enumerates `LONGLONG` and carries `DMAAUTOEN` exactly where the header defines the
    // matching constant, on all fifteen families which have one, so a disagreement means a source
    // bump changed one of them.
    //
    // Stride is deliberately not checked this way. Every SVD enumerates `STRIDE_2`-`STRIDE_9` in
    // `DMASRCINCR`, including the families whose header and datasheet both deny stride, so the
    // check would fail on every DMA_A device. The datasheet's "Stride mode" row is the second
    // source there, and it agrees with the header on all ten families which state both.
    if let Some(svd) = svd {
        for (feature, from_header, from_svd) in [
            (
                "128-bit DMA transfers",
                header.dma_long_long,
                svd.dma_long_long,
            ),
            (
                "the DMA's automatic enable",
                header.dma_auto_enable,
                svd.dma_auto_enable,
            ),
        ] {
            ensure!(
                from_header == from_svd,
                "{}: the header {} {feature} but the SVD {}",
                family.family,
                if from_header { "has" } else { "lacks" },
                if from_svd { "has" } else { "lacks" },
            );
        }
    }

    for peripheral in peripherals
        .values_mut()
        .filter(|peripheral| peripheral.ty == PeripheralType::Dma)
    {
        peripheral.dma = Some(Dma {
            long_long_transfers: header.dma_long_long,
            auto_enable: header.dma_auto_enable,
            stride_mode: header.dma_stride,
            // `DMA_SYS_MMR_EM` tracks gather alone. Fill and table are on every device's full
            // channels, ticked by every datasheet which carries the table.
            gather_mode: header.dma_extended_mode,
        });
    }

    Ok(())
}

fn apply_flashctl(
    family: &PartFamily,
    header: &Header,
    peripherals: &mut BTreeMap<String, Peripheral>,
) {
    for peripheral in peripherals
        .values_mut()
        .filter(|peripheral| peripheral.ty == PeripheralType::FlashCtl)
    {
        peripheral.flashctl = Some(Flashctl {
            sector_bytes: family.flash_sector_bytes,
            word_bytes: header.flash.datawidth_bits / 8,
            weprota_bits: header.flash.weprota_bits,
            weprotb_bits: header.flash.weprotb_bits,
            weprotc_bits: header.flash.weprotc_bits,
            has_ecc: header.flash.has_ecc,
        });
    }
}

/// Attach each UART instance's extended-feature flags from `data/uart`.
fn apply_uart(
    family: &PartFamily,
    sysconfig: &SysconfigFile,
    uarts: Option<&Uarts>,
    peripherals: &mut BTreeMap<String, Peripheral>,
) -> anyhow::Result<()> {
    for (name, peripheral) in peripherals.iter_mut() {
        if !matches!(
            peripheral.ty,
            PeripheralType::Uart | PeripheralType::UnicommUart
        ) {
            continue;
        }

        // Absent data is a gap verify.rs reports, like the other datasheet extractions.
        peripheral.uart = uarts.and_then(|family| family.get(name)).copied();
    }

    let Some(uarts) = uarts else {
        return Ok(());
    };

    // Cross-check against sysconfig, which is per instance and agrees with every datasheet read so
    // far, so a disagreement means the table was misread rather than that the two sources describe
    // different things. `SYS_UARTADV` marks the legacy extend instances; the UNICOMM hosts state
    // the features themselves, with DALI and Manchester folded into one attribute just as the
    // MSPM0G5187 table folds them into one row. `SYS_LIN_EN` is deliberately not consulted: it is
    // `1` on the main UARTs of mspm0g350x and its siblings, against their datasheet and SVD.
    for peripheral in sysconfig.peripherals.values() {
        if let Some(adv) = peripheral.attributes.get("SYS_UARTADV") {
            let Some(uart) = uarts.get(&peripheral.name) else {
                continue;
            };
            let extend = uart.lin || uart.dali || uart.irda || uart.iso7816 || uart.manchester;
            ensure!(
                adv.as_str() == Some(if extend { "true" } else { "false" }),
                "{}: data/uart says {} {} extend but sysconfig SYS_UARTADV disagrees",
                family.family,
                peripheral.name,
                if extend { "is" } else { "is not" },
            );
        }

        let checks: [(&str, fn(&Uart) -> bool); 5] = [
            ("SYS_UART_LIN_EN", |uart| uart.lin),
            ("SYS_UART_IRDA_EN", |uart| uart.irda),
            ("SYS_UART_SMARTCARD_EN", |uart| uart.iso7816),
            ("SYS_UART_DALI_MENC_EN", |uart| uart.dali),
            ("SYS_UART_DALI_MENC_EN", |uart| uart.manchester),
        ];
        for (attribute, check) in checks {
            let Some(value) = peripheral.attributes.get(attribute) else {
                continue;
            };
            let Some(uart) = uarts.get(&format!("{}_UART", peripheral.name)) else {
                continue;
            };
            ensure!(
                value.as_str() == Some(if check(uart) { "true" } else { "false" }),
                "{}: data/uart and sysconfig {attribute} disagree about {}_UART",
                family.family,
                peripheral.name,
            );
        }
    }

    Ok(())
}

fn apply_adc(
    family: &PartFamily,
    sysconfig: &SysconfigFile,
    adc_channels: Option<&AdcChannels>,
    adc_internal_sample: Option<&AdcInternalSample>,
    adc_sample: Option<&AdcSample>,
    adc_wakeup: Option<AdcWakeup>,
    peripherals: &mut BTreeMap<String, Peripheral>,
) -> anyhow::Result<()> {
    let chip_name = &family.family;
    let vrsel = adc_vrsel_mapping(&family.adc_vrsel)?;

    // Seven datasheets print the `tSample_PGA` table and only four of those families have an OPA:
    // the row is footnoted "Only applies for devices with OPA" because one document covers several
    // devices. Gating on the instance is what keeps that superset out of the metadata.
    let has_opa = peripherals
        .values()
        .any(|peripheral| peripheral.ty == PeripheralType::Opa);

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

        // Absent data is a gap verify.rs reports, like the other datasheet extractions. The
        // per-signal sample minimums are joined on here: the datasheets state one row per signal,
        // so every channel routing that signal on this device carries the same figure, which is
        // what a dual-ADC part reaching one signal from both ADCs needs.
        let internal_channels = adc_channels
            .and_then(|family| family.get(name))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|(channel, source)| {
                let sample_min_ns = adc_internal_sample
                    .and_then(|family| family.sample_min_ns.get(&source))
                    .copied();

                (
                    channel,
                    AdcInternalChannel {
                        source,
                        sample_min_ns,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();

        // One page of the datasheet against another: the `tSample_VREF` row states the channel it
        // was measured on, and it has to be the channel the ADC Channel Mapping table routes VREF
        // to. It is the only independent check available on a channel map, so a disagreement fails
        // generation rather than being reported and passed over.
        if let Some(stated) = adc_internal_sample.and_then(|family| family.vref_channel) {
            let mapped = internal_channels
                .iter()
                .find(|(_, route)| route.source == AdcInternalSource::Vref)
                .map(|(channel, _)| *channel);

            ensure!(
                mapped == Some(stated),
                "{chip_name}: {name}'s tSample_VREF row is measured on channel {stated}, but the \
                 channel mapping table routes VREF to {mapped:?}"
            );
        }

        peripheral.adc = Some(Adc {
            memctl,
            vrsel,
            internal_channels,
            // Only the column the datasheet fills is set. A `None` maximum is the statement that
            // the family publishes no ceiling, so it must not be filled in from the typical.
            wakeup_max_ns: adc_wakeup.and_then(|w| w.max_ns),
            wakeup_typ_ns: adc_wakeup.and_then(|w| w.typ_ns),
            sample_min_ns: adc_sample.map(|s| s.min_ns),
            pga_sample_ns: match (has_opa, adc_sample) {
                (true, Some(sample)) => sample.pga_ns.clone(),
                _ => BTreeMap::new(),
            },
        });
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
        MemoryKind::Ram => PowerMode::Standby1,
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
        let own = interrupts
            .values()
            .filter(|interrupt| &interrupt.name == name)
            .map(|interrupt| PeripheralInterrupt {
                name: interrupt.name.clone(),
                num: interrupt.num,
                group_iidx: None,
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
