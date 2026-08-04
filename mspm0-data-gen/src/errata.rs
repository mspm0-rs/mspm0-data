//! Reads `data/errata/<family>.yaml`, which `tools/errata.py` extracts from the errata sheets.
//!
//! Which errata apply is not derivable from anything else in the sources, and it is the one kind of
//! data whose absence is silent: a missing workaround leaves a device that mostly works.

use std::{collections::BTreeMap, fs};

use anyhow::Context;
use serde::Deserialize;

/// The errata of one family, by TI's identifier.
#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct Errata {
    pub errata: Vec<String>,
}

impl Errata {
    /// Read every `data/errata/<family>.yaml`, keyed by family name.
    pub fn parse() -> anyhow::Result<BTreeMap<String, Self>> {
        let mut families = BTreeMap::new();

        for path in glob::glob("data/errata/*.yaml").unwrap().flatten() {
            let family = path.file_stem().unwrap().to_string_lossy().to_string();
            let content = fs::read_to_string(&path)?;
            let errata = serde_yaml::from_str::<Self>(&content)
                .context(format!("Error reading errata for {family}"))?;

            families.insert(family, errata);
        }

        Ok(families)
    }
}
