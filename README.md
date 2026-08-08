# mspm0-data

`mspm0-data` aims to produce clean machine-readable data about MSPM0 microcontroller families, including:

- ✔️ Base chip information
  - Packages
  - 🚧 Flash, RAM size
- ✔️ Peripheral addresses and interrupts
- ✔️ Interrupts
- ✔️ GPIO peripheral function (PF) mappings
- 🚧 Register blocks for all peripherals
- 🚧 DMA mappings
- ✔️ Per package pinouts
- 🚧 Links to applicable technical reference manual and datasheet PDFs.
- ✔️ Low power data
  - Power domain per peripheral
  - How deep a sleep each peripheral and memory region is retained through
  - How deep a sleep each peripheral stays usable in (the datasheet `EN`/`DIS`/`OPT`/`NS`/`OFF` table)
  - Which timers stay clocked in STANDBY1
  - Which peripheral instances have a `CLKCFG.BLOCKASYNC` bit
  - Which pins can wake the device from SHUTDOWN
  - MCLK and ULPCLK ceilings, backup power domain presence

✔️ = done, 🚧 = work in progress, ❌ = to do

The generated JSON files are [available here in the mspm0-data-generated](https://github.com/mspm0-rs/mspm0-data-generated/tree/master/data) repo.

# mspm0-metapac

The generated PAC is [available here in the mspm0-data-generated](https://github.com/mspm0-rs/mspm0-data-generated/tree/master/mspm0-metapac) repo.

# Data sources

These are the data sources currently used.

* SysConfig metadata from Code Composer Studio
  * Packages and package pinouts
  * Mapping from GPIO pin to IOMUX::PINCM register.
  * Peripheral PF (pin function) mappings.
  * Peripheral pin names.
  * Which pins have wakeup logic (`io_wakeup`).
  * Number of ADC conversion channels (`SYS_ADC_MEMCTL_DIM`).
  * How many counters a basic timer instance has (`SYS_NUM_COUNTERS`), and a cross-check of the
    datasheet's capture/compare channel counts (`SYS_NUM_CC`).
* SysConfig `clocktree.json`
  * Which crystal drivers, external clock inputs and SYSPLL a family has. This is not derivable from
    the SYSCTL register block: MSPM0C110x and MSPM0C1105/C1106 share one, but only the latter has a
    crystal driver.
* mspm0-sdk header files
  * Interrupt number, name
  * Peripheral addresses
  * NVIC interrupt priority bits
* mspm0 SVDs
  * Register blocks
  * Which peripheral instances have a `CLKCFG.BLOCKASYNC` bit. TI does not publish an SVD for every
    family, so this is optional per family.
* Device datasheets, read by the scripts in [`tools/`](./tools)
  * How deep a sleep each PD1 peripheral is retained through, and how deep each peripheral
    stays usable, from the "Supported Functionality by Operating Mode" table
    ([`data/operating_modes/`](./data/operating_modes))
  * What each timer instance can do, from the TIMx configuration table
    ([`data/timers/`](./data/timers))
  * How long the device takes to reach RUN from each sleep mode, from the wake-up timing table
    ([`data/wakeup/`](./data/wakeup))
  * Which timers stay clocked in STANDBY1 (`standby1_timers` in [`parts.yaml`](./data/parts.yaml))
  * MCLK and ULPCLK ceilings, the SYSOSC base frequency, the flash wait-state bands, `fADCCLK` and
    `TRNGCLKF` (all in [`parts.yaml`](./data/parts.yaml))
* Device errata sheets
  * Which functional advisories apply ([`data/errata/`](./data/errata))
* Manually entered
  * IIDX values for interrupts within a `INT_GROUP`
  * Whether the device has `MCLKCFG.UDIV` and the STOP1 sub-mode (`clock_tree` in
    [`parts.yaml`](./data/parts.yaml))

Run `./d download-docs` to fetch the datasheets, errata sheets and reference manuals into `./files/`;
the `tools/` scripts read them from there.

# Adding a new chip

1. Update the data sources to include the new chip. You will need to get the SVD and sysconfig metadata.
2. Add the new chip family and part numbers to [`parts.yaml`](./data/parts.yaml). Besides the part
   numbers and memory this needs every frequency, the `clock_tree` entries and `standby1_timers`,
   all from the datasheet.
3. If needed, add any chip specific register blocks like `sysctl`.
4. Check the peripheral mapping in [`perimap.rs`](./mspm0-data-gen/src/perimap.rs) to use the correct peripherals.
5. Fetch the documents with `./d download-docs`, then regenerate the extracted data:
   `tools/operating_modes.py`, `tools/timers.py`, `tools/wakeup.py` and `tools/errata.py`, each with
   `--write files`.
6. Run `./d gen` and read its output. `verify.rs` reports every per-chip gap it can detect, including
   a family with no timer, errata or operating-mode data.

# Adding support for a new peripheral

This will help you add support for a new peripheral to all MSPM0 families. (Please take the time to add it for all families, even if you personally
are only interested in one. It's easier than it looks, and doing all families at once is significantly less work than adding one now then having to revisit everything later when adding more. It also helps massively in catching mistakes and inconsistencies in the source SVDs.)

- Install chiptool with `./d install-chiptool`
- Download MSPM0 data sources with `./d download-all`
- Run `./d extract-all CANFD0`. This'll output a bunch of yamls in `tmp/CANFD0`. NOTE sometimes peripherals have a number sometimes not (`CANFD0` vs `CANFD`). You might want to try both and merge the outputted YAMLs into a single directory.
- Diff them between themselves, to identify differences. The differences can either be:
  - 1: Legitimate differences between families, because there are different CANFD versions. For example, added registers/fields.
  - 2: SVD inconsistencies, like different register names for the same register
  - 3: SVD mistakes (yes, there are some)
  - 4: Missing stuff in SVDs, usually enums or doc descriptions.
- Identify how many actually-different (incompatible) versions of the peripheral exist, as we must _not_ merge them. Name them v1, v2.. (if possible, by order of chip release date
- For each version, pick the "best" YAML (the one that has less enums/docs missing), place them in `data/registers/canfd_vX.yaml`
- Cleanup the register yamls (see below).
- Minimize the diff between each pair of versions. For example between `canfd_v1.yaml` and `canfd_v2.yaml`. If one is missing enums or descriptions, copy it from another.
- Add entries to [`perimap`](./mspm0-data-gen/src/perimap.rs), see below.
- Add corresponding `PeripheralType` to [`GENERATE_PERIPHERALS`](./mspm0-metapac-gen/src/peripheral.rs).
- Rebuild (`./d gen && ./d build-metapac`), then:
  - Check `mspm0-metapac/src/chips/<chip>/pac.rs` has the right `#[path = "../../peripherals/canfd_v1.rs"]` paths.
  - Ensure a successful build of the affected pac. e.g.

    ```
    cd build/mspm0-metapac
    cargo build --features mspm0g3507pm
    ```

Please separate manual changes and changes resulting from regen in separate commits. It helps tremendously with review and rebasing/merging.

## Register cleanup

SVDs have some widespread annoyances that should be fixed when adding register YAMLs to this repo. Check out `chiptool` transforms, they can help in speeding up the cleanups.

- Remove "useless prefixes". For example if all regs in the `RNG` peripheral are named `RNG_FOO`, `RNG_BAR`, the `RNG_` peripheral is not conveying any useful information at all, and must go.
- Remove "useless enums". Useless enums is one of the biggest cause of slow compilation times in STM32 PACs.
  - 0=disabled, 1=enabled. Common in `xxEN` and `xxIE` fields. If a field says "enable foo" and is one bit, it's obvious "true" means enabled and "false" means disabled.
  - "Write 0/1 to clear" enums, common in `xxIF` fields.
  - Check out the `DeleteEnums` chiptool transforms.
- Convert repeated registers or fields (like `FOO0 FOO1, FOO2, FOO3`) to arrays `FOO[n]`.
  - Check out the `MakeRegisterArray`, `MakeFieldArray` chiptool transforms.
- Use `chiptool fmt` on each of the register yamls.

## Peripheral mapping (perimap)

The `mspm0-data-gen` binary has a map to match peripherals to the right version in all chips, the [perimap](./mspm0-data-gen/src/perimap.rs).

When parsing a chip, for each peripheral a "key" string is constructed using this format: `FAMILY:PERIPHERAL_NAME`, where:

- `FAMILY`: chip family in lowercase, for example `mspm0g350x`
- `PERIPHERAL_NAME`: peripheral name, for example `spi`.

`perimap` entries are regexes matching on the above "key" string. First regex that matches wins. For example:

```
(".*:tim", ("tim", "v1")),
("mspm0c110x:sysctl", ("sysctl", "c110x")),
("mspm0g..0x:sysctl", ("sysctl", "g350x_g310x_g150x_g110x")),
```

`PERIPHERAL_NAME` is the peripheral type, so every instance of a type on a chip gets the same
version. Where one type covers instances with different register blocks the key has to say which,
as `timb` does for the basic timers alongside `tim` — see `get_peripheral_type_version`. Such a
version also needs an entry in `VARIANT_MODULES` in
[`peripheral.rs`](./mspm0-metapac-gen/src/peripheral.rs) to give it a module name of its own,
otherwise the metapac generator panics on the two blocks colliding.
