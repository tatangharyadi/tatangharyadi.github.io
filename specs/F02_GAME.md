# F02: Helm, an age-of-sail trading simulation in Rust and WebAssembly

**Status:** done

**What this document is for.** It is written to be sufficient to rebuild the
feature from nothing. "From scratch" here means re-implementable in the same
shape: the same exported ABI, the same byte encodings, the same JSON payloads,
the same data files. Those are specified exhaustively below, because they are the
seams a rebuild has to hit exactly or the two halves do not meet.

**Where the line is drawn.** Full fidelity on **interfaces**: the export
signatures, the render buffer, the text protocol, the TSV schemas, the class and
store tables. Behavioural fidelity on **rules**: the shape of each formula and
the constants that tune it, not every branch. The simulation is over four
thousand lines in `game/src/sim.rs` and an exhaustive transcription of it would
be a worse copy of the source. Where this document gives a constant and a
direction, that is the contract; where you need the exact arithmetic, the source
is named.

## Overview

Helm is a turn-based trading game on a hexagonal world map. The player commands a
ship, buys and sells across regional markets, hires and feeds a crew, refits and
repairs, takes commissions to unfamiliar ports, fights or avoids pirates, and
earns or loses standing with the crown.

It is reached as an easter egg from the masthead text rather than from the nav,
and it is on the site for one reason: it is the hardest thing here to fake. A
static page can claim depth. A simulation with a rule set, a generated world that
is asserted to be playable, and a test suite that runs on a real machine either
holds up or does not.

The whole simulation is Rust. `js/game.js` knows no rules at all: it instantiates
the module, calls exported functions, reads strings back out of linear memory and
draws SVG. Every decision about what happens is on the other side of that
boundary, which is what makes the tests worth anything.

**Implementation status:** shipped and live. `cargo test` reports 136 passing.

---

## Key files

| File | Role |
| --- | --- |
| `game.html` | The page. Loads nothing but `js/game.js` and `css/game.css`. |
| `js/game.js` | The boundary and the renderer. No rules. |
| `css/game.css` | Mocha-only stylesheet, separate from `css/style.css` |
| `assets/game.wasm` | The committed binary |
| `assets/game.wasm.sha256` | Its hash, and the rustc version that produced it |
| `game/Cargo.toml` | The crate. No dependencies. |
| `game/src/lib.rs` | The export surface, the text writers, the JSON helpers |
| `game/src/sim.rs` | The simulation, the turn loop, the render pass, most tests |
| `game/src/market.rs` | Prices, glut, cooldown, favour, investment |
| `game/src/ship.rs` | Classes, tiers, guns, draught, crew, stores |
| `game/src/nav.rs` | Movement, sight, fog, sea room |
| `game/src/hex.rs` | Axial hex geometry and offset conversion |
| `game/src/commission.rs` | Optional cargo commissions |
| `game/src/reputation.rs` | Standing, infamy, pursuit thresholds |
| `game/src/rng.rs` | Deterministic, seeded |
| `game/src/world.rs` | **Generated.** Land grid, ports, economies, goods. |
| `game/data/*.tsv` | Source of truth for the world |
| `scripts/gen_game_data.py` | Writes `world.rs`, asserts the map is playable |
| `scripts/build_game.sh` | Builds the wasm, records the hash and the rustc version |

---

## The crate

No dependencies, and none are wanted. `cargo tree` prints one line.

```toml
[package]
name = "helm"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]

[lib]
crate-type = ["cdylib", "rlib"]

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

`cdylib` is what produces a `.wasm`. `rlib` is what lets `cargo test` link the
same code on the host, so the tests run on a real machine rather than nowhere.
That pair is the reason the rule set can be tested at all.

---

## The ABI

There is no `wasm-bindgen` and no `wasm-pack`. Every export is a plain
`#[no_mangle] extern "C"` function taking and returning `i32`, and anything
larger crosses as a pointer into the module's own linear memory that the page
reads with a `Uint8Array`. That is the whole protocol.

The binary has **no import section at all**, which is why
`WebAssembly.instantiateStreaming(fetch("assets/game.wasm"))` is called with no
import object. There is nothing for the host to inject, so there is nothing the
host can change about how the simulation behaves.

State is two globals: one `Game` and one `String` text buffer, both lazily boxed.
One game, one thread. That is right for a browser, where wasm is single-threaded
and there is exactly one page, and wrong for `cargo test`, which runs in parallel
against the same globals. See "Testing" below.

### Exports

Unless stated otherwise, a return of `1` means the order was **accepted** and `0`
means it was refused. Accepted is not the same as succeeded: `attack()` returns
`1` for a fight that was lost, because the attempt happened and there is a
chronicle line about it. Refusals are silent to the caller and explained in the
chronicle, so the page never has to duplicate a rule to know why.

