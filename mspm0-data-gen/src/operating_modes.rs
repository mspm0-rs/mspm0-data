//! How deep a sleep each PD1 peripheral keeps its configuration through.
//!
//! SYSCTL forces every PD1 peripheral to a disabled state upon entry into STOP or STANDBY, but only
//! some of the peripherals retain their configuration while disabled.
//! This information isn't available in any machine-readable format, so it's extracted from the
//! datasheet's "Supported Functionality by Operating Mode" table using `tools/operating_modes.py`.

use std::{collections::BTreeMap, fs};

use anyhow::Context;
use mspm0_data_types::PowerMode;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct OperatingModes {
    /// Maps peripheral names to the deepest mode its configuration survives.
    pub retained_through: BTreeMap<String, PowerMode>,

    /// Peripheral instance name to the deepest mode it can be used in.
    #[serde(default)]
    pub usable_through: BTreeMap<String, PowerMode>,
}

impl OperatingModes {
    pub fn parse() -> anyhow::Result<BTreeMap<String, OperatingModes>> {
        let mut map = BTreeMap::new();

        for f in glob::glob("data/operating_modes/*.yaml")? {
            let f = f?;
            let content = fs::read_to_string(&f)?;
            let modes = serde_yaml::from_str::<OperatingModes>(&content)
                .context(format!("Error reading {}", f.display()))?;

            map.insert(
                f.file_stem()
                    .context("File has no stem")?
                    .to_string_lossy()
                    .to_string(),
                modes,
            );
        }

        Ok(map)
    }
}
