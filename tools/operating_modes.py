"""Extract the "Supported Functionality by Operating Mode" table from MSPM0 datasheets.

This is the source for the low power data which has no machine-readable origin: how deep a sleep each
PD1 peripheral keeps its configuration through, and how deep each peripheral stays usable. It writes
`data/operating_modes/<family>.yaml`, so those values have a traceable path back to the datasheet
instead of being hand-transcribed.

Like `transforms/`, this is an offline aid rather than part of the build. Re-run it after a data
source bump rather than editing the YAML by hand.

Usage:
    python tools/operating_modes.py <datasheet.pdf> [...]          # print the table
    python tools/operating_modes.py --write <dir-of-datasheets>    # regenerate data/operating_modes

Requires `pdfplumber`.

## How the table is read

The table is read from its **ruling lines** rather than from extracted text. `pdftotext` flattens the
page to a character grid, which destroys the one thing that matters here: a value covering several
modes is printed once, and its position does not identify the columns it spans. Reconstructing spans
from text positions does not work -- neither a partition fitted to the header columns nor one fitted
to column anchors recovers the true boundaries, and both fail silently by producing a plausible
answer.

`pdfplumber`'s lattice mode instead recovers the drawn cell rectangles. A merged cell is one
rectangle covering several grid positions, so the spans come from the geometry:

    row.cells[i] is None  <=>  column i is covered by a merge that began earlier

which makes a forward fill from the last real cell exact. Row spans work the same way, so the group
column ("PD1 Peripherals", "Core Functions") needs no special handling, and labels wrapped over
several lines stay within their own cell.

Column identity comes from the header row, which names the mode groups (RUN, SLEEP, STOP, STANDBY,
SHUTDOWN) as merged cells; the policies within a group follow in order. Not every device has every
policy -- the C and H series have no STOP1 -- so this is derived per file rather than assumed.
"""

import glob
import json
import re
import sys
from pathlib import Path

try:
    import pdfplumber
except ImportError:  # pragma: no cover
    raise SystemExit("pdfplumber is required: python -m pip install pdfplumber")

CAPTION = "Supported Functionality by Operating Mode"

LATTICE = {"vertical_strategy": "lines", "horizontal_strategy": "lines"}

#: Mode groups in order of increasing depth. The datasheet names these as merged header cells.
GROUPS = ["RUN", "SLEEP", "STOP", "STANDBY", "SHUTDOWN"]

#: Per the table's legend: EN and OPT mean the function is usable, DIS/OFF/NS do not. `NS` is "not
#: automatically disabled in the specified mode, but its use is not supported", which is the case a
#: boolean would hide. Only OFF loses configuration.
USABLE = ("EN", "OPT")

STATE = re.compile(r"^(EN|DIS|OPT|NS|OFF)\b")

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


def read_tables(path):
    """Yield (column_groups, rows) for each page of the operating-mode table.

    `column_groups` maps a column index to its mode group; `rows` is a list of (label, states) where
    `states` is one entry per mode column, merges already resolved.
    """
    with pdfplumber.open(str(path)) as pdf:
        for page in pdf.pages:
            if CAPTION not in (page.extract_text() or ""):
                continue

            for table in page.find_tables(LATTICE):
                if len(table.rows) < 6 or max(len(r.cells) for r in table.rows) < 10:
                    continue
                data = table.extract()

                # The header row names each group in the column it starts at; the columns it spans
                # follow as `None` cells. Anything past the last named group is SHUTDOWN, which some
                # datasheets leave unlabelled.
                header = [(c or "").replace("\n", "").strip() for c in data[0]]
                column_group, current = {}, None
                for i, cell in enumerate(header):
                    if i < 2:
                        continue                     # group label and row label columns
                    name = next((g for g in GROUPS if cell.upper().startswith(g)), None)
                    if name:
                        current = name
                    elif cell and current == "STANDBY":
                        current = "SHUTDOWN"         # trailing unlabelled column
                    if current:
                        column_group[i] = current
                # A trailing column after STANDBY with no header of its own is SHUTDOWN.
                tail = max(column_group, default=1)
                if column_group.get(tail) == "STANDBY" and tail == len(header) - 1:
                    column_group[tail] = "SHUTDOWN"

                rows = []
                for ri, row in enumerate(table.rows[1:], start=1):
                    # A label wrapped inside its cell breaks mid-word ("MATHA"/"CL", "SYSOS"/"C",
                    # "TIMG6/7"/"/12"), so the lines join with no separator. Comma-separated lists
                    # keep their commas, so nothing is lost by not inserting spaces.
                    label = (data[ri][1] or "").replace("\n", "").strip()
                    if not label:
                        continue

                    states, held = {}, None
                    for ci in sorted(column_group):
                        if ci >= len(row.cells):
                            continue
                        if row.cells[ci] is not None:
                            # A real cell boundary starts a new value here.
                            text = (data[ri][ci] or "").replace("\n", " ").strip()
                            m = STATE.match(text)
                            held = m.group(1) if m else None
                        states[ci] = held
                    rows.append((label, states))

                yield column_group, rows


