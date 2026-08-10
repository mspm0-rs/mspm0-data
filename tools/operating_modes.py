"""Extract the "Supported Functionality by Operating Mode" table from MSPM0 datasheets.

This is the source for the low power data which has no machine-readable origin: how deep a sleep each
PD1 peripheral keeps its configuration through, and how deep each peripheral stays usable. It writes
`data/operating_modes/<family>.yaml`, so those values have a traceable path back to the datasheet
instead of being hand-transcribed.

Like `transforms/`, this is an offline aid rather than part of the build. Re-run it after a data
source bump rather than editing the YAML by hand.

Usage:
    uv run tools/operating_modes.py <datasheet.pdf> [...]          # print the table
    uv run tools/operating_modes.py --write <dir-of-datasheets>    # regenerate data/operating_modes

`uv run` installs the dependencies below on its own; a bare `python` needs them on the path.

Cells spanning several rows or columns mean the table needs lattice reconstruction from its ruling
lines; reading extracted text instead silently loses the spans.

    row.cells[i] is None  <=>  column i is covered by a merge that began earlier

STOP and STANDBY are reported per sub-mode, RUN and SLEEP as a whole. That asymmetry is in the
hardware, not a shortcut: STOP0/1/2 and STANDBY0/1 each disable a superset of the one before, but
RUN1 and RUN2 are *clock-source policies* rather than depths -- RUN2 runs the CPU with SYSOSC off,
and SLEEP0 turns it back on. Across all 16 datasheets every row whose usability switches back on
does so at `RUN2 -> SLEEP0` or `SLEEP2 -> STOP0`, and none within STOP or within STANDBY, so a
single threshold is only meaningful once RUN and SLEEP are each collapsed.

Which sub-modes exist varies: most families have STOP0/1/2, while the C-series, H3216 and L2117
tables have no STOP1 column at all. The sub-mode names are read from the table's second header row
rather than counted off, so a missing one cannot shift the rest.
"""

# /// script
# requires-python = ">=3.10"
# dependencies = ["pdfplumber", "pypdfium2", "pyyaml"]
# ///

import glob
import json
import re
import sys
from collections.abc import Iterable, Iterator
from pathlib import Path

try:
    import pdfplumber
    import pypdfium2
    import yaml
except ImportError as e:  # pragma: no cover
    raise SystemExit(f"{e.name} is missing; run this with `uv run` instead of `python`")

ColumnModes = dict[int, str]                       # column index -> PowerMode variant name
States = dict[int, str | None]                     # column index -> state, merges resolved
Rows = list[tuple[str, States]]                    # (row label, states)
ByMode = dict[str, set[str]]                       # PowerMode variant -> states seen in it
Overrides = dict[str, dict[str, dict[str, str]]]   # family -> field -> name -> value

CAPTION = "Supported Functionality by Operating Mode"

LATTICE = {"vertical_strategy": "lines", "horizontal_strategy": "lines"}

#: Mode groups in order of increasing depth. The datasheet names these as merged header cells.
GROUPS = ["RUN", "SLEEP", "STOP", "STANDBY", "SHUTDOWN"]

#: Groups reported per sub-mode rather than collapsed, and the `PowerMode` stem each maps onto.
SPLIT = {"STOP": "Stop", "STANDBY": "Standby"}

#: `PowerMode` variants a column can map to, deepest last. `Stop1` is absent on several families.
MODES = ["Run", "Sleep", "Stop0", "Stop1", "Stop2", "Standby0", "Standby1", "Shutdown"]

#: A sub-mode name from the table's second header row. That row is rotated, and pdfplumber returns
#: rotated text back to front ("0POTS"), so the cell is reversed before matching.
SUBMODE = re.compile(r"^(RUN|SLEEP|STOP|STANDBY)(\d)$")

#: Per the table's legend: EN and OPT mean the function is usable, DIS/OFF/NS do not. `NS` is "not
#: automatically disabled in the specified mode, but its use is not supported", which is the case a
#: boolean would hide. Only OFF loses configuration.
USABLE = ("EN", "OPT")

