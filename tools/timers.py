"""Extract the TIMx configuration table from MSPM0 datasheets.

Every MSPM0 timer shares the `tim_v1` register block, so nothing in the generated PAC says which
instance has a repeat counter, deadband insertion or a 32-bit counter. This reads the table that
does, writing `data/timers/<family>.yaml`.

Like `tools/operating_modes.py`, this is an offline aid rather than part of the build. Re-run it
after a data source bump rather than editing the YAML by hand.

Usage:
    uv run tools/timers.py <datasheet.pdf> [...]          # print the table
    uv run tools/timers.py --write <dir-of-datasheets>    # regenerate data/timers

`uv run` installs the dependencies below on its own; a bare `python` needs them on the path.

The datasheet table is used rather than the SVDs or the TRM because it is the only source which is
both per-device and complete:

- The SVDs describe timers per instance, but mspm0h321x's describes `TIMG1` and `TIMG2` with the
  full advanced-timer register set, which its own datasheet and its pin list both contradict. They
  also omit `TIMA1`'s `RC`/`RCLD` registers, which the datasheet says it has.
- Every TRM carries the same portfolio-wide "TIMx Instance Configuration" table, keyed on instance
  name, so it repeats the same mistake about mspm0h321x.
- sysconfig's `SYS_FLAVOR` is per instance, but TI reuses a flavour name for different capabilities
  between families, so it cannot be read on its own either.

Capability does not follow the instance name: `TIMG2` is a plain two-channel timer on mspm0l110x and
sysconfig calls the mspm0l112x one `flavorA`, the same flavour as `TIMA0`.
"""

# /// script
# requires-python = ">=3.10"
# dependencies = ["pdfplumber", "pypdfium2", "pyyaml"]
# ///

import glob
import json
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

Timer = dict[str, int | bool]
Timers = dict[str, Timer]

#: The table's caption, which three datasheet generations word differently.
CAPTION = re.compile(
    r"TIMx Configurations|TIMx Instance Configuration|Different TIMG Configurations"
)

LATTICE = {"vertical_strategy": "lines", "horizontal_strategy": "lines"}

#: Column header -> field, after `key()` has stripped case, spaces and hyphens.
#:
#: Headers wrap mid-word inside their cell ("PRESCALE R", "Counter Resolutio n"), which is why the
#: whitespace goes before the lookup rather than being normalised to single spaces.
COLUMNS = {
    "timername": "name",
    "timname": "name",
    "instance": "name",
    "resolution": "bits",
    "counterresolution": "bits",
    "prescaler": "prescaler",
    "repeatcounter": "repeat_counter",
    "capture/comparechannels": "ccp_channels",
    "ccpchannels(external/internal)": "ccp_channels",
    "externalpwmchannels": "external_pwm_channels",
    "phaseload": "phase_load",
    "shadowload": "shadow_load",
    "shadowcc": "shadow_ccs",
    "shadowccs": "shadow_ccs",
    "deadband": "deadband",
    "fault": "fault_handler",
    "faulthandler": "fault_handler",
    "qei": "qei_hall",
    "qei/hallinputmode": "qei_hall",
}

#: Fields which are a plain yes/no in the table.
FLAGS = (
    "prescaler",
    "repeat_counter",
    "phase_load",
    "shadow_load",
    "shadow_ccs",
    "deadband",
    "fault_handler",
    "qei_hall",
)

#: Cell contents meaning "this instance does not have it". The dash is an en dash in most tables and
#: a hyphen in a few cells of the MSPM0L2117 one.
ABSENT = {"", "-", "–", "—", "n/a", "na", "no"}

INSTANCE = re.compile(r"^TIM[A-Z]\d+$")


def key(header: str) -> str:
    """Normalise a column header to its `COLUMNS` key."""
    return re.sub(r"[^a-z0-9/()]", "", header.lower())


def present(cell: str | None) -> bool:
    """Whether a yes/no cell says the instance has the feature."""
    return (cell or "").strip().lower() not in ABSENT


def count(cell: str | None) -> int:
    """First integer in a cell, or 0 if it has none."""
    match = re.search(r"\d+", cell or "")
    return int(match.group()) if match else 0


def caption_pages(path: Path) -> list[int]:
    """Indices of the pages carrying the table's caption.

    Located with PDFium rather than pdfplumber, for the reason `operating_modes.py` gives.
    """
    doc = pypdfium2.PdfDocument(str(path))
    try:
        return [
            i for i in range(len(doc)) if CAPTION.search(doc[i].get_textpage().get_text_range())
        ]
    finally:
        doc.close()


def read_tables(path: Path) -> Iterator[tuple[bool, Timers]]:
    """Yield `(had_pwm_column, timers)` for each page of the table.

    The MSPM0L2117 table spans two pages and repeats its header on the second, so each page is read
    on its own and the caller merges them.
    """
    pages = caption_pages(path)
    if not pages:
        return

    with pdfplumber.open(str(path)) as pdf:
        for page in (pdf.pages[i] for i in pages):
            for table in page.extract_tables(LATTICE):
                had_pwm_column, timers = read_table(table)
                if timers:
                    yield had_pwm_column, timers


