"""Extract the ADC wake-up time from MSPM0 datasheets.

`CTL0.PWRDN` resets to `0`, which is automatic power down: the ADC powers off after each conversion
and has to wake again for the next one. The TRM's note under its AUTO-mode timing diagram says the
wake costs part of the sample window and has to be paid for out of `SCOMPx` -- "if the maximum ADC
wakeup/enable time is 5uS, it means the duration set by SCOMPx should be > (5uS + Duration for sample
window)". Nothing machine-readable carries the figure: no header constant, nothing in driverlib, and
sysconfig only emits a warning telling you to read the datasheet. This writes
`data/adc_wakeup/<family>.yaml`.

Like `tools/vref.py`, this is an offline aid rather than part of the build. Re-run it after a data
source bump rather than editing the YAML by hand.

Usage:
    uv run tools/adc_wakeup.py <datasheet.pdf> [...]      # print what it finds
    uv run tools/adc_wakeup.py --write <dir-of-datasheets> # regenerate data/adc_wakeup

`uv run` installs the dependencies below on its own; a bare `python` needs them on the path.

**Which column the figure sits in is the point, and it differs.** Thirteen datasheets put it in MAX;
mspm0l110x, mspm0l130x and mspm0l134x put 1us in TYP and publish no maximum at all. A driver that
has to guarantee a settled input needs the ceiling, and on those three families the datasheet does
not give one -- so the two are recorded as separate keys rather than as one number with a footnote.
Reading a typical as a bound is the failure this shape prevents.

The table is not ruled, so a lattice read returns nothing and the column has to be decided
geometrically: the MIN/TYP/MAX header positions are found on the same page and the figure is
assigned to whichever header its centre is nearest. Layout text cannot answer this -- it collapses
the three columns into whitespace, which is how the difference went unnoticed until it was checked.
"""

# /// script
# requires-python = ">=3.10"
# dependencies = ["pdfplumber", "pyyaml"]
# ///

import re
import sys
from pathlib import Path

try:
    import pdfplumber
    import yaml
except ImportError as e:  # pragma: no cover
    raise SystemExit(f"{e.name} is missing; run this with `uv run` instead of `python`")

#: The row's description cell. Specific enough not to catch `tWAKEUP`, the SHUTDOWN wake-up time,
#: which is a different figure in a different table.
CAPTION = "ADC Wakeup Time"

#: Time units the figure may be given in, as a multiplier to nanoseconds.
UNITS = {"ns": 1, "us": 1_000, "µs": 1_000, "ms": 1_000_000}

NUMBER = re.compile(r"\d+(?:\.\d+)?")

#: Rows are grouped by rounding the word top to this many points. Large enough to hold a row whose
#: cells sit a fraction of a point apart, small enough not to merge two rows.
ROW_TOLERANCE = 3


def centre(word: dict) -> float:
    return (word["x0"] + word["x1"]) / 2


def rows_of(page) -> dict[int, list[dict]]:
    """Words grouped into visual rows, keyed by rounded vertical position."""
    grouped: dict[int, list[dict]] = {}
    for word in page.extract_words():
        grouped.setdefault(round(word["top"] / ROW_TOLERANCE), []).append(word)
    return grouped


def read(path: Path) -> tuple[str, int, str] | None:
    """Return `(column, nanoseconds, conditions)` for the ADC wake-up row, or `None`.

    `column` is `MIN`, `TYP` or `MAX` -- whichever header the figure sits under.
    """
    with pdfplumber.open(str(path)) as pdf:
        for page in pdf.pages:
            text = page.extract_text() or ""
            if CAPTION not in text:
                continue

            grouped = rows_of(page)
            headers: dict[str, float] = {}

            for key in sorted(grouped):
                words = sorted(grouped[key], key=lambda w: w["x0"])
                line = " ".join(w["text"] for w in words)

                # The column header row precedes the figures on the same page.
                if "MIN" in line and "MAX" in line:
                    headers = {
                        w["text"]: centre(w) for w in words if w["text"] in ("MIN", "TYP", "MAX")
                    }

                if CAPTION.replace(" ", "") not in line.replace(" ", "") or not headers:
                    continue

                unit = next((w["text"] for w in reversed(words) if w["text"].lower() in UNITS), "")
                figures = [w for w in words if NUMBER.fullmatch(w["text"])]
                if not unit or not figures:
                    continue

                figure = figures[-1]
                column = min(headers, key=lambda c: abs(headers[c] - centre(figure)))
                conditions = " ".join(
                    w["text"]
                    for w in words
                    if w is not figure
                    and w["text"] != unit
                    and w["text"] not in CAPTION.split()
                    and not w["text"].startswith("T")
                )
                ns = round(float(figure["text"]) * UNITS[unit.lower()])
                return column, ns, " ".join(conditions.split())

    return None


HEADER = """\
# How long this family's ADC needs to wake from automatic power down, in nanoseconds.
#
# GENERATED by `tools/adc_wakeup.py --write` from the `Twakeup` row of the {ds} datasheet.
# Re-run the tool rather than editing by hand.
#
# CTL0.PWRDN resets to 0 (automatic power down), and the TRM requires this to be paid for out of the
# programmed sample window: SCOMPx must cover the wake plus the sampling the signal actually needs.
#
# Stated under one condition, quoted here:
#
#   {conditions}
#
# Only the column the datasheet fills is written. A missing max_ns means the datasheet publishes no
# ceiling for this family, not that the wake is free.
"""


def write(datasheets: str) -> int:
    """Regenerate data/adc_wakeup/*.yaml. Returns the number of problems reported."""
    parts = yaml.safe_load(Path("data/parts.yaml").read_text(encoding="utf-8"))
    families = [(f["family"], f["datasheet_url"].rsplit("/", 1)[-1]) for f in parts["families"]]

    Path("data/adc_wakeup").mkdir(parents=True, exist_ok=True)
    problems = 0

    for family, gpn in families:
        pdf = Path(datasheets) / f"{gpn}_datasheet.pdf"
        if not pdf.exists():
            print(f"{family}: no datasheet for {gpn} in {datasheets}")
            problems += 1
            continue

        found = read(pdf)
        if found is None:
            print(f"{family}: no ADC wake-up time in {pdf.name}")
            problems += 1
            continue

        column, ns, conditions = found
        if column == "MIN":
            print(f"{family}: ADC wake-up time is a MIN in {pdf.name}, which says nothing useful")
            problems += 1
            continue

        key = "max_ns" if column == "MAX" else "typ_ns"
        path = Path(f"data/adc_wakeup/{family}.yaml")
        with open(path, "w", encoding="utf-8", newline="\n") as out:
            out.write(HEADER.format(ds=gpn.upper(), conditions=conditions or "none stated"))
            out.write(f"\n{key}: {ns}\n")
        print(f"{family}: {ns / 1000:g} us ({column}), from {pdf.name}")

    return problems


def main(argv: list[str]) -> None:
    if "--write" in argv:
        i = argv.index("--write")
        raise SystemExit(1 if write(argv[i + 1] if i + 1 < len(argv) else ".") else 0)

    paths = [Path(a) for a in argv if not a.startswith("--")]
    if not paths:
        raise SystemExit(__doc__)

    for path in paths:
        found = read(path)
        if found is None:
            print(f"########## {path.name}: no ADC wake-up time")
            continue
        column, ns, conditions = found
        print(f"########## {path.name}")
        print(f"    {ns / 1000:>8g} us   {column}   {conditions}")


if __name__ == "__main__":
    main(sys.argv[1:])
