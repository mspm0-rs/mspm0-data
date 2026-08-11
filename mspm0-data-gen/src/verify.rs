use std::{collections::HashSet, sync::LazyLock};

use anyhow::bail;
use mspm0_data_types::{AdcInternalSource, Chip, IoStructure, PeripheralType, PowerDomain};
use regex::Regex;

/// Run every check, returning one error per failure.
pub fn verify(chip: &Chip, name: &str) -> Vec<anyhow::Error> {
    const CHECKS: &[fn(&Chip, &str) -> anyhow::Result<()>] = &[
        core_peripherals,
        pin_names,
        gpio_no_duplicates,
        peripheral_types_known,
        register_blocks_exist,
        vref_startup_known,
        adc_channels_known,
        adc_internal_sources_exist,
        uart_features_known,
        opa_inputs_known,
        opa_input_sources_exist,
        comp_features_known,
        flashctl_known,
        dma_widths_known,
        // Peripherals which don't actually exist
        no_gpamp_c110x_l151x,
        // Low power data which is only as complete as the data sources
        block_async_known,
        standby1_timer_exists,
        timer_capabilities_known,
        timer_counters_addressable,
        errata_known,
        interrupts_claimed,
        wake_times_ordered,
        // Frequencies transcribed from the datasheets
        clock_frequencies_consistent,
        retention_known,
        wakeup_pins_known,
        io_structures_known,
        temperature_sensor_known,
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

/// Report a VREF instance with no startup time.
///
/// A device carrying `VREF_ERR_01` cannot trust `CTL1.READY` after the first enable since reset, so
/// without this figure a consumer has no way to know the reference has settled.
fn vref_startup_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for peripheral in chip
        .peripherals
        .values()
        .filter(|peripheral| peripheral.ty == PeripheralType::Vref)
    {
        if peripheral.vref.and_then(|vref| vref.startup_ns).is_none() {
            bail!(
                "{name}: {} has no startup time; data/vref/{}.yaml is missing or has no Tstartup row",
                peripheral.name,
                chip.family
            );
        }
    }

    Ok(())
}

/// Report an ADC instance with no internal-channel data.
///
/// Every datasheet so far has the channel-mapping table, so an empty map means
/// `data/adc_channels/<family>.yaml` is missing or the extraction lost the instance, not a device
/// without internal channels.
fn adc_channels_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for peripheral in chip
        .peripherals
        .values()
        .filter(|peripheral| peripheral.ty == PeripheralType::Adc)
    {
        let empty = peripheral
            .adc
            .as_ref()
            .is_none_or(|adc| adc.internal_channels.is_empty());
        if empty {
            bail!(
                "{name}: {} has no internal channels; data/adc_channels/{}.yaml is missing or lost \
                 the instance",
                peripheral.name,
                chip.family
            );
        }
    }

    Ok(())
}

/// An internal channel naming a peripheral instance requires that instance to exist.
///
/// This is the check that separates the datasheets from the SDK's family-superset tables, which
/// route the OPA outputs on families that have no OPA.
fn adc_internal_sources_exist(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for peripheral in chip
        .peripherals
        .values()
        .filter(|peripheral| peripheral.ty == PeripheralType::Adc)
    {
        let Some(adc) = &peripheral.adc else {
            continue;
        };

        for (channel, source) in &adc.internal_channels {
            let instance = match source {
                AdcInternalSource::Opa0 => "OPA0",
                AdcInternalSource::Opa1 => "OPA1",
                AdcInternalSource::Gpamp => "GPAMP",
                AdcInternalSource::Dac0 => "DAC0",
                AdcInternalSource::Vref => "VREF",
                AdcInternalSource::TemperatureSensor
                | AdcInternalSource::SupplyMonitor
                | AdcInternalSource::VbatMonitor
                | AdcInternalSource::VusbMonitor => continue,
            };

            if !chip.peripherals.contains_key(instance) {
                bail!(
                    "{name}: {} channel {channel} samples {instance}, which the chip does not have",
                    peripheral.name
                );
            }
        }
    }

    Ok(())
}

