use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

use mspm0_data_types::Chip;

pub fn generate_device_x(name: &str, chip: &Chip, out_dir: &Path) -> anyhow::Result<()> {
    let dir = out_dir.join("src/chips").join(&name);
    fs::create_dir_all(&dir)?;

    let path = dir.join("device.x");
    let mut file = File::create(&path)?;

    for (_, interrupt) in chip.interrupts.iter() {
        if interrupt.num < 0 {
            continue;
        }

        writeln!(file, "PROVIDE({} = DefaultHandler);", interrupt.name)?;
    }

    drop(file);
    Ok(())
}
