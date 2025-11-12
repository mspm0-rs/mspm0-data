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
* mspm0-sdk header files
  * Interrupt number, name
  * Peripheral addresses
* mspm0 SVDs: register blocks
* Manually entered
  * IIDX values for interrupts within a `INT_GROUP`

# Adding a new chip

1. Update the data sources to include the new chip. You will need to get the SVD and sysconfig metadata.
2. Add the new chip family and part numbers to [`parts.yaml`](./data/parts.yaml)
3. If needed, add any chip specific register blocks like `sysctl`.
4. Check the peripheral mapping in [`perimap.rs`](./mspm0-data-gen/src/perimap.rs) to use the correct peripherals.

# Adding support for a new peripheral

This will help you add support for a new peripheral to all MSPM0 families. (Please take the time to add it for all families, even if you personally
are only interested in one. It's easier than it looks, and doing all families at once is significantly less work than adding one now then having to revisit everything later when adding more. It also helps massively in catching mistakes and inconsistencies in the source SVDs.)

- Install chiptool with `./d install-chiptool`
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

The `mspm0-metapac-gen` binary has a map to match peripherals to the right version in all chips, the [perimap](./mspm0-data-gen/src/perimap.rs).

When parsing a chip, for each peripheral a "key" string is constructed using this format: `FAMILY:PERIPHERAL_NAME`, where:

- `FAMILY`: chip family in lowercase, for example `mspm0g350x`
- `PERIPHERAL_NAME`: peripheral name, for example `spi`.

`perimap` entries are regexes matching on the above "key" string. First regex that matches wins. For example:

```
(".*:tim", ("tim", "v1")),
("mspm0c110x:sysctl", ("sysctl", "c110x")),
("mspm0g..0x:sysctl", ("sysctl", "g350x_g310x_g150x_g110x")),
```