| Export | Signature | Contract |
| --- | --- | --- |
| `init` | `(seed: u32)` | Drop any existing game and start a new one on this seed |
| `cols` | `() -> i32` | `72` |
| `rows` | `() -> i32` | `36` |
| `render_ptr` | `() -> *const u8` | Redraws, then points at the render buffer |
| `render_len` | `() -> i32` | `CELLS * STRIDE`, which is `2592 * 3 = 7776` |
| `step` | `(dir: i32) -> i32` | Sail one hex. `dir` outside `0..6` returns `0` without touching the game. |
| `set_course` | `(port: i32) -> i32` | Autopilot to a port index. Negative returns `0`. |
| `set_course_hex` | `(col: i32, row: i32) -> i32` | Autopilot to an odd-r offset hex |
| `under_way` | `() -> i32` | `1` if a course is laid |
| `sail_on` | `() -> i32` | Advance one leg of the laid course |
| `wait_here` | `()` | Pass time in place. No return. |
| `buy` | `(good: i32, qty: i32) -> i32` | Negative `good` returns `0` |
| `sell` | `(good: i32, qty: i32) -> i32` | Negative `good` returns `0` |
| `upgrade` | `(which: i32) -> i32` | `which` indexes `Upgrade`: 0 hull, 1 rigging, 2 guns |
| `buy_ship` | `(class: i32) -> i32` | Index into `ship::CLASSES`. Refuses for any reason the yard listing already gives. |
| `repair` | `() -> i32` | Yard repair, paid in gold |
| `mend` | `() -> i32` | Repair with lumber and hands instead of gold and a yard |
| `hire` | `(n: i32) -> i32` | Sign hands on. `hire(0)` returns `0`. |
| `discharge` | `(n: i32) -> i32` | Pay hands off. Never takes the last one. `discharge(0)` returns `0`. |
| `provision` | `(kind: i32, qty: i32) -> i32` | `kind` indexes `ship::STORES`: 0 food, 1 water, 2 lumber. Negative `qty` sells back. Out-of-range `kind` or zero `qty` returns `0`. |
| `invest` | `() -> i32` | Buy a share in the port under the keel, opening one more of its goods |
| `accept_commission` | `() -> i32` | Take the errand on offer. Nothing is paid and nothing is loaded. |
| `settle_commission` | `() -> i32` | Hand over and be paid. `0` at the wrong port or short of the goods. |
| `abandon_commission` | `() -> i32` | Give it up. No penalty, so this succeeds whenever there was one. |
| `attack` | `() -> i32` | Fire on the merchant sharing this hex. `0` if nobody is alongside or there is nothing to shoot with. |
| `text_ptr` | `() -> *const u8` | Points at the text buffer |
| `text_len` | `() -> i32` | Its length in bytes |
| `write_atlas` | `()` | Fill the text buffer with the atlas |
| `write_status` | `()` | Fill it with the current status |
| `write_look` | `(col: i32, row: i32)` | Fill it with what is known about one hex |

### Reading memory back

Strings cross as UTF-8 in linear memory. The text buffer is **reused**, so its
contents are valid only until the next call that writes it. The page reads it
immediately and does not keep the pointer. That is the one rule a caller has to
follow.

**`memory.grow` detaches every `ArrayBuffer` view, silently.** A cached
`Uint8Array` does not throw when this happens, it reads zeroes. `js/game.js`
therefore never caches a view. Both accessors re-derive the pointer *and* a fresh
view on every single call:

```js
function renderBytes() {
  const { render_ptr, render_len, memory } = wasm;
  return new Uint8Array(memory.buffer, render_ptr(), render_len());
}

function takeText() {
  const { text_ptr, text_len, memory } = wasm;
  const bytes = new Uint8Array(memory.buffer, text_ptr(), text_len());
  return JSON.parse(decoder.decode(bytes));
}
```

`memory.buffer` is re-read inside the function, not closed over. A rebuild that
hoists either the view or the buffer out has a bug that appears only after the
heap grows, which is to say not during development. Passing a subarray rather
than the whole heap also means `TextDecoder` copies out exactly the bytes wanted
and the view is dead immediately.

---

## The render buffer

`sim::STRIDE = 3` bytes per hex, row-major over the odd-r offset grid, so hex
`(col, row)` starts at `(row * COLS + col) * 3`.

| Offset | Constant | Channel |
| --- | --- | --- |
| 0 | `AT_TERRAIN` | What the sea floor is |
| 1 | `AT_MARK` | What is on it |
| 2 | `AT_FOG` | How well it can be seen |

Terrain is sent separately from the mark rather than packed into one byte,
because the page fills the hex by terrain and draws the mark on top. Packed, the
two channels would be exclusive: a raider reported as a raider says nothing about
the water he is sitting in, and whether that water is shallow or deep is the
whole question when you are deciding to run for it.

### Codes

Both channels draw from one table. `AT_TERRAIN` only ever holds 0, 1 or 2;
`AT_MARK` can hold any of them, and defaults to the terrain code when nothing is
there.

