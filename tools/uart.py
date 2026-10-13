"""Extract which extended-UART features each UART instance has from MSPM0 datasheets.

Every legacy UART instance shares the `uart_v1` register block and every UNICOMM UART function
shares `unicommuart_v1`, so nothing in the generated PAC says which instance implements the
extended features: LIN, DALI, IrDA, ISO7816 smart card and Manchester coding. The table that does
is the datasheet's "UART Features" table, whose column headers also carry TI's Extend/Main naming.
This writes `data/uart/<family>.yaml`.

Like `tools/timers.py`, this is an offline aid rather than part of the build. Re-run it after a
data source bump rather than editing the YAML by hand.

Usage:
    uv run tools/uart.py <datasheet.pdf> [...]       # print what it finds
    uv run tools/uart.py --write <dir-of-datasheets> # regenerate data/uart

`uv run` installs the dependencies below on its own; a bare `python` needs them on the path.

The machine-readable alternatives were checked and lost:

- sysconfig's `SYS_LIN_EN` is `1` on the *main* UARTs of MSPM0G350X and its siblings, which their
  own datasheet, SVD and driverlib all contradict; on MSPM0G351X the same attribute is `0` there.
  Its per-instance `SYS_UARTADV` does agree with every datasheet, and `generate.rs` cross-checks
  against it, but one demonstrably wrong attribute in the same block is why none of them is the
  source.
- The SVDs give the main instances a reduced register set which agrees with the datasheets, but
  they are the least reliable source in this repo and two families have none.

The UNICOMM tables differ from the legacy ones in more than shape. The MSPM0G5187 splits its UART
functions three ways -- UC0 "(Advanced)", UC1 "(Basic w/ LIN)", UC3 "(Basic)" -- and the features do
not nest: UC1 has LIN but no smart card, UC3 smart card but no LIN. A single extend/main flag
cannot carry that, which is why the YAML records the features themselves. The same table folds DALI
and Manchester into one "UART-DALI-MANCHESTER" row, as does TI's own `SYS_UART_DALI_MENC_EN`
attribute, so that row sets both flags.
"""

# /// script
# requires-python = ">=3.10"
# dependencies = ["pdfplumber", "pypdfium2", "pyyaml"]
# ///

import glob
import json
import re
import sys
from pathlib import Path

try:
    import pdfplumber
    import pypdfium2
    import yaml
except ImportError as e:  # pragma: no cover
    raise SystemExit(f"{e.name} is missing; run this with `uv run` instead of `python`")

Uart = dict[str, bool]
Uarts = dict[str, Uart]

CAPTION = re.compile(
    r"UART Features|UART FEATURES|UNICOMM-UART feature support|UART \(UNICOMM\) Features"
)

LATTICE = {"vertical_strategy": "lines", "horizontal_strategy": "lines"}

FEATURES = ("lin", "dali", "irda", "iso7816", "manchester")

#: Feature-row label -> the flags the row sets, after `key()` has collapsed case and whitespace.
#: The legacy and MSPM0L2117 tables label rows with the left-column description; the MSPM0G5187
#: labels them with a feature tag. An empty tuple is a row known not to matter here; a label in
#: neither form is a problem, so a new datasheet's new row has to be classified by a human.
ROWS = {
    "support lin mode": ("lin",),
    "support dali": ("dali",),
    "support irda": ("irda",),
    "support iso7816 smart card": ("iso7816",),
    "support manchester coding": ("manchester",),
    "uart-lin": ("lin",),
    "uart-dali-manchester": ("dali", "manchester"),
    "uart-irda": ("irda",),
    "uart-smartcard": ("iso7816",),
    # Rows the data does not carry: operating-mode facts are `data/operating_modes/`, the FIFO
    # depth is sysconfig's `SYS_FENTRIES`, and the rest is common to every instance.
    "active in stop and standby mode": (),
    "active in stop and standby modes": (),
    "separate transmit and receive fifos": (),
    "support hardware flow control": (),
    "support 9-bit configuration": (),
    "fifo depth": (),
    "fifo entry depth": (),
    "-": (),
    "uart-rx-timeout": (),
    "uart-idleline-multiproc": (),
    "uart-flow-control": (),
    "uart-multidrop-9-bit": (),
    "uart-ext-driver": (),
    "uart-fifo": (),
    "uart-dma": (),
}

