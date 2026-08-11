//! Reads `data/temp_sensor/<family>.yaml`, which `tools/temp_sensor.py` extracts from the
//! datasheets and `data/temp_sensor_overrides.yaml` corrects.
//!
//! `FACTORYREGION.TEMP_SENSE0` is one ADC code per device and means nothing on its own: turning it
//! and a later reading into a temperature needs the trim temperature, the sensor's slope, how long
//! the ADC has to sample, and which reference the factory measured against. None of the four is in
//! a register, a header constant or a sysconfig attribute.

use std::collections::BTreeMap;

use mspm0_data_types::TemperatureSensor;

use crate::util;

/// Read every `data/temp_sensor/<family>.yaml`, keyed by family name.
pub fn parse() -> anyhow::Result<BTreeMap<String, TemperatureSensor>> {
    util::per_family("temp_sensor")
}
