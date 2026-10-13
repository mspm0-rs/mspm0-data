//! Reads `data/adc_sample/<family>.yaml`, which `tools/adc_sample.py` extracts from the datasheets.
//!
//! `SAMPCLK` has no published ceiling, so what bounds an ADC conversion is the sample *window*.
//! Nothing machine-readable carries it: no header constant, nothing in driverlib, and sysconfig has
//! no attribute for it. Transcribed like the VREF startup time and the ADC wake-up figure.
//!
//! Two rows, because they answer different questions. `min_ns` is the bare-pin minimum and applies
//! to every channel that reaches a package pin. `pga_ns` is the window when the channel is an OPA
//! output, and it is keyed by PGA gain rather than by channel — at x32 it is an order of magnitude
//! above `min_ns`, so a driver using the bare-pin figure there is short by that much.
//!
//! `pga_ns` is present in seven of the eighteen files and only four of those families have an OPA:
//! the datasheet row carries the footnote "Only applies for devices with OPA" because one document
//! covers several devices. `apply_adc` attaches the map only where an OPA instance exists, so the
//! superset the document prints does not reach the metadata.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::util;

/// One family's ADC sample-window figures, as the datasheet states them.
#[derive(Debug, Clone, Deserialize)]
pub struct AdcSample {
    /// The `tSample`/`tSample_step` MIN column, in nanoseconds, rounded up. Every datasheet states
    /// one, so an absent file is an extraction failure rather than a device without a minimum.
    pub min_ns: u32,

    /// The `tSample_PGA` MIN column, in nanoseconds, keyed by PGA gain. Absent where the datasheet
    /// prints no such table. A gain missing from a family that has the table is unpublished for
    /// that family and cannot be interpolated — the G and L curves cross.
    #[serde(default)]
    pub pga_ns: BTreeMap<u8, u32>,
}

/// Read every `data/adc_sample/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, AdcSample>> {
    util::per_family("adc_sample")
}
