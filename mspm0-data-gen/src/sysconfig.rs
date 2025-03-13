use std::{collections::BTreeMap, fs, path::Path};

use serde::Deserialize;

#[derive(Debug)]
pub struct Sysconfig {
    pub files: BTreeMap<String, SysconfigFile>,
}

impl Sysconfig {
    pub fn parse(data_sources: &Path) -> anyhow::Result<Self> {
        let mut sysconfig = Sysconfig {
            files: BTreeMap::new(),
        };
        let sysconfigs = data_sources.join("sysconfig");

        for path in glob::glob(&format!("{}/**/MSP*.json", sysconfigs.display())).unwrap() {
            let path = path.unwrap();

            // TODO: file_prefix when stable (only .json, so this is fine)
            let name = path.file_stem().unwrap().to_string_lossy();
            let content = fs::read_to_string(&path).unwrap();

            let file = serde_json::from_str::<SysconfigFile>(&content)?;

            sysconfig.files.insert(name.to_string(), file);
        }

        Ok(sysconfig)
    }
}

// What can I get from a sysconfig file?
// - Pins which belong to a peripheral (by id)
// - PinCM for every pin
// - IO type (useful for kicad)
//
// Related: clocktree.json

#[derive(Debug, Deserialize)]
pub struct SysconfigFile {
    // Ignore `rowColumnInverted` and `rotation`. Not needed.
    // Ignore `cores`. Not used for MSPM0
    #[serde(rename(deserialize = "interfacePins"))]
    pub interface_pins: BTreeMap<String, InterfacePin>,

    pub interfaces: BTreeMap<String, Interface>,

    // TODO: useCases: maybe relevant?
    // Ignore `powerDomains`. Not used for MSPM0. Power domains exist at the peripheral level.
    #[serde(rename(deserialize = "devicePins"))]
    pub device_pins: BTreeMap<String, DevicePin>,

    #[serde(rename(deserialize = "peripheralPins"))]
    pub peripheral_pins: BTreeMap<String, PeripheralPin>,

    /// It's called peripherals but also contains each pin.
    pub peripherals: BTreeMap<String, Peripheral>,

    // TODO: `parts` doesn't seem relevant yet.
    pub packages: BTreeMap<String, Package>,

    // Ignore `reverseMuxes`. Already have `muxes`
    // Ignore `pinCommonInfos`. Always `DUMMY` for MSPM0
    pub muxes: Vec<Muxes>,
}

#[derive(Debug, Deserialize)]
pub struct InterfacePin {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct Interface {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename(deserialize = "showUsePeripheral"))]
    pub show_use_peripheral: bool,
    #[serde(rename(deserialize = "interfacePinWrapper"))]
    pub interface_pin_wrapper: Option<Vec<InterfacePinWrapper>>,
}

#[derive(Debug, Deserialize)]
pub struct InterfacePinWrapper {
    #[serde(rename(deserialize = "interfacePinID"))]
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct DevicePin {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename(deserialize = "devicePinType"))]
    pub device_pin_type: String,
    pub description2: String,
    pub attributes: PinAttributes,
}

#[derive(Debug, Deserialize)]
pub struct PinAttributes {
    pub pad_name: String,
    pub io_type: Option<String>,
    pub iomux_pincm: String,
}

#[derive(Debug, Deserialize)]
pub struct PeripheralPin {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename(deserialize = "interfacePinID"))]
    pub interface_pin_id: String,
}

#[derive(Debug, Deserialize)]
pub struct Peripheral {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Attributes are dynamic and depend on the peripheral type.
    pub attributes: serde_json::Value,
    #[serde(rename(deserialize = "peripheralPinWrapper"))]
    pub peripheral_pin_wrapper: Vec<PeripheralPinWrapper>,
    #[serde(rename(deserialize = "interfaceWrapper"))]
    pub interface_wrapper: Vec<PeripheralInterfaceWrapper>,
}

#[derive(Debug, Deserialize)]
pub struct PeripheralPinWrapper {
    #[serde(rename(deserialize = "peripheralPinID"))]
    pub peripheral_pin_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PeripheralInterfaceWrapper {
    #[serde(rename(deserialize = "interfaceID"))]
    pub interface_id: String,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename(deserialize = "packagePin"))]
    pub package_pins: Vec<PackagePin>,
}

#[derive(Debug, Deserialize)]
pub struct PackagePin {
    #[serde(rename(deserialize = "devicePinID"))]
    pub device_pin_id: String,
    pub ball: String,
    pub description: Option<PackageDescription>,
}

#[derive(Debug, Deserialize)]
pub struct PackageDescription {
    #[serde(rename(deserialize = "type"))]
    pub ty: String,

    #[serde(rename(deserialize = "numRows"))]
    pub rows: u32,

    #[serde(rename(deserialize = "numColumns"))]
    pub columns: u32,
}

#[derive(Debug, Deserialize)]
pub struct Muxes {
    #[serde(rename(deserialize = "devicePinID"))]
    pub device_pin_id: String,

    #[serde(rename(deserialize = "muxSetting"))]
    pub mux_setting: Vec<MuxSetting>,
}

#[derive(Debug, Deserialize)]
pub struct MuxSetting {
    #[serde(rename(deserialize = "peripheralPinID"))]
    pub peripheral_pin_id: String,
    pub mode: String,
}
