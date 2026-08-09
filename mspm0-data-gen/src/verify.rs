use std::{collections::HashSet, sync::LazyLock};

use anyhow::{bail, Context};
use mspm0_data_types::{Chip, PeripheralType, PowerDomain};
use regex::Regex;

/// Run every check, returning one error per failure.
///
/// Every check runs: a failing one must not mask those after it, or the families with a gap in their
/// data sources would go otherwise unverified.
pub fn verify(chip: &Chip, name: &str) -> Vec<anyhow::Error> {
    const CHECKS: &[fn(&Chip, &str) -> anyhow::Result<()>] = &[
        core_peripherals,
        pin_names,
        gpio_no_duplicates,
        peripheral_types_known,
        register_blocks_exist,
        // Peripherals which don't actually exist
        no_gpamp_c110x_l151x,
        // Power domains
        verify_aesadv_power_domain,
        verify_cpuss_power_domain,
        verify_crc_power_domain,
        verify_gpamp_power_domain,
        verify_spi_power_domain,
        verify_trng_power_domain,
        // TODO: UART may be in either power domain, add checks if something is wrong
    ];

    CHECKS
        .iter()
        .filter_map(|check| check(chip, name).err())
        .collect()
}

/// Every register block `data/registers` provides, by file stem.
static REGISTER_BLOCKS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    glob::glob("data/registers/*.yaml")
        .unwrap()
        .flatten()
        .filter_map(|path| Some(path.file_stem()?.to_string_lossy().into_owned()))
        .collect()
});

/// A peripheral whose name matched no known prefix.
///
/// `PeripheralType::Unknown` is not inert: `mspm0-metapac-gen` drops those peripherals from the
/// generated metadata, so the chip silently loses one. That is how a source bump which adds a
/// peripheral goes unnoticed, which is why this is an error rather than a note.
fn peripheral_types_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    let unknown = chip
        .peripherals
        .values()
        .filter(|peripheral| peripheral.ty == PeripheralType::Unknown)
        .map(|peripheral| peripheral.name.as_str())
        .collect::<Vec<_>>();

    if !unknown.is_empty() {
        bail!(
            "{name}: no peripheral type for {}, so it will be dropped from the metadata. Add a \
             prefix to `peripheral_type_from_name` and a `PeripheralType` variant",
            unknown.join(", ")
        );
    }

    Ok(())
}

/// A version names a register block, so the block has to be there.
///
/// `Peripheral::version` selects `data/registers/<type>_<version>.yaml`. A version with no such
/// file promises the consumer a register block which does not exist; either curate the block or
/// drop the `perimap` entry until it is written.
fn register_blocks_exist(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for peripheral in chip.peripherals.values() {
        let Some(version) = &peripheral.version else {
            continue;
        };

        let block = format!("{}_{version}", peripheral.ty);

        if !REGISTER_BLOCKS.contains(&block) {
            bail!(
                "{name}: {} claims version {version}, but data/registers/{block}.yaml does not exist",
                peripheral.name
            );
        }
    }

    Ok(())
}

/// Verify all core peripherals are present.
fn core_peripherals(chip: &Chip, name: &str) -> anyhow::Result<()> {
    if !chip.peripherals.contains_key("CPUSS") {
        bail!("{name}: does not have CPUSS");
    }

    if !chip.peripherals.contains_key("DEBUGSS") {
        bail!("{name}: does not have DEBUGSS");
    }

    if !chip.peripherals.contains_key("DMA") {
        bail!("{name}: does not have DMA");
    }

    if !chip.peripherals.contains_key("EVENT") {
        bail!("{name}: does not have EVENT");
    }

    if !chip.peripherals.contains_key("FLASHCTL") {
        bail!("{name}: does not have FLASHCTL");
    }

    // At least GPIOA
    if !chip.peripherals.contains_key("GPIOA") {
        bail!("{name}: does not have GPIOA");
    }

    if !chip.peripherals.contains_key("IOMUX") {
        bail!("{name}: does not have IOMUX");
    }

    if !chip.peripherals.contains_key("SYSCTL") {
        bail!("{name}: does not have SYSCTL");
    }

    if !chip.peripherals.contains_key("WWDT0") {
        bail!("{name}: does not have WWDT0");
    }

    Ok(())
}

