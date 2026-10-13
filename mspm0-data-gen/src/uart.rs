//! Reads `data/uart/<family>.yaml`, which `tools/uart.py` extracts from the datasheets' "UART
//! Features" tables.
//!
//! The datasheets are the source rather than sysconfig: `SYS_LIN_EN` is `1` on the main UARTs of
//! mspm0g350x and its siblings, which their own datasheet, SVD and driverlib all contradict.
//! sysconfig's `SYS_UARTADV` does agree with every datasheet, and `apply_uart` cross-checks
//! against it. The tool's docs have the details.

use std::collections::BTreeMap;

use mspm0_data_types::Uart;

use crate::util;

/// One family's mapping: UART instance name (`UART0`, `UC4_UART`) to its features.
pub type Uarts = BTreeMap<String, Uart>;

/// Read every `data/uart/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, Uarts>> {
    util::per_family("uart")
}
