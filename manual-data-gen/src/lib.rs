use std::{collections::BTreeMap, str::FromStr};

use serde::{Deserialize, Serialize};

/// Manually entered data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualFile {
    /// Documentation for the chip.
    pub docs: BTreeMap<DocType, String>,
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
