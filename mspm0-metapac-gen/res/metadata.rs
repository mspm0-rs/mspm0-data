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
    pub sys_fentries: Option<usize>,

    /// Which register maps this UNICOMM instance implements.
    ///
    /// `None` for peripherals which are not UNICOMM instances.
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
