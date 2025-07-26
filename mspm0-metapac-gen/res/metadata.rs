#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Metadata {
    pub name: &'static str,
    pub family: &'static str,
    // pub memory: &'static [MemoryRegion],
    pub peripherals: &'static [Peripheral],
    pub pins: &'static [Pin],
    // pub nvic_priority_bits: Option<u8>,
    pub interrupts: &'static [Interrupt],
    pub interrupt_groups: &'static [InterruptGroup],
    pub dma_channels: &'static [DmaChannel],
    pub adc_memctl: u8,
    pub adc_vrsel: u8,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Peripheral {
    pub name: &'static str,
    pub kind: &'static str,
    pub version: Option<&'static str>,
    pub pins: &'static [PeripheralPin],
    pub power_domain: PowerDomain,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Pin {
    pub pin: &'static str,
    pub pincm: u8,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct PeripheralPin {
    pub pin: &'static str,
    pub signal: &'static str,
    pub pf: Option<u8>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
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

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Interrupt {
    pub name: &'static str,
    pub number: u32,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct InterruptGroup {
    pub name: &'static str,
    pub number: u32,
    pub interrupts: &'static [GroupInterrupt],
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct GroupInterrupt {
    pub name: &'static str,
    pub number: u32,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct DmaChannel {
    /// The number of the dma channel.
    pub number: u8,

    /// Whether this is a full or basic dma channel.
    pub full: bool,
}
