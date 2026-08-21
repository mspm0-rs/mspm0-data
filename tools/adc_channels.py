"""Extract which internal signals the ADC channels sample from MSPM0 datasheets.

Some ADC channel numbers do not go to a package pin: they sample the temperature sensor, an OPA or
GPAMP output, the internal reference, or a supply monitor. Which channel carries which signal is
published only in the datasheet's "ADC Channel Mapping" table, and differs per family and per ADC
instance. This writes `data/adc_channels/<family>.yaml`.

Like `tools/vref.py`, this is an offline aid rather than part of the build. Re-run it after a data
source bump rather than editing the YAML by hand.

Usage:
    uv run tools/adc_channels.py <datasheet.pdf> [...]       # print what it finds
    uv run tools/adc_channels.py --write <dir-of-datasheets> # regenerate data/adc_channels

`uv run` installs the dependencies below on its own; a bare `python` needs them on the path.

The obvious machine-readable source is worse. The SDK's
`source/ti/driverlib/.meta/adc12/ADC12_internalConnections.js` holds the same mapping, but keyed by
SDK family, and an SDK family is a superset of its devices: it gives MSPM0G110x and MSPM0G310x the
OPA routes of the G150x/G350x they share a key with, when those parts have no OPA and their
datasheets leave channel 13 empty. It also omits channel 28 (VREF) on MSPM0L122x/L222x, and lists
channel 12 as "Internal VREF" on MSPM0C110x where the datasheet shows `-` and TI's own channel-range
comment in the same file excludes 12. The sysconfig `SYS_OA*_CHANNELS` attributes repeat the
superset mistake. The datasheets are per part and correct on all five points, so they are the
source.

Reading the table:

- A cell is a package pin (`A5`, `A1_12`), empty, `Reserved`, or an internal signal. The datasheet's
  own rule is footnote (1): "Italicized signal names are purely internal to the SoC". Italics do not
  survive table extraction, so the internal names are matched against a fixed list instead, and a
  cell which is neither a pin nor a known name is reported as a problem rather than skipped -- a new
  datasheet's new signal name has to be classified by a human.
- A shared cell (`A1_0 / DAC_OUT`) is a pin and an internal signal on one channel; the pin half is
  dropped. `VREF+`/`VREF-` name the external reference pins, not the internal reference, and are
  dropped with the pins.
"""

# /// script
# requires-python = ">=3.10"
# dependencies = ["pdfplumber", "pypdfium2", "pyyaml"]
# ///

import re
import sys
from pathlib import Path

try:
    import pdfplumber
    import pypdfium2
    import yaml
except ImportError as e:  # pragma: no cover
    raise SystemExit(f"{e.name} is missing; run this with `uv run` instead of `python`")

CAPTION = re.compile(r"ADC\d* Channel Mapping")

LATTICE = {"vertical_strategy": "lines", "horizontal_strategy": "lines"}

#: Internal signal names as the datasheets print them, lowercased, with whitespace around any `/`
#: removed. The values are the `AdcInternalSource` variants of `mspm0-data-types`.
INTERNAL = {
    "temperature sensor": "TemperatureSensor",
    "opa0 output": "Opa0",
    "opa1 output": "Opa1",
    "gpamp output": "Gpamp",
    "dac_out": "Dac0",
    "vref": "Vref",
    "vrefint": "Vref",
    "supply monitor": "SupplyMonitor",
    "supply/battery monitor": "SupplyMonitor",
    "vbat monitor": "VbatMonitor",
    "vusb monitor": "VusbMonitor",
}

#: A package pin, `A5` or `A1_12`, or the external reference pins a channel can share a pin with.
EXTERNAL = re.compile(r"A\d+(_\d+)?|VREF[+-]")

FOOTNOTE = re.compile(r"\(\d+\)")

SUBHEADER = re.compile(r"ADC\d+")


class Problem(Exception):
    pass


def caption_pages(path: Path) -> list[int]:
    """Indices of the pages carrying the ADC channel-mapping table."""
    doc = pypdfium2.PdfDocument(str(path))
    try:
        return [i for i in range(len(doc)) if CAPTION.search(doc[i].get_textpage().get_text_range())]
    finally:
        doc.close()


def read_tables(path: Path) -> dict[str, dict[int, str]]:
    """Instance name to channel number to internal source, from every channel-mapping table."""
    channels: dict[str, dict[int, str]] = {}
    with pdfplumber.open(str(path)) as pdf:
        for page_index in caption_pages(path):
            for table in pdf.pages[page_index].extract_tables(LATTICE):
                read_table(table, channels)

    return channels


