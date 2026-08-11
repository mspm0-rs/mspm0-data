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
    /// `Standby1` means the configuration survives everything short of SHUTDOWN; `Sleep` means it
    /// is already gone by STOP0, so the peripheral must be fully reconfigured on wake. `Sleep` is
    /// the shallowest value this takes, since PD1 is powered in RUN and SLEEP alike.
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

    /// Which extended-UART features this UART implements.
    ///
    /// `None` for peripherals which are neither a UART nor a UNICOMM UART function.
    pub uart: Option<Uart>,

    /// What this OPA instance's input muxes select.
    ///
    /// `None` for peripherals which are not OPAs.
    pub opa: Option<Opa>,

    /// The parts of the VREF instance which the register block does not describe.
    ///
    /// `None` for peripherals which are not the VREF.
    pub vref: Option<Vref>,

    /// The parts of this COMP instance which its register block does not describe.
    ///
    /// `None` for peripherals which are not comparators.
    pub comp: Option<Comp>,

    /// How this flash controller writes and protects its flash.
    ///
    /// `None` for peripherals which are not the FLASHCTL.
    pub flashctl: Option<Flashctl>,

    /// The parts of the SYSCTL which its register block does not describe.
    ///
    /// `None` for peripherals which are not the SYSCTL.
    pub sysctl: Option<Sysctl>,

    /// The parts of the DMA which its register block does not describe.
    ///
    /// `None` for peripherals which are not the DMA.
    pub dma: Option<Dma>,
}

/// The parts of the DMA which its register block does not describe.
///
/// A device-wide statement, not a per-channel one. Where the widest transfer exists it is
/// available on the basic channels as well as the full ones, so [`DmaChannel::full`] answers a
/// different question and cannot stand in for this one.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Dma {
    /// Whether `DMACTL.DMASRCWDTH` and `DMADSTWDTH` honour the 128-bit `LONGLONG` encoding (`4h`).
    /// The other four widths — 8, 16, 32 and 64 bits — exist on every device.
    ///
    /// TI builds the DMA in two variants and only DMA_B has the wide transfer: true on
    /// mspm0c1105_c1106, mspm0g151x, mspm0g351x, mspm0g518x, mspm0h321x, mspm0l112x and
    /// mspm0l211x, false on the other eleven families. What a device without it does with a `4h`
    /// written to either field is documented nowhere, so a driver has to gate on this rather than
    /// try the width and check.
    ///
    /// Three sources state it per device and agree on all 18 families: the header's
    /// `DMA_SYS_MMR_LLONG`, the `LONGLONG` enumerated value in the SVDs which have one, and the
    /// datasheet's "Long long (128-bit) transfer" row (older datasheets carry no such table and
    /// say it in the DMA feature list, which stops at "long word (64-bit)"). Not sysconfig, whose
    /// `DMAChannel.syscfg.js` gates the option on a family list missing four families that have
    /// it.
    pub long_long_transfers: bool,

    /// Whether `DMACTL.AUTOEN` does anything. The field is in the register block on every device,
    /// because one block serves them all, but the older DMA implements no automatic enable and
    /// writing it there is accepted and ignored.
    ///
    /// True on exactly the same seven families as [`Dma::long_long_transfers`] — the two features
    /// arrived together, and the header states them as separate constants
    /// (`DMA_SYS_MMR_AUTO`) which have never disagreed. Recorded separately anyway: nothing says
    /// TI must keep shipping them as a pair.
    ///
    /// Same three sources, agreeing on all 18 families: the header constant, the presence of the
    /// `DMAAUTOEN` field in the SVDs which have one, and the datasheet's "Auto enable" row.
    pub auto_enable: bool,
}

