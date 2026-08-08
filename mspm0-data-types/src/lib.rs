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

    /// Frequency SYSOSC runs at in its factory trimmed base mode (`SYSOSCCFG.FREQ = 0`), in Hz.
    ///
    /// This is the rate the device boots at, and the rate SYSOSC returns to when a peripheral raises
    /// an asynchronous fast clock request. It does not follow [`Chip::max_mclk_hz`]: the two happen
    /// to agree on the one part where the base rate is not 32MHz.
    ///
    /// The fixed low-power operating point (`SYSOSCCFG.FREQ = 1`) is not described separately: it is
    /// 4MHz on every device whose datasheet specifies one, and whether it exists at all is what
    /// [`ClockTree::stop1`] says.
    pub sysosc_base_hz: u32,

    /// MCLK ceiling, in Hz, for each `MCLKCFG.FLASHWAIT` setting, starting at zero wait states.
    ///
    /// `[24_000_000, 48_000_000, 80_000_000]` means zero wait states up to 24MHz, one up to 48MHz
    /// and two up to 80MHz. A single entry means the device's MCLK ceiling is within the zero wait
    /// state band, so software never has a reason to raise `FLASHWAIT`.
    ///
    /// SYSCTL manages wait states on its own unless MCLK is sourced from a high speed clock, which
    /// is the case where a consumer has to program them.
    pub flash_wait_hz: Vec<u32>,

    /// Whether the chip has an independent `VBAT` supply and therefore a real backup power domain
    /// (PDB).
    ///
    /// Peripherals in the backup power domain are the only wake sources which survive SHUTDOWN.
    pub backup_domain: bool,

    /// Which clock sources and dividers this device's SYSCTL provides.
    pub clock_tree: ClockTree,

    /// Errata which apply to this device, by TI's identifier (`GPIO_ERR_01`, `UART_ERR_03`, ...).
    ///
    /// The functional advisories of the family's errata sheet, meaning the ones TI describes as
    /// affecting "the device's operation, function, or parametrics". The preprogrammed-software,
    /// debug-only and fixed-by-compiler advisories are not here.
    ///
    /// An erratum is listed when any silicon revision is affected, since a consumer built for a part
    /// has to run on whichever revision it meets.
    pub errata: Vec<String>,

    /// How long this device takes to reach RUN from each sleep mode.
    pub wake_ns: WakeTimes,
}

/// Time to reach RUN from each sleep mode, in nanoseconds.
///
/// This is what decides whether a sleep is worth entering: a mode whose wake-up costs more than the
/// time left before the next deadline is not usable for that deadline.
///
/// The sub-modes are named as the datasheet's wake-up timing table names them, which is also how
/// `SYSCTL` describes them: STOP0/1/2 and STANDBY0/1 are selected by different register fields and
/// have measurably different costs.
///
/// Every figure is **typical, not a guaranteed ceiling**. The datasheets give one unqualified number
/// per mode, in a cell spanning their MIN, TYP and MAX columns, so there is no worst case to report.
/// A consumer needing a margin has to add its own.
///
/// `None` means the datasheet has no figure: either the device does not have that mode, or the figure
/// is given in CPU cycles rather than a time, which is how several state SLEEP0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WakeTimes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep0: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep1: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep2: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop0: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop1: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop2: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standby0: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standby1: Option<u32>,

    /// SHUTDOWN is a reset rather than a wake, so this is a boot time. Where the datasheet gives it
    /// for fast boot both enabled and disabled, the slower figure is the one recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shutdown: Option<u32>,
}

