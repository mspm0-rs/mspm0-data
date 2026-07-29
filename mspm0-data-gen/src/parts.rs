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

    /// Timers which keep receiving ULPCLK or LFCLK in STANDBY1.
    ///
    /// Only the timers named here can wake the core from STANDBY1.
    pub standby1_timers: Vec<String>,

    /// Part numbers in this family.
    pub part_numbers: Vec<PartNumber>,
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
