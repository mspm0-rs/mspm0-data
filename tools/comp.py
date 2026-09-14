"""Extract the comparator timing figures from MSPM0 datasheets.

The COMP has no status bit behind either figure: nothing reports that the comparator has reached
its propagation-delay specification after `CTL1.ENABLE`, or that the 8-bit reference DAC has
settled after a code change. The datasheet's `ten` and `tdac_settle` rows are the only way to know,
so a consumer waits them out the same way `Vref::startup_ns` is waited out on devices where
`VREF_ERR_01` breaks `CTL1.READY`. This writes `data/comp/<family>.yaml`.

Like `tools/vref.py`, this is an offline aid rather than part of the build. Re-run it after a data
source bump rather than editing the YAML by hand.

Usage:
    uv run tools/comp.py <datasheet.pdf> [...]      # print what it finds
    uv run tools/comp.py --write <dir-of-datasheets> # regenerate data/comp

`uv run` installs the dependencies below on its own; a bare `python` needs them on the path.

What the rows look like, and what this reads out of them:

- **`ten` is two rows sharing one symbol cell**, one condition per comparator mode. The figures
  genuinely differ per family: high-speed mode reaches its specification in 5us on the
  newer-generation comparators and 10us on the older, while low-power mode is 10us everywhere.
  The mode is read from the condition text, not the row order.
- **`tdac_settle` is stated once, or twice on the devices whose DAC reaches a pin.** The bare row
  (to 1 LSB, unloaded) is the internal path -- the one an OPA or the comparator itself sees. The
  second row, `DAC output on pin PA11, Cload = 15pF`, exists exactly on the datasheets whose COMP
  has `CTL1.DACOUTEN`, and only applies when that bit drives the pin.
- **Each figure's cell spans the MIN, TYP and MAX columns**, as the wake-up and VREF rows do, so
  they are stated figures rather than guaranteed ceilings.

A family whose datasheet has no `Comparator enable time` row is skipped without complaint -- that
is what a family without a comparator looks like. A COMP instance with no timing data is reported
by `verify.rs` instead, which knows which chips have one.
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

ENABLE = "Comparator enable time"
DAC_SETTLE = "8-bit DAC settling time"

LATTICE = {"vertical_strategy": "lines", "horizontal_strategy": "lines"}

#: Time units the figures may be given in, as a multiplier to nanoseconds.
UNITS = {"ns": 1, "us": 1_000, "µs": 1_000, "ms": 1_000_000}

NUMBER = re.compile(r"\d+(?:\.\d+)?")

#: Condition-text fragment to the `CTL1.MODE` value it describes.
MODES = {"high speed": "fast", "low power": "ulp"}


def caption_pages(path: Path) -> list[int]:
    """Indices of the pages carrying either timing row."""
    doc = pypdfium2.PdfDocument(str(path))
    try:
        return [
            i
            for i in range(len(doc))
            if any(c in doc[i].get_textpage().get_text_range() for c in (ENABLE, DAC_SETTLE))
        ]
    finally:
        doc.close()


def read_row(cells: list[str]) -> tuple[int, str] | None:
    """Read one row into `(nanoseconds, condition text)`, or `None` if it does not parse."""
    unit = next((c for c in reversed(cells) if c and c.lower() in UNITS), "")
    if not unit:
        return None

    condition = ""
    value = None
    for cell in cells:
        stripped = cell.replace(ENABLE, "").replace(DAC_SETTLE + " in static mode", "").strip()
        if NUMBER.fullmatch(stripped):
            value = float(stripped)
            break
        if stripped and not stripped.startswith("t"):
            condition = f"{condition} {stripped}".strip()

    if value is None:
        return None

    return round(value * UNITS[unit.lower()]), condition


def read_tables(path: Path) -> tuple[dict[str, int], list[str]]:
    """Extract the timing figures from one datasheet.

    Returns the figures keyed by YAML field name, and a list of problems. The `ten` and
    `tdac_settle` blocks each span rows -- the symbol cell is stated once and the continuation rows
    carry only a condition and a figure -- so rows are attributed to the block opened above them
    until a row names a different symbol.
    """
    found: dict[str, int] = {}
    problems: list[str] = []

    def record(key: str, ns: int, condition: str) -> None:
        if found.get(key, ns) != ns:
            problems.append(f"{key} stated twice with different figures: {found[key]} and {ns} ns")
        found[key] = ns

    with pdfplumber.open(str(path)) as pdf:
        for page_index in caption_pages(path):
            for table in pdf.pages[page_index].extract_tables(LATTICE):
                block = None
                for row in table:
                    cells = [(c or "").replace("\n", " ").strip() for c in row]
                    text = " ".join(cells)

                    if ENABLE in text:
                        block = "enable"
                    elif DAC_SETTLE in text:
                        block = "dac"
                    elif any(cells[:2]):
                        # A row with its own symbol or description belongs to another parameter.
                        block = None
                        continue

                    if block is None:
                        continue

                    parsed = read_row(cells)
                    if parsed is None:
                        continue
                    ns, condition = parsed

                    if block == "enable":
                        mode = next(
                            (m for frag, m in MODES.items() if frag in condition.lower()), None
                        )
                        if mode is None:
                            problems.append(f"enable-time condition names no mode: {condition!r}")
                        else:
                            record(f"enable_{mode}_ns", ns, condition)
                    else:
                        key = "dac_settle_pin_ns" if "pin" in condition.lower() else "dac_settle_ns"
                        record(key, ns, condition)

    if found:
        for key in ("enable_fast_ns", "enable_ulp_ns", "dac_settle_ns"):
            if key not in found:
                problems.append(f"{key} not found")

    return found, problems


HEADER = """\
# The comparator timing figures, in nanoseconds.
#
# GENERATED by `tools/comp.py --write` from the `ten` and `tdac_settle` rows of the {ds}
# datasheet. Re-run the tool rather than editing by hand.
#
# Neither figure has a status bit behind it, so waiting them out is the only way to know the
# comparator output or the reference DAC is meaningful. Each figure's cell spans the datasheet's
# MIN, TYP and MAX columns, so it is a stated figure rather than a guaranteed ceiling.
"""

FIELD_COMMENTS = {
    "enable_fast_ns": "# Startup to the propagation-delay specification, per CTL1.MODE.",
    "dac_settle_ns": "# Full-scale DACCODE step to 1 LSB, unloaded. The internal path.",
    "dac_settle_pin_ns": "# The same step driven out on a pin (CTL1.DACOUTEN), Cload = 15pF.",
}


def write(datasheets: str) -> int:
    """Regenerate data/comp/*.yaml. Returns the number of problems reported."""
    parts = yaml.safe_load(Path("data/parts.yaml").read_text(encoding="utf-8"))
    families = [(f["family"], f["datasheet_url"].rsplit("/", 1)[-1]) for f in parts["families"]]

    Path("data/comp").mkdir(parents=True, exist_ok=True)
    problem_count = 0

    for family, gpn in families:
        pdf = Path(datasheets) / f"{gpn}_datasheet.pdf"
        if not pdf.exists():
            print(f"{family}: no datasheet for {gpn} in {datasheets}")
            problem_count += 1
            continue

        found, problems = read_tables(pdf)
        for problem in problems:
            print(f"{family}: {problem}")
        problem_count += len(problems)

        if not found:
            print(f"{family}: no comparator timing rows in {pdf.name}")
            continue

        path = Path(f"data/comp/{family}.yaml")
        with open(path, "w", encoding="utf-8", newline="\n") as out:
            out.write(HEADER.format(ds=gpn.upper()))
            for key in ("enable_fast_ns", "enable_ulp_ns", "dac_settle_ns", "dac_settle_pin_ns"):
                if key not in found:
                    continue
                comment = FIELD_COMMENTS.get(key)
                if comment:
                    out.write(f"{comment}\n")
                out.write(f"{key}: {found[key]}\n")
        print(f"{family}: {found} from {pdf.name}")

    return problem_count


def main(argv: list[str]) -> None:
    if "--write" in argv:
        i = argv.index("--write")
        raise SystemExit(1 if write(argv[i + 1] if i + 1 < len(argv) else ".") else 0)

    paths = [Path(a) for a in argv if not a.startswith("--")]
    if not paths:
        raise SystemExit(__doc__)

    for path in paths:
        print(f"########## {path.name}")
        found, problems = read_tables(path)
        for key, ns in found.items():
            print(f"    {key}: {ns / 1000:g} us")
        for problem in problems:
            print(f"    PROBLEM: {problem}")


if __name__ == "__main__":
    main(sys.argv[1:])
