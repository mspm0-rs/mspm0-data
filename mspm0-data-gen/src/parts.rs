use std::fs;

use mspm0_data_types::PowerMode;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PartsFile {
    pub families: Vec<PartFamily>,
}

impl PartsFile {
    pub fn read() -> anyhow::Result<Self> {
        let content = fs::read_to_string("data/parts.yaml")?;
        Ok(serde_yaml::from_str(&content)?)
    }
}

#[derive(Debug, Deserialize)]
pub struct PartFamily {
    /// The family name for the part.
    ///
    /// This is something like `mspm0g110x`
    pub family: String,

    /// The URL to the datasheet
    pub datasheet_url: String,

    /// The URL to the reference manual.
    pub reference_manual_url: String,

    /// The URL to the errata.
    pub errata_url: String,

    /// The number of options for VRSEL of the ADC peripheral.
    pub adc_vrsel: String,

    /// Maximum frequency of MCLK, in Hz.
    pub max_mclk_hz: u32,

    /// Maximum frequency of ULPCLK, in Hz.
    pub max_ulpclk_hz: u32,

    /// Frequency SYSOSC runs at in its factory trimmed base mode, in Hz.
    pub sysosc_base_hz: u32,

    /// MCLK ceiling, in Hz, for each `MCLKCFG.FLASHWAIT` setting, starting at zero wait states.
    pub flash_wait_hz: Vec<u32>,

    /// The range `fADCCLK` must stay within, which applies to every ADC instance of the family.
    pub adc_clock_hz: ClockRange,

    /// The range `TRNGCLKF` must stay within.
    ///
    /// Absent for the families with no TRNG.
    #[serde(default)]
    pub trng_clock_hz: Option<ClockRange>,

    /// The flash erase granularity in bytes, from the datasheet's "minimum erase resolution"
    /// bullet. 1024 on every family so far; stated per device so the question stays closed.
    pub flash_sector_bytes: u32,

    /// The parts of the clock tree with no machine-readable source.
    pub clock_tree: ClockTreeSpec,

    /// Timers which keep receiving ULPCLK or LFCLK in STANDBY1.
    ///
    /// Only the timers named here can wake the core from STANDBY1.
    pub standby1_timers: Vec<String>,

    /// Part numbers in this family.
    pub part_numbers: Vec<PartNumber>,
}

/// The clock tree facts which have to be curated.
///
/// The crystal drivers, the digital clock inputs and the SYSPLL are not here: sysconfig's
/// `clocktree.json` has a node for each of those exactly when the family has one. These two do not
/// follow from it, and cannot be taken from the register YAMLs either, since two families sharing one
/// SYSCTL version can differ.
#[derive(Debug, Deserialize)]
pub struct ClockTreeSpec {
    /// The range HFCLK must stay within, whether it comes from the crystal or the digital input.
    ///
    /// Absent where the datasheet gives no figure, which is the families with no HFCLK path and
    /// MSPM0C1103/C1104, which has an `HFCLKIN` pin but no `fHFIN` row.
    #[serde(default)]
    pub hfclk_hz: Option<ClockRange>,

    /// Whether the device has `MCLKCFG.UDIV`.
    ///
    /// Cross-checked in `verify` against the clock ceilings: a device with a divider is one whose
    /// ULPCLK ceiling is below its MCLK ceiling.
    pub ulpclk_div: bool,

    /// Whether the device has the STOP1 sub-mode, from the presence of a `SYSOSCCFG.FREQ=01`
    /// operating point.
    pub stop1: bool,
}

/// An inclusive frequency range as `parts.yaml` writes it, in Hz.
///
/// Spelled `min`/`max` there rather than `min_hz`/`max_hz`, since the key it sits under already says
/// the unit.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ClockRange {
    pub min: u32,
    pub max: u32,
}

impl From<ClockRange> for mspm0_data_types::ClockRange {
    fn from(range: ClockRange) -> Self {
        Self {
            min_hz: range.min,
            max_hz: range.max,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PartNumber {
    /// The part number.
    pub name: String,

    /// Memory layout.
    pub memory: Vec<PartMemory>,

    /// The packages available for this part number.
    pub packages: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PartMemory {
    /// The memory partition.
    pub name: String,

    /// Amount of memory in KB.
    pub length: u32,

    /// Address of the memory.
    pub address: u32,

    /// The deepest mode through which the contents of this partition survive.
    #[serde(default)]
    pub retained_through: Option<PowerMode>,
}
