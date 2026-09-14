"""Check `data/parts.yaml`'s flash and SRAM sizes against the datasheets.

`data/parts.yaml` is hand-maintained and sysconfig says nothing about memory per part number, so
until now nothing checked these two numbers at all. This reads each datasheet's "Device Comparison"
table, whose `FLASH / SRAM (KB)` column states both per orderable part, and reports every
disagreement. It found MSPM0G1507 recorded as 64KB of flash against the datasheet's 128KB.

This checks rather than writes: the rest of a part number's entry -- packages, the curated
frequencies -- has no machine-readable source, so there is nothing to regenerate.

Usage:
    uv run tools/memory.py <datasheet.pdf> [...]   # print each part the datasheet states
    uv run tools/memory.py --check <dir-of-pdfs>   # compare against data/parts.yaml

`uv run` installs the dependencies below on its own; a bare `python` needs them on the path.

**Sum the entries by kind rather than reading the one named `RAM`.** The Gx51x parts split their
128KB of SRAM into `RAM` and `RAM_BANK`, contiguous at 0x20200000 and 0x20210000, because the upper
bank is wiped by any mode deeper than SLEEP and carries its own `retained_through`. Comparing the
first entry against the datasheet's single figure reports all four as half their real size.

The table is ruled, so a lattice read finds it. What needs care is the leading column: a row states
its part number and size, and the columns shared by every part of the family are stated once against
the first row and left blank below, so a row cannot be skipped for having empty cells.
"""

# /// script
# requires-python = ">=3.10"
# dependencies = ["pdfplumber", "pyyaml"]
# ///

import re
import sys
from pathlib import Path

import pdfplumber
import yaml

PART = re.compile(r"\b(MSPM0[A-Z]?\d{4}|MSPS\d{3}F\d)", re.IGNORECASE)
SIZE = re.compile(r"^\s*(\d+)\s*/\s*(\d+)\s*$")


def read(path: Path) -> dict[str, set[tuple[int, int]]]:
    """Every part number the datasheet's comparison table states, to its (flash, sram) in KB.

    A set, because one part number appears once per package it is offered in, and every row has to
    agree. Two sizes for one part means the table was misread.
    """
    found: dict[str, set[tuple[int, int]]] = {}

    with pdfplumber.open(path) as pdf:
        for page in pdf.pages[:14]:
            for table in page.extract_tables():
                if not table:
                    continue
                header = " ".join(str(cell) for cell in table[0] if cell).upper()
                if "FLASH" not in header or "SRAM" not in header:
                    continue

                for row in table[1:]:
                    cells = [(cell or "").replace("\n", " ").strip() for cell in row]
                    name = next((PART.search(c) for c in cells if PART.search(c)), None)
                    size = next((SIZE.match(c) for c in cells if SIZE.match(c)), None)
                    if name and size:
                        part = name.group(1).lower()
                        found.setdefault(part, set()).add((int(size[1]), int(size[2])))

    return found


def check(datasheets: str) -> int:
    """Compare every part in data/parts.yaml against its datasheet. Returns problems reported."""
    parts = yaml.safe_load(Path("data/parts.yaml").read_text(encoding="utf-8"))
    problems = 0
    checked = 0

    for family in parts["families"]:
        gpn = family["datasheet_url"].rsplit("/", 1)[-1]
        pdf = Path(datasheets) / f"{gpn}_datasheet.pdf"
        if not pdf.exists():
            print(f"{family['family']}: no datasheet for {gpn} in {datasheets}")
            problems += 1
            continue

        stated = read(pdf)
        for part in family["part_numbers"]:
            # Summed by kind: a part with an upper SRAM bank spends its total over two entries.
            flash = sum(m["length"] for m in part["memory"] if m["name"].startswith("FLASH"))
            ram = sum(m["length"] for m in part["memory"] if m["name"].startswith("RAM"))

            sizes = stated.get(part["name"])
            if not sizes:
                print(f"{part['name']}: no row in {pdf.name}")
                problems += 1
            elif len(sizes) > 1:
                print(f"{part['name']}: {pdf.name} states {sorted(sizes)} for one part")
                problems += 1
            elif (size := next(iter(sizes))) != (flash, ram):
                print(
                    f"{part['name']}: parts.yaml says {flash}KB flash and {ram}KB SRAM, "
                    f"{pdf.name} says {size[0]}KB and {size[1]}KB"
                )
                problems += 1
            else:
                checked += 1

    print(f"{checked} part numbers agree with their datasheet, {problems} do not")
    return problems


def main(argv: list[str]) -> None:
    if "--check" in argv:
        i = argv.index("--check")
        raise SystemExit(1 if check(argv[i + 1] if i + 1 < len(argv) else ".") else 0)

    paths = [Path(a) for a in argv if not a.startswith("--")]
    if not paths:
        raise SystemExit(__doc__)

    for path in paths:
        print(f"########## {path.name}")
        for part, sizes in sorted(read(path).items()):
            for flash, ram in sorted(sizes):
                print(f"    {part:<12} {flash:>4}KB flash  {ram:>4}KB SRAM")


if __name__ == "__main__":
    main(sys.argv[1:])