#: Cell contents meaning "this instance does not have it".
ABSENT = {"", "-", "–", "—", "no"}

INSTANCE = re.compile(r"UART\d+|UC\d+")


class Problem(Exception):
    pass


def key(label: str) -> str:
    """Normalise a row label: lowercase, and no whitespace splitting a wrapped tag."""
    label = " ".join(label.split())
    return label.replace("- ", "-").lower() if label.upper().startswith("UART-") else label.lower()


def present(cell: str) -> bool:
    """Whether a yes/no cell says the instance has the feature."""
    text = cell.strip().lower()
    if text not in ABSENT and text != "yes":
        raise Problem(f"feature cell is neither yes nor no: {cell!r}")
    return text == "yes"


def instances(header: str) -> list[str]:
    """The instances a legacy column header names.

    The headers group and abbreviate: "UART1 and 2 (Main)", "UART0, UART7 (Extend, low-power)",
    "UART3-UART6 (Main)". The parenthetical label is dropped -- the feature rows carry what it
    abbreviates.
    """
    names: list[str] = []
    for token in re.split(r",|\band\b", header.split("(")[0].replace("\n", " ")):
        token = token.strip()
        if not token:
            continue
        if match := re.fullmatch(r"UART(\d+)\s*-\s*UART(\d+)", token):
            names += [f"UART{n}" for n in range(int(match.group(1)), int(match.group(2)) + 1)]
        elif re.fullmatch(r"UART\d+", token):
            names.append(token)
        elif token.isdigit():  # "UART1 and 2"
            names.append(f"UART{token}")
        else:
            raise Problem(f"cannot read column header {header!r}")
    return names


def caption_pages(path: Path) -> list[int]:
    """Indices of the pages carrying the table's caption."""
    doc = pypdfium2.PdfDocument(str(path))
    try:
        return [i for i in range(len(doc)) if CAPTION.search(doc[i].get_textpage().get_text_range())]
    finally:
        doc.close()


def read_tables(path: Path) -> Uarts:
    """Instance name to feature flags, merged over every page of the table.

    The MSPM0C1106 and MSPM0H3216 tables span two pages and repeat their header on the second, so
    the pages accumulate into one mapping and completeness is checked at the end.
    """
    uarts: Uarts = {}
    with pdfplumber.open(str(path)) as pdf:
        for page_index in caption_pages(path):
            for table in pdf.pages[page_index].extract_tables(LATTICE):
                read_table(table, uarts)

    for name, uart in uarts.items():
        if missing := [feature for feature in FEATURES if feature not in uart]:
            raise Problem(f"the {name} column has no {', '.join(missing)} row")

    return uarts


