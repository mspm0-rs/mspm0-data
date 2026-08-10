//! Everything the generator reads, and the per-family view of it.
//!
//! The sources divide into two kinds: vendor data under `sources/`, which is parsed, and curated
//! YAML under `data/`, which is deserialized. Both are keyed by family, so the generator only ever
//! wants one family's worth at a time — which is what [`FamilySources`] is.

use std::collections::BTreeMap;

use mspm0_data_types::{Vref, WakeTimes};

use crate::{
    adc_channels::AdcChannels,
    clock_tree::{ClockTreeFile, ClockTrees},
    comp::CompTiming,
    errata::Errata,
    header::{Header, Headers},
    int_group::Groups,
    opa::Opas,
    operating_modes::OperatingModes,
    parts::PartsFile,
    svd::{Svd, Svds},
    sysconfig::{Sysconfig, SysconfigFile},
    timers::Timers,
    uart::Uarts,
};

/// Every source, whole.
pub struct Sources {
    pub parts: PartsFile,
    pub headers: Headers,
    pub adc_channels: BTreeMap<String, AdcChannels>,
    pub sysconfig: Sysconfig,
    pub svds: Svds,
    pub clock_trees: ClockTrees,
    pub operating_modes: BTreeMap<String, OperatingModes>,
    pub int_groups: BTreeMap<String, Groups>,
    pub timers: BTreeMap<String, Timers>,
    pub uart: BTreeMap<String, Uarts>,
    pub opa: BTreeMap<String, Opas>,
    pub errata: BTreeMap<String, Errata>,
    pub wake: BTreeMap<String, WakeTimes>,
    pub vref: BTreeMap<String, Vref>,
    pub comp: BTreeMap<String, CompTiming>,
}

/// What one family is described by.
///
/// Only `header` and `sysconfig` are required: a family with no SVD, no curated timer table or no
/// errata sheet is a gap `verify.rs` reports, not a reason to stop generating.
pub struct FamilySources<'a> {
    pub header: &'a Header,
    pub sysconfig: &'a SysconfigFile,
    pub adc_channels: Option<&'a AdcChannels>,
    pub svd: Option<&'a Svd>,
    pub clock_tree: Option<&'a ClockTreeFile>,
    pub operating_modes: Option<&'a OperatingModes>,
    pub timers: Option<&'a Timers>,
    pub uart: Option<&'a Uarts>,
    pub opa: Option<&'a Opas>,
    pub errata: Option<&'a Errata>,
    pub wake: Option<WakeTimes>,
    pub vref: Option<Vref>,
    pub comp: Option<CompTiming>,

    /// Not narrowed to the family: `generate_irqs` looks groups up per chip, not per family.
    pub int_groups: &'a BTreeMap<String, Groups>,
}

impl Sources {
    /// Narrow every source to one family.
    pub fn family(&self, family: &str) -> anyhow::Result<FamilySources<'_>> {
        use anyhow::Context;

        let sysconfig = self
            .sysconfig
            .files
            .get(&family.to_uppercase())
            .context(format!("No sysconfig data available for {family}"))?;

        // MSPS003FX is the same die as C110X, differing only in package options and some pins.
        let header_name = if family == "msps003fx" {
            "mspm0c110x"
        } else {
            family
        };

        let header = self
            .headers
            .files
            .get(header_name)
            .context(format!("Could not lookup header for {header_name}"))?;

        Ok(FamilySources {
            header,
            sysconfig,
            adc_channels: self.adc_channels.get(family),
            svd: self.svds.files.get(family),
            clock_tree: self.clock_trees.files.get(family),
            operating_modes: self.operating_modes.get(family),
            timers: self.timers.get(family),
            uart: self.uart.get(family),
            opa: self.opa.get(family),
            errata: self.errata.get(family),
            wake: self.wake.get(family).copied(),
            vref: self.vref.get(family).copied(),
            comp: self.comp.get(family).copied(),
            int_groups: &self.int_groups,
        })
    }
}
