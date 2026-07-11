use std::{borrow::Cow, collections::BTreeMap, sync::LazyLock};

use anyhow::{Context, bail};
use regex::{Match, Regex};

use crate::util::{self, RegexMap};

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