| Value | Constant | Meaning | Glyph |
| --- | --- | --- | --- |
| 0 | `CODE_SHALLOW` | Shallow water | `~` |
| 1 | `CODE_DEEP` | Deep water | `≈` |
| 2 | `CODE_LAND` | Land | `#` |
| 3 | `CODE_PORT` | A discovered port | `⌂` |
| 4 | `CODE_SHIP` | The player | `@` |
| 5 | `CODE_PIRATE` | One raider | `X` |
| 6 | `CODE_MERCHANT` | One trader | `o` |
| 7 | `CODE_NAVY` | One king's ship | `▲` |
| 8 | `CODE_PIRATES` | More than one raider | `XX` |
| 9 | `CODE_MERCHANTS` | More than one trader | `oo` |
| 10 | `CODE_NAVIES` | More than one king's ship | `▲▲` |

`js/game.js` holds exactly this order as `GLYPH`, indexed by the mark byte:

```js
const GLYPH = ["~", "≈", "#", "⌂", "@", "X", "o", "▲", "XX", "oo", "▲▲"];
```

The stack codes are marked, not counted. Three raiders and four are the same
decision; one and more-than-one is not. They exist because before them a crowded
hex looked exactly like a quiet one and the "in sight" count in the status column
disagreed with what a reader could count on the chart. Traders cluster at ports,
which is where every voyage starts, so the first thing a new player saw was a
number that did not add up.

### Fog

| Value | Constant |
| --- | --- |
| 0 | `UNSEEN` |
| 1 | `REMEMBERED` |
| 2 | `VISIBLE` |

Fog is monotonic. A hex that has been seen never returns to `UNSEEN`; it falls
back to `REMEMBERED`, which renders dimmed rather than hidden. Sailing away from
somewhere you have been does not un-explore it.

### Draw order

`Game::render()` writes the whole buffer every call, in this order. A rebuild
must keep it, because later writers overwrite earlier ones and the order is the
priority rule.

1. Every cell: terrain code into both `AT_TERRAIN` and `AT_MARK`, current fog
   into `AT_FOG`.
2. Every **discovered** port: `CODE_PORT` into `AT_MARK`, and if the hex is
   `UNSEEN` it is promoted to `REMEMBERED`. A charted harbour stays on the chart.
3. Every **merchant** on a `VISIBLE` hex: `CODE_MERCHANT`, or `CODE_MERCHANTS` if
   a trader is already marked there.
4. Every **pirate** on a `VISIBLE` hex, over the top of merchants: `CODE_PIRATE`
   or `CODE_NAVY`, promoted to the stacked code if a raider or king's ship is
   already marked. Only raiders count toward crowding; a pirate standing over a
   trader is a raider on his own, and the trader is not the news.
5. The player's hex: `CODE_SHIP` and `VISIBLE`.

---

## Hex geometry

Axial coordinates internally, odd-r offset at the boundary. The world wraps east
to west and does not wrap north to south.

- `COLS = 72`, `ROWS = 36`, `CELLS = 2592`.
- `from_offset(col, row) = Hex(col - (row - (row & 1)) / 2, row)`.
- `to_offset(h) = ((h.q + (h.r - (h.r & 1)) / 2).rem_euclid(COLS), h.r)`. The
  column wraps; the row does not, so a row outside the map stays outside and the
  caller has to notice.
- `normalise()` is `from_offset(to_offset(h))`. Axial arithmetic runs unwrapped,
  because sight lines and distances need it to, and every *stored* position goes
  through `normalise` first. Without that, a ship that sailed west out of the
  Atlantic and round to Japan would stop recognising the harbour it was sitting
  in, because `q` and `q + 72` are the same place and compare unequal.

**The six directions, in axial deltas, starting due east and going
anticlockwise.** This order is fixed and three things index into it: the keyboard
bindings, the wind and current directions in the status, and `DIR_NAMES`.

| Index | Delta `(q, r)` | Name | Key |
| --- | --- | --- | --- |
| 0 | `(1, 0)` | east | `d` |
| 1 | `(1, -1)` | north-east | `e` |
| 2 | `(0, -1)` | north-west | `q` |
| 3 | `(-1, 0)` | west | `a` |
| 4 | `(-1, 1)` | south-west | `z` |
| 5 | `(0, 1)` | south-east | `c` |

which is why `js/game.js` reads:

```js
const KEYS = { q: 2, e: 1, a: 3, d: 0, z: 4, c: 5 };
```

---

## The generated world

`game/data/*.tsv` is the source of truth and `scripts/gen_game_data.py` is the
only thing allowed to write `game/src/world.rs`. Its header says so and CI runs
`--check`.

### The inputs

| File | Columns | Purpose |
| --- | --- | --- |
| `ports.tsv` | `name`, `lat`, `lon`, `economy`, `specialty`, `capital` | The port roster, on the reference table's own 5-degree coordinates. `economy` of `-` means the port does not trade and serves only as a landfall. |
| `port_place.tsv` | `port`, `lat`, `lon`, `note` | Where each port actually is, in real degrees. Exists because the reference coordinates are the old game's sextant readings rather than geography. |
| `port_nudge.tsv` | `port`, `dcol`, `drow`, `why` | One-hex displacements applied after placement, for harbours closer together than the 5 degrees a hex covers |
| `goods.tsv` | `good`, `economy`, `buy`, `sell` | Base prices by economy. `buy` of `-` means no port of that economy sells it. `sell` of `-` means every port of that economy already has it, so unloading there pays badly. That dash is the same-goods penalty, read from the source table rather than invented. |
| `coast.tsv` | `name`, then vertices as `lon,lat` pairs | Coastlines as closed outlines |