/// The parts of the SYSCTL which its register block does not describe.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Sysctl {
    /// Whether the BOR warning thresholds BOR1–BOR3 exist. When true, `BORTHRESHOLD.LEVEL`
    /// values 1–3 arm an early-warning interrupt above the BOR0 reset level; when false the
    /// device implements only BOR0 and the upper `LEVEL` encodings select nothing the datasheet
    /// documents. False only on the MSPM0H321x so far, whose datasheet publishes no
    /// `VBOR1`–`VBOR3` rows and qualifies its BOR hysteresis "Level 0"; every other family's
    /// datasheet states all three with rising, falling and STANDBY figures. The register
    /// interface cannot say: the field is two bits and the SVDs enumerate four levels
    /// everywhere, and driverlib's per-family enums contradict the datasheets in *both*
    /// directions (`dl_sysctl_mspm0c110x.h` allows only level 0 against the C1104's full table,
    /// `dl_sysctl_mspm0h321x.h` offers all four against the H3216's one).
    ///
    /// A new threshold takes effect only after `BORCLRCMD` is written with its key and `GO` —
    /// `BORTHRESHOLD.LEVEL` alone changes nothing, and no TI code performs the second write.
    /// Confirm via `SYSSTATUS.BORCURTHRESHOLD` after the documented ~15µs; do not poll it in
    /// place of the delay, since it reads the old level until the new one lands and a refused
    /// change would hang the poll. On mspm0l110x/l130x/l134x, `PMCU_ERR_03`: BOR1–3 are not
    /// functional in STANDBY.
    pub bor_warning_levels: bool,
}

/// How the flash controller writes and protects its flash.
///
/// One `flashctl` register block serves every device, so these are the per-device facts a flash
/// driver needs and the block cannot state. They describe the controller: no current device varies
/// its flash geometry per bank, and the sources state them once per device, so a per-region
/// statement would only repeat them.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Flashctl {
    /// Erase granularity in bytes. 1024 on every device so far — stated per device by the
    /// datasheet ("minimum erase resolution of 1KB"), recorded per device so a driver need not
    /// assume TI's portfolio-wide driverlib constant holds for a part it was never checked on.
    pub sector_bytes: u32,

    /// The widest single program command in bytes. 8 on every device except the G518x's 16,
    /// which programs two flash words in one command (TI's `programMemory128` sets
    /// `CMDTYPE.SIZE` to `TWO_WORDS`). Not the minimum programming unit: the flash word is
    /// 8 bytes on every device, the G518x included ("Flash word size is 64 data bits (8 bytes)"
    /// in its own datasheet), and a word can only be programmed once per erase. The value is the
    /// header's `FLASHCTL_SYS_DATAWIDTH`, whose comment miscalls it the flash-word width.
    pub word_bytes: u8,

    /// Implemented bits in `CMDWEPROTA`, each write-protecting one sector of physical bank 0.
    /// 0 means the register does not exist and `CMDWEPROTB` alone protects MAIN memory — TI's
    /// newer scheme; 16 bits on the C110x, 32 on the other older families.
    ///
    /// These registers reset to fully protected on every program or erase completion, so a driver
    /// clears the relevant bit before every operation, not once.
    pub weprota_bits: u8,

    /// Implemented bits in `CMDWEPROTB`, each write-protecting eight sectors. Which eight depends
    /// on more than these widths: on a single-bank part with `CMDWEPROTA` its bit 0 starts above
    /// `CMDWEPROTA`'s 32 sectors, while on a multi-bank part it starts at each bank's base — the
    /// three mask formulas in TI's `dl_flashctl.c` `DL_FlashCTL_unprotectSector` are the
    /// reference, and the bank count is a runtime fact (`FACTORYREGION`), not a metadata one.
    pub weprotb_bits: u8,

    /// Implemented bits in `CMDWEPROTC`. 0 on every current device; carried so the day TI ships
    /// one it is data rather than a schema change.
    pub weprotc_bits: u8,

    /// Whether the flash carries ECC — a 72-bit stored word of 64 data bits plus 8 ECC bits.
    /// False on the C, H321x and L110x/L130x/L134x families, whose datasheets hedge their flash
    /// word footnote with "on devices with ECC" and never state the ECC variant.
    pub has_ecc: bool,
}

