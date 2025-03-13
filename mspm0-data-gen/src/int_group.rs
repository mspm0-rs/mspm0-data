use std::{collections::BTreeMap, fs};

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct Groups {
    pub groups: BTreeMap<String, Vec<Interrupt>>,
}

impl Groups {
    pub fn parse() -> anyhow::Result<BTreeMap<String, Groups>> {
        let mut map = BTreeMap::new();

        for f in glob::glob("data/int_group/*.yaml")? {
            let f = f?;
            let content = fs::read_to_string(&f)?;
            let groups = serde_yaml::from_str::<Groups>(&content)?;

            map.insert(
                f.file_stem()
                    .context("File has no stem")?
                    .to_string_lossy()
                    .to_string(),
                groups,
            );
            // dbg!(groups);
        }

        Ok(map)
    }
}

#[derive(Debug, Deserialize)]
pub struct Interrupt {
    pub name: String,
    pub iidx: u8,
}
