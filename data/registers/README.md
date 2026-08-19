# Register blocks

These files are [chiptool](https://github.com/embassy-rs/chiptool) IR. They start life as
`chiptool extract-peripheral` output from the SVDs in `sources/svd/`. The checked-in YAML is the
source of truth, not the SVD. It is hand-cleaned afterwards, and in the places listed below it
deliberately contradicts the SVD. The cleanup rules are in the [root README](../../README.md).

Keep this file up to date rather than commenting the YAML. `chiptool fmt` silently strips comments.

Notes about what a register means belong in its `description:` text instead. Those survive `fmt` and
reach the generated PAC's documentation.

## How much of this is reproducible

Not all of it, so re-running a transform over an existing block discards hand work silently.

`transforms/` gets a block most of the way there. Two of the deviations below are encoded in it and
verified end to end:

| block | reproduces the checked-in YAML? |
| --- | --- |
| `sysctl_c110x` | yes: `chiptool extract-peripheral --svd sources/svd/MSPM0C110X.svd --peripheral SYSCTL`, then `transforms/transform.sh` with `SYSCTL_C110x.yaml` |
| `sysctl_l110x_l130x_l134x` | yes: same, from `MSPM0L130X.svd` |
| everything else with a transform | no, see below |
| `beeper_v1`, `cpuss_v1`, `factoryregion_v1`, `sysctl_c1105_c1106`, `tim_btimer`, `unicomm_v1`, `vref_v1` | no transform exists at all |

A transform that does not reproduce its block is usually written against a different source instance.
`TIM.yaml` runs from `TIMA0` on MSPM0G350X, which has four capture/compare channels and neither the
`DC` nor the `QEIERR` event. Its output cannot match `tim_v1` however the transform is written.

## Deviations from the SVD

Each entry says what the SVD claims, what this repo says instead, and what the evidence is.

When checking a release for unintended movement, fingerprint the generated Rust rather than the YAML.
Read the register offsets back out of `wrapping_add` and the field positions out of each accessor's
`(shift, mask)`. That covers everything the generator does between IR and Rust, and
flatten-and-compare sees none of it.

### `sysctl_c110x`, `sysctl_c1105_c1106`: three unlock-protected control bits are at bit 0

| fieldset | field |
| --- | --- |
| `EXLFCTL` | `SETUSEEXLF` |
| `EXRSTPIN` | `DISABLE` |
| `SYSSTATUSCLR` | `ALLECC` |

The C-series SVD puts each of these at bit 2. SLAU893 and every other family's SYSCTL agree on bit 0.
Taking the SVD's word for it would produce a HAL whose writes to these three registers never take
effect.

### `sysctl_l110x_l130x_l134x`: `RSTCAUSE.BOOTSW` is 13

The SVD says 10, copied from SLAU847's field description for this SYSCTL variant. That description
contradicts the same TRM's "Reset Causes" table, which lists 10 as reserved and 13 as the
software-triggered BOOTRST. Every other family agrees with the table. `BOOTWWDT0` in the same enum
had already been corrected against that table for the same reason.

### `cpuss_v1`: the interrupt group's `MIS`, `ISET` and `ICLR` are 8 bits

The SVD gives these three a single bit, where it gives `IMASK` and `RIS` eight. `hw_cpuss.h` masks
all five with `0xFF`, and the group's `IIDX` enumerates `INT0` to `INT7`. A single bit would leave
only interrupt 0 reachable through three of the five registers.

### `gpio_v1`: the generic event masks are indexed by absolute DIO number

The SVD bases both generic event blocks at bit 0 with sixteen bits each. `hw_gpio.h` puts
`GEN_EVENT0`'s `DIO0` to `DIO15` at bits 0 to 15, but `GEN_EVENT1`'s `DIO16` to `DIO31` at bits 16 to
31. Both blocks now share the same 32-bit fieldset as `CPU_INT` and the `DOUT`/`DIN` masks, so one
bit index means one DIO number everywhere. Each event block implements only its own half.

### `tim_v1`: `CCD` and `CCU` are offset-list arrays

Not a contradiction. Capture/compare channels 4 and 5 are compare-only and sit above the `CCU` bits
rather than next to `CCD0` to `CCD3` (`CCD` at bits 4-7 and 12-13, `CCU` at 8-11 and 14-15, per
`hw_gptimer.h`). A plain stride cannot describe that, so the arrays use chiptool's explicit
`offsets:` list, which keeps `ccd(n)` and `ccu(n)` taking a channel number.
