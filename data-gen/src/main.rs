mod sysconfig;
mod serde_helper;

use std::{fs, path::{Path, PathBuf}};

use anyhow::Context;
use data_gen::Package;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator as _};
use serde_json::Value;

fn main() -> anyhow::Result<()> {
    let sources = PathBuf::from("./sources/");
    let parts = get_parts(&sources)?;

    let data2 = sources.join("build").join("data2");
    fs::remove_dir_all(&data2)?;

    for part in parts {

    }

    Ok(())
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

    gpns
        .par_iter()
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
                    let tys = packages.iter().map(|p| p.package.clone()).collect::<Vec<_>>().join(", ");

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

            Ok(PartData { part_number, device_family: gpn.device_family.clone(), package: package.clone(), data })
        })
        .collect::<anyhow::Result<Vec<PartData>>>()
}