def read_table(table: list[list[str | None]]) -> tuple[bool, Timers]:
    """Read one extracted table, or return nothing if it is not the timer table.

    The cross-trigger maps sit on the same pages and have an instance name in their header too, so
    a table is only accepted once its header names a resolution column.
    """
    if not table:
        return False, {}

    columns = {}
    for index, header in enumerate(table[0]):
        field = COLUMNS.get(key(header or ""))
        if field:
            columns[field] = index
    if "name" not in columns or "bits" not in columns:
        return False, {}

    timers: Timers = {}
    for row in table[1:]:
        name = (row[columns["name"]] or "").strip()
        if not INSTANCE.match(name):
            continue

        # The MSPM0L2117 table drops the external PWM count from its TIMA0 row and shifts every
        # column after it one to the left, which would otherwise read as a timer with no fault
        # handler. A count column holding a yes/no is the tell; the MSPM0L1228 table has the same
        # row intact and agrees with the corrected reading.
        pwm = columns.get("external_pwm_channels")
        shift = 1 if pwm is not None and (row[pwm] or "").strip().lower() == "yes" else 0
        if shift:
            print(f"{name}: external PWM count missing, columns after it read one to the left")

        def cell(field: str, row=row, shift=shift) -> str | None:
            index = columns.get(field)
            if index is None:
                return None
            if shift and index > columns["external_pwm_channels"]:
                index -= shift
            return row[index]

        # "4/2" is four channels with a CCP output plus two compare-only ones, which only the
        # L-series tables spell out. Only the first is described here: nothing has asked for the
        # compare-only channels, and no other datasheet states how many there are.
        external, _, _ = (cell("ccp_channels") or "").partition("/")
        timer: Timer = {"bits": count(cell("bits")), "ccp_channels": count(external)}
        timer.update({flag: present(cell(flag)) for flag in FLAGS})

        # Only the L-series tables have the column, and the shifted row above has lost it. Elsewhere
        # the count follows from deadband insertion, which is what pairs each channel with a
        # complementary output.
        if pwm is not None and not shift:
            timer["external_pwm_channels"] = count(row[pwm])
        else:
            timer["external_pwm_channels"] = timer["ccp_channels"] * (
                2 if timer["deadband"] else 1
            )

        timers[name] = timer

    return "external_pwm_channels" in columns, timers


HEADER = """\
# What each timer instance of this family can do.
#
# GENERATED by `tools/timers.py --write` from the TIMx configuration table of the {ds} datasheet.
# Re-run the tool rather than editing by hand.
#
# `ccp_channels` counts the capture/compare channels with a CCPx output, which the table calls
# external. The compare-only channels behind them (CC_45, TIMA only) are not recorded: only the
# L-series tables state how many there are.
{derived}"""

DERIVED = """\
# `external_pwm_channels` is not a column of this datasheet's table and is derived: deadband
# insertion is what pairs each channel with a complementary output, so it doubles the count.
"""


def write(datasheets: str) -> int:
    """Regenerate data/timers/*.yaml. Returns the number of problems reported."""
    parts = yaml.safe_load(Path("data/parts.yaml").read_text(encoding="utf-8"))
    families = [(f["family"], f["datasheet_url"].rsplit("/", 1)[-1]) for f in parts["families"]]

    known: dict[str, set[str]] = {}
    for path in glob.glob("build/data/*.json"):
        chip = json.loads(Path(path).read_text(encoding="utf-8"))
        known.setdefault(chip["family"], set()).update(
            name for name, p in chip["peripherals"].items() if p["type"] == "Tim"
        )
    if not known:
        raise SystemExit("build/data is empty -- run ./d gen first")

    Path("data/timers").mkdir(parents=True, exist_ok=True)
    problems = 0

    for family, gpn in families:
        pdf = Path(datasheets) / f"{gpn}_datasheet.pdf"
        if not pdf.exists():
            print(f"{family}: no datasheet for {gpn} in {datasheets}")
            problems += 1
            continue

        timers: Timers = {}
        had_pwm_column = False
        for page_had_pwm_column, page in read_tables(pdf):
            had_pwm_column |= page_had_pwm_column
            timers.update(page)

        # The datasheet covers several families at once, so it describes instances this family does
        # not have. Those are dropped; instances it has and the table misses are reported.
        wanted = known.get(family, set())
        timers = {name: timer for name, timer in timers.items() if name in wanted}
        for name in sorted(wanted - set(timers)):
            print(f"{family}: {name} is a timer of this family but the {gpn} table has no row")
            problems += 1

        body = ""
        for name in sorted(timers):
            body += f"{name}:\n"
            for field, value in timers[name].items():
                body += f"  {field}: {str(value).lower() if isinstance(value, bool) else value}\n"

        path = Path(f"data/timers/{family}.yaml")
        derived = "" if had_pwm_column else DERIVED
        with open(path, "w", encoding="utf-8", newline="\n") as out:
            out.write(HEADER.format(ds=gpn.upper(), derived=derived) + "\n" + body)
        print(f"{family}: {len(timers)} timers, from {pdf.name}")

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
        for _, timers in read_tables(path):
            for name, timer in timers.items():
                print(f"    {name:8} {timer}")


if __name__ == "__main__":
    main(sys.argv[1:])
