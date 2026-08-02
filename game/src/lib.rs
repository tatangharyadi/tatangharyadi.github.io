//! The boundary between the simulation and the page.
//!
//! There is no `wasm-bindgen` here and no `wasm-pack`. Every export below is a
//! plain `extern "C"` function taking and returning integers, and everything
//! larger than an integer is handed over as a pointer into this module's own
//! linear memory, which the page reads with a `Uint8Array`. That is the whole
//! protocol. It fits in a paragraph, it needs no generated glue, and the
//! JavaScript side is one hand-written module rather than an artefact.
//!
//! Two buffers cross the line:
//!
//! * the **render buffer**, two bytes per hex, at [`render_ptr`];
//! * a **text buffer** of UTF-8 JSON, at [`text_ptr`], rewritten by whichever
//!   query was last called.
//!
//! The text buffer is reused, so its contents are only valid until the next
//! call that writes it. The page reads it immediately and does not keep the
//! pointer, which is the one rule a caller has to follow.

pub mod hex;
pub mod market;
pub mod nav;
pub mod rng;
pub mod ship;
pub mod sim;
pub mod world;

use market::Markets;
use ship::Upgrade;
use sim::Game;
use world::{ECONOMIES, GOODS, PORTS};

/// One game, one thread. Wasm has no threads to race with, and a raw pointer
/// rather than a `static mut Option<Game>` keeps this free of references to a
/// mutable static, which is the part the compiler is right to complain about.
static mut GAME: *mut Game = core::ptr::null_mut();
static mut TEXT: *mut String = core::ptr::null_mut();

fn game() -> &'static mut Game {
    unsafe {
        if GAME.is_null() {
            GAME = Box::into_raw(Box::new(Game::new(1)));
        }
        &mut *GAME
    }
}

fn text() -> &'static mut String {
    unsafe {
        if TEXT.is_null() {
            TEXT = Box::into_raw(Box::new(String::with_capacity(16 * 1024)));
        }
        &mut *TEXT
    }
}

// -- lifecycle -------------------------------------------------------------

#[no_mangle]
pub extern "C" fn init(seed: u32) {
    unsafe {
        if !GAME.is_null() {
            drop(Box::from_raw(GAME));
        }
        GAME = Box::into_raw(Box::new(Game::new(seed)));
    }
}

// -- the map ---------------------------------------------------------------

#[no_mangle]
pub extern "C" fn cols() -> i32 {
    world::COLS
}

#[no_mangle]
pub extern "C" fn rows() -> i32 {
    world::ROWS
}

/// Redraw and return a pointer to `cols * rows * 2` bytes: for each hex, what
/// is there and how well it can be seen.
#[no_mangle]
pub extern "C" fn render_ptr() -> *const u8 {
    game().render().as_ptr()
}

#[no_mangle]
pub extern "C" fn render_len() -> i32 {
    (nav::CELLS * 2) as i32
}

// -- orders ----------------------------------------------------------------

#[no_mangle]
pub extern "C" fn step(dir: i32) -> i32 {
    if !(0..6).contains(&dir) {
        return 0;
    }
    game().step(dir as usize) as i32
}

#[no_mangle]
pub extern "C" fn set_course(port: i32) -> i32 {
    if port < 0 {
        return 0;
    }
    game().set_course(port as usize) as i32
}

#[no_mangle]
pub extern "C" fn under_way() -> i32 {
    game().under_way() as i32
}

#[no_mangle]
pub extern "C" fn sail_on() -> i32 {
    game().sail_on() as i32
}

#[no_mangle]
pub extern "C" fn wait_here() {
    game().wait();
}

#[no_mangle]
pub extern "C" fn buy(good: i32, qty: i32) -> i32 {
    if good < 0 {
        return 0;
    }
    game().buy(good as usize, qty) as i32
}

#[no_mangle]
pub extern "C" fn sell(good: i32, qty: i32) -> i32 {
    if good < 0 {
        return 0;
    }
    game().sell(good as usize, qty) as i32
}

#[no_mangle]
pub extern "C" fn upgrade(which: i32) -> i32 {
    game().upgrade(which) as i32
}

#[no_mangle]
pub extern "C" fn repair() -> i32 {
    game().repair() as i32
}

// -- text out --------------------------------------------------------------

#[no_mangle]
pub extern "C" fn text_ptr() -> *const u8 {
    text().as_ptr()
}

#[no_mangle]
pub extern "C" fn text_len() -> i32 {
    text().len() as i32
}

