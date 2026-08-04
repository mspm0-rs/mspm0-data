use std::{collections::HashSet, sync::LazyLock};

use anyhow::{bail, Context};
use mspm0_data_types::{Chip, PeripheralType, PowerDomain};
use regex::Regex;

/// Run every check, returning one error per failure.
pub fn verify(chip: &Chip, name: &str) -> Vec<anyhow::Error> {
    const CHECKS: &[fn(&Chip, &str) -> anyhow::Result<()>] = &[
        core_peripherals,
        pin_names,
        gpio_no_duplicates,
        // Peripherals which don't actually exist
        no_gpamp_c110x_l151x,
        // Low power data which is only as complete as the data sources
        block_async_known,
        standby1_timer_exists,
        timer_capabilities_known,
        errata_known,
        interrupts_claimed,
        wake_times_ordered,
        // Frequencies transcribed from the datasheets
        clock_frequencies_consistent,
        retention_known,
        wakeup_pins_known,
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
/// Not fatal, because some of these gaps are genuine disagreements between the
/// datasheet and sysconfig rather than something we can fix.
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

/// Report families whose sysconfig does not describe pin wakeup logic, which is not the same as the
/// family having no wake-capable pin.
fn wakeup_pins_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    if chip.wakeup_pins.is_none() {
        bail!(
            "{name}: sysconfig has no io_wakeup attribute for family {}, so wake-capable pins are unknown",
            chip.family
        );
    }

    Ok(())
}

/// Verify the frequencies which have to agree with each other.
///
/// These are transcribed per family from the datasheet, so a typo is the likely failure and it would
/// otherwise be silent.
fn clock_frequencies_consistent(chip: &Chip, name: &str) -> anyhow::Result<()> {
    if chip.max_ulpclk_hz > chip.max_mclk_hz {
        bail!(
            "{name}: max_ulpclk_hz {} is above max_mclk_hz {}",
            chip.max_ulpclk_hz,
            chip.max_mclk_hz
        );
    }

    // The only reason to have a MCLK to ULPCLK divider is a ULPCLK which cannot run as fast as MCLK,
    // so the curated flag and the two ceilings have to say the same thing.
    let ulpclk_slower = chip.max_ulpclk_hz < chip.max_mclk_hz;
    if chip.clock_tree.ulpclk_div != ulpclk_slower {
        bail!(
            "{name}: clock_tree.ulpclk_div is {} but ULPCLK {} runs slower than MCLK {}",
            chip.clock_tree.ulpclk_div,
            if ulpclk_slower { "does" } else { "does not" },
            chip.max_mclk_hz
        );
    }

    // A range for a clock the device cannot source is a transcription slip. The converse is fine:
    // mspm0c110x has an HFCLKIN pin whose frequency its datasheet never specifies.
    if let Some(hfclk) = chip.clock_tree.hfclk_hz {
        if !chip.clock_tree.hfxt && !chip.clock_tree.hfclk_in {
            bail!(
                "{name}: clock_tree.hfclk_hz is {}..{} but the device has neither HFXT nor an HFCLK input",
                hfclk.min_hz,
                hfclk.max_hz
            );
        }

        // HFCLK feeds HSCLK, which feeds MCLK, so a range above the MCLK ceiling is unusable at its
        // top end and most likely a wrong row.
        if hfclk.max_hz > chip.max_mclk_hz {
            bail!(
                "{name}: clock_tree.hfclk_hz reaches {} but max_mclk_hz is {}",
                hfclk.max_hz,
                chip.max_mclk_hz
            );
        }
    }

    // SYSOSC is one of MCLK's sources, so the tree cannot be built if it overshoots the ceiling.
    if chip.sysosc_base_hz > chip.max_mclk_hz {
        bail!(
            "{name}: sysosc_base_hz {} is above max_mclk_hz {}",
            chip.sysosc_base_hz,
            chip.max_mclk_hz
        );
    }

    if chip.flash_wait_hz.is_empty() {
        bail!("{name}: flash_wait_hz has no zero wait state ceiling");
    }

    if chip.flash_wait_hz.windows(2).any(|pair| pair[0] >= pair[1]) {
        bail!(
            "{name}: flash_wait_hz {:?} is not ascending",
            chip.flash_wait_hz
        );
    }

    // The deepest band has to reach the ceiling, or MCLK could be configured at a rate no number of
    // wait states covers.
    let deepest = *chip.flash_wait_hz.last().unwrap();
    if deepest != chip.max_mclk_hz {
        bail!(
            "{name}: deepest flash_wait_hz band is {deepest} but max_mclk_hz is {}",
            chip.max_mclk_hz
        );
    }

    for peripheral in chip.peripherals.values() {
        // The datasheets specify an input clock range for both of these, so a missing one is a
        // missing `adc_clock_hz` or `trng_clock_hz` in parts.yaml rather than a silent peripheral.
        let specified = matches!(peripheral.ty, PeripheralType::Adc | PeripheralType::Trng);

        let Some(range) = peripheral.clock_range_hz else {
            if specified {
                bail!(
                    "{name}, {}: the datasheet specifies an input clock range for this peripheral \
                     but parts.yaml has none for family {}",
                    peripheral.name,
                    chip.family
                );
            }
            continue;
        };

        if range.min_hz > range.max_hz {
            bail!(
                "{name}, {}: clock range {}..{} is inverted",
                peripheral.name,
                range.min_hz,
                range.max_hz
            );
        }
    }

    Ok(())
}

/// Report NVIC interrupts which no peripheral claims.
///
/// `apply_peripheral_interrupts` ties the two together by name, so a naming convention it does not
/// recognise leaves the peripheral with an empty list and nothing to say so. That is what the MSPM33
/// parts would do here: their headers name the HSADC's five lines `ADC0_INT_PUB1` through
/// `ADC0_EVT_INT_PUB1`, none of which equals the peripheral's own name.
fn interrupts_claimed(chip: &Chip, name: &str) -> anyhow::Result<()> {
    /// Interrupts which belong to no peripheral, so nothing can claim them.
    ///
    /// The generic event subscriber ports are part of the event fabric rather than a peripheral, and
    /// sysconfig lists no peripheral for them. Only mspm0h321x has them.
    static NO_PERIPHERAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^GENSUB\d+$").unwrap());

    let claimed = chip
        .peripherals
        .values()
        .flat_map(|peripheral| peripheral.interrupts.iter())
        .map(|interrupt| interrupt.num)
        .collect::<HashSet<_>>();

    let unclaimed = chip
        .interrupts
        .values()
        // Interrupts handled by cortex-m rather than by a peripheral of ours.
        .filter(|interrupt| interrupt.num >= 0)
        .filter(|interrupt| !claimed.contains(&interrupt.num))
        .filter(|interrupt| !NO_PERIPHERAL.is_match(&interrupt.name))
        .map(|interrupt| format!("{} ({})", interrupt.name, interrupt.num))
        .collect::<Vec<_>>();

    if !unclaimed.is_empty() {
        bail!(
            "{name}: no peripheral claims these interrupts, so the names did not match: {}",
            unclaimed.join(", ")
        );
    }

    Ok(())
}

/// Report chips with no errata data, and check the identifiers are well formed and sorted.
///
/// Empty is a claim that no functional advisory applies, which no MSPM0 device makes, so it means the
/// sheet has not been read rather than that the device is clean. Sorted is what lets a consumer look
/// an erratum up by binary search.
fn errata_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z0-9]+_ERR_\d+$").unwrap());

    if chip.errata.is_empty() {
        bail!(
            "{name}: data/errata/{}.yaml is missing or empty",
            chip.family
        );
    }

    for erratum in chip.errata.iter() {
        if !PATTERN.is_match(erratum) {
            bail!("{name}: `{erratum}` is not an errata identifier");
        }
    }

    if !chip.errata.is_sorted() {
        bail!("{name}: errata are not sorted");
    }

    Ok(())
}

