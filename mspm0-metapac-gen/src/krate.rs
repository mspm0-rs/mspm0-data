use std::{collections::BTreeMap, fs, path::Path};

use mspm0_data_types::Chip;

pub fn generate(out_dir: &Path, chips: &BTreeMap<String, Chip>) -> anyhow::Result<()> {
    use std::fmt::Write;

    fs::copy("mspm0-metapac-gen/res/build.rs", out_dir.join("build.rs"))?;
    fs::copy("mspm0-metapac-gen/res/lib.rs", out_dir.join("src/lib.rs"))?;
    fs::copy(
        "mspm0-metapac-gen/res/metadata.rs",
        out_dir.join("src/metadata.rs"),
    )?;

    let mut cargo_toml = fs::read_to_string("mspm0-metapac-gen/res/Cargo.toml")?;

    writeln!(cargo_toml, "# Chip features - automatically generated")?;

    // Features are done by chip and package.
    for (name, chip) in chips {
        for package in chip.packages.iter() {
            writeln!(cargo_toml, "{name}{} = []", package.package.to_lowercase())?;
        }
    }

    fs::write(out_dir.join("Cargo.toml"), cargo_toml)?;

    Ok(())
}
