#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Metadata {
    pub name: &'static str,
    pub family: &'static str,
    pub memory: &'static [MemoryRegion],
    pub peripherals: &'static [Peripheral],
    pub pins: &'static [Pin],
    pub nvic_priority_bits: u8,
    pub interrupts: &'static [Interrupt],
    pub interrupt_groups: &'static [InterruptGroup],
    pub dma_channels: &'static [DmaChannel],

    /// Maximum frequency of MCLK, in Hz.
    ///
    /// MCLK sources the CPU and the PD1 peripherals.
    pub max_mclk_hz: u32,

    /// Maximum frequency of ULPCLK, in Hz.
    ///
    /// ULPCLK sources the PD0 peripherals. This is the ceiling in RUN and SLEEP; entering STOP
    /// throttles ULPCLK to 4MHz and STANDBY to 32kHz on every device.
    pub max_ulpclk_hz: u32,

    /// Frequency SYSOSC runs at in its factory trimmed base mode (`SYSOSCCFG.FREQ = 0`), in Hz.
    ///
    /// This is the rate the device boots at, and the rate SYSOSC returns to when a peripheral raises
    /// an asynchronous fast clock request. Do not infer it from `max_mclk_hz`: the two happen to
    /// agree on the one part whose base rate is not 32MHz, and nothing makes them agree in general.
    ///
    /// The fixed low-power operating point (`SYSOSCCFG.FREQ = 1`) is not described separately: it is
    /// 4MHz on every device whose datasheet specifies one, and whether it exists at all is what
    /// `clock_tree.stop1` says.
    pub sysosc_base_hz: u32,

    /// MCLK ceiling, in Hz, for each `MCLKCFG.FLASHWAIT` setting, starting at zero wait states.
    ///
    /// `[24_000_000, 48_000_000, 80_000_000]` means zero wait states up to 24MHz, one up to 48MHz
    /// and two up to 80MHz. A single entry means this device's MCLK ceiling is inside the zero wait
    /// state band, so software never has a reason to raise `FLASHWAIT`.
    ///
    /// SYSCTL manages wait states on its own unless MCLK is sourced from a high speed clock, which
    /// is the case where they have to be programmed.
    pub flash_wait_hz: &'static [u32],

    /// Whether the chip has an independent `VBAT` supply and therefore a real backup power domain
    /// (PDB).
    ///
    /// Peripherals in the backup power domain are the only wake sources which survive SHUTDOWN.
    pub backup_domain: bool,

    /// Which clock sources and dividers this device's SYSCTL provides.
    pub clock_tree: ClockTree,

    /// Errata which apply to this device, by TI's identifier (`GPIO_ERR_01`, `UART_ERR_03`, ...),
    /// sorted.
    ///
    /// These are the functional advisories of the device's errata sheet, the ones TI describes as
    /// affecting "the device's operation, function, or parametrics". The preprogrammed-software,
    /// debug-only and fixed-by-compiler advisories are not listed.
    ///
    /// An erratum is listed when any silicon revision is affected, since a driver built for a part has
    /// to run on whichever revision it meets. It is not a claim that a workaround is needed in every
    /// case -- read the advisory.
    pub errata: &'static [&'static str],

    /// How long this device takes to reach RUN from each sleep mode.
    pub wake_ns: WakeTimes,
}

/// Time to reach RUN from each sleep mode, in nanoseconds.
///
/// This is what decides whether a sleep is worth entering: a mode whose wake-up costs more than the
/// time left before the next deadline is not usable for that deadline.
///
/// The sub-modes are named as SYSCTL describes them, since STOP0/1/2 and STANDBY0/1 are selected by
/// different register fields and have measurably different costs.
///
/// Every figure is **typical, not a guaranteed ceiling**. The datasheets give one unqualified number
/// per mode, in a cell spanning their MIN, TYP and MAX columns, so there is no worst case to report and
/// a consumer needing a margin has to add its own.
///
/// `None` means the datasheet has no figure: either the device does not have that mode, or the figure
/// is given in CPU cycles rather than a time, which is how several state SLEEP0.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct WakeTimes {
    pub sleep0: Option<u32>,
    pub sleep1: Option<u32>,
    pub sleep2: Option<u32>,
    pub stop0: Option<u32>,
    pub stop1: Option<u32>,
    pub stop2: Option<u32>,
    pub standby0: Option<u32>,
    pub standby1: Option<u32>,

    /// SHUTDOWN is a reset rather than a wake, so this is a boot time. Where the datasheet gives it
    /// for fast boot both enabled and disabled, the slower figure is the one recorded.
    pub shutdown: Option<u32>,
}

