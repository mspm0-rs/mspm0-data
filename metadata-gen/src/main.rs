use std::{collections::HashMap, fs, io, path::PathBuf, sync::LazyLock};

use anyhow::{Context, bail};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use regex::RegexSet;
use serde_json::Value;

mod package;

/// Chips to skip
const SKIP: LazyLock<RegexSet> =
    LazyLock::new(|| RegexSet::new(&["CC3500"]).expect("Error creating skip regex"));

fn main() -> anyhow::Result<()> {
    let sources = PathBuf::from("sources");
    let output = PathBuf::from("build/data2");
    fs::create_dir_all(&output)?;

    if !fs::exists(&sources)? {
        bail!("Sources directory does not exist? help: clone git submodules");
    }

    let sysconfigs = fs::read_dir(sources.join("sysconfig"))?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<io::Result<Vec<PathBuf>>>()?;

    let sysconfig_jsons = sysconfigs
        .par_iter()
        .map(|sysconfig_path| {
            // <path>/<path>.json
            let dir_name = sysconfig_path.file_name().unwrap();
            let json_path = sysconfig_path.join(dir_name).with_extension("json");

            let file = fs::File::open(&json_path)
                .with_context(|| format!("Error opening {path}", path = json_path.display()))?;
            let json = serde_json::from_reader(file)
                .with_context(|| format!("Error parsing {path}", path = json_path.display()))?;

            Ok((dir_name.to_string_lossy().to_string(), json))
        })
        .collect::<anyhow::Result<HashMap<String, Value>>>()
        .context("Error reading all sysconfig jsons")?;

    verify_sysconfig_jsons(&sysconfig_jsons)?;

    // Sorted print for a moment
    {
        // TODO: Human sorting
        let mut kvs = sysconfig_jsons.iter().collect::<Vec<_>>();
        kvs.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

        for (k, v) in kvs.iter() {
            let packages =
                package::get_packages(v).with_context(|| format!("Resolving packages for: {k}"))?;

            for package in packages {
                if let Some(multi) = package
                    .pins
                    .iter()
                    .find(|p| p.signals.iter().any(|s| s.contains('/')))
                {
                    println!("Multi pin for {k}: {pin:?}", pin = multi.signals);
                }
            }

            println!(
                "{k} packages: {}",
                v.get("packages").unwrap().as_object().unwrap().len()
            );
        }
    }

    Ok(())
}

/// Verify the required objects are present.
fn verify_sysconfig_jsons(sysconfig_jsons: &HashMap<String, Value>) -> anyhow::Result<()> {
    if let Some((k, _)) = sysconfig_jsons
        .iter()
        .find(|(_, v)| v.get("packages").is_none())
    {
        bail!("{k} has no packages object");
    }

    if let Some((k, _)) = sysconfig_jsons
        .iter()
        .find(|(_, v)| v.get("devicePins").is_none())
    {
        bail!("{k} has no devicePins object");
    }

    if let Some((k, _)) = sysconfig_jsons
        .iter()
        .find(|(_, v)| v.get("peripherals").is_none())
    {
        bail!("{k} has no peripherals object");
    }

    if let Some((k, _)) = sysconfig_jsons
        .iter()
        .find(|(_, v)| v.get("peripheralPins").is_none())
    {
        bail!("{k} has no peripheralPins object");
    }

    Ok(())
}
