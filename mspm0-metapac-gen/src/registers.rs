use std::{fs, path::Path, str::FromStr};

use anyhow::Context;
use chiptool::{
    generate::{self, CommonModule},
    ir, transform,
};
use proc_macro2::TokenStream;
use regex::Regex;

use crate::write_rust;

pub fn generate(out_dir: &Path) -> anyhow::Result<()> {
    let common = out_dir.join("src/common.rs");

    // Must create `src` directory which common is eventually written to.
    let out_dir = out_dir.join("src/peripherals");
    fs::create_dir_all(&out_dir)?;

    // Common module
    fs::write(common, generate::COMMON_MODULE)?;

    let options = generate::Options::new().with_common_module(CommonModule::External(
        // Not `?`: LexError is neither Send nor Sync, and this parses a fixed path anyway.
        TokenStream::from_str("crate::common").expect("crate::common is a valid path"),
    ));

    let re = Regex::new("# *! *\\[.*\\]")?;

    for f in glob::glob("data/registers/*.yaml")?.flatten() {
        if !f.is_file() {
            continue;
        }

        let ctx = || format!("register block {}", f.display());

        let mut ir: ir::IR =
            serde_yaml::from_str(&fs::read_to_string(&f).with_context(ctx)?).with_context(ctx)?;

        transform::expand_extends::ExpandExtends {}
            .run(&mut ir)
            .with_context(ctx)?;

        transform::map_names(&mut ir, |k, s| match k {
            transform::NameKind::Block => *s = s.to_string(),
            transform::NameKind::Fieldset => *s = format!("regs::{}", s),
            transform::NameKind::Enum => *s = format!("vals::{}", s),
            _ => {}
        });

        transform::sort::Sort {}.run(&mut ir).with_context(ctx)?;
        transform::sanitize::Sanitize::default()
            .run(&mut ir)
            .with_context(ctx)?;

        let items = generate::render(&ir, &options).with_context(ctx)?;

        let name = f
            .file_name()
            .context("register block has no file name")?
            .to_string_lossy()
            .replace(".yaml", ".rs");

        // chiptool emits inner attributes between items, which is only valid at the top of a file.
        let items = items.to_string().replace("] ", "]\n");
        let items = re.replace_all(&items, "");

        let contents = format!(
            r"#![allow(clippy::missing_safety_doc)]
            #![allow(clippy::identity_op)]
            #![allow(clippy::unnecessary_cast)]
            #![allow(clippy::erasing_op)]
            {items}"
        );

        write_rust(out_dir.join(&name), &contents)?;
    }

    Ok(())
}
