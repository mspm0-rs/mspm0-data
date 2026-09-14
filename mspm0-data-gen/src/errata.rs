//! Reads `data/errata/<family>.yaml`, which `tools/errata.py` extracts from the errata sheets.
//!
//! Which errata apply is not derivable from anything else in the sources, and it is the one kind of
//! data whose absence is silent: a missing workaround leaves a device that mostly works.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::util;

/// The errata of one family, by TI's identifier.
#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct Errata {
    pub errata: Vec<String>,
}

/// Read every `data/errata/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, Errata>> {
    util::per_family("errata")
}