impl Metadata {
    /// Whether the named erratum applies to this device.
    pub fn has_erratum(&self, erratum: &str) -> bool {
        self.errata.binary_search(&erratum).is_ok()
    }
}

/// The clock sources and dividers a device provides.
///
/// These are presence questions with a definite answer, so they are `bool` rather than
/// `Option<bool>`.
///
/// Do not derive these from the SYSCTL version: mspm0c110x and mspm0c1105_c1106 share
/// `sysctl_c110x` but only the latter has a high frequency crystal driver, and mspm0l112x and
/// mspm0l211x share `sysctl_l122x_l222x` but have no STOP1 where mspm0l122x does.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct ClockTree {
    /// Has a high frequency crystal driver (`HSCLKEN.HFXTEN`, `HFXIN`/`HFXOUT` pins).
    pub hfxt: bool,

    /// Has an external digital HFCLK input (`HSCLKEN.USEEXTHFCLK`, `HFCLKIN` pin).
    ///
    /// Separate from `hfxt`: mspm0c110x accepts a digital HFCLK but has no crystal driver.
    pub hfclk_in: bool,

    /// The range HFCLK must stay within, from the datasheet's `fHFXT` and `fHFIN`.
    ///
    /// Both paths share one range on every device which specifies them, so this covers a crystal and
    /// a digital input alike. **It is not the SYSPLL reference range**: `fSYSPLLREF` is 4-48MHz on
    /// every device that has a SYSPLL, but HFCLK only reaches 48MHz on the G families and stops at
    /// 32MHz on the rest, so one constant cannot serve both checks.
    ///
    /// `None` where the datasheet gives no figure: the families with no HFCLK path, and mspm0c110x,
    /// which has an `HFCLKIN` pin but no `fHFIN` row.
    pub hfclk_hz: Option<ClockRange>,

    /// Has a low frequency crystal driver (`LFXTCTL.SETUSELFXT`, `LFXIN`/`LFXOUT` pins).
    pub lfxt: bool,

    /// Has an external digital LFCLK input (`EXLFCTL.SETUSEEXLF`, `LFCLKIN` pin).
    pub lfclk_in: bool,

    /// Has a SYSPLL, and therefore an `HSCLKCFG.HSCLKSEL` mux.
    ///
    /// Without it HSCLK is HFCLK and there is no mux field to program.
    pub syspll: bool,

    /// Has `MCLKCFG.UDIV`, the MCLK to ULPCLK divider.
    ///
    /// Only the devices whose `max_ulpclk_hz` is below their `max_mclk_hz` have one.
    pub ulpclk_div: bool,

    /// Has the STOP1 sub-mode, in which SYSOSC drops to 4MHz rather than stopping
    /// (`SYSOSCCFG.USE4MHZSTOP`).
    ///
    /// Where this is `false` the device's STOP mode is only STOP0 and STOP2, and a consumer with a
    /// STOP1 of its own has to round it to STOP0 rather than weakening the guard.
    pub stop1: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Peripheral {
    pub name: &'static str,
    pub kind: &'static str,
    pub version: Option<&'static str>,
    pub pins: &'static [PeripheralPin],
    pub power_domain: PowerDomain,
    pub sys_fentries: Option<usize>,

    /// The interrupts raised by this peripheral.
    ///
    /// Usually one, and empty for a peripheral which raises none. A peripheral can have several: the
    /// MSPM33 parts route a peripheral's interrupt outputs to more than one NVIC line, so the HSADC
    /// has five. Note this is the opposite multiplicity to an `INT_GROUP`, where several peripherals
    /// share one line and are told apart by `PeripheralInterrupt::group_iidx`; the two can coexist.
    pub interrupts: &'static [PeripheralInterrupt],

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
    /// `Standby` means the configuration survives everything short of SHUTDOWN; `Sleep` means it is
    /// already gone in STOP, so the peripheral must be fully reconfigured on wake.
    ///
    /// All PD1 peripherals need re-enabling after STOP or STANDBY regardless (TRM §2.2.6.1).
    ///
    /// `None` when `power_domain` is not `Pd1`.
    pub retained_through: Option<PowerMode>,

    /// The deepest mode in which the datasheet says this peripheral can be used.
    ///
    /// From the same table as `retained_through`, reading `EN` and `OPT` as usable and `DIS`, `OFF`
    /// and `NS` as not.
    ///
    /// `None` when the row does not resolve to a single mode.
    pub usable_through: Option<PowerMode>,

    /// Whether this timer keeps receiving ULPCLK or LFCLK in STANDBY1.
    ///
    /// STANDBY1 unclocks all of PD0 except a handful of general purpose timers, so these are the
    /// only timers which can wake the core from the deepest sleep. `None` for peripherals which are
    /// not timers.
    pub clocked_in_standby1: Option<bool>,

    /// What this timer instance can do.
    ///
    /// `None` for peripherals which are not timers.
    pub timer: Option<Timer>,

    /// The range this peripheral's functional clock input must stay within.
    ///
    /// For the ADC this is `fADCCLK`, the rate of the source selected by `CLKCFG.SAMPCLK` before
    /// `CTL0.SCLKDIV` divides it down — not the sampling clock itself. For the TRNG it is
    /// `TRNGCLKF`, the rate reaching the module after `CLKDIV.RATIO`.
    ///
    /// It does not follow anything else here. Two parts can share `max_mclk_hz` and a SYSCTL version
    /// and still differ, and the minimum is not always 4MHz.
    ///
    /// `None` where the datasheet specifies no such range, which is every peripheral other than the
    /// ADC and the TRNG so far.
    pub clock_range_hz: Option<ClockRange>,

    /// What this ADC instance provides.
    ///
    /// `None` for peripherals which are not ADCs.
    pub adc: Option<Adc>,

    /// Which register maps this UNICOMM instance implements.
    ///
    /// `None` for peripherals which are not UNICOMM instances.
    pub unicomm: Option<Unicomm>,

    /// The parts of the VREF instance which the register block does not describe.
    ///
    /// `None` for peripherals which are not the VREF.
    pub vref: Option<Vref>,
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
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
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

/// An inclusive frequency range, in Hz.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct ClockRange {
    pub min_hz: u32,
    pub max_hz: u32,
}