#: A cell's state, ignoring any footnote marker after it. Most are parenthesised ("OPT(2)"), but the
#: MSPM0C1106 GPIOA/B row renders one bare ("OPT2"), which a `\b` here would reject -- and rejecting
#: it silently costs that row its STANDBY1 column.
STATE = re.compile(r"^(EN|DIS|OPT|NS|OFF)(?![A-Za-z])")

#: Row labels which do not use the peripheral's own name.
ALIASES = {
    "MCAN": "CANFD",        # the datasheets call the CAN-FD peripheral MCAN
    "CRC-P": "CRCP",
    "CRCP": "CRCP",
    "USB2.0-FS": "USBFS",
    "USB": "USBFS",
    "I2S/TDM": "I2S",
    "CPU": "CPUSS",         # the "Core Functions" row covering the CPU subsystem
}


def caption_pages(path: Path) -> list[int]:
    """Indices of the pages carrying the table's caption.

    Located with PDFium rather than pdfplumber: pdfminer's content-stream parse costs ~90ms a page,
    and only the two or three pages found here need it.
    """
    doc = pypdfium2.PdfDocument(str(path))
    try:
        return [i for i in range(len(doc)) if CAPTION in doc[i].get_textpage().get_text_range()]
    finally:
        doc.close()


def read_tables(path: Path) -> Iterator[tuple[ColumnModes, Rows]]:
    """Yield (column_modes, rows) for each page of the operating-mode table.

    `column_modes` maps a column index to the `PowerMode` variant it reports; `rows` is a list of
    (label, states) where `states` is one entry per mode column, merges already resolved.
    """
    pages = caption_pages(path)
    if not pages:
        return

    with pdfplumber.open(str(path)) as pdf:
        for page in (pdf.pages[i] for i in pages):
            for table in page.find_tables(LATTICE):
                if len(table.rows) < 6 or max(len(r.cells) for r in table.rows) < 10:
                    continue
                data = table.extract()

                # The group row names each group in the column it starts at; the columns it spans
                # follow as `None` cells. It is used only to find where the mode columns begin --
                # which mode each one reports comes from the sub-mode row below it.
                header = [(c or "").replace("\n", "").strip() for c in data[0]]
                columns: list[int] = []
                seen_group = False
                for i, cell in enumerate(header):
                    if i < 2:
                        continue                     # group label and row label columns
                    seen_group |= any(cell.upper().startswith(g) for g in GROUPS)
                    if seen_group:
                        columns.append(i)

                column_mode = split_submodes(columns, data[1] if len(data) > 1 else [])
                if column_mode is None:
                    continue

                rows: Rows = []
                for ri, row in enumerate(table.rows[1:], start=1):
                    # A label wrapped inside its cell breaks mid-word ("MATHA"/"CL", "SYSOS"/"C",
                    # "TIMG6/7"/"/12"), so the lines join with no separator. Comma-separated lists
                    # keep their commas, so nothing is lost by not inserting spaces.
                    label = (data[ri][1] or "").replace("\n", "").strip()
                    if not label:
                        continue

                    states: States = {}
                    held: str | None = None
                    for ci in sorted(column_mode):
                        if ci >= len(row.cells):
                            continue
                        if row.cells[ci] is not None:
                            # A real cell boundary starts a new value here.
                            text = (data[ri][ci] or "").replace("\n", " ").strip()
                            m = STATE.match(text)
                            held = m.group(1) if m else None
                        states[ci] = held
                    rows.append((label, states))

                yield column_mode, rows


def split_submodes(columns: Iterable[int], subheader: list) -> ColumnModes | None:
    """Map each mode column to a `PowerMode` variant, or `None` if the sub-mode row does not parse.

    The sub-mode row is the authority, not the merged group row above it. RUN and SLEEP collapse to
    one variant each; STOP and STANDBY keep the index the table gives them, read rather than counted
    because several families have no STOP1.

    Deciding SHUTDOWN here is what makes MSPM0H3216 come out right. Its table stops at STANDBY1 with
    no SHUTDOWN column at all, and a trailing-column heuristic reads that last column as SHUTDOWN and
    loses the deeper half of STANDBY.
    """
    labels = [(c or "").replace("\n", "").strip()[::-1].upper() for c in subheader]
    last = max(columns, default=-1)

    out: ColumnModes = {}
    for ci in columns:
        label = labels[ci] if ci < len(labels) else ""
        m = SUBMODE.match(label)
        if m is None:
            # SHUTDOWN has no index, and its cell is routinely clipped ("SHUTDOW", "DOWN", "NWO").
            # Anywhere but the last column, an unreadable label means the row did not parse.
            if ci != last:
                return None
            out[ci] = "Shutdown"
            continue

        group, index = m.group(1), m.group(2)
        out[ci] = f"{SPLIT[group]}{index}" if group in SPLIT else group.capitalize()

    return out


