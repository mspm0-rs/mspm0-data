//! Reads `data/vref/<family>.yaml`, which `tools/vref.py` extracts from the datasheets.
//!
//! `CTL1.READY` says when a reference buffer has settled, but `VREF_ERR_01` leaves that bit set once
//! a buffer has been enabled a first time since reset. On the devices carrying it the datasheet's
//! `Tstartup` is the only way to know a later enable has taken effect, which is why a figure with no
//! machine-readable source is worth transcribing.

use std::collections::BTreeMap;

use mspm0_data_types::Vref;

use crate::util;

/// Read every `data/vref/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, Vref>> {
    util::per_family("vref")
}