impl ClockRange {
    /// Whether `hz` is within the range.
    pub const fn contains(&self, hz: u32) -> bool {
        self.min_hz <= hz && hz <= self.max_hz
    }
}

/// The parts of one ADC instance which the single `adc_v1` register block does not describe.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Adc {
    /// Number of configurable conversion channels (`MEMCTL`).
    pub memctl: u8,

    /// Number of options `CTL2.VRSEL` accepts.
    pub vrsel: u8,
}

/// The capabilities of one timer instance.
///
/// Every MSPM0 timer shares the `tim_v1` register block, so the `pac` module says nothing about
/// which of these an instance actually implements. Instances differ widely: `TIMA0` has deadband
/// insertion and a fault handler, `TIMG12` is 32-bit with no prescaler, and `TIMB0` is a bare
/// counter with no capture/compare at all.
///
/// Capability does not follow the instance name, so do not key a table on it. `TIMG2` has two
/// capture/compare channels on mspm0l110x, and TI's own metadata gives the mspm0l112x one the same
/// feature tier as `TIMA0`.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Timer {
    /// Counter width in bits, either 16 or 32.
    pub bits: u8,

    /// Independent counters the instance implements.
    ///
    /// One on every general-purpose and advanced timer. The basic timers are a counter array and
    /// are the only instances where this is not 1: four counters on the G-series `TIMBx`, two on
    /// the L-series ones. The `timb` register block addresses the eight the TRM documents, so this
    /// is what says which of those exist.
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
    /// Twice `ccp_channels` on the instances with deadband insertion, since those pair each channel
    /// with a `CCPx_CMPL` output, and equal to it on the rest.
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

