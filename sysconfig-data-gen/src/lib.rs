use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    /// The human readable device name.
    pub device_name: String,

    /// The family name for this type.
    ///
    /// This is intended to allow sharing peripheral IPs between chips that only different in amounts of RAM/FLASH.
    ///
    /// This is usually something like `mspm0c110x` or `cc13x4_26x4`
    pub family: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagePin {
    /// The position by pin name.
    ///
    /// Examples:
    /// - `5`
    /// - `A4`
    pub position: String,

    /// The physical signals attached to this pin.
    ///
    /// Examples:
    /// - `PA0`
    /// - `NRST`
    pub signals: Vec<String>,
}

/// MSPM0/33 specific data
pub mod msp {
    use std::collections::BTreeMap;

    /// Mapping from pin to PINCM index.
    pub type PinCmIndices = BTreeMap<String, u8>;
}
