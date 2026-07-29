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
    ///
    /// `None` when sysconfig does not describe wakeup logic for this family, which is not the same as
    /// the family having no wake-capable pin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wakeup_pins: Option<BTreeSet<String>>,

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
    pub block_async: Option<bool>,

    /// The deepest mode through which this peripheral keeps its configuration.
    ///
    /// [`PowerMode::Standby`] means the configuration survives everything short of SHUTDOWN;
    /// [`PowerMode::Sleep`] means it is already gone in STOP, so the peripheral must be fully
    /// reconfigured on wake, either by re-running its init or by saving and restoring its registers
    /// around the low-power mode.
    ///
    /// Note that SYSCTL forces *every* PD1 peripheral to a disabled state on entry to STOP or
    /// STANDBY (TRM §2.2.6.1), so a driver has to re-enable the peripheral on wake either way. That
    /// is a property of PD1 rather than of any one peripheral, and is not encoded here.
    ///
    /// `None` when `power_domain` is not [`PowerDomain::Pd1`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retained_through: Option<PowerMode>,

    /// The deepest mode in which the datasheet says this peripheral can be used.
    ///
    /// Derived from the same table as [`Peripheral::retained_through`], reading `EN` and `OPT` as
    /// usable and `DIS`, `OFF` and `NS` as not. `NS` matters here: it means the peripheral is not
    /// automatically disabled but its use in that mode is unsupported, which a boolean would hide.
    ///
    /// `None` where the table cannot answer at the resolution it can be read — either the row is
    /// not uniform across the policies of a mode group, or it gives one value spanning every column.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usable_through: Option<PowerMode>,

    /// Whether this timer keeps receiving ULPCLK or LFCLK in STANDBY1.
    ///
    /// STANDBY1 unclocks all of PD0 except a handful of general purpose timers, so these are the
    /// only timers which can wake the core from the deepest sleep. `None` for peripherals which are
    /// not timers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clocked_in_standby1: Option<bool>,
}

/// An operating mode, ordered from shallowest to deepest.
///
/// Ordering is meaningful and is what makes retention comparable: a peripheral retained through
/// [`PowerMode::Standby`] is also retained in every shallower mode, so a consumer can ask
/// `retained_through >= PowerMode::Stop` rather than enumerating cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PowerMode {
    Run,
    Sleep,
    Stop,
    Standby,

    /// Nothing but the `SHUTDNSTORE` bytes in SYSCTL survives this, so it appears only for
    /// non-volatile memory.
    Shutdown,
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

    /// The deepest mode through which the contents of this partition survive.
    ///
    /// Flash is non-volatile, so it is [`PowerMode::Shutdown`]. SRAM is normally
    /// [`PowerMode::Standby`], since only the `SHUTDNSTORE` bytes in SYSCTL survive SHUTDOWN. The
    /// upper SRAM bank of the parts which have one is the exception.
    pub retained_through: PowerMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryKind {
    Flash,
    Ram,
}
