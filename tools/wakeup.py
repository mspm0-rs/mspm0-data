"""Extract the wake-up timing table from MSPM0 datasheets.

How long a device takes to reach RUN from each sleep mode is what a consumer needs to decide whether a
sleep pays off before its next deadline, and it has no machine-readable source. This writes
`data/wakeup/<family>.yaml`.

Like `tools/operating_modes.py`, this is an offline aid rather than part of the build. Re-run it after
a data source bump rather than editing the YAML by hand.

Usage:
    uv run tools/wakeup.py <datasheet.pdf> [...]      # print the table
    uv run tools/wakeup.py --write <dir-of-datasheets> # regenerate data/wakeup

`uv run` installs the dependencies below on its own; a bare `python` needs them on the path.

Three things about the table make it awkward to read:

- **The figure is not a maximum.** Its cell spans the MIN, TYP and MAX columns, so TI gives one
  unqualified number per mode. Treat it as typical.
- **One cell can name two modes.** The MSPM0H3216 table puts "Wakeup time from STOP0 ... Wakeup time
  from STOP2 ..." in a single description cell with the two figures on consecutive rows, so modes are
  read from the description text and figures are matched to them in order.
- **Not every figure is a time.** Several tables give SLEEP0 as "2 cycles", which is not convertible to
  a fixed duration without assuming a clock rate, so that mode is left absent. Any *other* non-time
  unit is reported, since it would mean the table has changed shape.
"""

# /// script
# requires-python = ">=3.10"
# dependencies = ["pdfplumber", "pypdfium2", "pyyaml"]
# ///

import re
import sys
from collections.abc import Iterator
from pathlib import Path

try:
    import pdfplumber
    import pypdfium2
    import yaml
except ImportError as e:  # pragma: no cover
    raise SystemExit(f"{e.name} is missing; run this with `uv run` instead of `python`")

Wake = dict[str, int]

CAPTION = "Wakeup time from"

LATTICE = {"vertical_strategy": "lines", "horizontal_strategy": "lines"}

#: A wake-up row's description, which names the mode being left. The sub-mode digit is absent on
#: SHUTDOWN and, in a few tables, on STANDBY.
ROW = re.compile(r"Wakeup time from (?P<mode>SLEEP|STOP|STANDBY|SHUTDOWN)(?P<sub>\d?)\s+to\s+RUN")

#: Time units the figure may be given in, as a multiplier to nanoseconds.
UNITS = {"ns": 1, "us": 1_000, "µs": 1_000, "ms": 1_000_000}

#: The unit several tables give SLEEP0 in. A count of CPU cycles is not a fixed duration, so that mode
#: is left absent rather than converted at some assumed clock rate.
CYCLES = "cycles"

#: The modes recorded, in the order they are written out. A consumer picking a sleep level cares about
#: STOP and STANDBY; SLEEP is plain `WFI` and SHUTDOWN is a reset rather than a wake.
MODES = ("sleep0", "sleep1", "sleep2", "stop0", "stop1", "stop2", "standby0", "standby1", "shutdown")


def caption_pages(path: Path) -> list[int]:
    """Indices of the pages carrying the wake-up table."""
    doc = pypdfium2.PdfDocument(str(path))
    try:
        return [i for i in range(len(doc)) if CAPTION in doc[i].get_textpage().get_text_range()]
    finally:
        doc.close()


def read_tables(path: Path) -> Iterator[tuple[Wake, list[str]]]:
    """Yield `(wake times in ns, problems)` for each wake-up table found."""
    for page_index in caption_pages(path):
        with pdfplumber.open(str(path)) as pdf:
            for table in pdf.pages[page_index].extract_tables(LATTICE):
                if not any(CAPTION in str(cell) for row in table for cell in row):
                    continue
                yield read_table(table)


def modes_named(text: str) -> list[str]:
    """Sub-modes a description names.

    A few tables write `SLEEP` and `STANDBY` with no sub-mode digit where they give one figure for the
    whole mode, so those expand to every sub-mode the digit could have been.
    """
    found = []
    for match in ROW.finditer(text):
        mode, sub = match["mode"].lower(), match["sub"]
        if sub:
            found.append(f"{mode}{sub}")
        elif mode == "standby":
            found.extend(("standby0", "standby1"))
        elif mode == "sleep":
            found.append("sleep0")
        else:
            found.append(mode)
    return found


