use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

#[derive(Debug, Serialize, Deserialize)]
pub struct Chip {
    pub packages: Vec<Package>,
    pub pin_cm: BTreeMap<String, PinCm>,
    pub peripherals: BTreeMap<String, Peripheral>,
    pub interrupts: BTreeMap<i32, Interrupt>,
    pub dma_channels: BTreeMap<u32, DmaChannel>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Package {
    /// The name of the package.
    ///
    /// Example: `LQFP-64`
    pub name: String,

    /// The name of the chip this package applies to.
    ///
    /// This field exists as a result of the MSPS003 being MSPM0C110x with a different package.
    pub chip: String,

    /// The type of package.
    ///
    /// Example: `DGS28`
    pub package: String,

    /// The pins of the package.
    pub pins: Vec<PackagePin>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackagePin {
    /// The position by pin name.
    ///
    /// Examples:
    /// - `5`
    /// - `A4`
    pub position: String,

    /// The signals attached to this pin.
    ///
    /// Examples:
    /// - `PA0`
    /// - `NRST`
    pub signals: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PinCm {
    /// The number of the IOMUX CM used used for this pin.
    pub iomux_cm: u32,

    pub pfs: BTreeMap<u32, Vec<PinFunction>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PinFunction {
    pub name: String,
}

// TODO: The rest
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PeripheralType {
    /// Peripheral type is not known. This is an error if used when generating.
    #[default]
    Unknown,

    Adc,

    Cpuss,

    Dma,

    Gpio,

    Iomux,

    /// System Controller
    ///
    /// This peripheral may have a different version per part family.
    Sysctl,

    /// A timer.
    // TODO: Timer types and attributes
    Tim,

    Uart,
}

impl fmt::Display for PeripheralType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let content = match self {
            Self::Unknown => "",
            Self::Adc => "adc",
            Self::Cpuss => "cpuss",
            Self::Dma => "dma",
            Self::Gpio => "gpio",
            Self::Iomux => "iomux",
            Self::Sysctl => "sysctl",
            // Self::SysctlC110x => "sysctl_c110x",
            // Self::SysctlL110xL130xL134x => "sysctl_l110x_l130x_l134x",
            // Self::SysctlL122xL222x => "sysctl_l122x_l222x",
            // Self::SysctlG350xG310xG150xG110x => "sysctl_g350x_g310x_g150x_g110x",
            // Self::SysctlG351xG151x => "sysctl_g351x_g151x",
            Self::Tim => "tim",
            Self::Uart => "uart",
        };

        write!(f, "{content}")
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Peripheral {
    pub name: String,

    #[serde(flatten, rename = "type")]
    pub ty: PeripheralType,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    // address
    pub address: u32,

    // TODO: irq (and group if applicable)
    pub pins: Vec<PeripheralPin>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PeripheralPin {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Interrupt {
    pub name: String,
    pub num: i32,
    pub group: BTreeMap<u32, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DmaChannel {
    /// Whether this is a full channel or basic channel.
    pub full: bool,
}