/// The things that never change: hex geometry, good names, port names and
/// positions. Fetched once when the page loads.
#[no_mangle]
pub extern "C" fn write_atlas() {
    let mut s = String::with_capacity(8 * 1024);
    s.push_str("{\"cols\":");
    s.push_str(&world::COLS.to_string());
    s.push_str(",\"rows\":");
    s.push_str(&world::ROWS.to_string());

    s.push_str(",\"goods\":[");
    for (i, g) in GOODS.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        push_str_json(&mut s, g);
    }
    s.push_str("],\"economies\":[");
    for (i, e) in ECONOMIES.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        push_str_json(&mut s, e);
    }
    s.push_str("],\"ports\":[");
    for (i, p) in PORTS.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str("{\"name\":");
        push_str_json(&mut s, p.name);
        s.push_str(",\"col\":");
        s.push_str(&p.col.to_string());
        s.push_str(",\"row\":");
        s.push_str(&p.row.to_string());
        s.push_str(",\"economy\":");
        push_str_json(&mut s, Markets::economy_name(i));
        s.push('}');
    }
    s.push_str("],\"directions\":[");
    for (i, d) in hex::DIR_NAMES.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        push_str_json(&mut s, d);
    }
    s.push_str("]}");
    *text() = s;
}

/// Everything the status column and the chronicle need, as of right now.
#[no_mangle]
pub extern "C" fn write_status() {
    let g = game();
    let mut s = String::with_capacity(8 * 1024);
    let (col, row) = hex::to_offset(g.at);
    let (wdir, wstr) = g.wind_here();
    let (cdir, cstr) = g.current_here();
    let port = g.port_here();

    s.push('{');
    kv_i(&mut s, "gold", g.gold, true);
    kv_i(&mut s, "day", g.day, false);
    kv_i(&mut s, "month", g.month, false);
    kv_i(&mut s, "year", g.year, false);
    kv_i(&mut s, "hour", g.hour as i32, false);
    kv_i(&mut s, "col", col, false);
    kv_i(&mut s, "row", row, false);
    kv_i(&mut s, "damage", g.ship.damage, false);
    kv_i(&mut s, "hull", g.ship.hull as i32, false);
    kv_i(&mut s, "rigging", g.ship.rigging as i32, false);
    kv_i(&mut s, "gunTier", g.ship.guns as i32, false);
    kv_i(&mut s, "guns", g.ship.gun_count(), false);
    kv_i(&mut s, "capacity", g.ship.capacity(), false);
    kv_i(&mut s, "cargo", g.ship.cargo(), false);
    kv_i(&mut s, "bluewater", g.ship.bluewater_rating(), false);
    kv_i(&mut s, "offshore", g.offshore(), false);
    kv_i(&mut s, "weather", g.weather_here(), false);
    kv_i(&mut s, "windDir", wdir as i32, false);
    kv_i(&mut s, "windStrength", wstr, false);
    kv_i(&mut s, "currentDir", cdir as i32, false);
    kv_i(&mut s, "currentStrength", cstr, false);
    kv_i(&mut s, "pirates", g.pirates_in_sight() as i32, false);
    kv_b(&mut s, "lost", g.lost);
    kv_b(&mut s, "underWay", g.under_way());
    kv_i(&mut s, "repairCost", g.ship.repair_cost(), false);

    s.push_str(",\"port\":");
    match port {
        Some(p) => s.push_str(&p.to_string()),
        None => s.push_str("-1"),
    }

    // What the yard would charge, so the page never has to know the tables.
    s.push_str(",\"upgrades\":[");
    for i in 0..3 {
        if i > 0 {
            s.push(',');
        }
        let u = Upgrade::from_index(i).unwrap();
        s.push_str("{\"name\":");
        push_str_json(&mut s, u.name());
        s.push_str(",\"tier\":");
        s.push_str(&g.ship.tier_of(u).to_string());
        s.push_str(",\"cost\":");
        match g.ship.upgrade_cost(u) {
            Some(c) => s.push_str(&c.to_string()),
            None => s.push_str("-1"),
        }
        s.push('}');
    }
    s.push(']');

    // The hold, and if there is a market here, what it will pay.
    s.push_str(",\"market\":[");
    let mut first = true;
    for good in 0..GOODS.len() {
        let have = g.ship.hold[good];
        let (buy, sell, glut) = match port {
            Some(p) => (
                g.markets.buy_price(p, good).unwrap_or(-1),
                g.markets.sell_price(p, good).unwrap_or(-1),
                Markets::is_glutted(p, good),
            ),
            None => (-1, -1, false),
        };
        if have == 0 && buy < 0 && sell < 0 {
            continue;
        }
        if !first {
            s.push(',');
        }
        first = false;
        s.push_str("{\"good\":");
        s.push_str(&good.to_string());
        s.push_str(",\"have\":");
        s.push_str(&have.to_string());
        s.push_str(",\"buy\":");
        s.push_str(&buy.to_string());
        s.push_str(",\"sell\":");
        s.push_str(&sell.to_string());
        s.push_str(",\"glut\":");
        s.push_str(if glut { "true" } else { "false" });
        s.push('}');
    }
    s.push(']');

    s.push_str(",\"known\":[");
    let mut first = true;
    for i in 0..PORTS.len() {
        if !g.discovered(i) {
            continue;
        }
        if !first {
            s.push(',');
        }
        first = false;
        s.push_str(&i.to_string());
    }
    s.push(']');

    s.push_str(",\"chronicle\":[");
    for (i, line) in g.chronicle().iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        push_str_json(&mut s, line);
    }
    s.push_str("]}");
    *text() = s;
}