def mode_states(column_mode: ColumnModes, states: States) -> ByMode:
    """States seen in each mode, as {mode: set of states}."""
    out: ByMode = {}
    for ci, mode in column_mode.items():
        s = states.get(ci)
        if s:
            out.setdefault(mode, set()).add(s)
    return out


def present(by_mode: ByMode) -> list[str]:
    """The modes this table has a column for, shallowest first, excluding SHUTDOWN."""
    return [m for m in MODES if m != "Shutdown" and m in by_mode]


def retained_through(by_mode: ByMode) -> str | None:
    """Deepest mode a PD1 peripheral's configuration survives, or None if the table is silent.

    `OFF` is the only state which loses configuration; `EN`, `OPT` and `DIS` all keep it. Losing it
    only in the deeper half of STANDBY now reports `Standby0` where this used to have to say `Stop`.

    Sleep is the floor rather than Run: PD1 is powered in both, so a row which is already `OFF` by
    the first STOP sub-mode says nothing finer than "not through STOP".
    """
    sleeps = [m for m in present(by_mode) if m not in ("Run", "Sleep")]
    if not sleeps:
        return None

    deepest = "Sleep"
    for mode in sleeps:
        if "OFF" in by_mode[mode]:
            break
        deepest = mode
    return deepest


def usable_through(by_mode: ByMode) -> str | None:
    """Deepest mode in which every policy is usable, or None if the table is silent.

    Walks only the modes the table has a column for, so a family without STOP1 still reaches STOP2.
    """
    deepest: str | None = None
    for mode in present(by_mode):
        if not by_mode[mode] <= set(USABLE):
            break
        deepest = mode
    return deepest


def expand(label: str) -> list[str]:
    """Instance names a row label refers to.

    Rows cover several instances at once, written as a shared prefix with the varying suffixes
    separated by slashes or commas: `TIMG6/7/12`, `SPI0/1`, `UART0, UART1`, `GPIOA/B`, `MCAN0/1`.
    """
    label = re.sub(r"\(\d\)", "", label).strip()
    if not label:
        return []

    for alias, real in ALIASES.items():
        if label == alias or label.startswith(alias + "/") or label.startswith(alias + "0"):
            label = label.replace(alias, real, 1)
            break

    if "," in label:
        return [p.strip() for p in label.split(",") if p.strip()]

    parts = label.split("/")
    head = parts[0].strip()
    # Non-greedy so bases containing digits work: "I2S" -> ("I2S", ""), "TIMG6" -> ("TIMG", "6").
    m = re.match(r"^([A-Za-z_][A-Za-z_0-9]*?)(\d*)$", head)
    if not m:
        return [head] if head else []

    base, first = m.group(1), m.group(2)
    names = [head]
    for suffix in parts[1:]:
        suffix = suffix.strip()
        if suffix.isdigit():
            names.append(base + suffix)          # TIMG6/7 -> TIMG7
        elif re.match(r"^[A-Za-z]$", suffix) and head[-1:].isalpha():
            names.append(head[:-1] + suffix)     # GPIOA/B -> GPIOB
    if not first:
        names.append(base + "0")                 # CRCP -> CRCP0
    return names


HEADER = """\
# How deep a sleep each peripheral keeps its configuration through, and stays usable in.
#
# GENERATED by `tools/operating_modes.py --write` from Table 8-1, "Supported Functionality by
# Operating Mode", of the {ds} datasheet. Re-run the tool rather than editing by hand; see
# `tools/operating_modes.py` for how the table's states map onto these values.
#
# `retained_through` covers PD1 peripherals only, since nothing else is automatically disabled.
"""