Position and trade data come from different files on purpose. A 72 by 36 grid is
2,592 cells, and a hand-corrected character grid of that size is not something a
reviewer can check by reading. An outline is: a vertex is a place.

### The output

`game/src/world.rs`, carrying:

| Item | Shape |
| --- | --- |
| `COLS`, `ROWS` | `i32`, 72 and 36 |
| `LAND` | `&[u8]`, a row-major odd-r grid of `#` for land and `.` for water. 1,773 of the 2,592 cells are sea. |
| `LANDMASSES` | `[&str; 28]`, the outlines from `coast.tsv` that were rasterised, recorded so the map can be traced back to its source |
| `Port` | `{ name: &str, col: i16, row: i16, econ: i8, specialty: i16, capital: bool }`. `econ` of `-1` is a landfall. |
| `PORTS` | `[Port; 70]` |
| `ECONOMIES` | `[&str; 8]`: Mediterranean, N. Europe, Americas, West Africa, East Africa, Arabia, SE Asia, Far East |
| `GOODS` | `[&str; 42]` |

### Carving

Coastlines are outlines and harbours sit on the coast, so rasterising the two
independently puts some harbours inland. The generator resolves this in the
harbour's favour: a port hex that came out land is carved back to water, and the
names are printed on every run. Twenty-eight of the seventy are carved. That is
not a defect to drive to zero, it is the outline resolution meeting a 5-degree
hex, and printing the list is what keeps it honest rather than silent.

### The three assertions

The generator asserts, and refuses to write, unless:

1. **No two ports share a hex.**
2. **Every port has land in an adjoining hex.**
3. **Every port is reachable by sea from every other**, one connected ocean.

The second is the one worth the note. The first version of the map checked only
reachability, which a port in the middle of the Pacific passes without
difficulty, and 21 of the harbours shipped with no shore anywhere near them. An
assertion that only tests the thing you were already thinking about is not much
of an assertion.

---

## The text protocol

Three writers fill the shared text buffer with UTF-8 JSON, hand-rolled rather
than serialised, which is shorter than the argument for taking on `serde` and
keeps the dependency list empty. `js/game.js` decodes and `JSON.parse`s.

### `write_atlas()`

The things that never change. Fetched once when the page loads.

| Key | Type |
| --- | --- |
| `cols`, `rows` | number |
| `goods` | array of 42 strings |
| `economies` | array of 8 strings |
| `ports` | array of 70 objects: `name`, `col`, `row`, `economy` |
| `directions` | array of 6 strings, in `DIRECTIONS` order |

### `write_status()`

Everything the status column, the panels and the chronicle need, as of right now.
This is the widest interface in the feature. Grouped for reading; the JSON is
flat except where noted.

| Group | Keys |
| --- | --- |
| Time and place | `day`, `month`, `year`, `hour`, `col`, `row` |
| Purse | `gold` |
| Ship identity | `shipClass`, `shipName`, `shipRig`, `shipBluewater` |
| Ship tiers | `hull`, `rigging`, `gunTier`, `guns`, `gunsWorked` |
| Hold | `capacity`, `cargo` |
| Condition | `damage`, `repairCost`, `mendable` |
| Crew | `crew`, `crewMin`, `crewMax`, `hireCost`, `wages` |
| Stores | `food`, `water`, `lumber`, `foodPerDay`, `waterPerDay`, `daysOfStores`, `stores`, `storeNames`, `storePrices`, `storePricesAfloat` |
| Sea state | `bluewater`, `offshore`, `weather`, `windDir`, `windStrength`, `currentDir`, `currentStrength` |
| Traffic | `pirates`, `merchants`, `navy`, `navyOut`, `merchantHere` |
| Standing | `reputation`, `standing`, `hunted` |
| Motion | `underWay`, `lost` |
| This port | `port` (index or `-1`), `favour`, `favourDiscount`, `invested`, `investCost`, `openGoods`, `stockedGoods` |
| Shipwright | `upgrades`: three objects, `{ name, tier, ceiling, cost }`, cost `-1` when unavailable |
| Shipyard | `yard`: array, empty away from a capital |
| Trade | `market`: array |
| Charts | `known`, `barred`: arrays of port indices |
| Errands | `offer`, `commission`: object or `null` |
| Log | `chronicle`: array of strings |

A `yard` entry: `class`, `name`, `blurb`, `rig`, `price`, `tradeIn`, `hold`,
`maxGuns`, `bluewater`, `deep`, `locked`. `locked` is `null` or a string reason.
Locked rows come through **with their reason attached rather than omitted**, so
the page can call `buy_ship` on one and get the explanation in the chronicle
instead of duplicating the rules.

A `market` entry: `good` (index into `GOODS`), `have`, `buy`, `sell`, `glut`,
`cool`, `shut`. Rows where the player holds none and neither price exists are
omitted entirely. Prices are `-1` where absent. `shut` means priced but not open,
which is a different row from unpriced: both read as blank otherwise, and one of
them is an invitation to invest.