/// What is known about one hex: the "there" panel, filled in when the player
/// points at somewhere that is not their own deck.
#[no_mangle]
pub extern "C" fn write_look(col: i32, row: i32) {
    let g = game();
    let h = hex::from_offset(col, row);
    let mut s = String::with_capacity(512);
    s.push('{');
    kv_i(&mut s, "col", col, true);
    kv_i(&mut s, "row", row, false);

    let idx = hex::index(h);
    let seen = idx.map(|i| g.fog[i]).unwrap_or(nav::UNSEEN);
    kv_i(&mut s, "seen", seen as i32, false);
    kv_i(&mut s, "distance", hex::wrapped_distance(g.at, h), false);

    let land = idx.map(nav::is_land_index).unwrap_or(false);
    kv_b(&mut s, "land", land);

    let port = PORTS
        .iter()
        .position(|p| p.col as i32 == col && p.row as i32 == row)
        .filter(|p| g.discovered(*p));
    s.push_str(",\"port\":");
    match port {
        Some(p) => {
            s.push_str(&p.to_string());
            s.push_str(",\"name\":");
            push_str_json(&mut s, PORTS[p].name);
            s.push_str(",\"economy\":");
            push_str_json(&mut s, Markets::economy_name(p));
        }
        None => s.push_str("-1"),
    }
    s.push('}');
    *text() = s;
}

// -- small JSON helpers ----------------------------------------------------
//
// Writing these out is shorter than the argument for pulling in serde would
// be, and it keeps the dependency list empty, which is the point.

fn kv_i(s: &mut String, key: &str, value: i32, first: bool) {
    if !first {
        s.push(',');
    }
    s.push('"');
    s.push_str(key);
    s.push_str("\":");
    s.push_str(&value.to_string());
}

fn kv_b(s: &mut String, key: &str, value: bool) {
    s.push_str(",\"");
    s.push_str(key);
    s.push_str("\":");
    s.push_str(if value { "true" } else { "false" });
}

fn push_str_json(s: &mut String, v: &str) {
    s.push('"');
    for c in v.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            c if (c as u32) < 0x20 => s.push(' '),
            c => s.push(c),
        }
    }
    s.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page parses these with `JSON.parse`, so a malformed string is a
    /// blank screen with a console error and no other clue. Checking the shape
    /// here is cheap; debugging it there is not.
    fn parses(s: &str) {
        // A dependency-free sanity check: balanced braces and brackets outside
        // of strings, and no trailing comma before a close.
        let (mut braces, mut brackets, mut in_str, mut esc) = (0i32, 0i32, false, false);
        let mut prev = ' ';
        for c in s.chars() {
            if in_str {
                if esc {
                    esc = false;
                } else if c == '\\' {
                    esc = true;
                } else if c == '"' {
                    in_str = false;
                }
                continue;
            }
            match c {
                '"' => in_str = true,
                '{' => braces += 1,
                '}' => {
                    braces -= 1;
                    assert_ne!(prev, ',', "trailing comma in {s}");
                }
                '[' => brackets += 1,
                ']' => {
                    brackets -= 1;
                    assert_ne!(prev, ',', "trailing comma in {s}");
                }
                _ => {}
            }
            if !c.is_whitespace() {
                prev = c;
            }
        }
        assert!(!in_str, "unterminated string in {s}");
        assert_eq!(braces, 0, "unbalanced braces in {s}");
        assert_eq!(brackets, 0, "unbalanced brackets in {s}");
    }

    #[test]
    fn the_atlas_is_well_formed() {
        write_atlas();
        let s = text().clone();
        parses(&s);
        assert!(s.contains("\"ports\""));
    }

    #[test]
    fn the_status_is_well_formed_at_sea_and_in_port() {
        init(11);
        write_status();
        parses(&text().clone());
        for d in 0..6 {
            if step(d) == 1 {
                break;
            }
        }
        write_status();
        let s = text().clone();
        parses(&s);
        assert!(s.contains("\"chronicle\""));
    }

    #[test]
    fn a_look_at_an_unseen_hex_is_still_well_formed() {
        init(12);
        write_look(0, 0);
        parses(&text().clone());
        write_look(-5, 999);
        parses(&text().clone());
    }

    #[test]
    fn the_render_buffer_is_the_length_it_says() {
        init(13);
        let len = render_len() as usize;
        assert_eq!(len, nav::CELLS * 2);
        let ptr = render_ptr();
        let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
        assert!(slice.iter().any(|b| *b == sim::CODE_SHIP));
    }

    #[test]
    fn orders_reject_nonsense_without_panicking() {
        init(14);
        assert_eq!(step(-1), 0);
        assert_eq!(step(6), 0);
        assert_eq!(set_course(-3), 0);
        assert_eq!(buy(-1, 5), 0);
        assert_eq!(sell(-1, 5), 0);
        assert_eq!(upgrade(9), 0);
    }
}
