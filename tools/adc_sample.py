"""Extract the ADC's minimum sample window from MSPM0 datasheets.

`SAMPCLK` has no published ceiling; what the datasheets bound is the *window*, and a window shorter
than this returns a conversion of an input that has not finished settling, with nothing to report it.
Nothing machine-readable carries the figure. This writes `data/adc_sample/<family>.yaml`.

Two rows are read from the ADC's switching-characteristics table:

`tSample` / `tSample_step`, the bare-pin minimum. Every figure in every datasheet sits in the MIN
column, which is what makes it safe to assert against rather than merely document. The two names are
one quantity: only mspm0h321x prints a `Vstep`, and it is the only 4.5-5.5V part, so the older
documents left the step implicit at the device's own full scale. The datasheets' own input-network
derivation (T = K x Tau) is a step-settling calculation and nothing else. No datasheet prints both
rows, so this is the best-supported reading rather than a proven one.

`tSample_PGA`, the window when the channel is an OPA output. It is per *gain*, not per channel, and
the bare-pin figure is short by an order of magnitude at high gain -- 0.31us against 1.5us at x32 on
mspm0l130x. The gain axis is a different length on the two series: the G datasheets print six gains
and the L datasheets print two, the endpoints only. The curves cross -- L is slower at x1 and faster
at x32 -- so the G shape cannot be interpolated onto the L families, and an absent gain is absent
rather than derivable.

**Every `tSample_PGA` row is conditioned `GBW = 0x1`**, which is `OA_CFGBASE_GBW_HIGHGAIN`. The reset
value is 0 and TI's `DL_OPA_init` leaves it there, so a caller which has not set it is outside every
published figure. No datasheet publishes a row for `GBW = 0`, and the low setting is slower, so the
figures become an underestimate rather than a bound there.

Like `tools/adc_wakeup.py`, this is an offline aid rather than part of the build. Re-run it after a
data source bump rather than editing the YAML by hand.

Usage:
    uv run tools/adc_sample.py <datasheet.pdf> [...]       # print what it finds
    uv run tools/adc_sample.py --write <dir-of-datasheets> # regenerate data/adc_sample

`uv run` installs the dependencies below on its own; a bare `python` needs them on the path.

The table is not ruled, so a lattice read returns nothing and the column has to be decided
geometrically: the MIN/TYP/MAX header positions are found on the same page and each figure is
assigned to whichever header its centre is nearest. Layout text cannot answer this -- it collapses
the three columns into whitespace, which is how a MIN reads as a bare number with no column at all.

Two rows are expected on mspm0h321x, one per `Vstep`. The **larger** is written: its recommended
supply reaches 5.5V, so the 4V figure is not a bound for the part. Both conditions are quoted in the
generated file.

A datasheet with no `tSample_PGA` table is not a failure -- most families have no OPA. The seven that
print one include mspm0g110x, mspm0g310x and mspm0l110x, which have no OPA either: the row carries
the footnote "Only applies for devices with OPA" because one document covers several devices. The
generator attaches the map only to a chip that has an OPA instance, the same superset trap the ADC
channel map already guards against.
"""

# /// script
# requires-python = ">=3.10"
# dependencies = ["pdfplumber", "pyyaml"]
# ///

import math
import re
import sys
from pathlib import Path

try:
    import pdfplumber
    import yaml
except ImportError as e:  # pragma: no cover
    raise SystemExit(f"{e.name} is missing; run this with `uv run` instead of `python`")

#: Time units the figures may be given in, as a multiplier to nanoseconds.
UNITS = {"ns": 1, "us": 1_000, "µs": 1_000, "ms": 1_000_000}

NUMBER = re.compile(r"\d+(?:\.\d+)?")

#: The bare-pin row's description cell. `\bwith\b` excludes the qualified rows -- `with OPA`,
#: `with GPAMP`, `with DAC as input` -- while leaving `without OPA` and `for step input`.
BARE = re.compile(r"Sampl\w*\s+time(?!.*\bwith\b)", re.IGNORECASE)

#: One sub-row of the `tSample_PGA` block. The gain is the key; the GBW value is captured so a
#: datasheet that ever prints `GBW = 0x0` is noticed rather than silently folded in.
PGA = re.compile(r"GBW\s*=\s*(0x[0-9A-Fa-f]+).*?PGA\s+gain\s*=\s*x(\d+)", re.IGNORECASE)

