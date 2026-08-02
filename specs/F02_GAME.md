# F02: Helm, an age-of-sail trading simulation in Rust and WebAssembly

**Status:** done

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
| `game/src/lib.rs` | The export surface and the text writers |
| `game/src/sim.rs` | The simulation, and most of the tests |
| `game/src/market.rs` | Prices, demand, favour, investment |
| `game/src/ship.rs` | Hulls, classes, guns, draught, crew limits, cargo |
| `game/src/nav.rs` | Movement, sight, fog |
| `game/src/hex.rs` | Axial hex geometry |
| `game/src/commission.rs` | Optional cargo commissions |
| `game/src/reputation.rs` | Standing, infamy, pursuit |
| `game/src/rng.rs` | Deterministic, seeded |
| `game/src/world.rs` | **Generated.** Map, ports, economies, goods. |
| `game/data/*.tsv` | Source of truth for the world |
| `scripts/gen_game_data.py` | Writes `world.rs`, asserts the map is playable |
| `scripts/build_game.sh` | Builds the wasm, records the hash and the rustc version |

---

## Architecture

```
game.html
   │
   └─ js/game.js
        │  WebAssembly.instantiateStreaming(fetch("assets/game.wasm"))
        │  no import object, because the binary imports nothing
        ▼
   ┌──────────────────── exported ABI, all i32 ────────────────────┐
   │  init, cols, rows                                            │
   │  render_ptr / render_len   → the map buffer                  │
   │  text_ptr / text_len       → the last written string         │
   │  step, set_course, set_course_hex, under_way, sail_on, wait_here │
   │  buy, sell, invest                                           │
   │  upgrade, buy_ship, repair, mend                             │
   │  hire, discharge, provision                                  │
   │  accept_commission, settle_commission, abandon_commission    │
   │  attack                                                      │
   │  write_atlas, write_status, write_look                       │
   └───────────────────────────────────────────────────────────────┘
        │
   game/src/*.rs  ← the entire rule set
```

### The ABI

There is no `wasm-bindgen` and no `wasm-pack`. Exports are `#[no_mangle] extern "C"`
taking and returning `i32`. Strings cross as UTF-8 in linear memory: Rust writes
into a buffer, `text_ptr` and `text_len` describe it, and JavaScript decodes.

The binary has **no import section at all**, which is why
`WebAssembly.instantiateStreaming` is called with no import object. That is
unusual and it is the point: there is nothing for the host to inject, so there is
nothing the host can change about how the simulation behaves.

**`memory.grow` detaches every `ArrayBuffer` view, silently.** A cached
`Uint8Array` does not throw when this happens, it just reads zeroes. `js/game.js`
therefore never caches a view: `renderBytes()` and `takeText()` re-derive both the
pointer and a fresh view on every single call.

### The render buffer

Three bytes per hex, `sim::STRIDE = 3`:

| Offset | Channel | Values |
| --- | --- | --- |
| 0 | `AT_TERRAIN` | terrain class |
| 1 | `AT_MARK` | ship, port, hazard, fleet size |
| 2 | `AT_FOG` | `UNSEEN` 0, `REMEMBERED` 1, `VISIBLE` 2 |

Fog is monotonic. A hex that has been seen never returns to `UNSEEN`; it falls
back to `REMEMBERED`, which renders dimmed rather than hidden. Sailing away from
somewhere you have been does not un-explore it.

Terrain is carried by hex fill and objects by glyph, so ships and ports read
clearly against land and water without competing with a terrain symbol.

### The generated world

`game/data/*.tsv` is the source of truth and `scripts/gen_game_data.py` is the
only thing allowed to write `game/src/world.rs`. The generator asserts three
things about the map it just built:

1. No two ports share a hex.
2. Every port has land in an adjoining hex.
3. Every port is reachable by sea from every other.

The second assertion is the one worth the note. The first version of the map
checked only reachability, which a port in the middle of the Pacific passes
without difficulty, and 21 of the harbours shipped with no shore anywhere near
them. An assertion that only tests the thing you were already thinking about is
not much of an assertion.

### The simulation's rules

Everything below is simulation-side and testable on the host.

**Ships.** Six classes with different hull, rigging, guns, cargo and draught.
Deep-draught hulls are barred from shallow harbours, capitals excepted. The
largest classes are sold only at capitals and only after investment.

**Crew.** Each class has a minimum and a maximum. Crew take wages monthly and
consume food and water daily. Run either out and they start dying. A short-handed
ship is slower and still sails, rather than being stuck, which is a deliberate
choice: a dead end in a trading game is worse than a penalty.

**Markets.** Goods are regional, and a port does not reveal its whole list on
first arrival. Trading the same cargo into the same port repeatedly depresses the
price and needs a cooldown before demand recovers. Favour rises with volume traded
and lowers buy prices, and decays a point a month against a point per thousand
gold traded. Investment widens what a port stocks.

**Reputation and pursuit.** Beating pirates raises standing; attacking merchants
lowers it and raises infamy. The crown sends one king's ship at 15 infamy and one
more every 20 to a cap of 5. King's ships ignore the shallow-water clause. Hunters
carry a last-seen hex and a six-tick countdown, so evasion is possible without
being free.