/// Every OPA instance should say what its input muxes select.
///
/// All four OPA-bearing families have the data — from the G datasheets' mapping tables or the L
/// datasheets' analog-connections figure — so an instance without it means `data/opa/<family>.yaml`
/// is missing or lost the instance.
fn opa_inputs_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for peripheral in chip
        .peripherals
        .values()
        .filter(|peripheral| peripheral.ty == PeripheralType::Opa)
    {
        if peripheral.opa.is_none() {
            bail!(
                "{name}: {} has no input-mux data; data/opa/{}.yaml is missing or lost the \
                 instance",
                peripheral.name,
                chip.family
            );
        }
    }

    Ok(())
}

/// An input-mux position naming another instance requires that instance to exist.
///
/// The same standing as `adc_internal_sources_exist`: a curated map naming a peer OPA, a COMP's
/// DAC, the 12-bit DAC, the GPAMP or the VREF is only right if the chip has it.
fn opa_input_sources_exist(chip: &Chip, name: &str) -> anyhow::Result<()> {
    use mspm0_data_types::OpaInput;

    for peripheral in chip
        .peripherals
        .values()
        .filter(|peripheral| peripheral.ty == PeripheralType::Opa)
    {
        let Some(opa) = &peripheral.opa else {
            continue;
        };

        for (mux, map) in [
            ("PSEL", &opa.pmux),
            ("NSEL", &opa.nmux),
            ("MSEL", &opa.mmux),
        ] {
            for (position, input) in map {
                let instance = match input {
                    OpaInput::Dac8(n) => format!("COMP{n}"),
                    OpaInput::Rtop(n) | OpaInput::Rbot(n) => format!("OPA{n}"),
                    OpaInput::Dac12 => "DAC0".to_string(),
                    OpaInput::Gpamp => "GPAMP".to_string(),
                    OpaInput::VrefPlus => "VREF".to_string(),
                    OpaInput::In(_) | OpaInput::OwnRtap | OpaInput::OwnRtop | OpaInput::Ground => {
                        continue
                    }
                };

                if !chip.peripherals.contains_key(&instance) {
                    bail!(
                        "{name}: {} {mux} position {position} selects {instance}, which the chip \
                         does not have",
                        peripheral.name
                    );
                }
            }
        }
    }

    Ok(())
}

/// Every UART instance should say which extended features it has.
///
/// Every datasheet so far has the "UART Features" table, so an instance without the data means
/// `data/uart/<family>.yaml` is missing or lost the instance.
fn uart_features_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for peripheral in chip.peripherals.values().filter(|peripheral| {
        matches!(
            peripheral.ty,
            PeripheralType::Uart | PeripheralType::UnicommUart
        )
    }) {
        if peripheral.uart.is_none() {
            bail!(
                "{name}: {} has no extended-feature data; data/uart/{}.yaml is missing or lost \
                 the instance",
                peripheral.name,
                chip.family
            );
        }
    }

    Ok(())
}

/// Every comparator states whether it has the `CTL2.REFSRC` internal-reference positions, and
/// carries the family's timing figures.
///
/// The first comes from sysconfig's `SYS_COMP_INT_VREF`, which every family's metadata carries per
/// instance, so an instance without it means the attribute moved rather than that the answer is
/// unknown. The timing comes from `data/comp/<family>.yaml`, and every COMP-bearing datasheet so
/// far has the `ten` and `tdac_settle` rows, so a missing figure means the extraction broke.
/// `dac_settle_pin_ns` is not checked: only the datasheets whose COMP drives a pin state it.
fn comp_features_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for peripheral in chip
        .peripherals
        .values()
        .filter(|peripheral| peripheral.ty == PeripheralType::Comp)
    {
        let Some(comp) = &peripheral.comp else {
            bail!(
                "{name}: {} has no comparator data; SYS_COMP_INT_VREF was not found on the \
                 sysconfig instance",
                peripheral.name,
            );
        };

        if comp.enable_fast_ns.is_none()
            || comp.enable_ulp_ns.is_none()
            || comp.dac_settle_ns.is_none()
        {
            bail!(
                "{name}: {} has no timing figures; data/comp/{}.yaml is missing or incomplete",
                peripheral.name,
                chip.family,
            );
        }
    }

    Ok(())
}