/// The clock sources and dividers a device provides.
///
/// These are presence questions with a definite answer, so they are `bool` rather than
/// `Option<bool>`.
///
/// Do not expect a consumer to be able to derive these from the SYSCTL version. mspm0c110x and
/// mspm0c1105_c1106 share `sysctl_c110x` but only the latter has a high frequency crystal driver, and
/// mspm0l112x and mspm0l211x share `sysctl_l122x_l222x` but have no STOP1 where mspm0l122x does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockTree {
    /// Has a high frequency crystal driver (`HSCLKEN.HFXTEN`, `HFXIN`/`HFXOUT` pins).
    pub hfxt: bool,

    /// Has an external digital HFCLK input (`HSCLKEN.USEEXTHFCLK`, `HFCLKIN` pin).
    ///
    /// Separate from [`ClockTree::hfxt`]: mspm0c110x accepts a digital HFCLK but has no crystal
    /// driver.
    pub hfclk_in: bool,

    /// The range HFCLK must stay within, from the datasheet's `fHFXT` and `fHFIN`.
    ///
    /// Both paths share one range on every device which specifies them, so this covers a crystal and
    /// a digital input alike. It is **not** the SYSPLL reference range: `fSYSPLLREF` is 4-48MHz on
    /// every device with a SYSPLL, which HFCLK is only on the G families.
    ///
    /// `None` where the datasheet gives no figure: the families with no HFCLK path, and
    /// mspm0c110x, which has an `HFCLKIN` pin but no `fHFIN` row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hfclk_hz: Option<ClockRange>,

    /// Has a low frequency crystal driver (`LFXTCTL.SETUSELFXT`, `LFXIN`/`LFXOUT` pins).
    pub lfxt: bool,

    /// Has an external digital LFCLK input (`EXLFCTL.SETUSEEXLF`, `LFCLKIN` pin).
    pub lfclk_in: bool,

    /// Has a SYSPLL, and therefore something other than HFCLK to select as HSCLK.
    ///
    /// Where this is `false` there is nothing to choose between, but that is **not** the same as
    /// there being no `HSCLKCFG.HSCLKSEL` field: only mspm0c110x and the l110x/l130x/l134x families
    /// lack one. The rest still have the field, still reset it to the SYSPLL position they cannot
    /// use, and still require software to write `HFCLKCLK` before MCLK can run from HSCLK - the
    /// TRMs say to set the bit (SLAU893 2.7, SLAU923, SLAU847), and TI's own SVDs document no
    /// encoding for the reset value on those parts.
    pub syspll: bool,

    /// Has `MCLKCFG.UDIV`, the MCLK to ULPCLK divider.
    ///
    /// Only the devices whose ULPCLK ceiling is below their MCLK ceiling have one.
    pub ulpclk_div: bool,

    /// Has the STOP1 sub-mode, in which SYSOSC drops to 4MHz rather than stopping
    /// (`SYSOSCCFG.USE4MHZSTOP`).
    ///
    /// Where this is `false` the device's STOP mode is only STOP0 and STOP2.
    pub stop1: bool,
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

    /// The interrupts raised by this peripheral.
    ///
    /// Usually one, and empty for a peripheral which raises none. A peripheral can have several: the
    /// MSPM33 parts route a peripheral's interrupt outputs to more than one NVIC line, so the HSADC
    /// has five. Note this is the opposite multiplicity to an `INT_GROUP`, where several peripherals
    /// share one line and are told apart by [`PeripheralInterrupt::group_iidx`]; the two can coexist.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub interrupts: Vec<PeripheralInterrupt>,

    /// Whether this peripheral instance has its own `CLKCFG.BLOCKASYNC` bit, masking the asynchronous
    /// fast clock request it raises.
    ///
    /// `None` when no SVD is published for the family. `mspm0-metapac-gen/res/metadata.rs` documents
    /// what `false` does not mean.
    pub block_async: Option<bool>,

    /// The deepest mode through which this peripheral keeps its configuration.
    ///
    /// [`PowerMode::Standby`] means the configuration survives everything short of SHUTDOWN;
    /// [`PowerMode::Sleep`] means it is already gone in STOP, so the peripheral must be fully
    /// reconfigured on wake. All PD1 peripherals need re-enabling after STOP or STANDBY regardless.
    ///
    /// `None` when `power_domain` is not [`PowerDomain::Pd1`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retained_through: Option<PowerMode>,

    /// The deepest mode in which the datasheet says this peripheral can be used.
    ///
    /// Derived from the same table as [`Peripheral::retained_through`], reading `EN` and `OPT` as
    /// usable and `DIS`, `OFF` and `NS` as not.
    ///
    /// `None` when the row does not resolve to a single mode: either its values differ between the
    /// policies within a mode group, or one value spans every column.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usable_through: Option<PowerMode>,

    /// Whether this timer keeps receiving ULPCLK or LFCLK in STANDBY1.
    ///
    /// STANDBY1 unclocks all of PD0 except a handful of general purpose timers, so these are the
    /// only timers which can wake the core from the deepest sleep. `None` for peripherals which are
    /// not timers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clocked_in_standby1: Option<bool>,

    /// What this timer instance can do.
    ///
    /// `None` for peripherals which are not timers, and for timers of a family which publishes no
    /// SVD and has no entry in `data/timers.yaml`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timer: Option<Timer>,

    /// The range this peripheral's functional clock input must stay within.
    ///
    /// For the ADC this is `fADCCLK`, the rate of the source selected by `CLKCFG.SAMPCLK` before
    /// `CTL0.SCLKDIV` divides it down. For the TRNG it is `TRNGCLKF`, the rate reaching the module
    /// after `CLKDIV.RATIO`.
    ///
    /// `None` where the datasheet specifies no such range for the peripheral, which is every
    /// peripheral other than those two so far.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_range_hz: Option<ClockRange>,

    /// What this ADC instance provides.
    ///
    /// `None` for peripherals which are not ADCs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adc: Option<Adc>,

    /// Which register maps this UNICOMM instance implements.
    ///
    /// `None` for peripherals which are not UNICOMM instances.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unicomm: Option<Unicomm>,
}