def read_table(table: list[list[str | None]], channels: dict[str, dict[int, str]]) -> None:
    """Fold one extracted table into `channels`. A table without the expected header is not the
    channel-mapping table and is ignored; the caption page carries others."""
    width = max(len(row) for row in table)
    rows = [
        [(cell or "").replace("\n", " ").strip() for cell in row] + [""] * (width - len(row))
        for row in table
    ]

    header = next(
        (
            row
            for row in rows
            if any(c.upper().startswith("CHANNEL") for c in row)
            and any("SIGNAL NAME" in c.upper() for c in row)
        ),
        None,
    )
    if header is None:
        return

    channel_columns = [i for i, c in enumerate(header) if c.upper().startswith("CHANNEL")]

    # Signal columns run from each channel column to the next. Their instance is named either by a
    # sub-header row of ADCn labels (the dual-ADC layouts) or inside the header cell itself, as
    # "Signal Name (ADC0)"; a table naming neither has one ADC, which every datasheet calls ADC0.
    instance_of = {}
    subheader = next(
        (
            row
            for row in rows
            if row is not header
            and not any(row[i].isdigit() for i in channel_columns if i < len(row))
            and any(SUBHEADER.fullmatch(c) for c in row)
        ),
        None,
    )
    for start, end in zip(channel_columns, channel_columns[1:] + [len(header)]):
        signal_columns = list(range(start + 1, end))
        for column in signal_columns:
            if subheader is not None and SUBHEADER.fullmatch(subheader[column]):
                instance_of[column] = subheader[column]
            elif named := SUBHEADER.search(header[column]):
                instance_of[column] = named.group(0)
            elif len(signal_columns) == 1:
                instance_of[column] = "ADC0"
            else:
                raise Problem(f"cannot name the ADC instance of column {column}")

    for row in rows:
        for start, end in zip(channel_columns, channel_columns[1:] + [len(header)]):
            if not row[start].isdigit():
                continue

            channel = int(row[start])
            for column in range(start + 1, end):
                source = read_cell(row[column])
                if source is None:
                    continue

                instance = instance_of[column]
                already = channels.setdefault(instance, {}).setdefault(channel, source)
                if already != source:
                    raise Problem(f"{instance} channel {channel} read as {already} and {source}")


def read_cell(cell: str) -> str | None:
    """The internal source a cell names, or `None` for pins, `Reserved` and empty cells."""
    text = " ".join(FOOTNOTE.sub(" ", cell).split())
    if text in ("", "-", "–", "Reserved"):
        return None

    key = re.sub(r"\s*/\s*", "/", text).lower()
    if key in INTERNAL:
        return INTERNAL[key]

    parts = [p for p in re.split(r"\s*/\s*", text) if p]
    internal = [INTERNAL[p.lower()] for p in parts if p.lower() in INTERNAL]
    external = [p for p in parts if EXTERNAL.fullmatch(p)]
    if len(internal) + len(external) != len(parts):
        unknown = [p for p in parts if p not in external and p.lower() not in INTERNAL]
        raise Problem(f"unrecognised signal {unknown} in cell {cell!r}")
    if len(internal) > 1:
        raise Problem(f"two internal signals in cell {cell!r}")

    return internal[0] if internal else None


HEADER = """\
# Which internal signals this family's ADC channels sample.
#
# GENERATED by `tools/adc_channels.py --write` from the ADC channel-mapping table of the {ds}
# datasheet. Re-run the tool rather than editing by hand.
#
# Channels not listed here go to package pins or nowhere.
"""


def write(datasheets: str) -> int:
    """Regenerate data/adc_channels/*.yaml. Returns the number of problems reported."""
    parts = yaml.safe_load(Path("data/parts.yaml").read_text(encoding="utf-8"))
    families = [(f["family"], f["datasheet_url"].rsplit("/", 1)[-1]) for f in parts["families"]]

    Path("data/adc_channels").mkdir(parents=True, exist_ok=True)
    problems = 0

    for family, gpn in families:
        pdf = Path(datasheets) / f"{gpn}_datasheet.pdf"
        if not pdf.exists():
            print(f"{family}: no datasheet for {gpn} in {datasheets}")
            problems += 1
            continue

        try:
            channels = read_tables(pdf)
        except Problem as problem:
            print(f"{family}: {problem} in {pdf.name}")
            problems += 1
            continue

        if not channels:
            print(f"{family}: no ADC channel-mapping table in {pdf.name}")
            problems += 1
            continue

        path = Path(f"data/adc_channels/{family}.yaml")
        with open(path, "w", encoding="utf-8", newline="\n") as out:
            out.write(HEADER.format(ds=gpn.upper()))
            for instance, mapping in sorted(channels.items()):
                out.write(f"{instance}:\n")
                for channel, source in sorted(mapping.items()):
                    out.write(f"  {channel}: {source}\n")

        found = ", ".join(
            f"{instance} {{{', '.join(f'{ch}: {src}' for ch, src in sorted(mapping.items()))}}}"
            for instance, mapping in sorted(channels.items())
        )
        print(f"{family}: {found}")

    return problems


def main(argv: list[str]) -> None:
    if "--write" in argv:
        i = argv.index("--write")
        raise SystemExit(1 if write(argv[i + 1] if i + 1 < len(argv) else ".") else 0)

    paths = [Path(a) for a in argv if not a.startswith("--")]
    if not paths:
        raise SystemExit(__doc__)

    for path in paths:
        print(f"########## {path.name}")
        for instance, mapping in sorted(read_tables(path).items()):
            for channel, source in sorted(mapping.items()):
                print(f"    {instance} channel {channel:>2}: {source}")


if __name__ == "__main__":
    main(sys.argv[1:])
