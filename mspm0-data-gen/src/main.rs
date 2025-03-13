mod clock_tree;
mod generate;
mod header;
mod int_group;
mod sysconfig;

use std::{path::PathBuf, time::Instant};

struct Stopwatch {
    start: Instant,
    section_start: Option<Instant>,
}

impl Stopwatch {
    fn new() -> Self {
        eprintln!("Starting timer");
        let start = Instant::now();
        Self {
            start,
            section_start: None,
        }
    }

    fn section(&mut self, status: &str) {
        let now = Instant::now();
        self.print_done(now);
        eprintln!("  {status}");
        self.section_start = Some(now);
    }

    fn stop(self) {
        let now = Instant::now();
        self.print_done(now);
        let total_elapsed = now - self.start;
        eprintln!("Total time: {:.2} seconds", total_elapsed.as_secs_f32());
    }

    fn print_done(&self, now: Instant) {
        if let Some(section_start) = self.section_start {
            let elapsed = now - section_start;
            eprintln!("    done in {:.2} seconds", elapsed.as_secs_f32());
        }
    }
}

fn main() -> anyhow::Result<()> {
    // TODO: Don't use an absolute path
    let data_sources = PathBuf::from("/home/i509vcb/Dev/msp-rs/mspm0-data-sources/");

    let mut stopwatch = Stopwatch::new();
    stopwatch.section("Parsing headers");

    let headers = header::Headers::parse(&data_sources)?;

    stopwatch.section("Sysconfig metadata");

    let sysconfig = sysconfig::Sysconfig::parse(&data_sources)?;
    let _clock_trees = clock_tree::ClockTree::read_clock_trees(&data_sources)?;

    stopwatch.section("Read interrupt group mappings");

    let int_groups = int_group::Groups::parse()?;

    // TODO: Expanded family names (ex. C110X -> C1103 & C1104)

    stopwatch.section("Generate data");
    generate::generate(&headers, &sysconfig, &int_groups)?;

    stopwatch.stop();

    Ok(())
}
