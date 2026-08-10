//! Reads `data/opa/<family>.yaml`, hand-curated from the datasheets.
//!
//! The G-family files come from the "OPAx Input Channel Mapping" tables. The L-family files come
//! from Figure 8-1 "Device Analog Connections": the L1306/L1346 datasheet promises those tables
//! and does not contain them, and the figure — with its gapped mux position numbering — is the
//! only per-device statement. Hand-curated rather than extracted because a figure cannot be
//! parsed and the fact set is small and stable; each file cites its source.

use std::collections::BTreeMap;

use mspm0_data_types::Opa;

use crate::util;

/// One family's mapping: OPA instance name to its input-mux maps.
pub type Opas = BTreeMap<String, Opa>;

/// Read every `data/opa/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, Opas>> {
    util::per_family("opa")
}
