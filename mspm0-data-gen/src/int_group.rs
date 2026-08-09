//! Reads `data/int_group/<family>.yaml`, which is hand-entered.
//!
//! Several peripherals can share one NVIC line; the handler tells them apart by reading the group's
//! `IIDX`. Which value means which peripheral is in the TRM's interrupt tables and nowhere
//! machine-readable, so it is transcribed here.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::util;

/// The interrupt groups of one family, keyed by group name.
#[derive(Debug, Default, Deserialize)]
pub struct Groups {
    pub groups: BTreeMap<String, Vec<Interrupt>>,
}

/// One peripheral within a group, and the `IIDX` value which selects it.
#[derive(Debug, Deserialize)]
pub struct Interrupt {
    pub name: String,
    pub iidx: u8,
}

/// Read every `data/int_group/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, Groups>> {
    util::per_family("int_group")
}