`offer` and `commission` are objects or `null`, not a spread of flat keys,
because an absent commission has no fields to give sensible values to and `-1`
for a port index reads as a bug. A commission object: `kind` (`deliver` or
`collect`), `text`, `good`, `goodName`, `qty`, `have`, `gold`, `favour`, `other`,
`otherName`, `paidAt`, `paidAtName`, `boughtAtName`, `atPayingPort`, `enough`.
The last two are the two halves of "can it be discharged", sent separately so the
page can say which one is missing rather than greying a button with no reason.

Three of these keys exist because the alternative was a status that lies.
`gunsWorked` is printed alongside `guns` whenever they differ, because a rated
broadside the crew cannot serve is otherwise the one number that misleads.
`navyOut` counts hunters whether or not any is in sight, because a hunt you
cannot see is the whole threat. `hunted` is the pirates' memory made visible;
without it the player cannot tell being chased from being near, and a mechanic
nobody can perceive is indistinguishable from one that is not there.

### `write_look(col, row)`

What is known about one hex: the "there" panel.

| Key | Type | Note |
| --- | --- | --- |
| `col`, `row` | number | Echoed back |
| `seen` | number | The fog value, `UNSEEN` for an off-map hex |
| `distance` | number | Wrapped hex distance from the player |
| `land` | boolean | |
| `deep` | boolean | Said in words because the chart says it in colour |
| `port` | number | Index, or `-1`. Only if discovered. |
| `name`, `economy`, `barred` | string, string, boolean | Present only when `port` is not `-1` |

Out-of-range coordinates are answered rather than rejected: `write_look(-5, 999)`
returns well-formed JSON, and there is a test that says so.

---

## The rules

Everything here is simulation-side and testable on the host. The constants are
the contract; the arithmetic that reads them is in the named module.

### Ships (`game/src/ship.rs`)

Six classes, ordered smallest to largest. The order is the discriminant that
crosses the boundary as `shipClass` and indexes `buy_ship`.

| Index | Class | Rig | Hull | Rig tier | Guns | Handling | Price | Crew | Deep | Needs invested |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | Balsa | three-point | 0 to 1 | 0 to 1 | 0 to 1 | 1.00 | 1,200 | 3 to 12 | no | 0 |
| 1 | Latina | three-point | 1 to 2 | 1 to 2 | 0 to 1 | 1.12 | 7,000 | 6 to 24 | no | 0 |
| 2 | Redonda | four-point | 1 to 2 | 2 to 3 | 0 to 2 | 1.30 | 14,000 | 8 to 30 | no | 0 |
| 3 | Carrack | four-point | 2 to 3 | 2 to 3 | 1 to 3 | 0.95 | 34,000 | 15 to 60 | no | 0 |
| 4 | Galleon | four-point | 3 to 4 | 3 to 4 | 1 to 4 | 0.90 | 78,000 | 25 to 100 | **yes** | 15,000 |
| 5 | Heavy Galleon | four-point | 4 to 4 | 3 to 4 | 2 to 4 | 0.85 | 165,000 | 35 to 130 | **yes** | 40,000 |

Each range is start to maximum: what a freshly bought ship comes fitted with, and
how far the shipwright will take her. `yards` is a bitmask over `ECONOMIES` in
declaration order, so where a class is built is data rather than code. Two shapes
are worth reading off the table. **Rig follows geography**: lateen craft are
built around the Mediterranean and the Arabian and African coasts, square-rigged
ocean ships come out of the Atlantic yards, so where you are matters when you
want a different ship. **Only the galleons are blue-water**, because only they
reach rigging tier 4, which is the whole reason to want one.

Tier tables, all indexed 0 to `MAX_TIER = 4`:

| Table | Values |
| --- | --- |
| `HULL_CAPACITY` | 40, 90, 160, 260, 400 |
| `BLUEWATER_RATING` (hexes from land) | 1, 2, 4, 7, 99 |
| `GUN_COUNT` | 0, 6, 14, 28, 48 |
| `GUN_HOLD_COST` | 0, 8, 20, 40, 70 |
| `HULL_COST` | 0, 2,400, 7,200, 19,000, 46,000 |
| `RIG_COST` | 0, 3,000, 9,500, 24,000, 58,000 |
| `GUN_COST` | 0, 1,800, 5,600, 15,000, 38,000 |

`BLUEWATER = 99` is the rating at and above which a ship can leave soundings
entirely. Sailing beyond your rating is possible and is how ships are lost.

The two systems, class and tier, meet at exactly one place: `ceiling()`. Class
does not scale a tier, it caps it. That is why adding classes disturbed neither
the costs nor any of the arithmetic that reads them.

**Draught runs the other way from everything else.** `deep_draught` is a hull
property, stated rather than inferred from the rig, because a galleon is deep
because she is big and blue-water because of what is above the deck. Reading one
off the other would tie a change in rigging to where she may anchor. Deep hulls
are barred from harbours with no sea room, capitals excepted, and king's ships
ignore the clause.

### Crew and stores (`game/src/ship.rs`)

