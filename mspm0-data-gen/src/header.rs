use std::{collections::BTreeMap, fs, path::Path, sync::LazyLock};

use anyhow::Context;
use mspm0_data_types::Unicomm;
use regex::Regex;

#[derive(Debug)]
pub struct Headers {
    /// Keyed by family name, lowercase (e.g. `mspm0g350x`).
    pub files: BTreeMap<String, Header>,
}

impl Headers {
    pub fn parse(data_sources: &Path) -> anyhow::Result<Self> {
        let header_path = data_sources
            .join("mspm0-sdk")
            .join("source")
            .join("ti")
            .join("devices")
            .join("msp")
            .join("m0p");

        let mut headers = BTreeMap::new();

        for header in glob::glob(&format!("{}/mspm0*.h", header_path.display())).unwrap() {
            let header = header.unwrap();
            // Two assignments to make the borrow checker happy
            let name = header.components().next_back().unwrap().as_os_str();
            let name = name.to_string_lossy();

            let name = name.split(".h").next().unwrap();
            headers.insert(name.to_string(), Header::read(name, &header)?);
        }

        Ok(Self { files: headers })
    }
}

#[derive(Debug)]
pub struct Header {
    pub peripheral_addresses: BTreeMap<String, u32>,

    /// Which register maps each UNICOMM instance implements, keyed by instance name.
    pub unicomm_modes: BTreeMap<String, Unicomm>,

    pub irq_numbers: BTreeMap<i32, Vec<String>>,

    /// Number of bits the NVIC uses for interrupt priority levels.
    pub nvic_priority_bits: u8,

    /// The flash-controller facts the header states as `FLASHCTL_SYS_*` constants.
    pub flash: HeaderFlash,

    /// Whether the DMA implements 128-bit transfers, from `DMA_SYS_MMR_LLONG`.
    ///
    /// The constant is defined, always as `1`, exactly on the devices whose datasheet gives the
    /// "Long long (128-bit) transfer" row a tick. driverlib gates `DL_DMA_WIDTH_LONG_LONG` on the
    /// same constant.
    pub dma_long_long: bool,
    // TODO: Available IOMUX indices
    // TODO: PF values (for non-analog)
    // TODO: DMA triggers (used for dma transfers)
}

/// The `FLASHCTL_SYS_*` constants and `__MSPM0_HAS_ECC__`, from the per-device header.
///
/// Deliberately not taken from the SVDs: the H321X SVD describes `CMDWEPROTA` although the part's
/// header gives it zero width, the same superset-IP pattern as elsewhere.
#[derive(Debug, Clone, Copy)]
pub struct HeaderFlash {
    /// Bits the widest single program command writes. 64 everywhere except the G518x's 128, which
    /// programs two flash words at once — the flash word itself is 64 bits on every device, and
    /// the header's own comment calling this the word width is wrong.
    pub datawidth_bits: u8,

    /// Implemented bits in `CMDWEPROTA`, one sector each. 0 means the register does not exist and
    /// `CMDWEPROTB` alone protects MAIN memory; 16 on the C110x, 32 on the other older families.
    pub weprota_bits: u8,

    /// Implemented bits in `CMDWEPROTB`. Eight sectors each on parts with `CMDWEPROTA` (covering
    /// the space above its 32 sectors), and the whole of MAIN at eight sectors per bit without it.
    pub weprotb_bits: u8,

    /// Implemented bits in `CMDWEPROTC`. 0 on every current part.
    pub weprotc_bits: u8,

    /// Whether the flash carries ECC (a 72-bit total word). Absent from the C, H321x and
    /// L110x/L130x/L134x headers, matching their datasheets' hedged "on devices with ECC".
    pub has_ecc: bool,
}

impl Header {
    fn read(chip_name: &str, path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let peripheral_addresses = Self::get_peripheral_addresses(chip_name, &content)?;

        // MSPM0 is a bit cursed in that multiple peripheral's interrupts exist under one IRQ.
        // The Cortex-M0 only has 32 IRQs. This means that "interrupt groups" need to be resolved
        // for truly handling IRQs.
        let irq_numbers = Self::get_irq_numbers(chip_name, &content)?;
        let nvic_priority_bits = Self::get_nvic_priority_bits(chip_name, &content)?;
        let unicomm_modes = Self::get_unicomm_modes(&content);
        let flash = Self::get_flash(chip_name, &content)?;

        Ok(Self {
            peripheral_addresses,
            unicomm_modes,
            irq_numbers,
            nvic_priority_bits,
            flash,
            dma_long_long: content.contains("DMA_SYS_MMR_LLONG"),
        })
    }

