# Register blocks

These files are [chiptool](https://github.com/embassy-rs/chiptool) IR. They start life as
`chiptool extract-peripheral` output from the SVDs in `sources/svd/`, but **the checked-in YAML is
the source of truth**, not the SVD: it is hand-cleaned afterwards, and in the places listed below it
deliberately contradicts the SVD. The cleanup rules are in the [root README](../../README.md).

Keep this file up to date rather than commenting the YAML. `chiptool fmt` silently strips comments,
so anything recorded only there is one `fmt` away from being lost — which is also why a deviation
that is not written down here tends to get "corrected" back to the SVD by the next person.

Notes about what a register *means*, as opposed to where its definition came from, belong in the
`description:` text instead. Those survive `fmt` and reach the generated PAC's documentation.

## How much of this is reproducible

Some of it. `transforms/` gets a block most of the way there, and two of the deviations below are now
encoded in it and verified end to end:

| block | reproduces the checked-in YAML? |
| --- | --- |
| `sysctl_c110x` | **yes** — `chiptool extract-peripheral --svd sources/svd/MSPM0C110X.svd --peripheral SYSCTL`, then `transforms/transform.sh` with `SYSCTL_C110x.yaml` |
| `sysctl_l110x_l130x_l134x` | **yes** — same, from `MSPM0L130X.svd` |
| everything else with a transform | no, see below |
| `beeper_v1`, `cpuss_v1`, `factoryregion_v1`, `sysctl_c1105_c1106`, `tim_btimer`, `unicomm_v1`, `vref_v1` | no transform exists at all |

Where a transform does not reproduce its block, the reason is usually that it is written against a
different source instance than the one the block was derived from — `TIM.yaml` runs from `TIMA0` on
MSPM0G350X, which has four capture/compare channels and neither the `DC` nor the `QEIERR` event, so
its output cannot match `tim_v1` however the transform is written. Closing those gaps means
re-deriving the block, not just editing the transform.

The rest of the cleanup — shared fieldsets, deleted enums, stripped prefixes — is hand-applied on top
and is not encoded anywhere. **Re-running a transform over an existing block therefore discards hand
work silently.** Diff the output against the checked-in YAML instead, and use the flatten-and-compare
check described in the root README to see what actually moved.

The exception worth copying is `TIM.yaml`'s `CCD`/`CCU` step: `!MakeFieldArray` with `mode: Cursed`
produces exactly the offset lists the checked-in block carries, so that piece of the cleanup at least
survives a re-derivation.

## Deviations from the SVD

Each entry says what the SVD claims, what this repo says instead, and what the evidence is.

### `sysctl_c110x`, `sysctl_c1105_c1106` — three unlock-protected control bits are at bit 0

| fieldset | field |
| --- | --- |
| `EXLFCTL` | `SETUSEEXLF` |
| `EXRSTPIN` | `DISABLE` |
| `SYSSTATUSCLR` | `ALLECC` |

The C-series SVD puts each of these at bit 2. That contradicts SLAU893 and every other family's
SYSCTL, which agree on bit 0. Taking the SVD's word for it would produce a HAL whose writes to these
three registers never take effect.

### `sysctl_l110x_l130x_l134x` — `RSTCAUSE.BOOTSW` is 13

The SVD says 10, copied from SLAU847's field description for this SYSCTL variant. That description
contradicts the same TRM's "Reset Causes" table, which lists 10 as reserved and 13 as the
software-triggered BOOTRST. Every other family agrees with the table. `BOOTWWDT0` in the same enum
had already been corrected against that table for the same reason.

### `cpuss_v1` — the interrupt group's `MIS`, `ISET` and `ICLR` are 8 bits

The SVD gives these three a single bit, where it gives `IMASK` and `RIS` eight. `hw_cpuss.h` masks
all five with `0xFF`, and the group's `IIDX` enumerates `INT0` to `INT7`, so a single bit would leave
only interrupt 0 reachable through three of the five registers.

### `gpio_v1` — the generic event masks are indexed by absolute DIO number

The SVD bases both generic event blocks at bit 0 with sixteen bits each. `hw_gpio.h` puts
`GEN_EVENT0`'s `DIO0` to `DIO15` at bits 0 to 15 but `GEN_EVENT1`'s `DIO16` to `DIO31` at bits **16
to 31**. Both blocks now share the same 32-bit fieldset as `CPU_INT` and the `DOUT`/`DIN` masks, so
one bit index means one DIO number everywhere; each event block implements only its own half.

### `tim_v1` — `CCD` and `CCU` are offset-list arrays

Not a contradiction, but the reason a plain stride does not appear here: capture/compare channels 4
and 5 are compare-only and sit *above* the `CCU` bits rather than next to `CCD0` to `CCD3`
(`CCD` at bits 4-7 and 12-13, `CCU` at 8-11 and 14-15, per `hw_gptimer.h`). The arrays use chiptool's
explicit `offsets:` list so that `ccd(n)` and `ccu(n)` still take a channel number.