| Constant | Value |
| --- | --- |
| `HIRE_ADVANCE` | 25 gold a hand, once |
| `MONTHLY_WAGE` | 8 gold a hand, every month |
| `HANDS_PER_FOOD` | 10 hands fed per unit per day |
| `HANDS_PER_WATER` | 5 hands watered per unit per day |
| `LUMBER_PER_POINT` | 1 unit mends one point of damage |

Stores are an enum, not three more goods: a good has a price that moves with what
you have done to the market, a port that may not deal in it and a glut that
punishes you for bringing too much, and none of that should apply to the crew's
drinking water.

| Index | Store | Price at the chandler | Afloat |
| --- | --- | --- | --- |
| 0 | food | 6 | 9 |
| 1 | water | 3 | 5 |
| 2 | lumber | 15 | 23 |

Afloat is `(price * 3 + 1) / 2`, half as much again: she is doing you a favour
and she knows it. It lives in Rust rather than at the call site because the page
prints the figure before the player commits, and a premium written out in both
languages is a premium that will drift.

Water is the one that runs out. A barrel waters five and feeds nobody, so a long
leg is provisioned two thirds in water by volume, which is both the historical
shape of the problem and the reason a hold that looked ample at the quay is not.
Stores ride in the hold and count against the same capacity as cargo, so
provisioning for a long leg is paid for in the profit you could have carried.

The crew minimum is not a bar on sailing. It is the figure everything else is
measured against: at or above it she sails and fights at her rated speed, below
it both fall off, and `manning()` is the single place that decides how steep.
Every penalty is a multiplier and none is a bar, because a crew that starves at
sea would otherwise leave the player becalmed forever with no order that could
help.

### Markets (`game/src/market.rs`)

| Constant | Value | Meaning |
| --- | --- | --- |
| `INDEX_NEUTRAL` | 100 | Price index at rest |
| `INDEX_FLOOR` / `INDEX_CEILING` | 50 / 150 | How far a market can move |
| `POINTS_PER_1000_GOLD` | 1 | How fast trading moves the index |
| `MAX_MOVE_PER_TRADE` | 10 | Cap on one transaction's effect |
| `GLUT_PAYS_PERCENT` | 35 | What a glutted market pays |
| `SPECIALTY_DISCOUNT_PERCENT` | 80 | What a port's own specialty costs |
| `COOLDOWN_MONTHS` | 2 | Months before a worn price starts climbing back |
| `FAVOUR_MAX` | 60 | |
| `FAVOUR_PER_1000_GOLD` | 1 | Favour earned by volume |
| `FAVOUR_DISCOUNT_MAX_PERCENT` | 12 | The most favour is ever worth |
| `FAVOUR_DECAY_PER_MONTH` | 1 | Against a point per thousand gold traded |
| `OPEN_AT_FIRST` | 3 | Goods a port reveals on first arrival |
| `INVEST_STEP` | 5,000 | Gold per share, each opening one more good |

The favour discount is small on purpose, which is exactly why it has to be
printed as a figure: a twelve percent cut nobody states is a rounding error the
player reads as noise in the index. The cooldown has to be printed too, as
`cool`, because a depressed market with no visible clock reads as a bad port
rather than a worn route, which is the opposite of what it is for.

### Reputation and pursuit (`game/src/reputation.rs`)

| Constant | Value |
| --- | --- |
| `FLOOR` / `CEILING` | -100 / 100 |
| `BEAT_PIRATE` | +2 |
| `SINK_BONUS` | +3 |
| `RAID_MERCHANT` | -12 |
| `RESIST_NAVY` | -6 |
| `SINK_NAVY` | -10 |
| `STRENGTH_PER_POINT` | 12 |
| `HEXES_PER_POINT` | 40, sailed clean |
| `NAVY_FROM` | -15, the first king's ship |
| `NAVY_PER_SHIP` | -20, each one after |
| `NAVY_MAX` | 5 |

Hunters carry a last-seen hex and a countdown, so evasion is possible without
being free. What range a given raider uses is not the base `HUNT_RANGE` of 5 but
`Game::hunt_range`, which reads your reputation and whether this one has fought
you before.

### Commissions (`game/src/commission.rs`)

| Constant | Value |
| --- | --- |
| `OFFER_CHANCE_PERCENT` | 35, on arrival |
| `MARGIN_PERCENT` | 70 |
| `GOLD_PER_HEX` | 18 |
| `NEAR` / `FAR` | 5 / 30 hexes |
| `QTY_MIN` / `QTY_MAX` | 6 / 18 |
| `FAVOUR_MIN` / `FAVOUR_MAX` | 2 / 5 |
| `UNVISITED_WEIGHT` | 8 |
| `UNTRADED_WEIGHT` | 6 |

The two weights are the whole mechanic: the far port is drawn 8:1 toward one
never visited and the good 6:1 toward one never bought, so the reward structure
pushes toward exploration without a quest log. No deadline, no penalty, no
consigned cargo, at most one at a time. The parcel is bought by the player like
any other cargo, which is why `accept_commission` pays nothing and loads nothing.

### Sea room (`game/src/nav.rs`)

