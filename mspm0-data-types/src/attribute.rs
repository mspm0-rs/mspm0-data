use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub value: AttrValue,
}

impl Attribute {
    /// When the peripheral is used as a UART, how many FIFO entries are available.
    ///
    /// For UNICOMM peripherals, this also indicates the peripheral supports this IP mode.
    ///
    /// This is a [`AttrValue::Uint`].
    pub const UART_FENTRIES: &str = "UART_FENTRIES";

    /// When the peripheral is used as a I2C Controller, how many FIFO entries are available.
    ///
    /// For UNICOMM peripherals, this also indicates the peripheral supports this IP mode.
    ///
    /// This is a [`AttrValue::Uint`].
    pub const I2CC_FENTRIES: &str = "I2CC_FENTRIES";

    /// When the peripheral is used as a I2C Target, how many FIFO entries are available.
    ///
    /// For UNICOMM peripherals, this also indicates the peripheral supports this IP mode.
    ///
    /// This is a [`AttrValue::Uint`].
    pub const I2CT_FENTRIES: &str = "I2CT_FENTRIES";

    /// When the peripheral is used as a SPI, how many FIFO entries are available.
    ///
    /// For UNICOMM peripherals, this also indicates the peripheral supports this IP mode.
    ///
    /// This is a [`AttrValue::Uint`].
    pub const SPI_FENTRIES: &str = "SPI_FENTRIES";

    /// When a peripheral is used as a UART, it is also capable of LIN.
    ///
    /// This is a [`AttrValue::Bool`] and its presence indicates LIN capability.
    pub const UART_IS_LIN: &str = "UART_IS_LIN";

    /// The SPGSS instance this UNICOMM belongs to.
    ///
    /// This is a [`AttrValue::String`],
    pub const SPGSS: &str = "SPGSS";

    pub fn bool(name: &str, v: bool) -> Self {
        Self { name: name.into(), value: AttrValue::Bool(v) }
    }

    pub fn uint(name: &str, v: u32) -> Self {
        Self { name: name.into(), value: AttrValue::Uint(v) }
    }

    pub fn str(name: &str, s: String) -> Self {
        Self { name: name.into(), value: AttrValue::String(s) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AttrValue {
    String(String),
    Uint(u32),
    Bool(bool),
}

impl AttrValue {
    pub fn as_str(&self) -> &str {
        let Self::String(s) = self else {
            panic!();
        };

        s
    }

    pub fn as_uint(&self) -> u32 {
        let Self::Uint(i) = self else {
            panic!();
        };

        *i
    }
}
