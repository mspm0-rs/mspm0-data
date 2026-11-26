use std::collections::{BTreeMap, btree_map::Entry};

use anyhow::{Context, bail};
use serde_json::Value;
use sysconfig_data_gen::msp::PinCmIndices;

use crate::GpnDevice;

pub fn make_pincm_indices(gpn: &GpnDevice, sysconfig_data: &Value) -> anyhow::Result<PinCmIndices> {
    let mut indices = BTreeMap::new();

    let device_pins = sysconfig_data
        .get("devicePins")
        .and_then(Value::as_object)
        .context("no devicePins object")?;

    for device_pin in device_pins.values() {
        let name = device_pin
            .get("name")
            .and_then(Value::as_str)
            .context("read name from device pin")?;

        // Multi-bonded pins are listed separate form elsewhere in this object.
        if name.contains('/') {
            continue;
        }

        let attributes = device_pin
            .get("attributes")
            .and_then(Value::as_object)
            .context("context")?;

        let iomux_pincm = attributes
            .get("iomux_pincm")
            .and_then(Value::as_str)
            .context("context")?;

        // iomux_pincm value is "None" if this is not an I/O pin.
        let Ok(iomux_pincm) = iomux_pincm.parse::<u8>() else {
            continue;
        };

        // The value from sysconfig starts counting at 1, but code uses indexes starting from zero.
        // Subtract 1 to make it a direct mapping for later.

        let Some(iomux_pincm) = iomux_pincm.checked_sub(1) else {
            bail!("for {name}, the iomux_pincm value ({iomux_pincm}) was less than 0");
        };

        let entry = indices.entry(name.into());

        match entry {
            Entry::Vacant(entry) => {
                entry.insert(iomux_pincm);
            }

            // If this is a duplicate it cannot conflict with the existing value.
            Entry::Occupied(entry) => {
                if *entry.get() != iomux_pincm {
                    bail!("multiple conflicting iomux_pincm values for {}", name);
                }
            }
        }
    }

    Ok(indices)
}
