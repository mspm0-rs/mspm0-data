//! Reads `data/wakeup/<family>.yaml`, which `tools/wakeup.py` extracts from the datasheets.
//!
//! How long a device takes to reach RUN from each sleep mode has no machine-readable source, and it is
//! what decides whether a sleep is worth entering before the next deadline.
//!
//! Unlike the other `data/` readers there is no intermediate type: the file's keys are exactly
//! [`WakeTimes`]' fields, so it deserializes straight into the shape the JSON carries.

use std::collections::BTreeMap;

use mspm0_data_types::WakeTimes;

use crate::util;

/// Read every `data/wakeup/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, WakeTimes>> {
    util::per_family("wakeup")
}
