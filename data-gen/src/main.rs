mod docs;
mod header;
mod serde_helper;
mod sysconfig;
mod util;

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use data_gen::{Chip, Core, Cpu, Package};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator as _};
use serde_json::Value;

use crate::{
    docs::{DOCS, Links},
    header::msp::CHIP_TO_HEADER_AND_FAMILY,
};

fn main() -> anyhow::Result<()> {
    let sources = PathBuf::from("./sources/");
    let parts = get_parts(&sources)?;

    let data2 = PathBuf::from("./build/").join("data2");
    let _ = fs::remove_dir_all(&data2);
    fs::create_dir_all(&data2)?;

    // TODO: Parallelize with rayon
    for part in parts {
        let path = data2.join(&part.part_number).with_extension("json");
        let chip = make_chip(&sources, &part)
            .with_context(|| format!("Could not make chip {part}", part = &part.part_number))?;

        fs::write(path, serde_json::to_string_pretty(&chip)?)?;
    }

    Ok(())
}

fn make_chip(sources: &Path, part: &PartData) -> anyhow::Result<Chip> {
    let cpu = cpu_for_part(&part.part_number)
        .with_context(|| format!("part number {} did not have CPU mapping", &part.part_number))?;
    let peripherals = sysconfig::get_peripherals(
        &part.part_number,
        &part.device_family,
        &part.data,
        &part.package,
    )?;

    // Resolve addressees and
    if part.part_number.starts_with("MSP") {
        let header_name = CHIP_TO_HEADER_AND_FAMILY
            .get(&part.part_number.to_lowercase())
            .with_context(|| format!("Cannot find header for {}", part.part_number))?;

        let header_path = if header_name.starts_with("mspm0") {
            sources
                .join("mspm0-sdk")
                .join("source")
                .join("ti")
                .join("devices")
                .join("msp")
                .join("m0p")
                .join(header_name)
                .with_extension("h")
        } else if header_name.starts_with("mspm33") {
            sources
                .join("mspm33-sdk")
                .join("source")
                .join("ti")
                .join("devices")
                .join("msp")
                .join("m33")
                .join(header_name)
                .with_extension("h")
        } else {
            bail!("path");
        };

        let _header = fs::read_to_string(&header_path)
            .with_context(|| format!("Reading {}", header_path.display()))?;
    }

    // No existing parts are dual core, we just generate one.
    let core = Core { cpu, peripherals };

    let Links {
        datasheet,
        reference,
        errata,
    } = DOCS
        .get(&part.part_number.to_lowercase())
        .unwrap_or_else(|| {
            println!("{part} has no doc links", part = &part.part_number);
            &Links {
                datasheet: "",
                reference: "",
                errata: "",
            }
        });

    Ok(Chip {
        name: part.part_number.clone(),
        datasheet: datasheet.to_string(),
        reference_manual: reference.to_string(),
        errata: errata.to_string(),
        cores: vec![core],
        package: part.package.clone(),
    })
}

/// TODO: Is there a nicer way to get this information?
fn cpu_for_part(part_number: &str) -> Option<Cpu> {
    // Must put M33 first to avoid matching M33 as M0
    if part_number.starts_with("MSPM33") {
        return Some(Cpu::CortexM33);
    }

    if part_number.starts_with("MSP") || part_number.starts_with("CC2340") {
        return Some(Cpu::CortexM0P);
    }

    if part_number.starts_with("CC1310")
        || part_number.starts_with("CC1350")
        || part_number.starts_with("CC2640")
    {
        return Some(Cpu::CortexM3);
    }

    if part_number.starts_with("CC1311") || part_number.starts_with("CC2651") {
        return Some(Cpu::CortexM4);
    }

    if part_number.starts_with("CC1312")
        || part_number.starts_with("CC1352")
        || part_number.starts_with("CC2642")
        || part_number.starts_with("CC2652")
        // FIXME: Is this M4F?
        || part_number.starts_with("CC2653")
        || part_number.starts_with("CC2662")
    {
        return Some(Cpu::CortexM4F);
    }

    if part_number.starts_with("CC1314")
        || part_number.starts_with("CC1354")
        || part_number.starts_with("CC2674")
        || part_number.starts_with("CC2744")
        || part_number.starts_with("CC2745")
        // FIXME: Is this M33?
        || part_number.starts_with("CC2765")
        || part_number.starts_with("CC2755")
        || part_number.starts_with("CC3551E")
    {
        return Some(Cpu::CortexM33);
    }

    None
}

#[derive(Debug)]
struct PartData {
    /// The device part number.
    ///
    /// This is slightly different than real part numbers, since we drop `S` and `Q1` which indicate temperature ratings.
    part_number: String,

    /// The device family is the sysconfig directory name.
    device_family: String,

    /// The package of this part.
    package: Package,

    /// Sysconfig data for this part.
    data: Value,
}

fn get_parts(sources: &Path) -> anyhow::Result<Vec<PartData>> {
    let mut gpns = sysconfig::get_gpns(&sources).context("Get gpns")?;
    // Remove the duplicate CC3551 entries.
    gpns.dedup_by(|a, b| a.part_group == b.part_group && a.package_name == b.package_name);

    gpns.par_iter()
        .map(|gpn| {
            let path = sources.join("sysconfig").join(&gpn.device_family);
            let data = sysconfig::read_data(&path)
                .with_context(|| format!("Read sysconfig info for {}", gpn.part_group))?;
            let packages = sysconfig::get_packages(&gpn.device_family, &data)
                .with_context(|| format!("get packages for {}", gpn.part_group))?;

            let (_, gpn_package_ty_with_ending_brace) = gpn
                .package_name
                .split_once('(')
                .context("package type is not in braces")?;
            let (gpn_package_ty, _) = gpn_package_ty_with_ending_brace
                .split_once(')')
                .context("package type is not in braces")?;

            let package = packages
                .iter()
                .find(|p| {
                    let found = p.package == gpn_package_ty;

                    // Special case CC2651R3SIPA
                    if !found && gpn_package_ty == "SIP" {
                        return p.package == "SIPA";
                    }

                    found
                })
                .with_context(|| {
                    let tys = packages
                        .iter()
                        .map(|p| p.package.clone())
                        .collect::<Vec<_>>()
                        .join(", ");

                    format!(
                        "No package of type {gpn_package_ty} for {gpn} (available: {tys})",
                        gpn = gpn.part_group
                    )
                })?;

            // Construct the real part number
            //
            // All MSPM (excluding MSPS and MSP32) require amendment to add the package type to end of group name to get part number.
            //
            // FIXME: CC3551 may need adjustment?
            let part_number = if gpn.part_group.starts_with("MSPM") {
                format!("{}{}", gpn.part_group, package.package)
            } else {
                gpn.part_group.clone()
            };

            Ok(PartData {
                part_number,
                device_family: gpn.device_family.clone(),
                package: package.clone(),
                data,
            })
        })
        .collect::<anyhow::Result<Vec<PartData>>>()
}
