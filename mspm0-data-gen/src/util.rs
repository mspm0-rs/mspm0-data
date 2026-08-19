use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::sync::{Mutex, OnceLock};

use anyhow::Context;
use regex::Regex;
use serde::de::DeserializeOwned;

/// Read every `data/<dir>/<family>.yaml` into `T`, keyed by family name.
///
/// The per-family directories under `data/` all load the same way — one file per family, named
/// after it, deserialized whole — so only the directory and the type differ. What each of them
/// means, and which tool writes it, is documented on the module that names the type.
pub fn per_family<T: DeserializeOwned>(dir: &str) -> anyhow::Result<BTreeMap<String, T>> {
    let mut families = BTreeMap::new();

    for path in glob::glob(&format!("data/{dir}/*.yaml"))?.flatten() {
        let family = path
            .file_stem()
            .context(format!(
                "{}: no file stem to take a family name from",
                path.display()
            ))?
            .to_string_lossy()
            .into_owned();

        let content = fs::read_to_string(&path).context(format!("reading {}", path.display()))?;

        let value =
            serde_yaml::from_str::<T>(&content).context(format!("reading {}", path.display()))?;

        families.insert(family, value);
    }

    Ok(families)
}

pub struct RegexMap<'a, T> {
    map: &'a [(&'a str, T)],
    regexes: OnceLock<Vec<Regex>>,
    cache: Mutex<Option<HashMap<String, Option<usize>>>>,
}

impl<'a, T> RegexMap<'a, T> {
    pub const fn new(map: &'a [(&'a str, T)]) -> Self {
        Self {
            map,
            regexes: OnceLock::new(),
            cache: Mutex::new(None),
        }
    }

    pub fn get(&self, key: &str) -> Option<&'a T> {
        if let Some(&val) = self
            .cache
            .lock()
            .unwrap()
            .get_or_insert_with(Default::default)
            .get(key)
        {
            return val.map(|i| &self.map[i].1);
        }
        let val = self.get_uncached(key);
        self.cache
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .insert(key.to_string(), val);
        val.map(|i| &self.map[i].1)
    }

    fn get_uncached(&self, key: &str) -> Option<usize> {
        let regexes = self.regexes.get_or_init(|| {
            self.map
                .iter()
                .map(|(k, _)| Regex::new(&format!("^{k}$")).unwrap())
                .collect()
        });

        for (i, k) in regexes.iter().enumerate() {
            if k.is_match(key) {
                return Some(i);
            }
        }
        None
    }
}
