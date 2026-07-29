use std::{collections::HashSet, sync::LazyLock};

use anyhow::{bail, Context};
use mspm0_data_types::{Chip, PeripheralType, PowerDomain};
use regex::Regex;

pub fn verify(chip: &Chip, name: &str) -> anyhow::Result<()> {
    core_peripherals(chip, name)?;

    pin_names(chip, name)?;

    gpio_no_duplicates(chip, name)?;

    // Peripherals which don't actually exist
    no_gpamp_c110x_l151x(chip, name)?;

    // Low power data which is only as complete as the data sources
    block_async_known(chip, name)?;
    standby1_timer_exists(chip, name)?;
    retention_known(chip, name)?;

    // Power domains
    verify_aesadv_power_domain(chip, name)?;
    verify_cpuss_power_domain(chip, name)?;
    verify_crc_power_domain(chip, name)?;
    verify_gpamp_power_domain(chip, name)?;
    verify_spi_power_domain(chip, name)?;
    // Timers may be in either power domain: add checks if something is wrong
    verify_trng_power_domain(chip, name)?;
    // UART may be in either power domain: add checks if something is wrong

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

/// Report families for which no SVD is published, and whose `BLOCKASYNC` bits are therefore
/// unknown.
///
/// This is not an error: TI publishes SVDs later than the rest of the metadata, so a newly added
/// family legitimately has none. It is reported so the gap is visible and gets closed on the next
/// data source bump rather than silently persisting.
fn block_async_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    if chip
        .peripherals
        .values()
        .all(|peripheral| peripheral.block_async.is_none())
    {
        bail!(
            "{name}: no SVD for family {}, so CLKCFG.BLOCKASYNC is unknown for every peripheral",
            chip.family
        );
    }

    Ok(())
}

/// Report PD1 peripherals which do not say how deep a sleep they are retained through.
///
/// Only PD1 peripherals are forced to a disabled state by SYSCTL, so they are the only ones for
/// which the question arises. Not fatal, because some of these gaps are genuine disagreements
/// between the datasheet and sysconfig rather than something we can fix.
fn retention_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    let unknown = chip
        .peripherals
        .values()
        .filter(|peripheral| peripheral.power_domain == PowerDomain::Pd1)
        .filter(|peripheral| peripheral.retained_through.is_none())
        .map(|peripheral| peripheral.name.as_str())
        .collect::<Vec<_>>();

    if !unknown.is_empty() {
        bail!(
            "{name}: PD1 peripherals missing from data/operating_modes/{}.yaml: {}",
            chip.family,
            unknown.join(", ")
        );
    }

    Ok(())
}

/// Verify that every timer knows whether it is clocked in STANDBY1, and that at least one is.
///
/// A family with no STANDBY1 timer at all would mean nothing can wake the core from the deepest
/// sleep, which is true of no MSPM0 device and means the data is missing.
fn standby1_timer_exists(chip: &Chip, name: &str) -> anyhow::Result<()> {
    let timers = chip
        .peripherals
        .values()
        .filter(|peripheral| peripheral.ty == PeripheralType::Tim)
        .collect::<Vec<_>>();

    if timers.is_empty() {
        return Ok(());
    }

    for timer in timers.iter() {
        if timer.clocked_in_standby1.is_none() {
            bail!(
                "{name}, {}: timer does not say whether it is clocked in STANDBY1",
                timer.name
            );
        }
    }

    if !timers
        .iter()
        .any(|timer| timer.clocked_in_standby1 == Some(true))
    {
        bail!("{name}: no timer is clocked in STANDBY1, so nothing can wake the core from it");
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