/// Verify the wake-up times, which are transcribed per family from the datasheet.
///
/// A deeper mode costs more to leave than a shallower one, so an inversion is a misread row rather
/// than a real measurement. Only the pairs the datasheets actually state are compared: a mode with no
/// figure is skipped rather than assumed.
fn wake_times_ordered(chip: &Chip, name: &str) -> anyhow::Result<()> {
    let wake = &chip.wake_ns;

    if wake == &Default::default() {
        bail!(
            "{name}: data/wakeup/{}.yaml is missing or empty",
            chip.family
        );
    }

    // Shallowest to deepest, with two deliberate omissions.
    //
    // STOP2 is not in the chain: it disables SYSOSC where STOP1 keeps it at 4MHz, and leaving it can
    // be *cheaper* than leaving STOP1 rather than dearer. mspm0l130x is 13.0us from STOP2 against
    // 14.0us from STOP1, and mspm0g350x 12.9us against 13.5us, so an ordering check would misfire.
    //
    // SHUTDOWN is not either: it is a boot rather than a wake, which is why its figure is an order of
    // magnitude larger. It is compared against STANDBY0 separately below.
    let ordered = [
        ("sleep0", wake.sleep0),
        ("sleep1", wake.sleep1),
        ("sleep2", wake.sleep2),
        ("stop0", wake.stop0),
        ("stop1", wake.stop1),
        ("standby0", wake.standby0),
        ("standby1", wake.standby1),
    ];

    let stated = ordered
        .iter()
        .filter_map(|(mode, ns)| ns.map(|ns| (mode, ns)))
        .collect::<Vec<_>>();

    for pair in stated.windows(2) {
        let ((shallow, shallow_ns), (deep, deep_ns)) = (pair[0], pair[1]);
        if shallow_ns > deep_ns {
            bail!(
                "{name}: waking from {shallow} costs {shallow_ns}ns but from the deeper {deep} only \
                 {deep_ns}ns"
            );
        }
    }

    if let (Some(standby), Some(shutdown)) = (wake.standby0, wake.shutdown) {
        if shutdown < standby {
            bail!("{name}: booting from SHUTDOWN ({shutdown}ns) is faster than waking from STANDBY0 ({standby}ns)");
        }
    }

    Ok(())
}

/// Report timers whose capabilities are unknown, which is a missing row in `data/timers`.
fn timer_capabilities_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    let unknown = chip
        .peripherals
        .values()
        .filter(|peripheral| peripheral.ty == PeripheralType::Tim)
        .filter(|peripheral| peripheral.timer.is_none())
        .map(|peripheral| peripheral.name.as_str())
        .collect::<Vec<_>>();

    if !unknown.is_empty() {
        bail!(
            "{name}: timers missing from data/timers/{}.yaml: {}",
            chip.family,
            unknown.join(", ")
        );
    }

    Ok(())
}

/// Verify that every timer knows whether it is clocked in STANDBY1, and that at least one is.
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
