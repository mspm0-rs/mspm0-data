use std::fs;

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

    /// Part numbers in this family.
    pub part_numbers: Vec<PartNumber>,
}

#[derive(Debug, Deserialize)]
pub struct PartNumber {
    /// The part number.
    pub name: String,

    /// Amount of flash in KB.
    pub flash: u32,

    /// Amount of ram in KB.
    pub ram: u32,

    /// The packages available for this part number.
    pub packages: Vec<String>,
}
