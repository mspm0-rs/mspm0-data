use std::{borrow::Cow, collections::BTreeMap, sync::LazyLock, thread::panicking};

use anyhow::{Context, bail};
use data_gen::{Chip, Peripheral, PeripheralInterrupt};
use regex::{Match, Regex};
use serde_json::{Number, Value};

use crate::{
    serde_helper::{map_get_object, map_get_string},
    sysconfig::get_peripheral_attributes,
    util::RegexMap,
};

pub static CHIP_TO_HEADER_AND_FAMILY: RegexMap<&str> = RegexMap::new(&[
    // MSPM0C
    ("mspm0c110(3|4).*|msps003.*", "mspm0c110x"),
    ("mspm0c110(5|6).*|msp32(c|g)031.*", "mspm0c1105_c1106"),
    // MSPM0G
    ("mspm0g110.*", "mspm0g110x"),
    ("mspm0g120.*", "mspm0g120x"),
    ("mspm0g121.*", "mspm0g121x"),
    ("mspm0g150.*", "mspm0g150x"),
    ("mspm0g151.*", "mspm0g151x"),
    ("mspm0g310.*", "mspm0g310x"),
    ("mspm0g320.*", "mspm0g320x"),
    ("mspm0g321.*", "mspm0g321x"),
    ("mspm0g350.*", "mspm0g350x"),
    ("mspm0g351.*", "mspm0g351x"),
    ("mspm0g511.*", "mspm0g511x"),
    ("mspm0g518.*", "mspm0g518x"),
    // MSPM0H
    ("mspm0h321.*", "mspm0h321x"),
    // MSPM0L
    ("mspm0l110.*", "mspm0l110x"),
    ("mspm0l111.*", "mspm0l111x"),
    ("mspm0l112.*", "mspm0l112x"),
    ("mspm0l122.*", "mspm0l122x"),
    ("mspm0l130.*", "mspm0l130x"),
    ("mspm0l134.*", "mspm0l134x"),
    ("mspm0l211.*", "mspm0l211x"),
    ("mspm0l222.*", "mspm0l222x"),
    // MSP33C
    ("mspm33c321.*", "mspm33c321x"),
]);

#[derive(Debug)]
pub struct DmaTrigger {
    /// The name of this DMA trigger.
    ///
    /// This is going to be a combination of the peripheral name and the signal.
    pub name: String,

    /// The DMA instance this trigger is available for.
    pub instance: String,

    /// The trigger value.
    pub trigger: u32,
}

pub struct CpuFeatures {
    pub mpu_present: bool,
    pub fpu_present: bool,
    pub vtor_present: bool,
    pub nvic_prio_bits: u16,
}

pub fn get_peripheral_addresses(header_content: &str) -> anyhow::Result<BTreeMap<String, u32>> {
    /// Example:
    /// ```c,no_run
    /// #define DEBUGSS_BASE                   (0x400C7000U)
    /// ```
    ///
    /// peripheral = `DEBUGSS`, address = `400C7000`
    static PERIPHERAL_BASE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?m)#define\s+(?<peripheral>\w+)_BASE\s+\(0x(?<address>\w+)U\)").unwrap()
    });

    let mut peripherals = BTreeMap::new();

    for capture in PERIPHERAL_BASE.captures_iter(header_content) {
        let peripheral = capture
            .name("peripheral")
            .context("capture group failed to resolve peripheral name for peripheral address")?;

        let address = capture.name("address").context(format!(
            "could not resolve address for {}",
            peripheral.as_str()
        ))?;

        let address = u32::from_str_radix(address.as_str(), 16).context(format!(
            "address for {} is not valid u32",
            peripheral.as_str()
        ))?;

        peripherals.insert(peripheral.as_str().to_string(), address);
    }

    assert!(
        !peripherals.is_empty(),
        "no matches in header for peripherals and addresses"
    );

    Ok(peripherals)
}

