//! Reads `data/int_group/<family>.yaml`, which is hand-entered.
//!
//! Several peripherals can share one NVIC line; the handler tells them apart by reading the group's
//! `IIDX`. Which value means which peripheral is in the TRM's interrupt tables and nowhere
//! machine-readable, so it is transcribed here.
//!
//! **The datasheet decides whether a row exists; the name is chosen for the consumer.** Those are
//! separate questions and the wake-up controller's two subscriber ports are where they come apart.
//!
//! Every datasheet's interrupt table calls them `EVENT SUB PORT0`/`PORT1` and both TRMs call them
//! `WUC FSUB0`/`FSUB1`. Both are TI's own words. `WUC_FSUB0` is recorded because a group member
//! becomes a variant of the generated `Group0` enum, and that name maps one-to-one onto the
//! register a driver writes - `WUC.fsub(0)` - where the datasheet spelling needs a lookup. Do not
//! "correct" it to the datasheet wording; it was chosen against it, deliberately, on 2026-08-27.
//!
//! Existence is the other question, and there the datasheet wins outright. c110x, l110x and
//! msps003fx have no `IIDX` 4 or 5: their datasheets' tables stop at 3 and resume at 6, while both
//! TRMs list the subscriber ports for the whole series. Do not fill those in from the TRM.

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
