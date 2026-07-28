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
    pub wakeup: bool,
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
