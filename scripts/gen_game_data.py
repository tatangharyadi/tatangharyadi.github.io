#!/usr/bin/env python3
"""Turn game/data/*.tsv into game/src/world.rs.

The simulation needs its world as Rust constants, but the world itself should
stay readable and reviewable, so the three TSV files are the source and this
script is the only thing allowed to write world.rs. Same arrangement as
scripts/propagate_work.py: run it to regenerate, run it with --check in CI to
prove the committed output still matches its input.

What it actually checks is the interesting part. Rasterising a coastline by
hand is exactly the kind of work that looks finished and is not, so the script
asserts three things about the map it just built: no two ports share a hex,
every port has land in an adjoining hex, and all 70 sit in a single connected
ocean. A stray vertex that seals off the Red Sea stops being a subtle gameplay
bug and starts being a build failure.

The middle one was added after the fact and is worth the note. The first
version of this map checked only that ports could reach each other by sea,
which a port in the middle of the Pacific passes without difficulty, and 21 of
the 70 harbours shipped with no shore anywhere near them. An assertion that
only tests the thing you were already thinking about is not much of an
assertion.
"""

import sys
from collections import deque
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA = ROOT / "game" / "data"
OUT = ROOT / "game" / "src" / "world.rs"

COLS, ROWS = 72, 36  # 5 degrees per hex, the granularity the ports come in

# The ports table writes South East Asia with full stops and the price table
# does not. One spelling has to win before they can be joined.
ECON_ALIASES = {"S.E. Asia": "SE Asia", "Meditteranean": "Mediterranean"}


def fail(msg):
    print(f"gen_game_data: {msg}", file=sys.stderr)
    sys.exit(1)


def rows(path, header=None):
    """Yield tab-split data rows, skipping comments.

    If `header` is given it is the column list the first data line must match,
    so a reordered or renamed column is caught here rather than surfacing as a
    nonsense port three steps later.
    """
    first = True
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        parts = line.split("\t")
        if first and header is not None:
            first = False
            if parts != header:
                fail(f"{path.name} header is {parts}, expected {header}")
            continue
        first = False
        yield parts


# --------------------------------------------------------------------------
# Hex geometry. Pointy-top, stored as odd-r offset, converted to axial for any
# arithmetic, per redblobgames.com/grids/hexagons. Offset coordinates cannot be
# added or subtracted safely because the direction vectors depend on the parity
# of the row, which is the whole reason the conversion exists.
#
# The world wraps east to west, because it is a globe. It does not wrap north
# to south, because sailing over the pole is not a thing this game models.
# --------------------------------------------------------------------------

ODD_R_NEIGHBOURS = {
    0: [(+1, 0), (0, -1), (-1, -1), (-1, 0), (-1, +1), (0, +1)],   # even rows
    1: [(+1, 0), (+1, -1), (0, -1), (-1, 0), (0, +1), (+1, +1)],   # odd rows
}


def neighbours(col, row):
    for dc, dr in ODD_R_NEIGHBOURS[row & 1]:
        r = row + dr
        if 0 <= r < ROWS:
            yield (col + dc) % COLS, r


def cell_centre(col, row):
    """Longitude and latitude at the middle of a cell."""
    return -180.0 + 5.0 * col + 2.5, 90.0 - 5.0 * row - 2.5