#: Parameter-name and caption words, dropped so the quoted conditions read as conditions. `Vstep=4V`
#: survives because it is one token rather than the bare word `step`.
DROP = re.compile(
    r"t?Sampl\w*|time|without|OPA|for|step|input|\(\d+\)|\d+(?:\.\d+)?", re.IGNORECASE
)

#: Rows are grouped by rounding the word top to this many points.
ROW_TOLERANCE = 3


def centre(word: dict) -> float:
    return (word["x0"] + word["x1"]) / 2


def rows_of(page) -> dict[int, list[dict]]:
    """Words grouped into visual rows, keyed by rounded vertical position."""
    grouped: dict[int, list[dict]] = {}
    for word in page.extract_words():
        grouped.setdefault(round(word["top"] / ROW_TOLERANCE), []).append(word)
    return grouped


def figure_in(words: list[dict], headers: dict[str, float]) -> tuple[str, float] | None:
    """The rightmost numeric cell of a row, as `(column, value)`. The unit is the caller's."""
    lo = min(headers.values()) - 20
    figures = [w for w in words if NUMBER.fullmatch(w["text"]) and centre(w) > lo]
    if not figures:
        return None
    figure = figures[-1]
    column = min(headers, key=lambda c: abs(headers[c] - centre(figure)))
    return column, float(figure["text"])


def nanoseconds(value: float, unit: str) -> int:
    """Convert to whole nanoseconds, rounding **up**.

    Every figure here is a lower bound on a sample window, so rounding a fraction down would hand a
    consumer a window the datasheet does not support. Only 62.5ns on the G families is not already
    whole.
    """
    return math.ceil(value * UNITS[unit.lower()])


def unit_of(words: list[dict]) -> str:
    return next((w["text"] for w in reversed(words) if w["text"].lower() in UNITS), "")


def read(path: Path) -> tuple[list[tuple[str, int, str]], dict[int, tuple[str, int, str]]]:
    """Return the bare-pin rows and the per-gain PGA rows found in one datasheet.

    Bare rows are `(column, nanoseconds, conditions, stepped)`, where `stepped` is whether the row
    is TI's newer `tSample_step` -- told by its description cell, since only mspm0h321x's conditions
    mention a step. PGA rows are keyed by gain and carry `(column, nanoseconds, gbw)`.
    """
    bare: list[tuple[str, int, str, bool]] = []
    pga: dict[int, tuple[str, int, str]] = {}

    with pdfplumber.open(str(path)) as pdf:
        for page in pdf.pages:
            text = page.extract_text() or ""
            if "Sampling time" not in text and "PGA gain" not in text:
                continue

            grouped = rows_of(page)
            headers: dict[str, float] = {}
            # The unit cell sits on the `tSample_PGA` parameter row, which the G datasheets print
            # *below* its gain sub-rows, so the page's rows are resolved together at the end.
            page_unit = ""
            raw: list[tuple[int, str, float, str]] = []

            for key in sorted(grouped):
                words = sorted(grouped[key], key=lambda w: w["x0"])
                line = " ".join(w["text"] for w in words)

                if "MIN" in line and "MAX" in line:
                    headers = {
                        w["text"]: centre(w) for w in words if w["text"] in ("MIN", "TYP", "MAX")
                    }
                if not headers:
                    continue

                page_unit = unit_of(words) or page_unit

                gain = PGA.search(line)
                if gain:
                    found = figure_in(words, headers)
                    if found:
                        raw.append((int(gain.group(2)), found[0], found[1], unit_of(words)))
                    continue

                if not BARE.search(line):
                    continue
                unit = unit_of(words)
                found = figure_in(words, headers) if unit else None
                if not found:
                    continue
                column, ns = found[0], nanoseconds(found[1], unit)
                conditions = " ".join(
                    w["text"] for w in words if w["text"] != unit and not DROP.fullmatch(w["text"])
                )
                stepped = "step input" in line.lower()
                bare.append((column, ns, " ".join(conditions.split()), stepped))

            for gain, column, value, own_unit in raw:
                unit = own_unit or page_unit
                if unit:
                    pga[gain] = (column, nanoseconds(value, unit), "0x1")

    return bare, pga


HEADER = """\
# The minimum ADC sample window for this family, in nanoseconds.
#
# GENERATED by `tools/adc_sample.py --write` from the {ds} datasheet's ADC switching
# characteristics. Re-run the tool rather than editing by hand.
#
# min_ns is the `{row}` row, MIN column: the shortest window that samples a full-scale step to
# within the converter's resolution. A shorter window returns a conversion of an input that has not
# settled, and nothing reports it.
{conditions}"""