/// The parts of one COMP instance which its register block does not describe.
///
/// Both COMP register blocks enumerate every `CTL2.REFSRC` value the IP has ever had, so on its own
/// the PAC offers reference sources some devices do not implement — the same shape as the OPA mux
/// positions, where a selection that does not exist selects nothing and reads as a healthy
/// configuration.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Comp {
    /// Whether `CTL2.REFSRC` positions 5 (`VDDA`), 6 (`INTVREF_DAC`) and 7 (`INTVREF`) select a
    /// source on this instance. The three come and go together; positions 0 through 3 exist on
    /// every comparator, and 4 is reserved everywhere.
    ///
    /// When false, writing 5 through 7 selects no reference. The internal-reference positions are
    /// the only way to a reference in modes where the VREF module is unavailable; when this is
    /// false, `REFSRC` 2 and 3 reach the VREF module's output instead, which follows the VREF
    /// configuration (internal where the device generates one, the `VREF+`/`VREF-` pins otherwise).
    pub int_vref: bool,

    /// Time from `CTL1.ENABLE` in high-speed mode (`CTL1.MODE = 0`) until the comparator meets its
    /// propagation-delay specification, in nanoseconds. Nothing reports this — there is no ready
    /// bit — so waiting it out is the only option. The figure's cell spans the datasheet's MIN,
    /// TYP and MAX columns, so it is a stated figure rather than a guaranteed ceiling; the same
    /// holds for the other three figures here.
    ///
    /// `None` when the family's datasheet has no `ten` row.
    pub enable_fast_ns: Option<u32>,

    /// Time from `CTL1.ENABLE` in low-power mode (`CTL1.MODE = 1`) until the comparator meets its
    /// propagation-delay specification, in nanoseconds. 10us on every device so far, where
    /// high-speed mode reaches its own specification in 5us on the newer-generation comparators.
    ///
    /// `None` when the family's datasheet has no `ten` row.
    pub enable_ulp_ns: Option<u32>,

    /// Time for the 8-bit reference DAC to settle to 1 LSB after a full-scale code change, in
    /// nanoseconds, unloaded. This is the internal path — what the comparator itself, or an OPA
    /// sampling the DAC through its input mux, sees.
    ///
    /// `None` when the family's datasheet has no `tdac_settle` row.
    pub dac_settle_ns: Option<u32>,

    /// The same settling figure with the DAC driven out to a package pin (`CTL1.DACOUTEN`) under
    /// the datasheet's stated load. Stated exactly on the families whose COMP has that bit, and
    /// slower than the internal path — 6us against 1.5us so far.
    ///
    /// `None` when the datasheet does not state the pin-loaded row, including every device whose
    /// DAC cannot reach a pin.
    pub dac_settle_pin_ns: Option<u32>,
}

/// Which extended-UART features a UART instance implements.
///
/// Every legacy UART shares one register block and every UNICOMM UART function shares another, so
/// the block does not say which instance implements these; the datasheet's "UART Features" table
/// does. TI's Extend/Main naming is deliberately not recorded: the UNICOMM UARTs do not nest that
/// way. On MSPM0G518x `UC1` has LIN but no smart card and `UC3` smart card but no LIN, so the
/// features stand alone. A legacy extend instance has all five; a legacy main instance has none.
///
/// The features do not follow the instance name — UART1 is main on mspm0l130x and extend on
/// mspm0l122x — so do not key them on it.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Uart {
    /// LIN mode: the `LINCTL`/`LINCNT`/`LINC0`/`LINC1` registers and their interrupts.
    pub lin: bool,

    /// DALI (IEC 62386) support.
    pub dali: bool,

    /// IrDA encoding and decoding.
    pub irda: bool,

    /// ISO 7816 smart card mode.
    pub iso7816: bool,

    /// The Manchester codec (`CTL0.MENC`).
    pub manchester: bool,
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

    /// Channels hard-wired to an internal signal rather than a package pin, sorted by channel.
    ///
    /// From the datasheet's "ADC Channel Mapping" table. Channels not listed go to package pins or
    /// nowhere. The routing differs per instance and per family -- the OPA0 output is ADC0
    /// channel 13 on mspm0g350x and channel 12 on mspm0l130x -- so do not key it on the instance
    /// name.
    pub internal_channels: &'static [AdcInternalChannel],
}

/// One ADC channel and the internal signal it samples.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct AdcInternalChannel {
    pub channel: u8,
    pub source: AdcInternalSource,
}