def cell_of(lon, lat):
    col = int((lon + 180.0) // 5.0) % COLS
    row = min(ROWS - 1, max(0, int((90.0 - lat) // 5.0)))
    return col, row


def inside(polygon, lon, lat):
    """Even-odd crossing test."""
    hit = False
    n = len(polygon)
    for i in range(n):
        x1, y1 = polygon[i]
        x2, y2 = polygon[(i + 1) % n]
        if (y1 > lat) != (y2 > lat):
            x = x1 + (lat - y1) * (x2 - x1) / (y2 - y1)
            if lon < x:
                hit = not hit
    return hit


# --------------------------------------------------------------------------
# Load
# --------------------------------------------------------------------------

def load_ports():
    """Trade data from ports.tsv, position from port_place.tsv.

    Two files because the reference table is authoritative about one and not
    the other. Its coordinates are the old game's sextant readings and put
    London in the Norwegian Sea; its economies and specialities are the trade
    matrix, which is what the table is for. See port_place.tsv for the long
    version. Every port must appear in both, exactly once.
    """
    out = []
    for name, _lat, _lon, econ, spec, cap in rows(
        DATA / "ports.tsv", ["name", "lat", "lon", "economy", "specialty", "capital"]
    ):
        econ = ECON_ALIASES.get(econ, econ)
        if cap not in ("yes", "-"):
            fail(f"ports.tsv gives {name} capital {cap!r}; expected 'yes' or '-'")
        if cap == "yes" and econ == "-":
            fail(f"ports.tsv makes {name} a capital, but it does not trade")
        out.append({"name": name, "econ": econ, "spec": spec, "capital": cap == "yes"})

    places = {}
    for name, lat, lon, _note in rows(
        DATA / "port_place.tsv", ["port", "lat", "lon", "note"]
    ):
        if name in places:
            fail(f"port_place.tsv lists {name} twice")
        places[name] = (float(lat), float(lon))

    named = {p["name"] for p in out}
    if len(named) != len(out):
        fail("ports.tsv lists the same port twice")
    if named != set(places):
        missing = sorted(named - set(places))
        extra = sorted(set(places) - named)
        fail(
            "ports.tsv and port_place.tsv disagree about which ports exist. "
            f"Missing a position: {missing or 'none'}. "
            f"Positioned but not a port: {extra or 'none'}."
        )

    for p in out:
        p["lat"], p["lon"] = places[p["name"]]
    return out


def load_goods():
    names, econs, table = [], [], {}
    for good, econ, buy, sell in rows(DATA / "goods.tsv", ["good","economy","buy","sell"]):
        econ = ECON_ALIASES.get(econ, econ)
        if good not in names:
            names.append(good)
        if econ not in econs:
            econs.append(econ)
        table[(good, econ)] = (buy, sell)
    return names, econs, table


def load_nudges():
    """Explicit one-hex corrections; see game/data/port_nudge.tsv for why."""
    out = {}
    for name, dcol, drow, _why in rows(
        DATA / "port_nudge.tsv", ["port", "dcol", "drow", "why"]
    ):
        out[name] = (int(dcol), int(drow))
    return out


def load_coast():
    out = []
    for name, verts in rows(DATA / "coast.tsv"):
        poly = []
        for pair in verts.split():
            lon, lat = pair.split(",")
            poly.append((float(lon), float(lat)))
        if len(poly) < 3:
            fail(f"{name} has {len(poly)} vertices; a landmass needs at least 3")
        out.append((name, poly))
    return out


# --------------------------------------------------------------------------
# Build
# --------------------------------------------------------------------------

def build_land(coast, ports, nudges):
    land = bytearray(b"." * (COLS * ROWS))
    # Land first, then the seas that sit inside it. A name beginning with "-"
    # subtracts, which is how the Black Sea and the Gulf exist at all: they are
    # holes in Eurasia, and painting them in the same pass would depend on file
    # order, which is a nasty thing to make a map depend on.
    for name, poly in coast:
        if name.startswith("-"):
            continue
        for row in range(ROWS):
            for col in range(COLS):
                lon, lat = cell_centre(col, row)
                if inside(poly, lon, lat):
                    land[row * COLS + col] = ord("#")
    for name, poly in coast:
        if not name.startswith("-"):
            continue
        for row in range(ROWS):
            for col in range(COLS):
                lon, lat = cell_centre(col, row)
                if inside(poly, lon, lat):
                    land[row * COLS + col] = ord(".")

    # A port is a harbour, so the ship floats there. Carve it back to water
    # whatever the coastline said, then prove the carving did not leave anyone
    # stranded in an inland pond.
    carved = []
    for p in ports:
        col, row = cell_of(p["lon"], p["lat"])
        dcol, drow = nudges.get(p["name"], (0, 0))
        col, row = (col + dcol) % COLS, row + drow
        if not (0 <= row < ROWS):
            fail(f"the nudge on {p['name']} pushes it off the map")
        p["col"], p["row"] = col, row
        if land[row * COLS + col] == ord("#"):
            carved.append(p["name"])
        land[row * COLS + col] = ord("~")

    return land, carved


def check_one_ocean(land, ports):
    """Every port must be reachable from every other port by sea."""
    def water(col, row):
        return land[row * COLS + col] != ord("#")

    start = (ports[0]["col"], ports[0]["row"])
    seen = {start}
    queue = deque([start])
    while queue:
        col, row = queue.popleft()
        for nc, nr in neighbours(col, row):
            if (nc, nr) not in seen and water(nc, nr):
                seen.add((nc, nr))
                queue.append((nc, nr))

    stranded = [p["name"] for p in ports if (p["col"], p["row"]) not in seen]
    if stranded:
        fail(
            f"{len(stranded)} port(s) cannot be reached by sea from "
            f"{ports[0]['name']}: {', '.join(sorted(stranded))}. "
            "A coastline vertex in game/data/coast.tsv has sealed them off."
        )
    return len(seen)


def check_ashore(land, ports):
    """Every port must have land in an adjoining hex.

    This is the assertion whose absence let the first version of the map ship
    with 21 harbours floating in open water: the old check only asked whether
    ports could reach one another by sea, and a port in the middle of the
    Pacific passes that easily. A harbour is a place where the sea meets the
    shore, so require the shore.
    """
    adrift = []
    for p in ports:
        if not any(
            land[r * COLS + c] == ord("#") for c, r in neighbours(p["col"], p["row"])
        ):
            lon, lat = cell_centre(p["col"], p["row"])
            adrift.append(f"{p['name']} ({lat:+.0f}, {lon:+.0f})")
    if adrift:
        fail(
            f"{len(adrift)} port(s) have no land in any adjoining hex, so they "
            f"are harbours in open ocean: {', '.join(sorted(adrift))}. Either "
            "the position in game/data/port_place.tsv is wrong, or "
            "game/data/coast.tsv has no outline for the island it sits on."
        )


def collisions(ports):
    """Two ports in one hex would be indistinguishable on the map."""
    seen = {}
    clashes = []
    for p in ports:
        key = (p["col"], p["row"])
        if key in seen:
            clashes.append(f"{seen[key]} and {p['name']}")
        seen[key] = p["name"]
    return clashes


# --------------------------------------------------------------------------
# Emit
# --------------------------------------------------------------------------

def rust_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def price(v):
    """A dash means the trade is not on offer; -1 carries that into Rust."""
    return "-1" if v.strip() in ("-", "") else str(int(v))


def emit(land, ports, goods, econs, table, coast, water_cells):
    econ_index = {e: i for i, e in enumerate(econs)}
    good_index = {g: i for i, g in enumerate(goods)}

    L = []
    a = L.append
    a("// GENERATED by scripts/gen_game_data.py from game/data/*.tsv.")
    a("// Do not edit. Change the TSV and regenerate; CI runs --check.")
    a("")
    a(f"pub const COLS: i32 = {COLS};")
    a(f"pub const ROWS: i32 = {ROWS};")
    a("")
    a("/// Odd-r offset grid, row-major. '#' is land, '~' is open water.")
    a(f"/// {water_cells} of {COLS * ROWS} cells are sea reachable from every port.")
    a("pub const LAND: &[u8] = b\"\\")
    for row in range(ROWS):
        chunk = land[row * COLS:(row + 1) * COLS].decode()
        a(f"{chunk}\\")
    a('";')
    a("")

    a("pub struct Port {")
    a("    pub name: &'static str,")
    a("    pub col: i16,")
    a("    pub row: i16,")
    a("    /// Index into ECONOMIES, or -1 for a landfall that does not trade.")
    a("    pub econ: i8,")
    a("    /// Index into GOODS for the good this port is known for, or -1.")
    a("    pub specialty: i16,")
    a("    /// Has a shipyard. Only a capital sells ships; everywhere else you")
    a("    /// sail in with what you have. Never true of a port that does not trade.")
    a("    pub capital: bool,")
    a("}")
    a("")
    a(f"pub const PORTS: [Port; {len(ports)}] = [")
    for p in ports:
        e = econ_index.get(p["econ"], -1) if p["econ"] != "-" else -1
        s = good_index.get(p["spec"], -1) if p["spec"] != "-" else -1
        c = "true" if p["capital"] else "false"
        a(f"    Port {{ name: {rust_str(p['name'])}, col: {p['col']}, "
          f"row: {p['row']}, econ: {e}, specialty: {s}, capital: {c} }},")
    a("];")
    a("")

    a(f"pub const ECONOMIES: [&str; {len(econs)}] = [")
    for e in econs:
        a(f"    {rust_str(e)},")
    a("];")
    a("")
    a(f"pub const GOODS: [&str; {len(goods)}] = [")
    for g in goods:
        a(f"    {rust_str(g)},")
    a("];")
    a("")

    a("/// Base price in gold to buy one unit, indexed [good][economy].")
    a("/// -1 means no port of that economy stocks it.")
    a(f"pub const BUY: [[i16; {len(econs)}]; {len(goods)}] = [")
    for g in goods:
        vals = ", ".join(price(table[(g, e)][0]) for e in econs)
        a(f"    [{vals}], // {g}")
    a("];")
    a("")
    a("/// Base price in gold to sell one unit, indexed [good][economy].")
    a("/// -1 means every port of that economy already sells it, so there is no")
    a("/// market: carrying pepper to a pepper coast is the mistake the game is")
    a("/// about. The simulation prices those at a fraction of the buy price.")
    a(f"pub const SELL: [[i16; {len(econs)}]; {len(goods)}] = [")
    for g in goods:
        vals = ", ".join(price(table[(g, e)][1]) for e in econs)
        a(f"    [{vals}], // {g}")
    a("];")
    a("")
    a(f"/// Landmasses rasterised into LAND, for the record: {len(coast)}.")
    a("pub const LANDMASSES: [&str; %d] = [" % len(coast))
    for name, _ in coast:
        a(f"    {rust_str(name)},")
    a("];")
    return "\n".join(L) + "\n"


def main():
    check = "--check" in sys.argv

    ports = load_ports()
    goods, econs, table = load_goods()
    coast = load_coast()

    missing = sorted(
        {p["econ"] for p in ports if p["econ"] != "-"} - set(econs)
    )
    if missing:
        fail(f"ports.tsv names economies absent from goods.tsv: {missing}")

    nudges = load_nudges()
    unknown = sorted(set(nudges) - {p["name"] for p in ports})
    if unknown:
        fail(f"port_nudge.tsv names ports that do not exist: {unknown}")

    land, carved = build_land(coast, ports, nudges)

    # Order matters for the error you get. Collisions first, because two ports
    # in one hex makes every later message name the wrong port.
    clashes = collisions(ports)
    if clashes:
        fail("two ports share one hex, which the map cannot draw: "
             + "; ".join(clashes))
    check_ashore(land, ports)
    water_cells = check_one_ocean(land, ports)

    text = emit(land, ports, goods, econs, table, coast, water_cells)

    if check:
        if not OUT.is_file():
            fail(f"{OUT.relative_to(ROOT)} is missing. Run scripts/gen_game_data.py")
        if OUT.read_text(encoding="utf-8") != text:
            fail(
                f"{OUT.relative_to(ROOT)} is out of step with game/data/*.tsv. "
                "Run scripts/gen_game_data.py and commit the result."
            )
        print(f"gen_game_data: OK, {len(ports)} ports in one ocean of "
              f"{water_cells} cells")
        return

    OUT.write_text(text, encoding="utf-8")
    print(f"gen_game_data: wrote {OUT.relative_to(ROOT)}")
    print(f"  {len(ports)} ports, {len(goods)} goods, {len(econs)} economies")
    print(f"  {water_cells} sea cells reachable from every port")
    if carved:
        print(f"  {len(carved)} port(s) sat on rasterised land and were carved "
              f"back to water: {', '.join(carved)}")


if __name__ == "__main__":
    main()
