//! Data generation for TI MCUs using multiple sources.
//!
//! ## Scope
//!
//! This is intended to support the following MCU families:
//! - CC13xx
//! - CC23xx
//! - CC26xx
//! - CC27xx
//! - CC35xx
//! - MSPM0
//! - MSPM33
//!
//! Additional MCU families may be considered on a case by case basis as long as rustc supports the target.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The type of CPU used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Cpu {
    /// Cortex-M0+.
    CortexM0P,

    /// Cortex-M3.
    CortexM3,

    /// Cortex-M4F.
    CortexM4F,

    /// Cortex-M33.
    CortexM33,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Chip {
    /// The chip name.
    ///
    /// This shall not contain any placeholders and be a full chip name like mspm0g3507.
    pub name: String,

    // TODO: Do we need a family name?
    /// Datasheet URL.
    pub datasheet: String,

    /// Reference Manual URL.
    pub reference_manual: String,

    /// Errata URL.
    pub errata: String,

    /// CPU cores on this chip.
    ///
    /// This is intended to prepare for a future where multi-core parts could exist.
    pub cores: Vec<Core>,

    /// The chip package.
    pub package: Package,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Core {
    /// CPU core type.
    pub cpu: Cpu,
    // TODO: NVIC priority bits

    // TODO: Clock info

    // TODO: Peripherals

    // TODO: Memory map

    // TODO: Interrupts

    // TODO: chip-specific ADC/DMA/EVENT/INTERRUPT/IOMUX info
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Package {
    /// The name of the package.
    ///
    /// Example: `LQFP-64`
    pub name: String,

    /// The type of package.
    ///
    /// Example: `DGS28`
    pub package: String,

    /// The pins of the package.
    pub pins: Vec<PackagePin>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PackagePin {
    /// The position by pin name.
    ///
    /// Examples:
    /// - `5`
    /// - `A4`
    pub position: String,

    /// The signals attached to this pin.
    ///
    /// This could be multiple pins due to on chip bonding of pins.
    ///
    /// Examples:
    /// - `PA0`
    /// - `NRST`
    pub signals: Vec<String>,
}