/// An internal signal an ADC channel samples instead of a package pin.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum AdcInternalSource {
    /// The temperature sensor. Its single-point calibration value is in `FACTORYREGION`.
    TemperatureSensor,

    /// The OPA0 output.
    Opa0,

    /// The OPA1 output.
    Opa1,

    /// The GPAMP output.
    Gpamp,

    /// The DAC0 output. The channel is shared with a package pin, which cannot sample external
    /// signals while the DAC drives it.
    Dac0,

    /// The internal voltage reference, `VREF` or `VREFINT` in the datasheets. Not the `VREF+`/
    /// `VREF-` pins, which are external and stay in the pin data.
    Vref,

    /// The supply monitor, "Supply/Battery Monitor" in most datasheets.
    SupplyMonitor,

    /// The VBAT backup-supply monitor.
    VbatMonitor,

    /// The VUSB supply monitor.
    VusbMonitor,
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
/// `PowerMode::Standby1` is also retained in every shallower mode, so a consumer can ask
/// `retained_through >= PowerMode::Stop0` rather than enumerating cases.
///
/// STOP and STANDBY are split per sub-mode because each disables a superset of the one before it.
/// RUN and SLEEP are not: their sub-modes are clock-source policies rather than depths, and RUN2
/// runs the CPU with SYSOSC off where the deeper SLEEP0 has it on, so ordering them would be a
/// lie. Not every family has every sub-mode — several have no STOP1 at all.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
pub enum PowerMode {
    Run,
    Sleep,
    Stop0,
    Stop1,
    Stop2,

    /// PD0 is still clocked here, which is what separates it from `PowerMode::Standby1`.
    Standby0,

    /// PD0 is unclocked apart from the handful of timers marked `clocked_in_standby1`, so those
    /// are the only peripherals which can wake the core from here.
    Standby1,

    /// Nothing but the `SHUTDNSTORE` bytes in SYSCTL survives this, so it appears only for
    /// non-volatile memory.
    Shutdown,
}

/// What an OPA instance's three input muxes select at each position, sorted by position.
///
/// The register block is shared, so which positions exist — and which peer instance the cascade
/// positions reach — comes from the datasheet: the "OPAx Input Channel Mapping" tables on the
/// G families, and the "Device Analog Connections" figure on the L families.
///
/// Position 0 is `Open` (deliberately no connection) on every mux of every instance and is not
/// listed. **Any other position absent from its map selects nothing**: the input floats, which on
/// the M-mux means the gain ladder pivots about a floating node and produces plausible-looking
/// wrong results. The known holes are on the L families, which lack `PSEL` 2 (`IN1+`), 3 (DAC12)
/// and 8 (ground), and `MSEL` 3 (DAC12).
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Opa {
    /// The non-inverting input mux, `CFG.PSEL`.
    pub pmux: &'static [OpaMuxEntry],

    /// The inverting input mux, `CFG.NSEL`.
    pub nmux: &'static [OpaMuxEntry],

    /// The gain-ladder bottom mux, `CFG.MSEL`.
    pub mmux: &'static [OpaMuxEntry],
}

/// One connected position of an OPA input mux.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct OpaMuxEntry {
    /// The mux selector value.
    pub position: u8,

    /// What the position selects.
    pub input: OpaInput,
}

/// A source an OPA input-mux position selects.
///
/// Pin polarity follows the mux: on the P-mux `In(0)` is the `OPAx_IN0+` package pin, on the
/// N-mux and M-mux it is the corresponding `-` pin.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum OpaInput {
    /// The instance's own `INn+`/`INn-` package pin; the payload is `n`.
    In(u8),

    /// The 12-bit DAC output. It reaches the mux through the `DAC_OUT` package pin, which is also
    /// the `OPAx_IN2±` input: the datasheets warn against external circuitry on that pin while
    /// the DAC drives the OPA.
    Dac12,

    /// The 8-bit reference DAC of the COMP instance named by the payload. On the G families
    /// `OPAn` is fed by `COMPn`'s DAC; on the L families both OPAs are fed by `COMP0`'s.
    Dac8(u8),

    /// The `VREF+` pin node. The internal reference reaches it only where `Vref::output_to_pin`
    /// is true; an external reference on the pin works regardless.
    VrefPlus,

    /// The gain-ladder top of the OPA instance named by the payload — the cascade connection.
    Rtop(u8),

    /// The gain-ladder bottom of the OPA instance named by the payload — the cascade connection.
    Rbot(u8),

    /// The instance's own gain-ladder tap.
    OwnRtap,

    /// The instance's own gain-ladder top.
    OwnRtop,

    /// The GPAMP output.
    Gpamp,

    /// Ground.
    Ground,
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

    /// Whether the internal reference is buffered out to the `VREF+` package pin.
    ///
    /// True on the G families, whose datasheets state an output drive strength for the pin and
    /// require the decoupling capacitor for internal-reference use. False on the C/H/L families,
    /// where `VREF+` only brings an external reference in — and on devices with no VREF pins at
    /// all. Anything reading the `VREF+` pin node, such as an OPA input mux position routed to it,
    /// sees the internal reference only where this is true; with an external reference driving
    /// the pin it works regardless.
    pub output_to_pin: bool,
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
    /// Flash is non-volatile, so it is `Shutdown`. SRAM is normally `Standby1`, since only the
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

    /// Which IO structure the pin is built from, and so which of its PINCM fields do anything.
    pub structure: IoStructure,
}

