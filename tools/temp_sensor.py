"""Extract the temperature sensor's conversion constants from MSPM0 datasheets.

`FACTORYREGION.TEMP_SENSE0` holds one ADC code per device: the sensor's output at the factory trim
temperature. Turning a later reading into a temperature needs three numbers that live only in the
datasheet -- the trim temperature itself, the sensor's slope, and how long the ADC has to sample --
plus one fact about how the factory measurement was taken. This writes
`data/temp_sensor/<family>.yaml`.

Like the other `tools/` scripts this is an offline aid rather than part of the build. Re-run it
after a data source bump rather than editing the YAML by hand.

Usage:
    uv run tools/temp_sensor.py <datasheet.pdf> [...]      # print what it finds
    uv run tools/temp_sensor.py --write <dir-of-datasheets> # regenerate data/temp_sensor

`uv run` installs the dependencies below on its own; a bare `python` needs them on the path.

Four things about these rows are worth knowing:

- **The settling time is a MAX, and it is a minimum ADC sample time.** The datasheets' own footnote
  says so: "This is the maximum time required for the temperature sensor to settle when measured by
  the ADC. It may be used to specify the minimum ADC sample time." A driver sampling faster gets a
  plausible-looking wrong reading, and the figure ranges 10us to 12.5us across the portfolio.

- **The sample time the factory used is a separate number** and is not always the settling time. The
  older L datasheets state a 12.5us calibration sample against their own 10us settling maximum, so
  reproducing the factory measurement and merely letting the sensor settle are two different
  requirements.

- **The reference the factory calibrated against varies per device**, and there is no portfolio
  default: the older G families and TI's own worked example use VDD, seven datasheets name the 1.4V
  internal reference, and the H3216 names 4.05V. A code taken against a different reference has to
  be rescaled before it is compared with `TEMP_SENSE0`, so this is the fact that decides whether a
  driver is right or merely plausible.

- **Three datasheets contradict themselves about it.** MSPM0L1106, MSPM0L1306 and MSPM0L1346 give
  the electrical table's test conditions as `VRSEL=2h (internal VREF), BUFCONFIG=1h (1.4V VREF)`
  and the peripheral section's prose as `VRSEL=0h (VDD)`, for the same measurement in the same
  document. Neither half is a wording artifact -- both name `VRSEL`. The tool reports the conflict
  and keeps both readings; `data/temp_sensor_overrides.yaml` resolves it, with the evidence.

No document settles that one. SLAU847 says "the 1.4V internal voltage reference" outright, but
SLAU846 and SLAU923 carry the same sentence with the reference edited out -- leaving the tell-tale
"the a voltage reference used during factory calibration" -- so the specific value there is stale
text rather than an L-series statement. All three then share one worked example, at 1.4V, with a
slope matching no device in any datasheet. driverlib repeats the 1.4V claim in a single
portfolio-wide header, while TI's own G3507 and G3519 examples compute against 3.3V.

**Hardware did settle it, and the electrical table was the half that was right.** An L1306's
`TEMP_SENSE0` reads 1885, which is 644mV against a 1.4V reference and 1519mV against VDD; only the
first is a temperature sensor output, and converting a live reading against the 1.4V trim gives
room temperature where the VDD trim gives 527degC. Worth remembering the next time a specification
table and a peripheral-section paragraph disagree: the table won.
"""

# /// script
# requires-python = ">=3.10"
# dependencies = ["pdfplumber", "pypdfium2", "pyyaml"]
# ///

import re
import sys
from dataclasses import dataclass
from pathlib import Path

try:
    import pdfplumber
    import pypdfium2
    import yaml
except ImportError as e:  # pragma: no cover
    raise SystemExit(f"{e.name} is missing; run this with `uv run` instead of `python`")

LATTICE = {"vertical_strategy": "lines", "horizontal_strategy": "lines"}

#: Time units the sample and settling figures may be given in, as a multiplier to nanoseconds.
UNITS = {"ns": 1, "us": 1_000, "µs": 1_000, "ms": 1_000_000}

NUMBER = re.compile(r"-?\d+(?:\.\d+)?")

