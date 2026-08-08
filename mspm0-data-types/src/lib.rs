use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

#[derive(Debug, Serialize, Deserialize)]
pub struct Chip {
    /// The chip name.
    ///
    /// This shall not contain any placeholders and be a full chip name like mspm0g3507.
    pub name: String,

    /// The device family.
    ///
    /// Usually this is a value like `mspm0g350x`.
    pub family: String,

    /// URL for the datasheet.
    pub datasheet_url: String,

    /// URL for the reference manual.
    pub reference_manual_url: String,

    /// URL for the errata.
    pub errata_url: String,

    /// Memory layout.
    pub memory: Vec<Memory>,

    /// Packages which this chip is available in.
    pub packages: Vec<Package>,

    /// Mapping from device pin to IOMUX register index.
    pub iomux: BTreeMap<String, u32>,

    /// The peripherals available on the chip.
    pub peripherals: BTreeMap<String, Peripheral>,

    /// Interrupts available on the chip.
    pub interrupts: BTreeMap<i32, Interrupt>,

    /// DMA channels available on the chip.
    pub dma_channels: BTreeMap<u32, DmaChannel>,

    /// Number configurable channels (MEMCTL) in the ADC peripheral.
    pub adc_memctl: u8,

    /// Number of options for VRSEL of the ADC peripheral.
    ///
    /// This is requried because we use a single adc_v1 pac for all chips.
    pub adc_vrsel: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

// TODO: The rest
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PeripheralType {
    /// Peripheral type is not known. This is an error if used when generating.
    #[default]
    Unknown,

    Adc,

    AesAdv,

    Aes,

    Canfd,

    Comp,

    Cpuss,

    Crc,

    Dac,

    Debugss,

    Dma,

    Event,

    /// This region contains read-only device constants such as the device id, flash and SRAM
    /// sizes and calibration values.
    FactoryRegion,

    FlashCtl,

    GpAmp,

    Gpio,

    I2c,

    I2s,

    Iomux,

    Iwdt,

    KeystoreCtl,

    Lcd,

    Lfss,

    Mathacl,

    Npu,

    Opa,

    Rtc,

    Spi,

    /// System Controller
    ///
    /// This peripheral may have a different version per part family.
    Sysctl,

    /// A timer.
    Tim,

    Trng,

    Uart,

    Unicomm,

    /// The UART register map of a UNICOMM instance, at a fixed offset below it.
    UnicommUart,

    /// The I2C controller register map of a UNICOMM instance, at a fixed offset below it.
    UnicommI2cc,

    /// The I2C target register map of a UNICOMM instance, at a fixed offset below it.
    UnicommI2ct,

    /// The SPI register map of a UNICOMM instance, at a fixed offset below it.
    UnicommSpi,

    Usbfs,

    Vref,

    Wuc,

    Wwdt,
}

impl fmt::Display for PeripheralType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let content = match self {
            PeripheralType::Unknown => "",
            PeripheralType::Adc => "adc",
            PeripheralType::Aes => "aes",
            PeripheralType::AesAdv => "aesadv",
            PeripheralType::Canfd => "canfd",
            PeripheralType::Comp => "comp",
            PeripheralType::Cpuss => "cpuss",
            PeripheralType::Crc => "crc",
            PeripheralType::Dac => "dac",
            PeripheralType::Debugss => "debugss",
            PeripheralType::Dma => "dma",
            PeripheralType::Event => "event",
            PeripheralType::FactoryRegion => "factoryregion",
            PeripheralType::FlashCtl => "flashctl",
            PeripheralType::GpAmp => "gpamp",
            PeripheralType::Gpio => "gpio",
            PeripheralType::I2c => "i2c",
            PeripheralType::I2s => "i2s",
            PeripheralType::Iomux => "iomux",
            PeripheralType::Iwdt => "iwdt",
            PeripheralType::KeystoreCtl => "keystorectl",
            PeripheralType::Lcd => "lcd",
            PeripheralType::Lfss => "lfss",
            PeripheralType::Mathacl => "mathacl",
            PeripheralType::Npu => "npu",
            PeripheralType::Opa => "opa",
            PeripheralType::Rtc => "rtc",
            PeripheralType::Spi => "spi",
            PeripheralType::Sysctl => "sysctl",
            PeripheralType::Tim => "tim",
            PeripheralType::Trng => "trng",
            PeripheralType::Uart => "uart",
            PeripheralType::Unicomm => "unicomm",
            PeripheralType::UnicommUart => "unicommuart",
            PeripheralType::UnicommI2cc => "unicommi2cc",
            PeripheralType::UnicommI2ct => "unicommi2ct",
            PeripheralType::UnicommSpi => "unicommspi",
            PeripheralType::Usbfs => "usbfs",
            PeripheralType::Vref => "vref",
            PeripheralType::Wuc => "wuc",
            PeripheralType::Wwdt => "wwdt",
        };

        write!(f, "{content}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerDomain {
    /// "low speed" power domain. This power domain is powered in RUN, SLEEP, STOP and STANDBY modes.
    Pd0,

    /// "high performance" power domain. This power domain is powered in RUN and SLEEP modes.
    Pd1,

    /// PDB backup power domain. This is usually powered by VBAT.
    Backup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peripheral {
    pub name: String,

    #[serde(flatten, rename = "type")]
    pub ty: PeripheralType,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<u32>,

    pub power_domain: PowerDomain,

    pub pins: Vec<PeripheralPin>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sys_fentries: Option<usize>,

    /// Which register maps this UNICOMM instance implements.
    ///
    /// `None` for peripherals which are not UNICOMM instances.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unicomm: Option<Unicomm>,
}

/// Which register maps a UNICOMM instance implements.
///
/// UNICOMM is one peripheral which is a UART, an SPI, an I2C controller or an I2C target depending
/// on `IPMODE.SELECT`, with a register map per mode at a fixed offset below the instance's own
/// address. **No instance implements all four**, and which it implements does not follow the
/// instance name: on MSPM0G518x `UC0` is a UART or either half of an I2C but never an SPI, `UC2` is
/// an SPI only, and `UC3` is a UART or an SPI.
///
/// An instance with one mode has nothing to select and no `IPMODE` register to select it with, so
/// writing `IPMODE` is only meaningful where more than one of these is true.
///
/// Read from the instance table in the SDK's device header, which populates a register pointer per
/// mode the instance has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unicomm {
    /// Implements the UART register map, `0x80000` below the instance address.
    pub uart: bool,

    /// Implements the I2C controller register map, `0x60000` below the instance address.
    pub i2c_controller: bool,

    /// Implements the I2C target register map, `0x40000` below the instance address.
    pub i2c_target: bool,

    /// Implements the SPI register map, `0x20000` below the instance address.
    pub spi: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeripheralPin {
    /// The name of the pin that this peripheral can be bound to.
    ///
    /// e.g. `PA0`, `PC8`
    pub pin: String,

    /// The signal provided by the peripheral.
    ///
    /// e.g. `SCL`, `TX`
    pub signal: String,

    /// The pin function value for this pin that selects the signal
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pf: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interrupt {
    pub name: String,
    pub num: i32,
    pub group: BTreeMap<u32, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmaChannel {
    /// Whether this is a full channel or basic channel.
    pub full: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// The memory partition.
    pub name: String,

    /// Amount of memory in KB.
    pub length: u32,

    /// Address of the memory.
    pub address: u32,
}