fn pin_names(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for peripheral in chip.peripherals.values() {
        for pin in peripheral.pins.iter() {
            // `+` and `-` are allowed
            //
            // `/` and `.` are not allowed, as these are likely bugs in generation.
            if pin.pin.contains('/') {
                let peripheral_name = &peripheral.name;
                let pin = &pin.pin;

                bail!("{name}, {peripheral_name}: pin {pin} contains a '/'");
            }

            if pin.pin.contains('.') {
                let peripheral_name = &peripheral.name;
                let pin = &pin.pin;

                bail!("{name}, {peripheral_name}: pin {pin} contains invalid characters");
            }
        }
    }

    Ok(())
}

fn gpio_no_duplicates(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for (_, peripheral) in chip
        .peripherals
        .iter()
        .filter(|(name, _)| name.starts_with("GPIO"))
    {
        let mut signals = HashSet::new();

        for pin in peripheral.pins.iter() {
            if !signals.insert(&pin.pin) {
                bail!(
                    "{name}: {} contains multiple pins of {}",
                    peripheral.name,
                    pin.pin
                );
            }
        }
    }

    Ok(())
}

fn verify_aesadv_power_domain(chip: &Chip, name: &str) -> anyhow::Result<()> {
    // L122X puts AESADV in the wrong power domain
    let Some(peripheral) = chip.peripherals.get("AESADV") else {
        return Ok(());
    };

    if peripheral.power_domain != PowerDomain::Pd1 {
        bail!("{name}: AESADV is not in power domain PD1");
    }

    Ok(())
}

/// A few parts have errors in sysconfig metadata where CPUSS is in PD0.
///
/// For all MSPM0 parts CPUSS is in PD1.
fn verify_cpuss_power_domain(chip: &Chip, name: &str) -> anyhow::Result<()> {
    let peripheral = chip.peripherals.get("CPUSS").context("CPUSS not present")?;

    if peripheral.power_domain != PowerDomain::Pd1 {
        bail!("{name}: CPUSS is not in power domain PD1");
    }

    Ok(())
}

fn verify_crc_power_domain(chip: &Chip, name: &str) -> anyhow::Result<()> {
    let Some(peripheral) = chip
        .peripherals
        .get("CRC")
        .or_else(|| chip.peripherals.get("CRCP0"))
    else {
        return Ok(());
    };

    if peripheral.power_domain != PowerDomain::Pd1 {
        bail!("{name}: CRC is not in power domain PD1");
    }

    Ok(())
}

fn verify_gpamp_power_domain(chip: &Chip, name: &str) -> anyhow::Result<()> {
    // Sysconfig states GPAMP has no power domain, but it belongs to PD0.
    let Some(peripheral) = chip.peripherals.get("GPAMP") else {
        return Ok(());
    };

    if peripheral.power_domain != PowerDomain::Pd0 {
        bail!("{name}: GPAMP is not in power domain PD0");
    }

    Ok(())
}

/// SPI peripherals always belong to PD1.
fn verify_spi_power_domain(chip: &Chip, name: &str) -> anyhow::Result<()> {
    // Maintainer note: This could change in the future.
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"SPI(\d+)").unwrap());

    for (peripheral_name, peripheral) in chip
        .peripherals
        .iter()
        .filter(|(name, _)| PATTERN.is_match(name))
    {
        if peripheral.power_domain != PowerDomain::Pd1 {
            bail!("{name}: {peripheral_name} is not in power domain PD1");
        }
    }

    Ok(())
}

/// TRNG peripherals always belong to PD1.
fn verify_trng_power_domain(chip: &Chip, name: &str) -> anyhow::Result<()> {
    let Some(peripheral) = chip.peripherals.get("TRNG") else {
        return Ok(());
    };

    if peripheral.power_domain != PowerDomain::Pd1 {
        bail!("{name}: TRNG is not in power domain PD1");
    }

    Ok(())
}

/// Verify GPAMP does not exist on these chips:
///
/// - C110X
/// - G151X
fn no_gpamp_c110x_l151x(chip: &Chip, name: &str) -> anyhow::Result<()> {
    if chip.peripherals.contains_key("GPAMP")
        && (name == "mspm0c110x" || name == "mspm0c1105_c1106" || name == "mspm0g151x")
    {
        bail!("{name}: should not have GPAMP");
    }

    Ok(())
}
