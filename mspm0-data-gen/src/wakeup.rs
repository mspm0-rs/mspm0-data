//! Reads `data/wakeup/<family>.yaml`, which `tools/wakeup.py` extracts from the datasheets.
//!
//! How long a device takes to reach RUN from each sleep mode has no machine-readable source, and it is
//! what decides whether a sleep is worth entering before the next deadline.
//!
//! Unlike the other `data/` readers there is no intermediate type: the file's keys are exactly
//! [`WakeTimes`]' fields, so it deserializes straight into the shape the JSON carries.

use std::{collections::BTreeMap, fs};

use anyhow::Context;
use mspm0_data_types::WakeTimes;

/// Read every `data/wakeup/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, WakeTimes>> {
    let mut families = BTreeMap::new();

    for path in glob::glob("data/wakeup/*.yaml").unwrap().flatten() {
        let family = path.file_stem().unwrap().to_string_lossy().to_string();
        let content = fs::read_to_string(&path)?;
        let wake = serde_yaml::from_str::<WakeTimes>(&content)
            .context(format!("Error reading wake-up times for {family}"))?;

        families.insert(family, wake);
    }

    Ok(families)
}