/// The flash controller states its geometry, and the figures are ones the sources can produce.
///
/// No check ties the `CMDWEPROT` widths to the flash size: the bit-to-sector mapping depends on
/// the bank count, which is a runtime fact (`FACTORYREGION`), and the obvious single-bank formula
/// is genuinely wrong for the dual-bank L122x/L222x.
fn flashctl_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for peripheral in chip
        .peripherals
        .values()
        .filter(|peripheral| peripheral.ty == PeripheralType::FlashCtl)
    {
        let Some(flashctl) = &peripheral.flashctl else {
            bail!("{name}: {} has no flash geometry data", peripheral.name);
        };

        if !matches!(flashctl.word_bytes, 8 | 16) {
            bail!(
                "{name}: {} states a {}-byte flash word; the portfolio has only 64- and 128-bit \
                 words, so the header read likely broke",
                peripheral.name,
                flashctl.word_bytes,
            );
        }
    }

    Ok(())
}

/// The temperature sensor's conversion constants are present and the slope has the right sign.
///
/// Every datasheet so far states all of them, so an absent set means the extraction lost a family
/// rather than a device without a sensor. The sign check is the cheap guard against a MIN/TYP/MAX
/// column misread: the sensor's output falls as the die warms on every device.
fn temperature_sensor_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    let Some(sensor) = chip.temperature_sensor else {
        bail!(
            "{name}: no temperature sensor constants; data/temp_sensor/{}.yaml is missing",
            chip.family
        );
    };

    if sensor.tsc_uv_per_c >= 0 {
        bail!(
            "{name}: the temperature coefficient is {}uV/C, but the sensor's output falls as the \
             die warms on every device",
            sensor.tsc_uv_per_c,
        );
    }

    Ok(())
}

/// Every pin has an IO structure, and no pin wakes the device without the logic to do it.
///
/// The second half is a subset check, not an equality: a structure which can carry wakeup logic
/// does not always have it — the mspm0c110x's open-drain pins do not.
fn io_structures_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for pin in chip.iomux.keys() {
        if !chip.io_structure.contains_key(pin) {
            bail!("{name}: {pin} has a PINCM but no IO structure");
        }
    }

    let Some(wakeup_pins) = &chip.wakeup_pins else {
        return Ok(());
    };

    for pin in wakeup_pins {
        let structure = chip.io_structure.get(pin);
        if !matches!(
            structure,
            Some(IoStructure::StandardWithWake | IoStructure::HighDrive | IoStructure::OpenDrain)
        ) {
            bail!(
                "{name}: {pin} wakes the device from SHUTDOWN but is {structure:?}, which has no \
                 wakeup logic"
            );
        }
    }

    Ok(())
}

/// The DMA says which transfer widths it implements.
///
/// Every chip has a DMA and the header states the fact for every device, so an instance without it
/// means the peripheral stopped being typed `Dma` rather than that a source is missing data.
fn dma_widths_known(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for peripheral in chip
        .peripherals
        .values()
        .filter(|peripheral| peripheral.ty == PeripheralType::Dma)
    {
        if peripheral.dma.is_none() {
            bail!("{name}: {} has no transfer width data", peripheral.name);
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

/// Verify that every counter an instance claims is one its register block can reach.
///
/// `tim_v1` has a single counter and `tim_btimer` an array of the eight the TRM documents, so a
/// count outside those is either a sysconfig attribute nobody expected or a timer mapped to the
/// wrong block.
fn timer_counters_addressable(chip: &Chip, name: &str) -> anyhow::Result<()> {
    for peripheral in chip.peripherals.values() {
        let Some(timer) = peripheral.timer else {
            continue;
        };

        let addressable = match peripheral.version.as_deref() {
            Some("btimer") => 8,
            _ => 1,
        };

        if timer.counters == 0 || timer.counters > addressable {
            bail!(
                "{name}: {} has {} counters, but {} addresses {addressable}",
                peripheral.name,
                timer.counters,
                peripheral
                    .version
                    .as_deref()
                    .unwrap_or("no register block for it")
            );
        }
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