pub fn get_dma_triggers(part: &str, header_content: &str) -> anyhow::Result<Vec<DmaTrigger>> {
    static DMA_TRIGGER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"#define\s+DMA(?<instance>\d+)?_(?<name>\w+?)_TRIG(?<trig_instance>\d+)?\s+\((?<trigger>\d+)\)").unwrap()
    });

    let mut triggers = Vec::new();

    for capture in DMA_TRIGGER.captures_iter(header_content) {
        let trigger_name = &capture["name"];
        let trigger = str::parse::<u32>(&capture["trigger"])
            .context("DMA trigger value is not an integer")?;
        // Instance only applies on M33, trig_instance qualifies which SEQ for HSADC.
        let instance_num = capture
            .name("instance")
            .as_ref()
            .map(Match::as_str)
            .unwrap_or_default()
            .to_string();
        let trig_instance = capture.name("trig_instance");

        let name = match trig_instance {
            // MSPM33 HSADC SEQ triggers need to be renamed.
            Some(num) if trigger_name.contains("ADC") && part.starts_with("MSPM33") => {
                Cow::Owned(format!("{trigger_name}_SEQ{num}", num = num.as_str()))
            }
            Some(_) => bail!("Unhandled case"),
            None => Cow::Borrowed(trigger_name),
        };

        triggers.push(DmaTrigger {
            name: name.into_owned(),
            instance: format!("DMA{instance_num}"),
            trigger,
        });
    }

    assert!(
        !triggers.is_empty(),
        "no matches in header for DMA triggers"
    );
    Ok(triggers)
}

pub fn get_interrupts(
    part: &str,
    header_content: &str,
) -> anyhow::Result<BTreeMap<i32, Vec<String>>> {
    /// Example:
    /// ```c,no_run
    /// GPIOB_INT_IRQn              = 1,
    /// ```
    ///
    /// name = `GPIOB`, number = `1`
    static IRQ_N: LazyLock<Regex> = LazyLock::new(|| {
        // Lazy regex (**U**ngreedy) is needed to avoid having `_INT` become part of the
        // <name> capture group if present.
        Regex::new(r"(?mU)^\s+(?<name>\w+)(?:_INT)?_IRQn\s+=\s+(?<number>-?\w+),").unwrap()
    });

    let mut irqs = BTreeMap::<i32, Vec<String>>::new();

    for capture in IRQ_N.captures_iter(header_content) {
        let name = capture
            .name("name")
            .context(format!("{part}: capture group failed to resolve irq name"))?;

        let number = capture.name("number").context(format!(
            "{part}: could not resolve irq number for {}",
            name.as_str()
        ))?;

        let number = number.as_str().parse::<i32>().context(format!(
            "{part}: irq number for {} is not valid i32",
            name.as_str()
        ))?;

        irqs.entry(number)
            .or_default()
            .push(name.as_str().to_string());
    }

    assert!(
        !irqs.is_empty(),
        "{part}: no matches in header for irq numbers"
    );

    Ok(irqs)
}

fn is_lx22x_lfss(part: &str, index: i32) -> bool {
    (part.starts_with("MSPM0L122") || part.starts_with("MSPM0L222")) && index == 30
}

pub fn set_core_interrupts(
    part: &str,
    interrupts: &mut BTreeMap<i16, String>,
    raw_interrupts: &BTreeMap<i32, Vec<String>>,
) -> anyhow::Result<()> {
    for (index, signals) in raw_interrupts.iter() {
        if !signals.is_empty() {
            let m0_lfss = is_lx22x_lfss(part, *index);

            let name = if signals.len() > 1 && !m0_lfss {
                format!("GROUP{index}")
            } else {
                signals
                    .first()
                    .with_context(|| format!("{part}: Interrupt has no entries"))?
                    .clone()
            };

            interrupts.insert(*index as i16, name);
        }
    }

    Ok(())
}