`SHALLOW_ROOM = 8` is the threshold: water within two hexes of a harbour, counted,
decides whether a deep hull can enter it.

---

## The page

`game.html` loads `css/game.css` and `js/game.js` and nothing else. The structure
`js/game.js` binds to:

| Id | Element | Role |
| --- | --- | --- |
| `boot` | `p.helm--notice` | "Loading the chart." Replaced or hidden once instantiated. |
| `helm` | `main` | `hidden` until the module is up |
| `chart` | `svg` | `role="img"`, `aria-hidden="true"`, `focusable="false"`. The markup carries a placeholder `viewBox`; `js/game.js` replaces it with one computed from the geometry below. |
| `who` / `who--facts` | `section` | The ship |
| `here` / `here--body` | `section` | The hex under the keel: market, chandler, shipwright, yard, errands |
| `there` / `there--body` / `known` | `section` | The looked-at hex, and the charted ports |
| `how` | `section` | The controls |
| `sail-0` … `sail-5` | `button` | One per direction, ids matching the direction index |
| `order-on` | `button` | Sail on, space |
| `order-wait` | `button` | Wait, `.` |
| `order-attack` | `button` | Attack, `f` |
| `order-new` | `button` | New game |
| `commission-order` | `button` | Built by `js/game.js`, not in the markup. Accept and settle share the one id, because they are one slot. |
| `chronicle` / `chronicle--lines` | `section` / `ol` | `role="list"`, `aria-live="polite"`, `aria-atomic="false"` |

The chart is one `<svg>` drawn from the render buffer. Geometry in `js/game.js`:

| Constant | Value |
| --- | --- |
| `VIEW_COLS` / `VIEW_ROWS` | 25 / 17, the window onto the 72 by 36 world |
| `SIZE` | 13 |
| `HEX_W` | `Math.sqrt(3) * SIZE` |
| `HEX_H` | `1.5 * SIZE` |
| `STRIDE` | 3, matching `sim::STRIDE` |

`STRIDE` and `GLYPH` are the two places `js/game.js` restates something Rust
owns. Both are read directly off the tables above, and both would fail loudly and
immediately if they drifted, which is why neither is fetched.

### Colour

`css/game.css` is **Catppuccin Mocha only**, regardless of system theme, and it
writes literal hex values rather than reading the custom properties in
`css/style.css`. That makes it the fifth copy of the palette, and
`scripts/check_palette.py` holds it to the dark block and makes it name any extra
Catppuccin colour it uses in `MOCHA_EXTRA`. Adding a colour to the chart means
registering it there or CI fails.

---

## Building and testing

`game/` is the only part of this repository that needs a toolchain, and the
containment is that the toolchain is never on anyone else's path: the binary is
committed, Pages serves it as a file, and a visitor, a contributor editing prose
and CI all run with no Rust installed.

```sh
CARGO="$(rustup which cargo)" bash scripts/build_game.sh   # rebuild the wasm
bash scripts/build_game.sh --check                         # verify the committed binary
cargo test --manifest-path game/Cargo.toml                 # host tests
python3 scripts/gen_game_data.py                           # regenerate world.rs
python3 scripts/gen_game_data.py --check                   # verify it is fresh
```

The target is `wasm32-unknown-unknown`. The two cargo invocations differ
deliberately: the wasm build needs `rustup which cargo` because the wasm target
lives under the rustup toolchain, and the host tests do not.

### What the hash is worth

`assets/game.wasm.sha256` proves the binary has not changed since it was
committed. It does **not** prove the binary is what `game/src` compiles to,
because nothing here reproduces the build. Anyone who wants that guarantee has to
run the build themselves and read the diff, which is the honest instruction and
is in the script's own header. So that this is worth attempting, the hash file
carries the rustc version that produced the committed binary on a second line,
written by the script rather than kept by hand. A different rustc gives a
different hash, and that is not a fault, it is why the version is recorded.

`scripts/check_corpus.py` is strictly stronger, because it re-derives its claim
from the source text. The difference is stated rather than hidden.

### Testing across the ABI

The C ABI is global by nature: one process, one game, reached through functions
that take no handle. `cargo test` runs in parallel threads, so `init` can free a
`Game` another thread still holds a reference into. That is not theoretical. It
surfaced as a SIGSEGV *after* all tests had reported ok, and it reproduced about
once in sixty runs at `--test-threads=16`.

It cannot affect the shipped binary, which has no threads at all. It still had to
be fixed: a test suite that is itself undefined behaviour is not evidence of
anything, and "it passed" from a racy harness means only that it passed this
time. Every test that touches an exported function takes a `HELM` mutex guard,
and **the guard is the only thing that seeds a game**, so a test that forgets it
has no way to choose its seed. That turns the omission into something you notice
while writing the test rather than one run in sixty on someone else's machine.

The writers are checked for shape by a dependency-free `parses()` helper:
balanced braces and brackets outside strings, no trailing comma before a close.
The page uses `JSON.parse`, so a malformed string is a blank screen with a
console error and no other clue. There is also a test that asserts the status
still carries each of forty keys the page reads, because a field Rust stops
writing is a silent `undefined` in a template string, which renders as the word
"undefined" and fails nothing.

