#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Metadata {
    pub name: &'static str,
    // pub memory: &'static [MemoryRegion],
    pub peripherals: &'static [Peripheral],
    pub pincm_mappings: &'static [PinCmMapping],
    // pub nvic_priority_bits: Option<u8>,
    pub interrupts: &'static [Interrupt],
    // pub dma_channels: &'static [DmaChannel],
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Peripheral {
    pub name: &'static str,
    pub kind: &'static str,
    pub pins: &'static [PeripheralPin],
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct PinCmMapping {
    pub pin: &'static str,
    pub pincm: u8,
}

// This feels wrong here...
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct PeripheralPin {
    pub pin: &'static str,
    pub signal: &'static str,
    pub pincm: Option<u8>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Interrupt {
    pub name: &'static str,
    pub number: u32,
}
