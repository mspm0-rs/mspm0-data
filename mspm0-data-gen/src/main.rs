mod clock_tree;
mod errata;
mod generate;
mod header;
mod int_group;
mod operating_modes;
mod parts;
mod perimap;
mod svd;
mod sysconfig;
mod timers;
mod sources;
mod util;
mod vref;
mod verify;
mod wakeup;

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
    let data_sources = PathBuf::from("./sources/");

    let mut stopwatch = Stopwatch::new();
    stopwatch.section("Parsing headers");

    let headers = header::Headers::parse(&data_sources)?;

    stopwatch.section("Sysconfig metadata");

    let sysconfig = sysconfig::Sysconfig::parse(&data_sources)?;
    let clock_trees = clock_tree::ClockTrees::parse(&data_sources)?;

    stopwatch.section("Parsing SVDs");

    let svds = svd::Svds::parse(&data_sources)?;

    stopwatch.section("Read interrupt group mappings");

    let int_groups = int_group::parse()?;
    let operating_modes = operating_modes::parse()?;
    let timers = timers::parse()?;
    let errata = errata::parse()?;
    let wake = wakeup::parse()?;
    let vref = vref::parse()?;
    let parts = parts::PartsFile::read()?;

    // TODO: Expanded family names (ex. C110X -> C1103 & C1104)

    let sources = sources::Sources {
        parts,
        headers,
        sysconfig,
        svds,
        clock_trees,
        operating_modes,
        int_groups,
        timers,
        errata,
        wake,
        vref,
    };

    stopwatch.section("Generate data");
    generate::generate(&sources)?;

    stopwatch.stop();

    Ok(())
}
