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
    pub adc_memctl: u8,
    pub adc_vrsel: u8,

    /// Maximum frequency of MCLK, in Hz.
    ///
    /// MCLK sources the CPU and the PD1 peripherals.
    pub max_mclk_hz: u32,

    /// Maximum frequency of ULPCLK, in Hz.
    ///
    /// ULPCLK sources the PD0 peripherals. This is the ceiling in RUN and SLEEP; entering STOP
    /// throttles ULPCLK to 4MHz and STANDBY to 32kHz on every device.
    pub max_ulpclk_hz: u32,

    /// Whether the chip has an independent `VBAT` supply and therefore a real backup power domain
    /// (PDB).
    ///
    /// Peripherals in the backup power domain are the only wake sources which survive SHUTDOWN.
    pub backup_domain: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Peripheral {
    pub name: &'static str,
    pub kind: &'static str,
    pub version: Option<&'static str>,
    pub pins: &'static [PeripheralPin],
    pub power_domain: PowerDomain,
    pub sys_fentries: Option<usize>,

    /// The interrupt raised by this peripheral, if it has one.
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
