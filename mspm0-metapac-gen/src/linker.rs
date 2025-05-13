use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

use mspm0_data_types::Chip;

pub fn generate_device_x(name: &str, chip: &Chip, out_dir: &Path) -> anyhow::Result<()> {
    let dir = out_dir.join("src/chips").join(name);
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

pub fn generate_memory_x(name: &str, chip: &Chip, out_dir: &Path) -> anyhow::Result<()> {
    let dir = out_dir.join("src/chips").join(name);
    fs::create_dir_all(&dir)?;

    let path = dir.join("memory.x");
    let mut file = File::create(&path)?;

    write!(file, "MEMORY\n{{\n").unwrap();
    for memory in chip.memory.iter() {
        if memory.name.contains("RAM_BANK") {
            write!(
                file,
                "    # Note: RAM on this device is continuous memory but partitioned in the
    # linker into two separate sections. This is to account for the upper 64kB
    # of RAM being wiped out upon the device entering any low-power mode
    # stronger than SLEEP. Thus, it is up to the end-user to enable RAM_BANK for
    # applications where the memory is considered lost outside of RUN and SLEEP Modes.
"
            )
            .unwrap();
        }
        writeln!(
            file,
            "    {} : ORIGIN = 0x{:08x}, LENGTH = {}K",
            memory.name, memory.address, memory.length,
        )
        .unwrap();
    }
    write!(file, "}}").unwrap();

    drop(file);
    Ok(())
}