#: A cell holding nothing but the MIN/TYP/MAX figures. The three columns are merged into one cell
#: by the lattice read, and a footnote marker in the neighbouring description is a number too.
FIGURES_ONLY = re.compile(r"\s*-?\d+(?:\.\d+)?(?:\s+-?\d+(?:\.\d+)?)*\s*")

#: The paragraph of the peripheral section which describes the factory measurement. Its wording
#: drifts between datasheets ("with the 3.3V VDD reference", "with VDD = 3.3V", "with the 1.4V
#: internal VREF"), so the reference is read from the `VRSEL=` it quotes rather than from the prose.
CALIBRATION = re.compile(
    r"A unit-specific single-point calibration value.*?trim value\.", re.S
)

#: `VRSEL=<n>h`, wherever it appears. 0h is VDD, 2h the internal reference buffer, 4h the H3216's
#: 4.05V one. The datasheets write it with and without a space after the equals sign.
VRSEL = re.compile(r"VRSEL\s*=\s*(\d)h")

#: The ADC sample time the factory measurement used, from the same paragraph.
SAMPLE_TIME = re.compile(r"t\s*(?:Sample|sample)\s*=\s*(\d+(?:\.\d+)?)\s*(us|µs|ns|ms)")

#: The same figure as the electrical table's test conditions state it. The subscript comes back
#: either attached (`tsample = 12.5uS`) or displaced to the end of the cell (`ADC t = 12.5uS
#: sample`), so the word is optional here where the prose always has it.
TABLE_SAMPLE_TIME = re.compile(
    r"ADC\s*t\s*(?:sample)?\s*=\s*(\d+(?:\.\d+)?)\s*(us|µs|ns|ms)", re.I
)

#: Corrections a better source settles, applied after the datasheet read.
OVERRIDES = Path("data/temp_sensor_overrides.yaml")

#: What each `VRSEL` encoding selects, for the generated file and for the metadata.
REFERENCES = {0: "vdd", 2: "internal_1v4", 4: "internal_4v05"}

#: The newer datasheets name the reference in prose and quote no ADC configuration, so the phrase
#: has to be read too. "with VDD = 3.3V" is deliberately absent: it names the supply rather than the
#: reference, and every datasheet which uses it also quotes a `VRSEL`.
PHRASES = (
    (re.compile(r"with the 1\.4-?V internal", re.I), 2),
    (re.compile(r"with the 4\.05-?V internal", re.I), 4),
    (re.compile(r"with the 3\.3-?V VDD reference", re.I), 0),
)


@dataclass
class TempSensor:
    """One family's temperature sensor constants."""

    tstrim_c: int
    tsc_uv_per_c: int
    #: `None` on the two families whose datasheet has no tSET,TS row.
    settling_ns: int | None
    calibration_sample_ns: int | None
    #: `VRSEL` per the peripheral section's prose, and per the electrical table's test conditions.
    #: They disagree on three families, which is why both are kept.
    reference_prose: int | None
    reference_table: int | None

    @property
    def reference(self) -> str | None:
        """The reference both halves agree on, or `None` where the datasheet contradicts itself.

        Either half alone is enough: the newer datasheets name the reference in prose and quote no
        ADC configuration, and the C1104's electrical table quotes one against a bare TSTRIM row.
        """
        if self.conflicting:
            return None

        stated = self.reference_prose if self.reference_prose is not None else self.reference_table

        return REFERENCES.get(stated) if stated is not None else None

    @property
    def conflicting(self) -> bool:
        """Whether the datasheet's two statements of the reference disagree."""
        return (
            self.reference_prose is not None
            and self.reference_table is not None
            and self.reference_prose != self.reference_table
        )


def page_text(path: Path) -> list[str]:
    """The text of every page, read with pypdfium2 as the other tools do."""
    doc = pypdfium2.PdfDocument(str(path))
    try:
        return [doc[i].get_textpage().get_text_range() for i in range(len(doc))]
    finally:
        doc.close()


def figures(cell: str) -> list[float]:
    """Every number in a merged MIN/TYP/MAX cell, in column order."""
    return [float(n) for n in NUMBER.findall(cell)]


