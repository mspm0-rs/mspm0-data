use std::{collections::BTreeMap, str::FromStr};

use serde::{Deserialize, Serialize};

/// Manually entered data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualFile {
    /// Documentation for the chip.
    pub docs: BTreeMap<DocType, String>,

    /// MSPM0/33 specific data.
    pub msp: Option<MspData>,
}

/// The type of documentation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocType {
    /// The datasheet.
    Datasheet,

    /// The reference manual.
    ReferenceManual,

    /// The errata.
    Errata,

    /// A functional safety manual.
    ///
    /// Not every device will have this documentation.
    SafetyManual,
}

impl FromStr for DocType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "datasheet" => Ok(Self::Datasheet),
            "reference_manual" => Ok(Self::ReferenceManual),
            "errata" => Ok(Self::Errata),
            "safety_manual" => Ok(Self::SafetyManual),
            _ => Err("unknown doc type"),
        }
    }
}

/// MSPM0/33 specific data
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MspData {
    pub adc: MspAdcData,

    /// ROM functions available on this chip and their address.
    #[serde(default)]
    pub rom_functions: BTreeMap<String, u32>,
}

/// MSPM0/33 specific ADC data
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MspAdcData {
    pub vrsel: AdcVrselOption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdcVrselOption {
    VddIntref,
    VddIntrefExtref,
}

impl FromStr for AdcVrselOption {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "vdd_intref" => Ok(Self::VddIntref),
            "vdd_intref_extref" => Ok(Self::VddIntrefExtref),
            _ => Err("unknown adc vrsel option"),
        }
    }
}
