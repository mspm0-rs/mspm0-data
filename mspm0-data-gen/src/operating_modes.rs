//! How deep a sleep each PD1 peripheral keeps its configuration through.
//!
//! SYSCTL forces every PD1 peripheral to a disabled state upon entry into STOP or STANDBY (TRM
//! §2.2.6.1, "Automatic Peripheral Disable in Low Power Modes"). *Most* of them retain their
//! configuration while disabled, and the TRM defers the specifics to the per-peripheral chapters,
//! so there is no single table in the TRM and no machine-readable source at all.
//!
//! The authoritative source is the "Supported Functionality by Operating Mode" table in each device
//! datasheet, whose legend is exact: `DIS` means "disabled (either clock or power gated) ... but the
//! function's configuration is retained", and `OFF` means "fully powered off ... and no
//! configuration information is retained". `tools/operating_modes.py` extracts that table, so the
//! values in `data/retention/` can be re-derived rather than taken on trust.
//!
//! That matters, because the set of peripherals for which the SDK ships
//! `DL_*_saveConfiguration`/`restoreConfiguration` (AES, MCAN, SPI, the timers, TRNG and UART) is
//! *not* the set which loses configuration. The datasheets mark PD1 UART and SPI as `DIS`, and mark
//! MATHACL — which has no save/restore API — as `OFF`. TI's own SysConfig points at the datasheet
//! too: the description of its `enableRetention` option reads "Some MSPM0G peripherals residing in
//! PD1 domain do not retain register contents when entering STOP or STANDBY modes. Please view the
//! datasheet for more details."
//!
//! The answer is per family and per instance rather than per IP block — `AESADV` is `OFF` on
//! mspm0g518x but `DIS` on mspm0l211x, and on the G devices the PD1 timers are `OFF` while the PD1
//! UARTs are `DIS` — so `data/retention/<family>.yaml` names instances explicitly rather than
//! matching patterns. `verify` reports both directions: a PD1 peripheral missing from its family's
//! file, and an entry naming a peripheral which is absent or not in PD1.

use std::{collections::BTreeMap, fs};

use anyhow::Context;
use mspm0_data_types::PowerMode;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct OperatingModes {
    /// Peripheral instance name to the deepest mode its configuration survives.
    ///
    /// `Sleep` for the peripherals a datasheet marks `OFF` in the STOP/STANDBY columns, `Standby`
    /// for those it marks `DIS`.
    pub retained_through: BTreeMap<String, PowerMode>,

    /// Peripheral instance name to the deepest mode it can be used in.
    #[serde(default)]
    pub usable_through: BTreeMap<String, PowerMode>,
}

impl OperatingModes {
    pub fn parse() -> anyhow::Result<BTreeMap<String, OperatingModes>> {
        let mut map = BTreeMap::new();

        for f in glob::glob("data/operating_modes/*.yaml")? {
            let f = f?;
            let content = fs::read_to_string(&f)?;
            let modes = serde_yaml::from_str::<OperatingModes>(&content)
                .context(format!("Error reading {}", f.display()))?;

            map.insert(
                f.file_stem()
                    .context("File has no stem")?
                    .to_string_lossy()
                    .to_string(),
                modes,
            );
        }

        Ok(map)
    }
}
