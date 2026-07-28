use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

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

    /// Device pins which have wakeup logic and can therefore wake the device from SHUTDOWN.
    ///
    /// The `FASTWAKE` mechanism, which wakes the device from STOP and STANDBY, works on any GPIO
    /// pin and is therefore not described here.
    pub wakeup_pins: BTreeSet<String>,

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

    /// Number of bits used by the NVIC for interrupt priority levels.
    pub nvic_priority_bits: u8,

    /// Maximum frequency of MCLK, in Hz.
    ///
    /// MCLK sources the CPU and the PD1 peripherals.
    pub max_mclk_hz: u32,

    /// Maximum frequency of ULPCLK, in Hz.
    ///
    /// ULPCLK sources the PD0 peripherals. Note that this is the ceiling in RUN and SLEEP modes;
    /// entering STOP throttles ULPCLK to 4MHz and STANDBY to 32kHz on every device.
    pub max_ulpclk_hz: u32,

    /// Whether the chip has an independent `VBAT` supply and therefore a real backup power domain
    /// (PDB).
    ///
    /// Peripherals in the backup power domain are the only wake sources which survive SHUTDOWN.
    pub backup_domain: bool,
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

    /// The interrupt raised by this peripheral.
    ///
    /// `None` if the peripheral has no interrupt of its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<PeripheralInterrupt>,

    /// Whether this peripheral instance has its own `CLKCFG.BLOCKASYNC` bit.
    ///
    /// An asynchronous fast clock request temporarily suspends a low-power mode and brings MCLK and
    /// ULPCLK back to full rate, which is how a PD0 peripheral wakes the system on an external
    /// event while not being clocked. `BLOCKASYNC` masks the request for one instance and must be
    /// clear to arm such a wake; `SYSCTL.SYSOSCCFG.BLOCKASYNCALL` masks every request at once.
    ///
    /// `false` does not mean the peripheral cannot raise a request: GPIO, the general purpose
    /// timers and the ADC all can, but have no per-instance mask and are gated only by
    /// `BLOCKASYNCALL`. `None` means no SVD is published for this family yet, so the answer is
    /// unknown rather than negative.
    /// This is deliberately per-instance: on mspm0g120x only `UC0`, `UC2`, `UC4`, `UC5` and
    /// `UC9` have the bit despite every `UC` being the same IP.
    pub block_async: Option<bool>,
}

/// The interrupt raised by a peripheral.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeripheralInterrupt {
    /// Name of the NVIC interrupt.
    ///
    /// For a peripheral inside an `INT_GROUP` this is the name of the group (e.g. `GROUP1`), not
    /// the name of the peripheral.
    pub name: String,

    /// Number of the NVIC interrupt.
    pub num: i32,

    /// The peripheral's `IIDX` value within its `INT_GROUP`.
    ///
    /// `None` if the peripheral has an NVIC interrupt of its own rather than sharing a group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_iidx: Option<u32>,
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

    /// What kind of memory this partition is.
    pub kind: MemoryKind,

    /// Amount of memory in KB.
    pub length: u32,

    /// Address of the memory.
    pub address: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryKind {
    Flash,
    Ram,
}
