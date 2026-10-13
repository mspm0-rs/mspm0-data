//! Reads the SVDs for facts which vary between instances of one peripheral version, and so cannot
//! come from the register YAMLs.
//!
//! The SVDs are scanned textually rather than parsed, since every fact read here is a presence test
//! on a field name, in one case narrowed to the register holding it. Parsing properly would mean
//! depending on `svd-parser` directly: chiptool does not re-export it, and is itself pinned to a
//! commit, so the dependency would have to track that commit.

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
///
/// It is read from the peripheral's own `<instance>_CLKCFG` and nowhere else, because SYSCTL's body
/// also contains every other peripheral's clock config as `SYSCTL_MGMT_<other>_CLKCFG` — twelve such
/// registers on a G350x, each carrying the field. A test over the whole body reports SYSCTL as having
/// a bit it does not have: no SYSCTL in the SDK defines a `CLKCFG` at all.
const BLOCK_ASYNC: &str = "BLOCKASYNC";

/// The register carrying [`BLOCK_ASYNC`], suffixed onto the instance name.
const CLKCFG: &str = "_CLKCFG";

/// The `DMACTL.DMASRCWDTH`/`DMADSTWDTH` value which selects a 128-bit transfer.
///
/// Enumerated only by the SVDs of devices which implement the width.
const LONG_LONG: &str = "LONGLONG";

/// The `DMACTL` field which enables a channel on a write to its address or size registers.
///
/// Present only in the SVDs of devices which implement it.
const AUTO_ENABLE: &str = "DMAAUTOEN";

/// One `<peripheral>` element, capturing its `derivedFrom` if it has one.
static PERIPHERAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<peripheral(?:\s+derivedFrom="(?<derived>[^"]*)")?\s*>(?<body>.*?)</peripheral>"#,
    )
    .unwrap()
});

/// One `<register>` element. Never carries attributes in any of these files.
static REGISTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<register>(?<body>.*?)</register>").unwrap());

/// The first `<name>` of a peripheral, which is its instance name. Within a `<register>` body the
/// same match is the register's own name, since it precedes the fields.
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

    /// Whether `DMACTL` has the `DMAAUTOEN` field, cross-checking `DMA_SYS_MMR_AUTO` the same way.
    ///
    /// The SVDs only describe fields the device implements here, which is what makes both checks
    /// worth having — `DMASRCINCR`'s stride encodings and `DMAEM` are in every SVD, including the
    /// families whose header says those two features are absent.
    pub dma_auto_enable: bool,
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

            let clkcfg = format!("{name}{CLKCFG}");
            let tag = format!("<name>{BLOCK_ASYNC}</name>");
            let has_block_async = REGISTER.captures_iter(body).any(|register| {
                let register = &register["body"];

                NAME.captures(register).is_some_and(|n| n[1] == *clkcfg) && register.contains(&tag)
            });

            own.insert(name.clone(), has_block_async);

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
            dma_auto_enable: content.contains(&format!("<name>{AUTO_ENABLE}</name>")),
        })
    }
}