/// The parts of one ADC instance which the single `adc_v1` register block does not describe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adc {
    /// Number of configurable conversion channels (`MEMCTL`).
    pub memctl: u8,

    /// Number of options `CTL2.VRSEL` accepts.
    pub vrsel: u8,
}

/// An inclusive frequency range, in Hz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockRange {
    pub min_hz: u32,
    pub max_hz: u32,
}

/// The capabilities of one timer instance.
///
/// Every MSPM0 timer shares the `tim_v1` register block, so the generated PAC says nothing about
/// which of these an instance actually implements. Instances differ widely: `TIMA0` has deadband
/// insertion and a fault handler, `TIMG12` is 32-bit with no prescaler, and `TIMB0` is a bare
/// counter with no capture/compare at all.
///
/// Capability does not follow the instance name, so a consumer must not key a table on it: `TIMG2`
/// has two capture/compare channels on mspm0l110x, and sysconfig gives the mspm0l112x one the same
/// `SYS_FLAVOR` as `TIMA0`.
///
/// Read from the datasheet's TIMx configuration table by `tools/timers.py`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timer {
    /// Counter width in bits, either 16 or 32.
    pub bits: u8,

    /// Independent counters the instance implements.
    ///
    /// One on every general-purpose and advanced timer. The basic timers are a counter array and
    /// are the only instances where this is not 1: four counters on the G-series `TIMBx`, two on
    /// the L-series ones. The `tim_btimer` register block addresses the eight the TRM documents, so
    /// this is what says which of those exist.
    ///
    /// Unlike the rest of this struct it does not come from `data/timers`: the datasheets state it
    /// only as a feature of `TIMBx` in general, in wording which is identical on devices that
    /// disagree. It is sysconfig's `SYS_NUM_COUNTERS`, which is per instance.
    #[serde(default = "one")]
    pub counters: u8,

    /// Whether the instance has the 8-bit prescaler.
    pub prescaler: bool,

    /// Whether the instance has the repeat counter.
    pub repeat_counter: bool,

    /// Capture/compare channels which have a `CCPx` output.
    ///
    /// Zero for the basic timers (`TIMBx`), which cannot capture or compare at all. The datasheet
    /// calls these the external channels; the compare-only channels behind them (`CC_45`, on the
    /// advanced timers only) are not described here, since only the L-series datasheets say how
    /// many there are.
    pub ccp_channels: u8,

    /// PWM outputs the instance drives, counting a channel's complementary output separately.
    ///
    /// Twice [`Timer::ccp_channels`] on the instances with deadband insertion, since those pair
    /// each channel with a `CCPx_CMPL` output, and equal to it on the rest.
    pub external_pwm_channels: u8,

    /// Whether the instance has the phase load register.
    pub phase_load: bool,

    /// Whether the load register is shadowed.
    pub shadow_load: bool,

    /// Whether the capture/compare registers are shadowed.
    pub shadow_ccs: bool,

    /// Whether the instance has deadband insertion.
    pub deadband: bool,

    /// Whether the instance has a fault handler.
    pub fault_handler: bool,

    /// Whether the instance can decode quadrature and Hall inputs.
    pub qei_hall: bool,
}

/// Default for [`Timer::counters`], which `data/timers` does not carry.
fn one() -> u8 {
    1
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
