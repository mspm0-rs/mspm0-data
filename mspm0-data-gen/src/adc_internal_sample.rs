//! Reads `data/adc_internal_sample/<family>.yaml`, which `tools/adc_internal_sample.py` extracts
//! from the datasheets.
//!
//! The ADC's switching-characteristics table states a separate `tSample_<signal>` row for most of
//! the signals routed to an internal channel, and those minimums are far above the bare-pin
//! `tSample` in `adc_sample.rs` — 10µs for the internal reference against a 156ns pin on
//! mspm0l211x. Nothing machine-readable carries any of it.
//!
//! **Keyed by signal, because that is how the datasheets state it**: one row per signal per family,
//! with the signal reaching different channel numbers on different families and sometimes two
//! channels on one device. `apply_adc` copies each figure onto every channel routing that signal,
//! which is where a consumer looks for it.
//!
//! Two signals are deliberately absent. The temperature sensor states *two* figures which differ
//! and are independently absent (`data/temp_sensor/`), so no single number here could carry it. The
//! OPA outputs are keyed by PGA gain rather than being scalar, and live in `Adc::pga_sample_ns`.

use std::collections::BTreeMap;

use mspm0_data_types::AdcInternalSource;
use serde::Deserialize;

use crate::util;

/// One family's per-signal ADC sample minimums, as the datasheet states them.
#[derive(Debug, Clone, Deserialize)]
pub struct AdcInternalSample {
    /// The MIN column of each `tSample_<signal>` row, in nanoseconds, keyed by the signal the row
    /// names. A signal absent here has no such row in this family's datasheet.
    #[serde(default)]
    pub sample_min_ns: BTreeMap<AdcInternalSource, u32>,

    /// The ADC channel named inside the `tSample_VREF` row's test condition.
    ///
    /// Not used to build anything — `verify.rs` compares it against the channel
    /// `data/adc_channels/` routes [`AdcInternalSource::Vref`] to. It is one page of the datasheet
    /// corroborating another, and the only independent check available on a channel map.
    ///
    /// `None` where the family's datasheet has no `tSample_VREF` row.
    #[serde(default)]
    pub vref_channel: Option<u8>,
}

/// Read every `data/adc_internal_sample/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, AdcInternalSample>> {
    util::per_family("adc_internal_sample")
}
