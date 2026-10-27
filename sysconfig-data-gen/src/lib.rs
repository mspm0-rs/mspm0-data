use serde::{Deserialize, Serialize};

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
