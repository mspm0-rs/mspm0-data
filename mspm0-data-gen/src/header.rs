use std::{collections::BTreeMap, fs, path::Path, sync::LazyLock};

use anyhow::Context;
use regex::Regex;

#[derive(Debug)]
pub struct Headers {
    pub headers: BTreeMap<String, Header>,
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
            let name = header.components().last().unwrap().as_os_str();
            let name = name.to_string_lossy();

            let name = name.split(".h").next().unwrap();
            headers.insert(name.to_string(), Header::read(name, &header)?);
        }

        Ok(Self { headers })
    }
}

#[derive(Debug)]
pub struct Header {
    pub peripheral_addresses: BTreeMap<String, u32>,

    pub irq_numbers: BTreeMap<i32, Vec<String>>,
    // TODO: flash info
    // TODO: Available IOMUX indices
    // TODO: PF values (for non-analog)
    // TODO: DMA triggers (used for dma transfers)
}

impl Header {
    fn read(chip_name: &str, path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let peripheral_addresses = Self::get_peripheral_addresses(chip_name, &content)?;

        // MSPM0 is a bit cursed in that multiple peripheral's interrupts exist under one IRQ.
        // The Cortex-M0 only has 32 IRQs. This means that "interrupt groups" need to be resolved
        // for truly handling IRQs.
        let irq_numbers = Self::get_irq_numbers(chip_name, &content)?;

        Ok(Self {
            peripheral_addresses,
            irq_numbers,
        })
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
