// probably look at ipInstances
// type:
// - Mux - should be configurable
// - Divider - specifies the allowed values

use std::{collections::BTreeMap, fs, path::Path};

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct ClockTree {
    #[serde(rename(deserialize = "ipInstances"))]
    pub ip_instances: Vec<IpInstance>,
}

impl ClockTree {
    pub fn read_clock_trees(data_sources: &Path) -> anyhow::Result<BTreeMap<String, Self>> {
        let mut clock_trees = BTreeMap::new();
        let sysconfigs = data_sources.join("sysconfig");

        for path in glob::glob(&format!("{}/**/clocktree.json", sysconfigs.display()))
            .unwrap()
            .flatten()
        {
            let Some(name) = path.iter().nth_back(1) else {
                continue;
            };

            let name = name.to_string_lossy().to_lowercase();
            let content = fs::read_to_string(path)?;
            let clock_tree = serde_json::from_str::<ClockTree>(&content)?;

            if name.contains("c110x") {
                for entry in clock_tree.ip_instances.iter() {
                    match entry.ty.as_str() {
                        "Divider" | "Mux" => {
                            // dbg!(&entry);
                        }
                        _ => {}
                    }
                }
            }

            clock_trees.insert(name, clock_tree);
        }

        Ok(clock_trees)
    }
}

#[derive(Debug, Deserialize)]
pub struct IpInstance {
    pub name: String,

    #[serde(rename(deserialize = "displayName"))]
    pub display_name: Option<String>,

    #[serde(rename(deserialize = "type"))]
    pub ty: String,

    #[serde(flatten)]
    pub mode: Option<Mode>,

    #[serde(rename(deserialize = "inPins"))]
    pub in_pins: Vec<Pin>,

    #[serde(rename(deserialize = "outPins"))]
    pub out_pins: Vec<Pin>,

    // TODO: divideValues (optional)
    #[serde(rename(deserialize = "divideValues"))]
    pub divide_values: Option<Vec<u32>>,

    /// The meaning of the reset value depends on the type of the instance.
    #[serde(rename(deserialize = "resetValue"))]
    pub reset_value: Option<Value>,

    pub range: Option<[Value; 2]>,
}

#[derive(Debug, Deserialize)]
pub enum Mode {
    #[serde(rename(deserialize = "enabled"))]
    Enabled(bool),

    #[serde(rename(deserialize = "fixed"))]
    Fixed(bool),
}

#[derive(Debug, Deserialize)]
pub struct Pin {
    pub name: String,

    #[serde(rename(deserialize = "displayName"))]
    pub display_name: Option<String>,
}