/// An operating mode, ordered from shallowest to deepest.
///
/// Ordering is meaningful and is what makes retention comparable: something retained through
/// `PowerMode::Standby` is also retained in every shallower mode, so a consumer can ask
/// `retained_through >= PowerMode::Stop` rather than enumerating cases.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
pub enum PowerMode {
    Run,
    Sleep,
    Stop,
    Standby,

    /// Nothing but the `SHUTDNSTORE` bytes in SYSCTL survives this, so it appears only for
    /// non-volatile memory.
    Shutdown,
}

/// The parts of the VREF instance which the register block does not describe.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Vref {
    /// Time from `CTL0.ENABLE` to a settled reference, in nanoseconds.
    ///
    /// `CTL1.READY` reports this and is the better signal, but `VREF_ERR_01` leaves the bit set once
    /// a buffer has been enabled a first time since reset, so on a device carrying that erratum it
    /// cannot report a later enable and this figure is the only way to know. Check `Metadata::errata`
    /// rather than assuming either way.
    ///
    /// Typical rather than a guaranteed ceiling: the datasheet cell spans its MIN, TYP and MAX
    /// columns. Where a datasheet states the row under several conditions this is the slowest, which
    /// on the G5187 is the 200us figure with a 1uF capacitor on `VREF+` rather than the 20us one
    /// without.
    ///
    /// `None` when the family's datasheet has no `Tstartup` row.
    pub startup_ns: Option<u32>,
}

/// The interrupt raised by a peripheral.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct PeripheralInterrupt {
    /// Name of the NVIC interrupt.
    ///
    /// For a peripheral inside an `INT_GROUP` this is the name of the group (e.g. `GROUP1`), not
    /// the name of the peripheral.
    pub name: &'static str,

    /// Number of the NVIC interrupt.
    pub number: u32,

    /// The peripheral's `IIDX` value within its `INT_GROUP`.
    ///
    /// `None` if the peripheral has an NVIC interrupt of its own rather than sharing a group.
    pub group_iidx: Option<u32>,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct MemoryRegion {
    pub name: &'static str,
    pub kind: MemoryKind,
    pub address: u32,

    /// Size of the region in bytes.
    pub size: u32,

    /// The deepest mode through which the contents of this region survive.
    ///
    /// Flash is non-volatile, so it is `Shutdown`. SRAM is normally `Standby`, since only the
    /// `SHUTDNSTORE` bytes in SYSCTL survive SHUTDOWN.
    pub retained_through: PowerMode,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum MemoryKind {
    Flash,
    Ram,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Pin {
    pub pin: &'static str,
    pub pincm: u8,

    /// Whether the pin has wakeup logic, and can therefore wake the device from SHUTDOWN.
    ///
    /// This is not the same as the pin being able to wake the device at all: `FASTWAKE` works on
    /// any GPIO pin, but only down to STANDBY.
    ///
    /// `None` when the vendor data does not describe wakeup logic for this chip, which is not the
    /// same as `Some(false)`.
    pub wakeup: Option<bool>,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct PeripheralPin {
    pub pin: &'static str,
    pub signal: &'static str,
    pub pf: Option<u8>,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum PowerDomain {
    /// "low speed" power domain. This power domain is powered in RUN, SLEEP, STOP and STANDBY modes.
    Pd0,

    /// "high performance" power domain. This power domain is powered in RUN and SLEEP modes.
    Pd1,

    /// PDB backup power domain. This is usually powered by VBAT.
    ///
    /// Not available on every chip.
    Backup,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Interrupt {
    pub name: &'static str,
    pub number: u32,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct InterruptGroup {
    pub name: &'static str,
    pub number: u32,
    pub interrupts: &'static [GroupInterrupt],
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct GroupInterrupt {
    pub name: &'static str,
    pub number: u32,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct DmaChannel {
    /// The number of the dma channel.
    pub number: u8,

    /// Whether this is a full or basic dma channel.
    pub full: bool,
}