/// The IO structure a pin is built from, which decides what its PINCM fields do.
///
/// The IOMUX register is the same for every pin, so a field a pin's structure does not implement
/// is written, read back, and ignored. What each structure implements:
///
/// | structure | `INV` | `DRV` | `HYSTEN` | `PIPU` | `PIPD` | wake |
/// |---|---|---|---|---|---|---|
/// | [`Standard`](IoStructure::Standard), [`StandardLowLeakage`](IoStructure::StandardLowLeakage) | yes | | | yes | yes | |
/// | [`StandardWithWake`](IoStructure::StandardWithWake) | yes | | | yes | yes | yes |
/// | [`HighDrive`](IoStructure::HighDrive) | yes | yes | | yes | yes | yes |
/// | [`HighSpeed`](IoStructure::HighSpeed) | yes | yes | | yes | yes | |
/// | [`OpenDrain`](IoStructure::OpenDrain) | yes | | yes | | yes | yes |
///
/// The table is SLAU846 Table 8-1, and TI's own `GPIOPin.syscfg.js` gates its options by exactly
/// these rules. Three caveats before treating it as complete:
///
/// - **Wake is not derivable from the structure.** On mspm0c110x and msps003fx the open-drain
///   pins have no wakeup logic — sysconfig marks `io_wakeup` false on them, and the C1104
///   datasheet's feature table has no wakeup column at all. Use [`Pin::wakeup`].
/// - **The per-device feature tables are not reliable in either direction.** The MSPM0G3519's
///   omits the open-drain row although its own pin table gives PA0 and PA1 that structure; the
///   MSPM0L2117's carries two low-drive rows although no pin on the device is low-drive; the
///   MSPM0L2117's also leaves high-drive's drive-strength cell empty against every other
///   datasheet, the TRM and TI's own tool; and the MSPM0H3216's marks no structure as having a
///   pulldown. Per-pin data is the reliable part.
/// - **Not every device has every structure**, and no device has all of them.
///
/// The source is sysconfig's per-pin `io_type`, which the datasheets' per-pin tables corroborate
/// with no disagreement on any pin of any family.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum IoStructure {
    /// Standard drive (`SDIO`).
    Standard,

    /// Standard drive, low leakage (`SDL` in sysconfig, which TI's tool calls "Low-leakage
    /// Standard"). The datasheets' pin tables print it as plain standard drive, and every rule in
    /// TI's tool treats the two identically, so the difference is leakage current rather than
    /// anything the IOMUX can express.
    ///
    /// One pin per family on the older G and L families — PA2 everywhere it appears — and every
    /// pin of msps003fx.
    StandardLowLeakage,

    /// Standard drive with wakeup logic (`SDIO` with wake).
    StandardWithWake,

    /// High drive (`HDIO`), the 20mA output.
    HighDrive,

    /// High speed (`HSIO`).
    HighSpeed,

    /// 5V-tolerant open drain (`ODIO`). The only structure with hysteresis control, and the only
    /// one with no pullup: `PIPU` on one of these pins does nothing.
    OpenDrain,

    /// A USB 2.0 full-speed pin (`USBIO`), on mspm0g518x only. Powered from `VUSB33` rather than
    /// `VDD`, and treated as standard drive by TI's tool.
    Usb,
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
