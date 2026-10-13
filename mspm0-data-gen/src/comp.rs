//! Reads `data/comp/<family>.yaml`, which `tools/comp.py` extracts from the datasheets.
//!
//! Neither comparator timing figure has a status bit behind it: nothing reports that the
//! comparator has reached its propagation-delay specification after `CTL1.ENABLE`, or that the
//! reference DAC has settled after a code change. The datasheet rows are the only source, which is
//! why figures with no machine-readable origin are worth transcribing. `Comp::int_vref` is not
//! here — it comes from sysconfig, per instance, in `apply_comp`.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::util;

/// The per-family timing figures, merged with sysconfig's per-instance facts in `apply_comp`.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CompTiming {
    pub enable_fast_ns: Option<u32>,
    pub enable_ulp_ns: Option<u32>,
    pub dac_settle_ns: Option<u32>,
    pub dac_settle_pin_ns: Option<u32>,
}

/// Read every `data/comp/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, CompTiming>> {
    util::per_family("comp")
}
