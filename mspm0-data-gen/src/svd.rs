//! Reads the SVDs for facts which vary between instances of one peripheral version, and so cannot
//! come from the register YAMLs.
//!
//! The SVDs are scanned textually rather than parsed, since every fact read here is a presence test
//! on a field name. Parsing properly would mean depending on `svd-parser` directly: chiptool does not
//! re-export it, and is itself pinned to a commit, so the dependency would have to track that commit.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::LazyLock,
};

use anyhow::Context;
use regex::Regex;

/// The field which gates a peripheral's asynchronous fast clock request.
///
/// Only peripherals which can raise such a request have it. Note that this is distinct from
/// `SYSCTL.SYSOSCCFG.BLOCKASYNCALL`, which masks the requests of every peripheral at once.
const BLOCK_ASYNC: &str = "BLOCKASYNC";

/// The `DMACTL.DMASRCWDTH`/`DMADSTWDTH` value which selects a 128-bit transfer.
///
/// Enumerated only by the SVDs of devices which implement the width.
const LONG_LONG: &str = "LONGLONG";

/// One `<peripheral>` element, capturing its `derivedFrom` if it has one.
static PERIPHERAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<peripheral(?:\s+derivedFrom="(?<derived>[^"]*)")?\s*>(?<body>.*?)</peripheral>"#,
    )
    .unwrap()
});

/// The first `<name>` of a peripheral, which is its instance name.
static NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<name>([^<]+)</name>").unwrap());

#[derive(Debug)]
pub struct Svds {
    /// Keyed by family name, lowercase (e.g. `mspm0g350x`).
    pub files: BTreeMap<String, Svd>,
}

impl Svds {
    pub fn parse(data_sources: &Path) -> anyhow::Result<Self> {
        let mut files = BTreeMap::new();

        for path in glob::glob(&format!("{}/svd/*.svd", data_sources.display()))
            .unwrap()
            .flatten()
        {
            let name = path.file_stem().unwrap().to_string_lossy().to_lowercase();
            let svd = Svd::read(&path).context(format!("Error reading SVD for {name}"))?;

            files.insert(name, svd);
        }

        Ok(Self { files })
    }
}

#[derive(Debug)]
pub struct Svd {
    /// Peripheral instances which have their own `CLKCFG.BLOCKASYNC` bit, masking the asynchronous
    /// fast clock request they raise.
    ///
    /// This varies per instance rather than per peripheral type: on mspm0g120x only `UC0`, `UC2`,
    /// `UC4`, `UC5` and `UC9` have the bit, despite every `UC` instance being the same IP.
    pub block_async: BTreeSet<String>,

    /// Whether `DMACTL.DMASRCWDTH` enumerates the 128-bit `LONGLONG` value.
    ///
    /// A cross-check on the header's `DMA_SYS_MMR_LLONG`, not a source: four families have no SVD.
    pub dma_long_long: bool,
}

impl Svd {
    fn read(path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;

        let mut own = BTreeMap::new();
        let mut derived_from = BTreeMap::new();

        for peripheral in PERIPHERAL.captures_iter(&content) {
            let body = &peripheral["body"];
            let name = NAME
                .captures(body)
                .context(format!("{path:?}: peripheral without a name"))?[1]
                .to_string();

            // The field name only ever appears as a `<name>`, so a substring test over the
            // peripheral's body is enough to tell whether one of its registers has the field.
            let tag = format!("<name>{BLOCK_ASYNC}</name>");
            own.insert(name.clone(), body.contains(&tag));

            // Peripherals in these SVDs are always fully expanded, but resolve `derivedFrom`
            // anyway so a source bump which starts using it does not silently drop instances.
            if let Some(base) = peripheral.name("derived") {
                derived_from.insert(name, base.as_str().to_string());
            }
        }

        let block_async = own
            .keys()
            .filter(|name| match derived_from.get(*name) {
                Some(base) => own.get(base).copied().unwrap_or(false),
                None => own[*name],
            })
            .cloned()
            .collect::<BTreeSet<_>>();

        Ok(Self {
            block_async,
            dma_long_long: content.contains(&format!("<name>{LONG_LONG}</name>")),
        })
    }
}