pub fn set_peripheral_interrupts(
    part: &str,
    peripherals: &mut BTreeMap<String, Peripheral>,
    raw_interrupts: &BTreeMap<i32, Vec<String>>,
) -> anyhow::Result<()> {
    for (peripheral_name, peripheral) in peripherals.iter_mut() {
        // Find interrupts for this peripheral.
        for (index, raw_interrupts) in raw_interrupts.iter() {
            for raw_interrupt in raw_interrupts {
                let generate = raw_interrupt == peripheral_name
                    // MSP33 ADC has multiple physical interrupts.
                    || (part.starts_with("MSPM33") && peripheral_name.starts_with("ADC") && raw_interrupt.starts_with("ADC"));

                if generate {
                    if !raw_interrupt.is_empty() {
                        let m0_lfss = is_lx22x_lfss(part, *index);

                        // If multiple raw interrupts exist then the physical interrupt is a group and the signal is the
                        // IIDX of the interrupt group.
                        let interrupt = if raw_interrupts.len() > 1 && !m0_lfss {
                            format!("GROUP{index}")
                        } else {
                            raw_interrupts
                                .first()
                                .with_context(|| format!("{part}: Interrupt has no entries"))?
                                .clone()
                        };

                        peripheral.interrupts.push(PeripheralInterrupt {
                            interrupt,
                            signal: raw_interrupt.clone(),
                        });
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn get_peripheral_extras(
    part: &str,
    peripherals: &mut BTreeMap<String, Peripheral>,
    sysconfig: &Value,
) -> anyhow::Result<()> {
    for (peripheral_name, peripheral) in peripherals.iter_mut() {
        insert_power_domain(part, peripheral, sysconfig)
            .with_context(|| format!("{part}: power domain for {name}", name = peripheral.name))?;

        if peripheral_name.starts_with("ADC") {
            insert_adc_temp_channel(peripheral, sysconfig)?;
            update_adc_supply_channels(part, peripheral)?;
        }
    }

    Ok(())
}

fn insert_adc_temp_channel(peripheral: &mut Peripheral, sysconfig: &Value) -> anyhow::Result<()> {
    let attributes = get_peripheral_attributes(sysconfig, &peripheral.name)?;

    // This does not exist on MSPM33C321X even though a temperature channel does exist.
    if let Some(sense_channel) = attributes.get("SYS_TEMP_SENSE_CHANNEL") {
        let sense = sense_channel.as_str().context("sense channel is not str")?;
        let sense = sense.parse::<u8>()?;

        peripheral.extra.insert(
            "msp_temp_channel".into(),
            Value::Number(Number::from(sense)),
        );
    }

    Ok(())
}

/// These are not available in any metadata sources.
fn update_adc_supply_channels(part: &str, peripheral: &mut Peripheral) -> anyhow::Result<()> {
    if part.starts_with("MSPS003")
        || part.starts_with("MSPM0C1103")
        || part.starts_with("MSPM0C1104")
        || part.starts_with("MSPM0G110")
        || part.starts_with("MSPM0G150")
        || part.starts_with("MSPM0G310")
        || part.starts_with("MSPM0G350")
        || part.starts_with("MSPM0L130")
        || part.starts_with("MSPM0L134")
    {
        peripheral
            .extra
            .insert("msp_supply_channel".into(), Value::Number(Number::from(15)));
    } else if part.starts_with("MSPM0L122") || part.starts_with("MSPM0L222") {
        peripheral
            .extra
            .insert("msp_vref_channel".into(), Value::Number(Number::from(28)));
        peripheral
            .extra
            .insert("msp_vbat_channel".into(), Value::Number(Number::from(30)));
        peripheral
            .extra
            .insert("msp_supply_channel".into(), Value::Number(Number::from(31)));
    } else if part.starts_with("MSPM0C1105")
        || part.starts_with("MSPM0C1106")
        || part.starts_with("MSP32C031")
        || part.starts_with("MSP32G031")
    {
        peripheral
            .extra
            .insert("msp_vref_channel".into(), Value::Number(Number::from(29)));
        peripheral
            .extra
            .insert("msp_supply_channel".into(), Value::Number(Number::from(31)));
    } else if part.starts_with("MSPM0L112") || part.starts_with("MSPM0L211") {
        peripheral.extra.insert(
            "msp_vrefint_channel".into(),
            Value::Number(Number::from(29)),
        );
        peripheral
            .extra
            .insert("msp_supply_channel".into(), Value::Number(Number::from(31)));
    } else if part.starts_with("MSPM0G120")
        || part.starts_with("MSPM0G121")
        || part.starts_with("MSPM0G320")
        || part.starts_with("MSPM0G321")
    {
        peripheral
            .extra
            .insert("msp_supply_channel".into(), Value::Number(Number::from(31)));

        if peripheral.name == "ADC0" {
            peripheral
                .extra
                .insert("msp_vref1_channel".into(), Value::Number(Number::from(30)));
        } else if peripheral.name == "ADC1" {
            peripheral
                .extra
                .insert("msp_vref2_channel".into(), Value::Number(Number::from(30)));
        }
    } else if part.starts_with("MSPM0G511") || part.starts_with("MSPM0G518") {
        peripheral.extra.insert(
            "msp_vrefint_channel".into(),
            Value::Number(Number::from(28)),
        );
        peripheral
            .extra
            .insert("msp_vusb_channel".into(), Value::Number(Number::from(30)));
        peripheral
            .extra
            .insert("msp_supply_channel".into(), Value::Number(Number::from(31)));
    } else if part.starts_with("MSPM0H321") {
        peripheral
            .extra
            .insert("msp_vref_channel".into(), Value::Number(Number::from(29)));
        peripheral
            .extra
            .insert("msp_supply_channel".into(), Value::Number(Number::from(31)));
    } else if part.starts_with("MSPM33C321") {
        peripheral
            .extra
            .insert("msp_vbat_channel".into(), Value::Number(Number::from(30)));
        peripheral
            .extra
            .insert("msp_supply_channel".into(), Value::Number(Number::from(31)));

        // TEMP is not specified in metadata for M33C321
        if peripheral.name.contains("ADC0") {
            peripheral
                .extra
                .insert("msp_temp_channel".into(), Value::Number(Number::from(11)));
        }
    }

    Ok(())
}

fn insert_power_domain(
    part: &str,
    peripheral: &mut Peripheral,
    sysconfig: &Value,
) -> anyhow::Result<()> {
    // GPIO is always PD0
    if peripheral.name.starts_with("GPIO") {
        peripheral
            .extra
            .insert("msp_power_domain".into(), "PD0".into());
        return Ok(());
    }

    // Some parts are missing DMA in metadata.
    if peripheral.name == "DMA" {
        peripheral
            .extra
            .insert("msp_power_domain".into(), "PD0".into());
        return Ok(());
    }

    let sys_peripheral = get_peripheral_attributes(sysconfig, &peripheral.name)?;
    // POWER_DOMAIN or power_domain depending on chip.
    let power_domain = match map_get_string(sys_peripheral, "POWER_DOMAIN")
        .or_else(|_| map_get_string(sys_peripheral, "power_domain"))
    {
        Ok(p) => p,
        // GPAMP is ULP_AON (PD0)
        Err(_) if peripheral.name == "GPAMP" => String::from("PD_ULP_AON"),
        Err(err) => return Err(err),
    };

    let domain = match &power_domain[..] {
        "PD_ULP_AON" | "PD0_ULP_AON_MCLKBY4" => "PD0",
        "PD_ULP_AAON" | "PD1_ULP_AAON_MCLK" | "PD1_ULP_AAON_MCLKBY2" | "PD1_ULP_AON_MCLKBY4" => {
            "PD1"
        }
        // Space at front is bug in MSPM33C321x sysconfig data
        "PD_VRTC_AON" | " PD_VRTC_AON" => "VBAT",
        _ => bail!(
            "({part}) {name}: unknown power domain? {power_domain}",
            name = peripheral.name
        ),
    };

    // Now pick the correct power domain. There are a LOT of wrong power domains.

    peripheral
        .extra
        .insert("msp_power_domain".into(), domain.into());

    Ok(())
}