    /// Read the `FLASHCTL_SYS_*` constants and the `__MSPM0_HAS_ECC__` flag.
    fn get_flash(chip_name: &str, content: &str) -> anyhow::Result<HeaderFlash> {
        /// Example:
        /// ```c,no_run
        /// #define FLASHCTL_SYS_DATAWIDTH                        (64)      /* !< Data bit width ... */
        /// ```
        static CONSTANT: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?m)#define\s+FLASHCTL_SYS_(?<name>\w+)\s+\((?<value>\d+)\)").unwrap()
        });

        let mut constants = BTreeMap::new();
        for capture in CONSTANT.captures_iter(content) {
            let value: u8 = capture["value"]
                .parse()
                .context(format!("{chip_name}: FLASHCTL_SYS_{} overflows", &capture["name"]))?;
            constants.insert(capture["name"].to_string(), value);
        }

        let get = |name: &str| {
            constants
                .get(name)
                .copied()
                .context(format!("{chip_name}: no FLASHCTL_SYS_{name} in header"))
        };

        Ok(HeaderFlash {
            datawidth_bits: get("DATAWIDTH")?,
            weprota_bits: get("WEPROTAWIDTH")?,
            weprotb_bits: get("WEPROTBWIDTH")?,
            weprotc_bits: get("WEPROTCWIDTH")?,
            has_ecc: content.contains("__MSPM0_HAS_ECC__"),
        })
    }

    /// Read which register maps each UNICOMM instance implements.
    ///
    /// A UNICOMM instance is a UART, an SPI, an I2C controller or an I2C target depending on
    /// `IPMODE.SELECT`, but no instance implements all four. The header's own instance table is the
    /// only source which says which: it initializes a pointer per map the instance has and leaves
    /// the rest null.
    ///
    /// ```c,no_run
    /// static const UNICOMM_Inst_Regs UC4_Inst = {
    ///     .inst      = (UNICOMM_Regs *) UC4_BASE,
    ///     .uart      = (UNICOMMUART_Regs *) UC4_UART_BASE,
    ///     .spi       = (UNICOMMSPI_Regs *) UC4_SPI_BASE,
    ///     .fixedMode = false
    /// };
    /// ```
    ///
    /// `fixedMode` is not read: it is true exactly when one map is initialized, which the maps
    /// themselves already say.
    fn get_unicomm_modes(content: &str) -> BTreeMap<String, Unicomm> {
        static INSTANCE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?s)UNICOMM_Inst_Regs\s+(?<instance>\w+)_Inst\s*=\s*\{(?<body>[^}]*)\}")
                .unwrap()
        });

        INSTANCE
            .captures_iter(content)
            .map(|capture| {
                let body = &capture["body"];
                let modes = Unicomm {
                    uart: body.contains(".uart"),
                    i2c_controller: body.contains(".i2cc"),
                    i2c_target: body.contains(".i2ct"),
                    spi: body.contains(".spi"),
                };

                (capture["instance"].to_string(), modes)
            })
            .collect()
    }

    /// Read the NVIC priority bit count from the CMSIS device header.
    ///
    /// Deliberately *not* taken from the SVDs, whose `<nvicPrioBits>` says 3 for every MSPM0 and is
    /// wrong; the header and the datasheets agree on 2.
    fn get_nvic_priority_bits(chip_name: &str, content: &str) -> anyhow::Result<u8> {
        /// Example:
        /// ```c,no_run
        /// #define __NVIC_PRIO_BITS        0x0002U    /* Number of bits used for Priority Levels */
        /// ```
        static NVIC_PRIO_BITS: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?m)#define\s+__NVIC_PRIO_BITS\s+0x(?<bits>\w+)U").unwrap()
        });

        let capture = NVIC_PRIO_BITS
            .captures(content)
            .context(format!("{chip_name}: no __NVIC_PRIO_BITS in header"))?;

        u8::from_str_radix(&capture["bits"], 16)
            .context(format!("{chip_name}: __NVIC_PRIO_BITS is not a valid u8"))
    }

    fn get_peripheral_addresses(
        chip_name: &str,
        content: &str,
    ) -> anyhow::Result<BTreeMap<String, u32>> {
        /// Example:
        /// ```c,no_run
        /// #define DEBUGSS_BASE                   (0x400C7000U)
        /// ```
        ///
        /// peripheral = `DEBUGSS`, address = `400C7000`
        static PERIPHERAL_BASE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?m)#define\s+(?<peripheral>\w+)_BASE\s+\(0x(?<address>\w+)U\)").unwrap()
        });

        let mut peripherals = BTreeMap::new();

        for capture in PERIPHERAL_BASE.captures_iter(content) {
            let peripheral = capture
                .name("peripheral")
                .context(format!("{chip_name}: capture group failed to resolve peripheral name for peripheral address"))?;

            let address = capture.name("address").context(format!(
                "{chip_name}: could not resolve address for {}",
                peripheral.as_str()
            ))?;

            let address = u32::from_str_radix(address.as_str(), 16).context(format!(
                "{chip_name}: address for {} is not valid u32",
                peripheral.as_str()
            ))?;

            peripherals.insert(peripheral.as_str().to_string(), address);
        }

        assert!(
            !peripherals.is_empty(),
            "{chip_name}: no matches in header for peripherals and addresses"
        );

        Ok(peripherals)
    }

    fn get_irq_numbers(
        chip_name: &str,
        content: &str,
    ) -> anyhow::Result<BTreeMap<i32, Vec<String>>> {
        /// Example:
        /// ```c,no_run
        /// GPIOB_INT_IRQn              = 1,
        /// ```
        ///
        /// name = `GPIOB`, number = `1`
        static IRQ_N: LazyLock<Regex> = LazyLock::new(|| {
            // Lazy regex (**U**ngreedy) is needed to avoid having `_INT` become part of the
            // <name> capture group if present.
            Regex::new(r"(?mU)^\s+(?<name>\w+)(?:_INT)?_IRQn\s+=\s+(?<number>-?\w+),").unwrap()
        });

        let mut irqs = BTreeMap::<i32, Vec<String>>::new();

        for capture in IRQ_N.captures_iter(content) {
            let name = capture.name("name").context(format!(
                "{chip_name}: capture group failed to resolve irq name"
            ))?;

            let number = capture.name("number").context(format!(
                "{chip_name}: could not resolve irq number for {}",
                name.as_str()
            ))?;

            let number = number.as_str().parse::<i32>().context(format!(
                "{chip_name}: irq number for {} is not valid i32",
                name.as_str()
            ))?;

            irqs.entry(number)
                .or_default()
                .push(name.as_str().to_string());
        }

        assert!(
            !irqs.is_empty(),
            "{chip_name}: no matches in header for irq numbers"
        );

        Ok(irqs)
    }
}
