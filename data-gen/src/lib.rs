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

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The type of CPU used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Cpu {
    /// Cortex-M0+.
    CortexM0P,

    /// Cortex-M3.
    CortexM3,

    /// Cortex-M4.
    CortexM4,

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
    pub peripherals: BTreeMap<String, Peripheral>,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Peripheral {
    /// Peripheral name.
    pub name: String,

    /// Signals provided by this peripheral.
    pub signals: BTreeMap<String, PeripheralSignal>,

    /// The address of this peripheral block.
    ///
    /// For some peripherals this is not defined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<u32>,

    /// The address of this peripheral block in secure mode.
    ///
    /// [`Self::address`] must be defined if this is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_secure: Option<u32>,

    /// The version of this peripheral block.
    ///
    /// This must be [`Some`] if [`Self::address`] is [`Some`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Peripheral DMA connections.
    ///
    /// This maps a DMA signal to a DMA instance + request/channel id.
    ///
    /// For example a key may be `TX`.
    pub dma: BTreeMap<String, Vec<DmaConnection>>,

    /// Extra data associated with this peripheral block.
    ///
    /// This is usually chip dependent.
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

/// A signal provided by a peripheral.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PeripheralSignal {
    // /// The signal name.
    // pub name: String,
    /// The signal routing.
    #[serde(flatten)]
    pub routing: Routing,
}

/// A signal routing to a pin.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Routing {
    /// A number of fixed pin routings.
    Pins(Vec<PinRouting>),

    /// A port id routing.
    ///
    /// This is used on chips where signals can be assigned (almost) arbitrarily.
    PortId(u32),
}

/// A fixed pin routing.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PinRouting {
    /// The pin this signal is routed to.
    pub pin: String,

    /// The function used to route the signal to the pin.
    pub function: u32,
}

/// A DMA connection.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DmaConnection {
    /// The DMA peripheral instance for this connection.
    pub dma: String,

    /// The request number for this connection.
    ///
    /// On MSPM0/33 this is the DMA trigger.
    ///
    /// On a device using uDMA this is the channel number.
    pub request: u32,
}
