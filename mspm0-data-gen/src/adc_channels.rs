//! Reads `data/adc_channels/<family>.yaml`, which `tools/adc_channels.py` extracts from the
//! datasheets' "ADC Channel Mapping" tables.
//!
//! The SDK's `ADC12_internalConnections.js` holds the same mapping but keyed by SDK family, which
//! is a superset of its devices: it gives mspm0g110x and mspm0g310x OPA routes those parts do not
//! have, and the sysconfig `SYS_OA*_CHANNELS` attributes repeat the mistake. The datasheets are per
//! part, so they are the source; the tool's docs list the discrepancies found.

use std::collections::BTreeMap;

use mspm0_data_types::AdcInternalSource;

use crate::util;

/// One family's mapping: ADC instance name to channel number to internal source.
pub type AdcChannels = BTreeMap<String, BTreeMap<u8, AdcInternalSource>>;

/// Read every `data/adc_channels/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, AdcChannels>> {
    util::per_family("adc_channels")
}
