//! Reads `data/adc_wakeup/<family>.yaml`, which `tools/adc_wakeup.py` extracts from the datasheets.
//!
//! `CTL0.PWRDN` resets to automatic power down, and the TRM makes the wake the caller's problem:
//! `SCOMPx` has to cover it on top of the sampling the signal itself needs. Nothing machine-readable
//! carries the figure — sysconfig only warns that it exists and points at the datasheet — so it is
//! transcribed like the VREF startup time.
//!
//! The two keys are separate because the datasheets fill different columns. A family with only
//! `typ_ns` has no published ceiling, which a consumer needing a guarantee has to notice rather than
//! round away.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::util;

/// One family's ADC wake-up figure, as the datasheet states it.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct AdcWakeup {
    /// The MAX column, in nanoseconds. Absent where the datasheet publishes no ceiling.
    #[serde(default)]
    pub max_ns: Option<u32>,

    /// The TYP column, in nanoseconds. Absent where the datasheet states a ceiling instead.
    #[serde(default)]
    pub typ_ns: Option<u32>,
}

/// Read every `data/adc_wakeup/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, AdcWakeup>> {
    util::per_family("adc_wakeup")
}
