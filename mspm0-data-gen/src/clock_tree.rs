//! Reads sysconfig's `clocktree.json`, which describes the clock tree a family actually has.
//!
//! This is what tells the two C-series families apart: they share the `sysctl_c110x` register block,
//! but only mspm0c1105_c1106 has a crystal driver. So neither the register YAMLs nor
//! `Peripheral::version` can answer these, and `sysctl_c110x.yaml` is in any case missing fields its
//! SVD has.
//!
//! Only the nodes whose presence is a real per-device capability are read. `UDIV` is not: the file
//! lists it for families whose SYSCTL has no such field and whose ULPCLK ceiling equals their MCLK
//! ceiling, so it is curated in `parts.yaml` instead.

use std::{collections::BTreeMap, fs, path::Path};

use mspm0_data_types::ClockTree;
use serde::Deserialize;

use crate::parts::PartFamily;

/// The node which is present exactly when the family has a high frequency crystal driver.
const HFXT: &str = "HFXT";

/// The node for the external digital HFCLK input.
const HFCLK_IN: &str = "HFCLKEXT";

/// The node which is present exactly when the family has a low frequency crystal driver.
const LFXT: &str = "LFXT";

/// The mux which selects the external digital LFCLK input.
const LFCLK_IN: &str = "EXLFMUX";

/// The mux which selects between HFCLK and the SYSPLL, and so is present only with a SYSPLL.
const SYSPLL: &str = "SYSPLLMUX";

#[derive(Debug)]
pub struct ClockTrees {
    /// Keyed by family name, lowercase (e.g. `mspm0g350x`).
    pub files: BTreeMap<String, ClockTreeFile>,
}

impl ClockTrees {
    pub fn parse(data_sources: &Path) -> anyhow::Result<Self> {
        let mut files = BTreeMap::new();
        let sysconfigs = data_sources.join("sysconfig");

        for path in glob::glob(&format!("{}/**/clocktree.json", sysconfigs.display()))
            .unwrap()
            .flatten()
        {
            // The file is named after its directory rather than the device.
            let Some(name) = path.iter().nth_back(1) else {
                continue;
            };

            let name = name.to_string_lossy().to_lowercase();
            let content = fs::read_to_string(&path)?;

            files.insert(name, serde_json::from_str::<ClockTreeFile>(&content)?);
        }

        Ok(Self { files })
    }
}

#[derive(Debug, Deserialize)]
pub struct ClockTreeFile {
    #[serde(rename(deserialize = "ipInstances"))]
    pub ip_instances: Vec<IpInstance>,
}

impl ClockTreeFile {
    /// The clock tree of a family, combining what this file describes with the curated remainder.
    pub fn clock_tree(&self, family: &PartFamily) -> ClockTree {
        let has = |name: &str| self.ip_instances.iter().any(|node| node.name == name);

        ClockTree {
            hfxt: has(HFXT),
            hfclk_in: has(HFCLK_IN),
            hfclk_hz: family.clock_tree.hfclk_hz.map(Into::into),
            lfxt: has(LFXT),
            lfclk_in: has(LFCLK_IN),
            syspll: has(SYSPLL),
            ulpclk_div: family.clock_tree.ulpclk_div,
            stop1: family.clock_tree.stop1,
        }
    }
}

/// One node of the tree.
///
/// Only the name is read: every fact taken from this file is the presence of a node.
#[derive(Debug, Deserialize)]
pub struct IpInstance {
    pub name: String,
}
