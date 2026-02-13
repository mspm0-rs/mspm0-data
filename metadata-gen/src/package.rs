use anyhow::Context;
use metadata_types::{Package, PackagePin};
use serde_json::Value;

pub fn get_packages(sysconfig: &Value) -> anyhow::Result<Vec<Package>> {
    let packages_json = sysconfig
        .get("packages")
        .context("no packages entry")?
        .as_object()
        .context("packages is not an object")?;

    let device_pins = sysconfig
        .get("devicePins")
        .context("no devicePins entry")?
        .as_object()
        .context("devicePins is not an object")?;

    let mut packages = Vec::with_capacity(packages_json.len());

    for package in packages_json.values() {
        let name = package
            .get("name")
            .context("package has no name")?
            .as_str()
            .context("package name is not a string")?;

        let package_pins = package
            .get("packagePin")
            .context("package has no packagePin")?
            .as_array()
            .context("packagePin is not an array")?;

        let mut package = Package {
            name: name.into(),
            // TODO: Get a nice package name?
            package: name.into(),
            pins: Vec::with_capacity(package_pins.len()),
        };

        for pin in package_pins {
            let ball = pin
                .get("ball")
                .context("package pin has no ball field")?
                .as_str()
                .context("ball is not a str")?;

            let pin_id = pin
                .get("devicePinID")
                .context("package pin has no devicePinID")?
                .as_str()
                .context("devicePinID is not a str")?;

            let device_pin = device_pins
                .get(pin_id)
                .with_context(|| format!("{pin_id} does not exist"))?;

            let pin_name = device_pin
                .get("name")
                .with_context(|| format!("{pin_id} has no name"))?
                .as_str()
                .with_context(|| format!("{pin_id} name is not a str"))?;

            package.pins.push(PackagePin {
                position: ball.into(),
                // FIXME: Some MSPM0 parts and packages bond two GPIO pins together... Fun...
                signals: vec![fix_pin_signals(pin_name)],
            });
        }

        package.pins.sort_by(|a, b| a.position.cmp(&b.position));
        packages.push(package);
    }

    Ok(packages)
}

fn fix_pin_signals(pin_name: &str) -> String {
    // C1105/6 has a bug where PA30/VCORE is a pin. VCORE and a pin being bonded together does not make sense.
    //
    // The datasheet confirms the pin is just PA30.
    if pin_name.ends_with("/VCORE") {
        return pin_name.strip_suffix("/VCORE").unwrap().into();
    }

    // On MSPMxx OPAx is removed since the signals are resolved later.
    if pin_name.contains("/OPA") {
        return pin_name.split_once("/OPA").unwrap().0.into();
    }

    pin_name.into()
}
