"""Extract the ADC's minimum sample window for each internal signal, from MSPM0 datasheets.

The ADC's switching-characteristics table states a separate `tSample_<signal>` row for most of the
signals routed to an internal channel, and those minimums are far above the bare-pin `tSample`:
10us for the internal reference against a 156ns pin on mspm0l211x. This writes
`data/adc_internal_sample/<family>.yaml`. Nothing machine-readable carries any of it.

Keyed by **signal**, not by channel, because that is how the datasheets state it -- one row per
signal per family. `apply_adc` copies the figure onto every channel routing that signal, which is
where a consumer looks for it, and a dual-ADC part reaching one signal from both ADCs gets the same
number on both routes.

Two signals are deliberately not extracted here:

- **The temperature sensor**, whose rows are in `data/temp_sensor/` already and are *two* quantities
  rather than one: `tSET,TS` (the settling maximum) and the `ADC tSample` the factory calibration
  used, which differ -- 10us against 12.5us on the older L datasheets -- and which are independently
  absent. One scalar cannot hold both, and dropping the calibration figure is the quiet failure,
  since a window long enough to settle but shorter than the factory's reads plausibly and drifts.
- **The OPA outputs**, whose `tSample_PGA` row is keyed by PGA gain rather than being a scalar and
  lives in `Adc::pga_sample_ns` (`tools/adc_sample.py`).

Usage:
    uv run tools/adc_internal_sample.py <datasheet.pdf> [...]       # print what it finds
    uv run tools/adc_internal_sample.py --write <dir-of-datasheets> # regenerate

`uv run` installs the dependencies below on its own; a bare `python` needs them on the path.

**Every figure is in the MIN column**, which is what makes them safe to assert against. The table is
not ruled, so the column is decided geometrically against the MIN/TYP/MAX header positions, the same
way `adc_wakeup.py` and `adc_sample.py` do it.

Three traps this had to survive, all silent:

- **A unit cell can be blank.** The MSPM0L2117 `tSample_VREF` row prints `10` with no `us` where
  every neighbouring row has one, so the unit falls back to the table's rather than the row's.
- **A figure and its condition can sit on different lines**, the condition wrapping above and below
  it. That is why the row is assembled from a small window of lines rather than from one.
- **A footnote can contradict the row it annotates.** The MSPM0L1228/L2228 footnote calls both
  supply-monitor dividers `VDD/3` while the row itself is named `Supply Monitor (VBAT/3)`. The row
  wins -- a monitor dividing a rail other than its own reports nothing about that rail -- so the
  signal is taken from the parameter name and never from the footnote.
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

NUMBER = re.compile(r"^\d+(?:\.\d+)?$")

#: The channel named inside the `tSample_VREF` condition. Extracted on its own rather than as part
#: of the condition text, because it is a cross-check worth having and a single token is robust
#: where the whole wrapped condition is not: it must equal the channel `data/adc_channels/` routes
#: `Vref` to, which is one page of the datasheet corroborating another.
CHANNEL = re.compile(r"CHANNEL\s*=\s*(\d+)", re.IGNORECASE)

#: The row has to be a sampling-time row. Without this, the signal names also match the accuracy and
#: supply-current rows of the electrical table -- `VSupplyMon ADC input channel: Supply Monitor` sits
#: on the same page and would otherwise be read as a 1.5us MAX sample window.
SAMPLE_ROW = re.compile(r"Sampl\w*\s+time\s+with\b", re.IGNORECASE)

#: Each `tSample_<signal>` row, mapped to its `AdcInternalSource` variant. Order matters: the VBAT
#: monitor's row name contains the VDD monitor's, so it has to be tested first. `tSample_PGA` and
#: the temperature sensor rows are deliberately absent -- see the module docs.
SIGNALS = [
    (re.compile(r"Supply\s+Monitor\s*\(?\s*VBAT", re.IGNORECASE), "VbatMonitor"),
    (re.compile(r"VUSB\s+Monitor|USBMon", re.IGNORECASE), "VusbMonitor"),
    (re.compile(r"Supply\s+Monitor", re.IGNORECASE), "SupplyMonitor"),
    (re.compile(r"\bwith\s+(internal\s+)?VREF\b", re.IGNORECASE), "Vref"),
    (re.compile(r"\bwith\s+GPAMP\b", re.IGNORECASE), "Gpamp"),
    (re.compile(r"\bwith\s+DAC\b", re.IGNORECASE), "Dac0"),
]

#: The test condition each row is stated under. Uniform across every family that states the row, so
#: it is recorded once here rather than extracted per family.
#:
#: The reference matters and differs per signal. The internal reference is measured with VDD as the
#: ADC reference because it cannot be measured against itself, so a caller sampling that channel
#: with `VRSEL` on the internal reference is outside the published figure. The supply and USB
#: monitors are the other way round, measured against the internal reference.
CONDITIONS = {
    "SupplyMonitor": "12-bit mode; internal reference (VRSEL=1h or 2h) where stated",
    "VbatMonitor": "12-bit mode; the VBAT divider, not the VDD one the footnote describes",
    "VusbMonitor": "12-bit mode, internal reference (VRSEL=1h or 2h)",
    "Vref": "12-bit mode, VDD as reference -- the internal reference cannot measure itself",
    "Gpamp": "12-bit mode",
    "Dac0": "12-bit mode",
}

#: Rows are grouped by rounding the word top to this many points.
ROW_TOLERANCE = 3

#: How many lines below a parameter row its figure may wrap to.
WRAP = 2


def centre(word: dict) -> float:
    return (word["x0"] + word["x1"]) / 2


def rows_of(page) -> dict[int, list[dict]]:
    """Words grouped into visual rows, keyed by rounded vertical position."""
    grouped: dict[int, list[dict]] = {}
    for word in page.extract_words():
        grouped.setdefault(round(word["top"] / ROW_TOLERANCE), []).append(word)
    return grouped


def read(path: Path) -> dict[str, tuple[str, int]]:
    """Return `{signal: (column, nanoseconds)}` for one datasheet.

    The test conditions are deliberately not extracted. They are uniform per signal across every
    family that states the row, so a per-family reading of them adds nothing and is fragile: TI
    centres the condition on its row, wrapping it above *and* below the figure, so no line window
    separates one row's condition from its neighbour's reliably. They are recorded once, as prose,
    in `CONDITIONS` below.
    """
    found: dict[str, tuple[str, int]] = {}
    vref_channel: int | None = None

    with pdfplumber.open(str(path)) as pdf:
        for page in pdf.pages:
            text = page.extract_text() or ""
            if "Sample time with" not in text and "Sampling time with" not in text:
                continue

            grouped = rows_of(page)
            keys = sorted(grouped)
            headers: dict[str, float] = {}
            # The unit cell is blank on one row of one datasheet, so the table's unit carries.
            page_unit = ""

            for index, key in enumerate(keys):
                words = sorted(grouped[key], key=lambda w: w["x0"])
                line = " ".join(w["text"] for w in words)

                if "MIN" in line and "MAX" in line:
                    headers = {
                        w["text"]: centre(w) for w in words if w["text"] in ("MIN", "TYP", "MAX")
                    }

                if not headers:
                    continue

                unit = next((w["text"] for w in reversed(words) if w["text"].lower() in UNITS), "")
                page_unit = unit or page_unit

                if not SAMPLE_ROW.search(line):
                    continue
                signal = next((name for pattern, name in SIGNALS if pattern.search(line)), None)
                if signal is None or signal in found:
                    continue

                low = min(headers.values()) - 20
                # The figure may wrap onto a following line, taking its unit with it.
                for follow in keys[index : index + 1 + WRAP]:
                    segment = sorted(grouped[follow], key=lambda w: w["x0"])
                    figures = [
                        w for w in segment if NUMBER.fullmatch(w["text"]) and centre(w) > low
                    ]
                    if not figures:
                        continue
                    figure = figures[-1]
                    row_unit = next(
                        (w["text"] for w in reversed(segment) if w["text"].lower() in UNITS), ""
                    )
                    scale = UNITS[(row_unit or unit or page_unit or "us").lower()]
                    column = min(headers, key=lambda c: abs(headers[c] - centre(figure)))
                    found[signal] = (column, math.ceil(float(figure["text"]) * scale))
                    if signal == "Vref":
                        # Nearest `CHANNEL=n` to this row. A wrong pick shows up immediately as a
                        # verify.rs failure rather than as silently wrong data, since every family
                        # stating the row agrees with the channel map today.
                        near = [
                            (abs(k - key), m.group(1))
                            for k in keys[max(0, index - WRAP) : index + 1 + WRAP]
                            for m in [CHANNEL.search(" ".join(w["text"] for w in grouped[k]))]
                            if m
                        ]
                        if near:
                            vref_channel = int(min(near)[1])
                    break

    return found, vref_channel


HEADER = """\
# The minimum ADC sample window for each internal signal on this family, in nanoseconds.
#
# GENERATED by `tools/adc_internal_sample.py --write` from the {ds} datasheet's ADC switching
# characteristics. Re-run the tool rather than editing by hand.
#
# Keyed by signal because that is how the datasheet states it; the generator copies each figure onto
# every channel routing that signal. Every figure is the MIN column of a `tSample_<signal>` row.
#
# These are far above the bare-pin minimum in data/adc_sample/: the internal reference alone can be
# 10us against a 156ns pin. A window sized for the pin samples an internal signal that has not
# settled, and nothing reports it.
#
# The temperature sensor is not here -- its two figures live in data/temp_sensor/ and differ from
# each other. Nor are the OPA outputs, whose row is keyed by PGA gain in data/adc_sample/.
#
# Conditions are quoted per signal because they differ per signal and are load-bearing: the internal
# reference is measured with VDD as the ADC reference, since it cannot be measured against itself.
"""


def write(datasheets: str) -> int:
    """Regenerate data/adc_internal_sample/*.yaml. Returns the number of problems reported."""
    parts = yaml.safe_load(Path("data/parts.yaml").read_text(encoding="utf-8"))
    families = [(f["family"], f["datasheet_url"].rsplit("/", 1)[-1]) for f in parts["families"]]

    Path("data/adc_internal_sample").mkdir(parents=True, exist_ok=True)
    problems = 0

    for family, gpn in families:
        pdf = Path(datasheets) / f"{gpn}_datasheet.pdf"
        if not pdf.exists():
            print(f"{family}: no datasheet for {gpn} in {datasheets}")
            problems += 1
            continue

        found, vref_channel = read(pdf)
        if not found:
            print(f"{family}: no tSample_<signal> rows in {pdf.name}")
            problems += 1
            continue

        off_column = {s: c for s, (c, _) in found.items() if c != "MIN"}
        if off_column:
            print(f"{family}: {off_column} is not a MIN in {pdf.name}")
            problems += 1
            continue

        path = Path(f"data/adc_internal_sample/{family}.yaml")
        with open(path, "w", encoding="utf-8", newline="\n") as out:
            out.write(HEADER.format(ds=gpn.upper()))
            if vref_channel is not None:
                out.write(
                    "\n# The channel the tSample_VREF condition names. Cross-checked against the\n"
                    "# channel data/adc_channels/ routes Vref to -- one page of the datasheet\n"
                    "# corroborating another, which is the only independent check on a channel map.\n"
                    f"vref_channel: {vref_channel}\n"
                )
            out.write("\nsample_min_ns:\n")
            for signal in sorted(found):
                out.write(f"  # {CONDITIONS[signal]}\n")
                out.write(f"  {signal}: {found[signal][1]}\n")

        summary = ", ".join(f"{s} {found[s][1]}" for s in sorted(found))
        print(f"{family}: {summary}, from {pdf.name}")

    return problems


def main(argv: list[str]) -> None:
    if "--write" in argv:
        i = argv.index("--write")
        raise SystemExit(1 if write(argv[i + 1] if i + 1 < len(argv) else ".") else 0)

    paths = [Path(a) for a in argv if not a.startswith("--")]
    if not paths:
        raise SystemExit(__doc__)

    for path in paths:
        found, vref_channel = read(path)
        print(f"########## {path.name}")
        if vref_channel is not None:
            print(f"    tSample_VREF condition names ADC channel {vref_channel}")
        if not found:
            print("    no tSample_<signal> rows")
        for signal in sorted(found):
            column, ns = found[signal]
            print(f"    {signal:16} {ns:>7} ns  {column}   {CONDITIONS[signal]}")


if __name__ == "__main__":
    main(sys.argv[1:])