def read_table(table: list[list[str | None]]) -> tuple[Wake, list[str]]:
    """Read one wake-up table.

    A row starts a group when its description names any mode; rows after it carrying only a figure
    belong to that group, which is how the tables that merge two modes into one description cell are
    read. A sub-heading row ends the group, so the startup-timing figures below the wake-up section do
    not get attributed to SHUTDOWN.
    """
    wake: Wake = {}
    problems: list[str] = []
    modes: list[str] = []
    figures: list[tuple[float, str]] = []

    def flush() -> None:
        nonlocal modes, figures
        if not modes:
            return

        if len(figures) == len(modes):
            pairs = list(zip(modes, figures))
        elif len(figures) == 1:
            # One figure covering several sub-modes, as the digitless rows give.
            pairs = [(mode, figures[0]) for mode in modes]
        elif len(modes) == 1:
            # One mode measured under several test conditions, such as SHUTDOWN with fast boot on and
            # off. The slowest is the one worth planning around.
            pairs = [(modes[0], max(figures, key=lambda f: f[0] * UNITS.get(f[1], 0)))]
        else:
            problems.append(f"{'/'.join(modes)}: {len(figures)} figures for {len(modes)} modes")
            modes, figures = [], []
            return

        for mode, (value, unit) in pairs:
            scale = UNITS.get(unit)
            if scale is None:
                # SLEEP0 in CPU cycles is expected and documented, not something to fix. Any other
                # unit is a table change worth looking at.
                if unit != CYCLES:
                    problems.append(f"{mode}: figure is in {unit!r}, not a time")
                continue
            ns = round(value * scale)
            # A mode stated twice keeps the slower figure.
            wake[mode] = max(wake.get(mode, 0), ns)

        modes, figures = [], []

    unit_of_table = ""

    for row in table:
        cells = [(c or "").replace("\n", " ").strip() for c in row]
        numbers = [c for c in cells if re.fullmatch(r"\d+(?:\.\d+)?", c)]
        found = modes_named(" ".join(cells))

        # A sub-heading is a lone label with no figure, and closes whatever group was open.
        if not found and not numbers:
            flush()
            continue

        # The unit is the last column. It is left empty where one unit is merged across several rows,
        # so the last one seen carries forward.
        if cells and cells[-1] and cells[-1] not in numbers:
            unit_of_table = cells[-1]

        if found:
            flush()
            modes = found

        if numbers and modes:
            figures.append((float(numbers[0]), unit_of_table))

    flush()
    return wake, problems


HEADER = """\
# How long this family takes to reach RUN from each sleep mode, in nanoseconds.
#
# GENERATED by `tools/wakeup.py --write` from the wake-up timing table of the {ds} datasheet.
# Re-run the tool rather than editing by hand.
#
# The datasheet gives one unqualified figure per mode -- its cell spans the MIN, TYP and MAX columns --
# so these are typical times, not guaranteed ceilings. A mode is absent when the device does not have
# it, or when the datasheet states it in clock cycles rather than a time.
"""


def write(datasheets: str) -> int:
    """Regenerate data/wakeup/*.yaml. Returns the number of problems reported."""
    parts = yaml.safe_load(Path("data/parts.yaml").read_text(encoding="utf-8"))
    families = [(f["family"], f["datasheet_url"].rsplit("/", 1)[-1]) for f in parts["families"]]

    Path("data/wakeup").mkdir(parents=True, exist_ok=True)
    problems = 0

    for family, gpn in families:
        pdf = Path(datasheets) / f"{gpn}_datasheet.pdf"
        if not pdf.exists():
            print(f"{family}: no datasheet for {gpn} in {datasheets}")
            problems += 1
            continue

        wake: Wake = {}
        for table, reported in read_tables(pdf):
            wake.update(table)
            for problem in reported:
                print(f"{family}: {problem}")
                problems += 1

        if not wake:
            print(f"{family}: no wake-up timing table found in {pdf.name}")
            problems += 1
            continue

        body = "".join(f"{mode}: {wake[mode]}\n" for mode in MODES if mode in wake)
        path = Path(f"data/wakeup/{family}.yaml")
        with open(path, "w", encoding="utf-8", newline="\n") as out:
            out.write(HEADER.format(ds=gpn.upper()) + "\n" + body)
        print(f"{family}: {len(wake)} modes, from {pdf.name}")

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
        for wake, problems in read_tables(path):
            for mode in MODES:
                if mode in wake:
                    print(f"    {mode:10} {wake[mode] / 1000:>8.2f} us")
            for problem in problems:
                print(f"    ! {problem}")


if __name__ == "__main__":
    main(sys.argv[1:])