def read_table(table: list[list[str | None]], uarts: Uarts) -> None:
    """Fold one extracted table into `uarts`.

    Three shapes exist. The legacy tables put "UART Features" over the row labels and the instances
    in the remaining header cells. The MSPM0L2117 writes "Supported Features" and names instances
    "UC4.UART" -- sharing its header text with the I2C and SPI tables alongside, which the `.UART`
    requirement rejects. The MSPM0G5187 adds a feature-tag column and names the instances on a
    second header row, under a first that only carries TI's Advanced/Basic labels.
    """
    if not table or not table[0]:
        return
    rows = [[(cell or "").strip() for cell in row] for row in table]
    corner = key(rows[0][0])

    if corner == "uart features":
        columns = {
            index: instances(header) for index, header in enumerate(rows[0]) if index and header
        }
        label_column, body = 0, rows[1:]
    elif corner == "supported features":
        columns = {
            index: [name.replace(".UART", "_UART") for name in header.replace(" ", "").split(",")]
            for index, header in enumerate(rows[0])
            if index and header
        }
        if not all(name.endswith("_UART") for names in columns.values() for name in names):
            return  # the I2C or SPI table
        label_column, body = 0, rows[1:]
    elif corner.startswith("unicomm-uart features"):
        columns = {
            index: [f"{header}_UART"]
            for index, header in enumerate(rows[1])
            if INSTANCE.fullmatch(header)
        }
        label_column, body = 0, rows[2:]
    else:
        return

    for row in body:
        label = key(row[label_column])
        if INSTANCE.fullmatch(row[label_column].split(".")[0].split("(")[0].strip()):
            continue  # a repeated header row
        flags = ROWS.get(label)
        if flags is None:
            raise Problem(f"unrecognised feature row {row[label_column]!r}")

        for index, names in columns.items():
            for flag in flags:
                value = present(row[index])
                for name in names:
                    already = uarts.setdefault(name, {}).setdefault(flag, value)
                    if already != value:
                        raise Problem(f"{name} {flag} read as both {already} and {value}")


HEADER = """\
# Which extended-UART features each UART instance of this family implements.
#
# GENERATED by `tools/uart.py --write` from the UART Features table of the {ds} datasheet.
# Re-run the tool rather than editing by hand.
"""


def write(datasheets: str) -> int:
    """Regenerate data/uart/*.yaml. Returns the number of problems reported."""
    parts = yaml.safe_load(Path("data/parts.yaml").read_text(encoding="utf-8"))
    families = [(f["family"], f["datasheet_url"].rsplit("/", 1)[-1]) for f in parts["families"]]

    known: dict[str, set[str]] = {}
    for path in glob.glob("build/data/*.json"):
        chip = json.loads(Path(path).read_text(encoding="utf-8"))
        known.setdefault(chip["family"], set()).update(
            name
            for name, p in chip["peripherals"].items()
            if p["type"] in ("Uart", "UnicommUart")
        )
    if not known:
        raise SystemExit("build/data is empty -- run ./d gen first")

    Path("data/uart").mkdir(parents=True, exist_ok=True)
    problems = 0

    for family, gpn in families:
        pdf = Path(datasheets) / f"{gpn}_datasheet.pdf"
        if not pdf.exists():
            print(f"{family}: no datasheet for {gpn} in {datasheets}")
            problems += 1
            continue

        try:
            uarts = read_tables(pdf)
        except Problem as problem:
            print(f"{family}: {problem} in {pdf.name}")
            problems += 1
            continue

        # The datasheet covers several families at once, so it can describe instances this family
        # does not have. Those are dropped; instances it has and the table misses are reported.
        wanted = known.get(family, set())
        uarts = {name: uart for name, uart in uarts.items() if name in wanted}
        for name in sorted(wanted - set(uarts)):
            print(f"{family}: {name} is a UART of this family but the {gpn} table has no column")
            problems += 1

        if not uarts:
            print(f"{family}: no UART Features table in {pdf.name}")
            problems += 1
            continue

        path = Path(f"data/uart/{family}.yaml")
        with open(path, "w", encoding="utf-8", newline="\n") as out:
            out.write(HEADER.format(ds=gpn.upper()))
            for name in sorted(uarts):
                out.write(f"{name}:\n")
                for feature in FEATURES:
                    out.write(f"  {feature}: {str(uarts[name][feature]).lower()}\n")

        found = ", ".join(
            f"{name} {{{', '.join(feature for feature in FEATURES if uarts[name][feature]) or 'none'}}}"
            for name in sorted(uarts)
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
        for name, uart in sorted(read_tables(path).items()):
            flags = ", ".join(feature for feature in FEATURES if uart[feature]) or "none"
            print(f"    {name:10} {flags}")


if __name__ == "__main__":
    main(sys.argv[1:])