def typical(cell: str) -> float | None:
    """The TYP figure of a merged MIN/TYP/MAX cell.

    A full row gives three numbers and the middle one is TYP; a row which fills only one column
    gives one. Anything else means the layout changed and the caller should report it.
    """
    found = figures(cell)
    if len(found) == 3:
        return found[1]
    if len(found) == 1:
        return found[0]

    return None


def maximum(cell: str) -> float | None:
    """The MAX figure of a merged cell, which is the last column that carries a number."""
    found = figures(cell)

    return found[-1] if found else None


def read(path: Path) -> tuple[TempSensor | None, str]:
    """Read one datasheet. Returns the constants and a note about anything unusual."""
    pages = page_text(path)

    # Searched over the whole document rather than page by page: on the L1105/L1106 datasheet the
    # paragraph straddles a page break, and a per-page search silently found no statement there and
    # fell back to the electrical table -- which is the half those datasheets contradict.
    prose_vrsel = sample_ns = None
    flat = " ".join(" ".join(pages).split())
    match = CALIBRATION.search(flat)
    if match:
        found = VRSEL.search(match.group(0))
        if found:
            prose_vrsel = int(found.group(1))
        else:
            prose_vrsel = next(
                (vrsel for phrase, vrsel in PHRASES if phrase.search(match.group(0))), None
            )
        sample = SAMPLE_TIME.search(match.group(0))
        if sample:
            sample_ns = round(float(sample.group(1)) * UNITS[sample.group(2).lower()])

    rows: dict[str, tuple[str, str]] = {}
    table_vrsel = None
    for index, text in enumerate(pages):
        if "TSTRIM" not in text.replace(" ", ""):
            continue

        with pdfplumber.open(str(path)) as pdf:
            for table in pdf.pages[index].extract_tables(LATTICE):
                for row in table:
                    cells = [(c or "").replace("\n", " ").strip() for c in row]
                    joined = " ".join(cells)
                    squashed = joined.replace(" ", "")
                    markers = (("tstrim", "TSTRIM"), ("tsc", "TSc"), ("tset", "tSET,"))
                    hit = [key for key, marker in markers if marker in squashed]
                    if not hit:
                        continue

                    found = VRSEL.search(joined)
                    if found and table_vrsel is None:
                        table_vrsel = int(found.group(1))

                    # The table states the calibration sample time on the datasheets whose prose
                    # quotes no ADC configuration.
                    sample = TABLE_SAMPLE_TIME.search(joined)
                    if sample and sample_ns is None:
                        sample_ns = round(
                            float(sample.group(1)) * UNITS[sample.group(2).lower()]
                        )

                    unit = next((c for c in reversed(cells) if c.lower() in UNITS), "")
                    # The figures share one cell. Pick the cell which is nothing but numbers, so a
                    # footnote marker in the description ("Factory trim temperature (2)") cannot be
                    # mistaken for one.
                    values = next((c for c in cells if FIGURES_ONLY.fullmatch(c)), "")
                    for key in hit:
                        if key not in rows:
                            rows[key] = (values, unit)
        if rows:
            break

    missing = [key for key in ("tstrim", "tsc") if key not in rows]
    if missing:
        return None, f"no {', '.join(missing)} row"

    tstrim = typical(rows["tstrim"][0])
    tsc = typical(rows["tsc"][0])
    if tstrim is None or tsc is None:
        return None, "a row did not parse into MIN/TYP/MAX"

    # The L1227/L1228/L2227/L2228 table has no tSET,TS row at all, so the minimum sample time is
    # unstated for those parts rather than misread.
    settling_ns = None
    if "tset" in rows:
        settling, unit = maximum(rows["tset"][0]), rows["tset"][1]
        if settling is None or unit.lower() not in UNITS:
            return None, "the tSET,TS row did not parse"
        settling_ns = round(settling * UNITS[unit.lower()])

    sensor = TempSensor(
        tstrim_c=round(tstrim),
        tsc_uv_per_c=round(tsc * 1000),
        settling_ns=settling_ns,
        calibration_sample_ns=sample_ns,
        reference_prose=prose_vrsel,
        reference_table=table_vrsel,
    )

    note = ""
    if sensor.conflicting:
        note = (
            f"the peripheral section says VRSEL={prose_vrsel}h and the electrical table says "
            f"VRSEL={table_vrsel}h"
        )
    elif sensor.reference is None:
        note = "no calibration reference stated"
    if sensor.settling_ns is None:
        note = f"{note}; no tSET,TS row".lstrip("; ")

    return sensor, note


