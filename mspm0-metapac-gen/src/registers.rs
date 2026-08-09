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
    fs::create_dir_all(&out_dir).unwrap();

    // Common module
    fs::write(common, generate::COMMON_MODULE).unwrap();

    let options = generate::Options::new().with_common_module(CommonModule::External(
        TokenStream::from_str("crate::common").unwrap(),
    ));

    let re = Regex::new("# *! *\\[.*\\]").unwrap();

    for f in glob::glob("data/registers/*.yaml").unwrap() {
        let f = f.unwrap();

        if !f.is_file() {
            continue;
        }

        if f.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("ignore.")
        {
            continue;
        }

        let ctx = format!("{:?}", f.file_name());

        let mut ir: ir::IR = serde_yaml::from_str(&fs::read_to_string(&f).unwrap())
            .context(ctx.clone())
            .expect("Error reading registers");

        transform::expand_extends::ExpandExtends {}
            .run(&mut ir)
            .unwrap();

        transform::map_names(&mut ir, |k, s| match k {
            transform::NameKind::Block => *s = s.to_string(),
            transform::NameKind::Fieldset => *s = format!("regs::{}", s),
            transform::NameKind::Enum => *s = format!("vals::{}", s),
            _ => {}
        });

        transform::sort::Sort {}.run(&mut ir).unwrap();
        transform::sanitize::Sanitize::default()
            .run(&mut ir)
            .unwrap();

        let items = generate::render(&ir, &options)
            .context(ctx)
            .expect("Failed to generate code for peripheral");

        let name = f
            .file_name()
            .unwrap()
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