def group_states(column_group, states):
    """States seen in each mode group, as {group: set of states}."""
    out = {}
    for ci, group in column_group.items():
        s = states.get(ci)
        if s:
            out.setdefault(group, set()).add(s)
    return out


def retained_through(by_group):
    """Deepest mode a PD1 peripheral's configuration survives, or None if the table is silent.

    `OFF` is the only state which loses configuration; `EN`, `OPT` and `DIS` all keep it. STOP and
    STANDBY are read separately, so losing it only in STANDBY reports `Stop` rather than `Sleep`.

    A blank group is answered conservatively: retention in a deeper mode implies it in a shallower one,
    but not the reverse.
    """
    stop = by_group.get("STOP", set())
    standby = by_group.get("STANDBY", set())
    if not stop and not standby:
        return None

    if "OFF" in stop:
        return "Sleep"
    if "OFF" in standby:
        return "Stop" if stop else "Sleep"
    return "Standby" if standby else "Stop"


def usable_through(by_group):
    """Deepest mode group in which every policy is usable, or None if the table is silent."""
    deepest = None
    for group in ("RUN", "SLEEP", "STOP", "STANDBY"):
        seen = by_group.get(group)
        if not seen:
            break
        if not seen <= set(USABLE):
            break
        deepest = group
    return {"RUN": "Run", "SLEEP": "Sleep", "STOP": "Stop", "STANDBY": "Standby"}.get(deepest)


def expand(label):
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
# How deep a sleep each peripheral is retained through and stays usable in.
#
# GENERATED by `tools/operating_modes.py --write` from Table 8-1, "Supported Functionality by
# Operating Mode", of the {ds} datasheet. Re-run the tool rather than editing by hand.
#
# `retained_through` is how deep a sleep a PD1 peripheral keeps its configuration through. SYSCTL
# forces every PD1 peripheral to a disabled state on entry to STOP or STANDBY (TRM 2.2.6.1), so a
# driver must re-enable any of them on wake regardless; what differs is whether the rest of the
# configuration is still there. `OFF` loses it: under STOP that is `Sleep`, under STANDBY alone
# `Stop`, and `EN`/`OPT`/`DIS` throughout is `Standby`, surviving everything short of SHUTDOWN. Only
# PD1 peripherals appear: nothing else is automatically disabled, so the question does not arise.
#
# `usable_through` is the deepest mode the table marks the peripheral usable in, reading `EN` and
# `OPT` as usable and `DIS`, `OFF` and `NS` as not. `NS` is "not automatically disabled in the
# specified mode, but its use is not supported" -- the case a boolean would hide.
"""


def load_overrides():
    """Values from `data/operating_mode_overrides.yaml`, where a better source beats the table.

    Parsed without a YAML library to keep this script dependency-light; the file is a fixed
    two-level shape (family -> field -> name: value).
    """
    path = Path("data/operating_mode_overrides.yaml")
    if not path.exists():
        return {}

    out, family, field = {}, None, None
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        indent = len(line) - len(line.lstrip())
        key = line.strip().rstrip(":")
        if indent == 0:
            family, field = key, None
        elif indent == 2:
            field = key
        elif indent == 4 and family and field:
            name, _, value = key.partition(":")
            out.setdefault(family, {}).setdefault(field, {})[name.strip()] = value.strip()
    return out


def write(datasheets):
    """Regenerate data/operating_modes/*.yaml. Returns the number of problems reported."""
    families = re.findall(
        r"^- family: (\S+)\n(?:.*\n)*?  datasheet_url: \S+/(\S+)$",
        Path("data/parts.yaml").read_text(encoding="utf-8"),
        re.M,
    )

    pd1, known = {}, {}
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

        retained, usable = {}, {}
        for column_group, rows in read_tables(pdf):
            for label, states in rows:
                by_group = group_states(column_group, states)
                r, u = retained_through(by_group), usable_through(by_group)
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
        existing = (
            dict(re.findall(r"^  (\w+): (\w+)$", path.read_text(encoding="utf-8"), re.M))
            if path.exists() else {}
        )
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


def main(argv):
    if "--write" in argv:
        i = argv.index("--write")
        raise SystemExit(1 if write(argv[i + 1] if i + 1 < len(argv) else ".") else 0)

    paths = [Path(a) for a in argv if not a.startswith("--")]
    if not paths:
        raise SystemExit(__doc__)

    for path in paths:
        print(f"########## {path.name}")
        for column_group, rows in read_tables(path):
            columns = sorted(column_group)
            print("    " + " ".join(f"{column_group[c][:4]:<5}" for c in columns))
            for label, states in rows:
                print(f"    {' '.join(f'{states.get(c) or chr(45):<5}' for c in columns)}  {label}")
            print()


if __name__ == "__main__":
    main(sys.argv[1:])