### Performance is not the reason for Rust

A step plus `write_status` plus decode plus `JSON.parse` measures around 37
microseconds in Chrome, of which the simulation itself is 0.23. The simulation
was never the cost. Rust is here for the type system, exhaustive matching and
`cargo test`, which is what lets rules this tangled be changed without breaking
quietly.

`panic = "abort"` sharpens that. One bug had the arrest path calling the crown's
recall from inside a loop still holding indices into the vector the recall
resizes. With `panic = "abort"` that is not a stack trace, it is a stopped page.

---

## Acceptance criteria

| ID | Criterion | Evidence |
| --- | --- | --- |
| F02-AC01 | No two ports share a hex, every port has land adjacent, every port is reachable by sea. | `scripts/gen_game_data.py --check`, CI |
| F02-AC02 | `game/src/world.rs` is exactly what the generator emits from `game/data/*.tsv`. | `scripts/gen_game_data.py --check`, CI |
| F02-AC03 | The committed `assets/game.wasm` matches its recorded hash. | `scripts/build_game.sh --check`, CI |
| F02-AC04 | The rustc version that produced the committed binary is recorded, not remembered. | Written by `build_game.sh` into the hash file |
| F02-AC05 | `render_len()` equals `CELLS * STRIDE`, and the buffer holds a `CODE_SHIP`. | `the_render_buffer_is_the_length_it_says` |
| F02-AC06 | All three writers emit well-formed JSON, in port and at sea, and for an off-map hex. | `the_atlas_is_well_formed`, `the_status_is_well_formed_at_sea_and_in_port`, `a_look_at_an_unseen_hex_is_still_well_formed` |
| F02-AC07 | The status still carries every key the page reads. | `the_status_carries_everything_the_page_reads` |
| F02-AC08 | Nonsense orders are refused rather than panicking: out-of-range directions, negative indices, zero quantities. | `orders_reject_nonsense_without_panicking` |
| F02-AC09 | A player can never be left unable to act by an empty strongbox. | `an_empty_strongbox_never_takes_the_last_hand` |
| F02-AC10 | A short-handed ship is penalised, not stranded. | `a_short_handed_ship_is_slower_and_still_sails` |
| F02-AC11 | Every ship class can provision for the longest stretch of open sea on the map. | `every_class_can_provision_the_emptiest_stretch_of_sea` |
| F02-AC12 | No ship class is shut out of any market. | `no_class_is_shut_out_of_any_market` |
| F02-AC13 | Every economy keeps at least one harbour a deep-draught hull can enter. | `every_economy_keeps_a_harbour_for_the_deepest_hulls` |
| F02-AC14 | The full rule set holds. | `cargo test`, 136 passing, CI |
| F02-AC15 | Fog is monotonic: a seen hex never returns to unseen. | `AT_FOG` transitions in `game/src/nav.rs`, covered by tests |
| F02-AC16 | No cached `ArrayBuffer` view survives a call. Every read re-derives pointer and view. | Structural: `renderBytes()` and `takeText()` in `js/game.js` |
| F02-AC17 | The binary imports nothing, and is instantiated with no import object. | Structural: `js/game.js` boot path |
| F02-AC18 | No rule lives in JavaScript. | Review: `js/game.js` calls exports and draws; it does not decide |
| F02-AC19 | Every control is keyboard reachable and shows a visible focus ring. Nothing uses `disabled`. | **Human**, keyboard traversal |
| F02-AC20 | The chronicle announces to a screen reader without stealing focus. | **Human**, `aria-live` on `#chronicle--lines` |
| F02-AC21 | Non-text marks on the chart hold 3:1 against every adjacency: tile against tile, ink against tile, tile against unexplored, grid stroke against tile, and dimmed against `--bg`. | **Human**, canvas compositing measurement. All five, not one. |
| F02-AC22 | `css/game.css` uses no Catppuccin colour not registered in `MOCHA_EXTRA`. | `scripts/check_palette.py`, CI |
| F02-AC23 | The layout holds at every breakpoint and under `prefers-reduced-motion`. | **Human** |

AC21 is written as five adjacencies on purpose. A land fill once shipped having
been measured only against the other tiles and against the ink, and it failed
against bare background at 1.09:1, against the grid stroke at 1.05:1, and it
inverted the fog, because dimming moves a tile toward the background it was
already indistinguishable from.

---

## Deferred

| Item | Note |
| --- | --- |
| Reproducible builds | Would make the hash prove what people assume it proves. Needs a pinned toolchain in CI and a byte-for-byte rebuild. |
| Discoverability | The easter egg is close to undiscoverable. A colophon sentence is proposed and unresolved. |
| Weather, wind and current as player-facing mechanics | The status already carries all four values. Making them bite would touch movement, sight and provisioning at once. |
| A Latte flavour for the chart | `css/game.css` is Mocha-only. Every colour would need re-measuring against the tighter flavour. |
| Save and resume | The simulation is seeded and deterministic, so a save is a seed plus a command log rather than a state dump. Not built. |