def load_overrides() -> Overrides:
    """Values from `data/operating_mode_overrides.yaml`, where a better source beats the table."""
    path = Path("data/operating_mode_overrides.yaml")
    if not path.exists():
        return {}
    return yaml.safe_load(path.read_text(encoding="utf-8")) or {}


def write(datasheets: str) -> int:
    """Regenerate data/operating_modes/*.yaml. Returns the number of problems reported."""
    parts = yaml.safe_load(Path("data/parts.yaml").read_text(encoding="utf-8"))
    families = [
        (f["family"], f["datasheet_url"].rsplit("/", 1)[-1])
        for f in parts["families"]
        if f.get("datasheet_url")
    ]

    pd1: dict[str, set[str]] = {}
    known: dict[str, set[str]] = {}
    for f in glob.glob("build/data/*.json"):
        chip = json.loads(Path(f).read_text(encoding="utf-8"))
        pd1.setdefault(chip["family"], set()).update(
            n for n, p in chip["peripherals"].items() if p["power_domain"] == "Pd1"
        )
        known.setdefault(chip["family"], set()).update(chip["peripherals"])
    if not pd1:
        raise SystemExit("build/data is empty -- run ./d gen first")

    Path("data/operating_modes").mkdir(parents=True, exist_ok=True)
    overrides = load_overrides()
    problems = 0

    for family, gpn in families:
        pdf = Path(datasheets) / f"{gpn}_datasheet.pdf"
        if not pdf.exists():
            print(f"{family}: no datasheet for {gpn} in {datasheets}")
            problems += 1
            continue

        retained: dict[str, str] = {}
        usable: dict[str, str] = {}
        for column_mode, rows in read_tables(pdf):
            for label, states in rows:
                by_mode = mode_states(column_mode, states)
                r, u = retained_through(by_mode), usable_through(by_mode)
                for name in expand(label):
                    if name not in known.get(family, ()):
                        continue
                    if r and name in pd1.get(family, ()):
                        retained[name] = r
                    if u:
                        usable[name] = u

        # A better source than the table, where one exists.
        overridden = overrides.get(family, {})
        for name, value in overridden.get("retained_through", {}).items():
            retained[name] = value
        for name, value in overridden.get("usable_through", {}).items():
            usable[name] = value

        # Entries the table cannot answer are reasoned out elsewhere; keep whatever is already
        # recorded rather than dropping it.
        path = Path(f"data/operating_modes/{family}.yaml")
        existing: dict[str, str] = {}
        if path.exists():
            loaded = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
            existing = loaded.get("retained_through") or {}
        missing = sorted(set(pd1.get(family, ())) - set(retained))
        carried = {n: existing[n] for n in missing if n in existing}
        for name in missing:
            if name not in carried:
                print(f"{family}: {name} is in PD1 but no row of the {gpn} table covers it")
                problems += 1

        body = "".join(f"  {n}: {retained[n]}\n" for n in sorted(retained))
        if carried:
            body += (
                "\n  # Not derivable from the table: these rows give one value spanning every mode,\n"
                "  # which says the peripheral is available but not whether its configuration\n"
                "  # survives. Entered by hand and preserved across regeneration.\n"
                + "".join(f"  {n}: {carried[n]}\n" for n in sorted(carried))
            )

        with open(path, "w", encoding="utf-8", newline="\n") as out:
            out.write(
                HEADER.format(ds=gpn.upper())
                + "\nretained_through:\n" + body
                + "\nusable_through:\n"
                + "".join(f"  {n}: {usable[n]}\n" for n in sorted(usable))
            )
        print(f"{family}: {len(retained)} retained, {len(usable)} usable, from {pdf.name}")

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
        for column_mode, rows in read_tables(path):
            columns = sorted(column_mode)
            print("    " + " ".join(f"{column_mode[c]:<9}" for c in columns))
            for label, states in rows:
                print(f"    {' '.join(f'{states.get(c) or chr(45):<9}' for c in columns)}  {label}")
            print()


if __name__ == "__main__":
    main(sys.argv[1:])