HEADER = """\
# The temperature sensor's conversion constants for this family.
#
# GENERATED by `tools/temp_sensor.py --write` from the {ds} datasheet's Temperature Sensor rows.
# Re-run the tool rather than editing by hand.
#
# tstrim_c              the factory trim temperature, in degrees Celsius (the TYP column; the
#                       datasheet's MIN and MAX bound where the trim was actually taken)
# tsc_uv_per_c          the sensor's slope in microvolts per degree Celsius, always negative
# settling_ns           the MAX of tSET,TS. The datasheet's own footnote calls this the minimum ADC
#                       sample time for the sensor, not a figure to note and move past
# calibration_sample_ns the ADC sample time the factory measurement used, which is not always the
#                       same number
# calibration_reference which reference TEMP_SENSE0 was measured against; null where the datasheet
#                       contradicts itself{conflict}
"""


def emit(sensor: TempSensor, resolved: str | None = None) -> dict:
    """The YAML body. A contradiction keeps both readings whether or not an override settles it."""
    body = {
        "tstrim_c": sensor.tstrim_c,
        "tsc_uv_per_c": sensor.tsc_uv_per_c,
        "settling_ns": sensor.settling_ns,
        "calibration_sample_ns": sensor.calibration_sample_ns,
        "calibration_reference": resolved or sensor.reference,
    }

    if sensor.conflicting:
        body["conflicting_references"] = [
            REFERENCES.get(sensor.reference_prose),
            REFERENCES.get(sensor.reference_table),
        ]

    return body


def write(datasheets: str) -> int:
    """Regenerate data/temp_sensor/*.yaml. Returns the number of problems reported."""
    parts = yaml.safe_load(Path("data/parts.yaml").read_text(encoding="utf-8"))
    families = [(f["family"], f["datasheet_url"].rsplit("/", 1)[-1]) for f in parts["families"]]

    overrides = yaml.safe_load(OVERRIDES.read_text(encoding="utf-8")) if OVERRIDES.exists() else {}

    Path("data/temp_sensor").mkdir(parents=True, exist_ok=True)
    problems = 0

    for family, gpn in families:
        pdf = Path(datasheets) / f"{gpn}_datasheet.pdf"
        if not pdf.exists():
            print(f"{family}: no datasheet for {gpn} in {datasheets}")
            problems += 1
            continue

        sensor, note = read(pdf)
        if sensor is None:
            print(f"{family}: {note} in {pdf.name}")
            problems += 1
            continue

        override = (overrides or {}).get(family, {})
        resolved = override.get("calibration_reference")

        conflict = ""
        if note:
            # A recorded conflict is data, not a failure: it is the same on every revision of the
            # three datasheets which have it, and a consumer needs to know rather than be told a
            # value the document does not support.
            print(f"{family}: {note}")
            conflict = f"\n#\n# The {gpn} datasheet contradicts itself here: {note}."
            if resolved:
                conflict += (
                    f"\n# Resolved to {resolved} by data/temp_sensor_overrides.yaml"
                    f" ({override.get('evidence', 'no evidence stated')}), which holds the reasoning."
                )

        body = yaml.safe_dump(emit(sensor, resolved), sort_keys=False)
        out = Path("data/temp_sensor") / f"{family}.yaml"
        out.write_text(HEADER.format(ds=gpn, conflict=conflict) + body, encoding="utf-8")

    return problems


def main(argv: list[str]) -> int:
    if len(argv) >= 2 and argv[0] == "--write":
        return 1 if write(argv[1]) else 0

    if not argv:
        raise SystemExit(__doc__)

    for name in argv:
        sensor, note = read(Path(name))
        print(f"{Path(name).name}: {sensor}{' -- ' + note if note else ''}")

    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