**Commissions.** Roughly one arrival in three offers optional cargo work. The far
port is weighted 8:1 toward one never visited and the good 6:1 toward one never
bought, so the reward structure pushes toward exploration. No deadline, no penalty,
no consigned cargo, at most one at a time.

### Performance is not the reason for Rust

A step plus `write_status` plus decode plus `JSON.parse` measures around 37
microseconds in Chrome, of which the simulation itself is 0.23. The simulation was
never the cost. Rust is here for the type system, exhaustive matching and `cargo
test`, which is what lets rules this tangled be changed without breaking quietly.

`panic = "abort"` sharpens that. One bug had the arrest path calling the crown's
recall from inside a loop still holding indices into the vector the recall
resizes. With `panic = "abort"` that is not a stack trace, it is a stopped page.

### The build, and what the hash is worth

`game/` is the only part of this repository that needs a toolchain. The
containment is that the toolchain is never on anyone else's path: the binary is
committed, Pages serves it as a file, and a visitor, a contributor editing prose
and CI all run with no Rust installed.

```sh
CARGO="$(rustup which cargo)" bash scripts/build_game.sh   # rebuild
scripts/build_game.sh --check                              # verify the committed binary
cargo test --manifest-path game/Cargo.toml                 # host tests
```

Be clear about what the hash proves. It proves the binary has not changed since it
was committed. It does **not** prove the binary is what `game/src` compiles to,
because nothing here reproduces the build. Anyone who wants that guarantee has to
run the build themselves and read the diff. So that this is worth attempting, the
hash file carries the rustc version that produced the committed binary on a second
line, written by the script rather than kept by hand. A different rustc gives a
different hash, and that is not a fault, it is why the version is recorded.

`scripts/check_corpus.py` is strictly stronger than this, because it re-derives its
claim from the source text. The difference is stated rather than hidden.

---

## Acceptance criteria

| ID | Criterion | Evidence |
| --- | --- | --- |
| F02-AC01 | No two ports share a hex, every port has land adjacent, every port is reachable by sea. | `scripts/gen_game_data.py --check`, CI |
| F02-AC02 | `game/src/world.rs` is exactly what the generator emits from `game/data/*.tsv`. | `scripts/gen_game_data.py --check`, CI |
| F02-AC03 | The committed `assets/game.wasm` matches its recorded hash. | `scripts/build_game.sh --check`, CI |
| F02-AC04 | The rustc version that produced the committed binary is recorded, not remembered. | Written by `build_game.sh` into the hash file |
| F02-AC05 | A player can never be left unable to act by an empty strongbox. | `an_empty_strongbox_never_takes_the_last_hand` |
| F02-AC06 | A short-handed ship is penalised, not stranded. | `a_short_handed_ship_is_slower_and_still_sails` |
| F02-AC07 | Every ship class can provision for the longest stretch of open sea on the map. | `every_class_can_provision_the_emptiest_stretch_of_sea` |
| F02-AC08 | No ship class is shut out of any market. | `no_class_is_shut_out_of_any_market` |
| F02-AC09 | Every economy keeps at least one harbour a deep-draught hull can enter. | `every_economy_keeps_a_harbour_for_the_deepest_hulls` |
| F02-AC10 | The full rule set holds. | `cargo test`, 136 passing, CI |
| F02-AC11 | Fog is monotonic: a seen hex never returns to unseen. | `AT_FOG` transitions in `game/src/nav.rs`, covered by tests |
| F02-AC12 | No cached `ArrayBuffer` view survives a call. Every read re-derives pointer and view. | Structural: `renderBytes()` and `takeText()` in `js/game.js` |
| F02-AC13 | The binary imports nothing, and is instantiated with no import object. | Structural: `js/game.js` boot path |
| F02-AC14 | No rule lives in JavaScript. | Review: `js/game.js` calls exports and draws; it does not decide |
| F02-AC15 | Every control is keyboard reachable and shows a visible focus ring. Nothing uses `disabled`. | **Human**, keyboard traversal |
| F02-AC16 | The chronicle announces to a screen reader without stealing focus. | **Human**, `aria-live` on `#chronicle--lines` |
| F02-AC17 | Non-text marks on the chart hold 3:1 against every adjacency: tile against tile, ink against tile, tile against unexplored, grid stroke against tile, and dimmed against `--bg`. | **Human**, canvas compositing measurement. All five, not one. |
| F02-AC18 | `css/game.css` uses no Catppuccin colour not registered in `check_palette.py`. | `scripts/check_palette.py`, CI |
| F02-AC19 | The layout holds at every breakpoint and under `prefers-reduced-motion`. | **Human** |

AC17 is written as five adjacencies on purpose. A land fill once shipped having
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
| Weather, wind and current | Discussed and not built. Would touch movement, sight and provisioning at once. |
| A Latte flavour for the chart | `css/game.css` is Mocha-only. Every colour would need re-measuring against the tighter flavour. |
| Save and resume | The simulation is seeded and deterministic, so a save is a seed plus a command log rather than a state dump. Not built. |
