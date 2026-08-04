//! Reads `data/timers/<family>.yaml`, which `tools/timers.py` extracts from the datasheets.
//!
//! Every timer instance shares the `tim_v1` register block, so nothing in `data/registers` or the
//! SVDs distinguishes a bare `TIMB0` from a `TIMA0` with deadband insertion and a fault handler.
//! The datasheet's TIMx configuration table is the only per-device source which does; see the
//! tool's module docs for why the SVDs and the TRM are not used.

use std::{collections::BTreeMap, fs};

use anyhow::Context;
use mspm0_data_types::Timer;
use serde::Deserialize;

/// Timers of one family, keyed by instance name.
#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct Timers {
    pub timers: BTreeMap<String, Timer>,
}

impl Timers {
    /// Read every `data/timers/<family>.yaml`, keyed by family name.
    pub fn parse() -> anyhow::Result<BTreeMap<String, Self>> {
        let mut families = BTreeMap::new();

        for path in glob::glob("data/timers/*.yaml").unwrap().flatten() {
            let family = path.file_stem().unwrap().to_string_lossy().to_string();
            let content = fs::read_to_string(&path)?;
            let timers = serde_yaml::from_str::<Self>(&content)
                .context(format!("Error reading timers for {family}"))?;

            families.insert(family, timers);
        }

        Ok(families)
    }
}