SINGLE_CONDITION = """\
#
# Stated under one condition, quoted here:
#
#   {conditions}
"""

PGA_HEADER = """
# pga_ns is the `tSample_PGA` row: the window when the channel is an OPA output, keyed by PGA gain.
# It is per gain and not per channel, and at high gain it is an order of magnitude above min_ns.
#
# Every published figure is conditioned GBW = 0x1 (OA_CFGBASE_GBW_HIGHGAIN). The reset value is 0
# and TI's DL_OPA_init leaves it there, so a caller which has not set it is outside every published
# figure. No datasheet publishes a row for GBW = 0, and the low setting is slower, so the figures
# become an underestimate rather than a bound there.
#
# A gain absent here is unpublished, not derivable: the G datasheets print six gains and the L
# datasheets two, and the curves cross, so neither can be interpolated onto the other.
"""

VSTEP_NOTE = """\
#
# This datasheet states the row twice, once per input step:
#
{rows}
#
# The larger is written. The recommended supply reaches 5.5V, so the smaller figure is not a bound
# for the part.
"""


def write(datasheets: str) -> int:
    """Regenerate data/adc_sample/*.yaml. Returns the number of problems reported."""
    parts = yaml.safe_load(Path("data/parts.yaml").read_text(encoding="utf-8"))
    families = [(f["family"], f["datasheet_url"].rsplit("/", 1)[-1]) for f in parts["families"]]

    Path("data/adc_sample").mkdir(parents=True, exist_ok=True)
    problems = 0

    for family, gpn in families:
        pdf = Path(datasheets) / f"{gpn}_datasheet.pdf"
        if not pdf.exists():
            print(f"{family}: no datasheet for {gpn} in {datasheets}")
            problems += 1
            continue

        bare, pga = read(pdf)
        if not bare:
            print(f"{family}: no ADC sampling-time row in {pdf.name}")
            problems += 1
            continue

        off_column = [c for c, _, _, _ in bare if c != "MIN"]
        if off_column:
            print(f"{family}: sampling time is a {off_column[0]} in {pdf.name}, not a MIN")
            problems += 1
            continue

        wrong_gbw = sorted({g for _, _, g in pga.values() if g != "0x1"})
        if wrong_gbw:
            print(f"{family}: tSample_PGA rows at GBW {', '.join(wrong_gbw)} in {pdf.name}")
            problems += 1
            continue

        _, ns, conditions, _ = max(bare, key=lambda r: r[1])
        row = "tSample_step" if any(stepped for _, _, _, stepped in bare) else "tSample"
        # Where the row is stated twice the Vstep note quotes both, so a second block saying "one
        # condition" would contradict it.
        if len(bare) > 1:
            quoted = "\n".join(f"#     {c or 'none stated'}: {n} ns" for _, n, c, _ in bare)
            conditions = VSTEP_NOTE.format(rows=quoted)
        else:
            conditions = SINGLE_CONDITION.format(conditions=conditions or "none stated")

        path = Path(f"data/adc_sample/{family}.yaml")
        with open(path, "w", encoding="utf-8", newline="\n") as out:
            out.write(HEADER.format(ds=gpn.upper(), row=row, conditions=conditions))
            out.write(f"\nmin_ns: {ns}\n")
            if pga:
                out.write(PGA_HEADER)
                out.write("\npga_ns:\n")
                for gain in sorted(pga):
                    out.write(f"  {gain}: {pga[gain][1]}\n")

        gains = f", pga x{'/x'.join(str(g) for g in sorted(pga))}" if pga else ""
        print(f"{family}: {ns} ns ({row}){gains}, from {pdf.name}")

    return problems


def main(argv: list[str]) -> None:
    if "--write" in argv:
        i = argv.index("--write")
        raise SystemExit(1 if write(argv[i + 1] if i + 1 < len(argv) else ".") else 0)

    paths = [Path(a) for a in argv if not a.startswith("--")]
    if not paths:
        raise SystemExit(__doc__)

    for path in paths:
        bare, pga = read(path)
        print(f"########## {path.name}")
        if not bare:
            print("    no ADC sampling-time row")
        for column, ns, conditions, _ in bare:
            print(f"    {ns:>8} ns   {column}   {conditions}")
        for gain in sorted(pga):
            column, ns, gbw = pga[gain]
            print(f"    {ns:>8} ns   {column}   PGA gain x{gain}, GBW {gbw}")


if __name__ == "__main__":
    main(sys.argv[1:])
