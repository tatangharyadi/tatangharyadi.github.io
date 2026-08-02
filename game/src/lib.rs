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
//! * the **render buffer**, [`sim::STRIDE`] bytes per hex, at [`render_ptr`];
//! * a **text buffer** of UTF-8 JSON, at [`text_ptr`], rewritten by whichever
//!   query was last called.
//!
//! The text buffer is reused, so its contents are only valid until the next
//! call that writes it. The page reads it immediately and does not keep the
//! pointer, which is the one rule a caller has to follow.

pub mod commission;
pub mod hex;
pub mod market;
pub mod nav;
pub mod reputation;
pub mod rng;
pub mod ship;
pub mod sim;
pub mod world;

use commission::{Commission, Kind};
use market::Markets;
use ship::{Ship, Upgrade};
use sim::Game;
use world::{ECONOMIES, GOODS, PORTS};

/// One game, one thread. Wasm has no threads to race with, and a raw pointer
/// rather than a `static mut Option<Game>` keeps this free of references to a
/// mutable static, which is the part the compiler is right to complain about.
///
/// "No threads to race with" is true of the shipped binary and false of
/// `cargo test`, which runs the tests below in parallel against this one
/// global. That is a real race and it really bit; see the `HELM` guard in the
/// test module for what it cost to find and how it is held off now.
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

/// Redraw and return a pointer to `cols * rows * sim::STRIDE` bytes: for each
/// hex, the terrain, what is on it, and how well it can be seen.
#[no_mangle]
pub extern "C" fn render_ptr() -> *const u8 {
    game().render().as_ptr()
}

#[no_mangle]
pub extern "C" fn render_len() -> i32 {
    (nav::CELLS * sim::STRIDE) as i32
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

/// Lay a course to an arbitrary charted hex, in odd-r offset coordinates.
#[no_mangle]
pub extern "C" fn set_course_hex(col: i32, row: i32) -> i32 {
    game().set_course_hex(col, row) as i32
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

/// Trade the present ship for a class, by its index in `ship::CLASSES`.
///
/// Refuses for any of the reasons the yard listing already gives, so the page
/// can call it on a locked row and get the explanation in the chronicle rather
/// than having to duplicate the rules.
#[no_mangle]
pub extern "C" fn buy_ship(class: i32) -> i32 {
    game().buy_ship(class) as i32
}

#[no_mangle]
pub extern "C" fn repair() -> i32 {
    game().repair() as i32
}

/// Sign hands on at the quayside.
#[no_mangle]
pub extern "C" fn hire(n: i32) -> i32 {
    game().hire(n) as i32
}

/// Pay hands off. Never takes the last one.
#[no_mangle]
pub extern "C" fn discharge(n: i32) -> i32 {
    game().discharge(n) as i32
}

/// Buy stores, or sell them back with a negative quantity. `kind` indexes
/// `ship::STORES`: nought food, one water, two lumber.
///
/// The same order serves the chandler and a merchant met at sea, because from
/// the player's side it is the same decision and only the price differs. Which
/// of the two answered is in the chronicle line.
#[no_mangle]
pub extern "C" fn provision(kind: i32, qty: i32) -> i32 {
    game().provision(kind, qty) as i32
}

/// Mend the ship with lumber and hands rather than gold and a yard.
#[no_mangle]
pub extern "C" fn mend() -> i32 {
    game().mend() as i32
}

/// Buy a share in the port under the keel, opening one more of its goods.
#[no_mangle]
pub extern "C" fn invest() -> i32 {
    game().invest() as i32
}

/// Take the errand the factor at this quayside has put to you.
///
/// Nothing is paid and nothing is loaded. See `commission` for why the parcel
/// is bought by the player like any other cargo.
#[no_mangle]
pub extern "C" fn accept_commission() -> i32 {
    game().accept_commission() as i32
}

/// Hand over the parcel and be paid. Nought if you are at the wrong port or
/// short of the goods; the chronicle says which.
#[no_mangle]
pub extern "C" fn settle_commission() -> i32 {
    game().settle_commission() as i32
}

/// Give up the errand. No penalty, so this always succeeds if there was one.
#[no_mangle]
pub extern "C" fn abandon_commission() -> i32 {
    game().abandon_commission() as i32
}

/// Fire on the merchant sharing this hex.
///
/// It returns whether the order was *accepted*, not whether it succeeded, which
/// is the same contract as [`buy`] and [`sell`]: nought means there was nobody
/// alongside or nothing to shoot with, and the page should say so rather than
/// animate anything. Losing the fight still returns one, because the attempt
/// happened and the chronicle has a line about it.
#[no_mangle]
pub extern "C" fn attack() -> i32 {
    game().attack() as i32
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
    kv_i(&mut s, "shipClass", g.ship.class.index() as i32, false);
    s.push_str(",\"shipName\":");
    push_str_json(&mut s, g.ship.class.name());
    s.push_str(",\"shipRig\":");
    push_str_json(&mut s, g.ship.class.spec().rig);
    kv_b(&mut s, "shipBluewater", g.ship.class.is_bluewater());
    kv_i(&mut s, "hull", g.ship.hull as i32, false);
    kv_i(&mut s, "rigging", g.ship.rigging as i32, false);
    kv_i(&mut s, "gunTier", g.ship.guns as i32, false);
    kv_i(&mut s, "guns", g.ship.gun_count(), false);
    // Guns she carries and guns there are hands to fire. The page prints both
    // whenever they differ, because a rated broadside the crew cannot serve is
    // the one number in the status that would otherwise lie.
    kv_i(&mut s, "gunsWorked", g.ship.guns_worked(), false);
    kv_i(&mut s, "capacity", g.ship.capacity(), false);
    kv_i(&mut s, "cargo", g.ship.cargo(), false);
    kv_i(&mut s, "crew", g.ship.crew, false);
    kv_i(&mut s, "crewMin", g.ship.crew_min(), false);
    kv_i(&mut s, "crewMax", g.ship.crew_max(), false);
    kv_i(&mut s, "hireCost", ship::HIRE_ADVANCE, false);
    kv_i(&mut s, "wages", g.ship.wages(), false);
    kv_i(&mut s, "food", g.ship.food, false);
    kv_i(&mut s, "water", g.ship.water, false);
    kv_i(&mut s, "lumber", g.ship.lumber, false);
    kv_i(&mut s, "foodPerDay", g.ship.food_per_day(), false);
    kv_i(&mut s, "waterPerDay", g.ship.water_per_day(), false);
    // Days before the first of the two runs out. Capped so the page never has
    // to print an integer overflow at a crew of nought.
    kv_i(&mut s, "daysOfStores", g.ship.days_of_stores().min(999), false);
    kv_i(&mut s, "stores", g.ship.stores(), false);
    kv_i(&mut s, "mendable", g.ship.mendable(), false);
    // The names go with the prices so the page never has to keep its own copy
    // of the enum's order. `provision` takes the index into this array.
    s.push_str(",\"storeNames\":[");
    for (i, st) in ship::STORES.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        push_str_json(&mut s, st.name());
    }
    s.push(']');
    // Prices are fixed and the same everywhere, so they go out once with the
    // status rather than per port. Both rows go out because the page has to
    // print what the order will cost before the player gives it, and afloat
    // that is the dearer figure.
    for (key, afloat) in [("storePrices", false), ("storePricesAfloat", true)] {
        s.push_str(",\"");
        s.push_str(key);
        s.push_str("\":[");
        for (i, st) in ship::STORES.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let p = if afloat { st.price_afloat() } else { st.price() };
            s.push_str(&p.to_string());
        }
        s.push(']');
    }
    kv_i(&mut s, "bluewater", g.ship.bluewater_rating(), false);
    kv_i(&mut s, "offshore", g.offshore(), false);
    kv_i(&mut s, "weather", g.weather_here(), false);
    kv_i(&mut s, "windDir", wdir as i32, false);
    kv_i(&mut s, "windStrength", wstr, false);
    kv_i(&mut s, "currentDir", cdir as i32, false);
    kv_i(&mut s, "currentStrength", cstr, false);
    kv_i(&mut s, "pirates", g.pirates_in_sight() as i32, false);
    kv_i(&mut s, "merchants", g.merchants_in_sight() as i32, false);
    kv_i(&mut s, "navy", g.navy_in_sight() as i32, false);
    // Out looking for you, whether or not any of them is in sight. A hunt you
    // cannot see is the whole threat, so the count has to be legible before the
    // first topsail comes over the horizon rather than after.
    kv_i(&mut s, "navyOut", g.navy_out() as i32, false);
    kv_i(&mut s, "reputation", g.reputation, false);
    // `hunted` is the memory made visible. Without it the player has no way to
    // tell being chased from being near, and a mechanic nobody can perceive is
    // indistinguishable from one that is not there.
    kv_b(&mut s, "hunted", g.hunted());
    kv_b(&mut s, "merchantHere", g.merchant_here().is_some());
    kv_b(&mut s, "lost", g.lost);
    s.push_str(",\"standing\":");
    push_str_json(&mut s, g.standing());
    kv_b(&mut s, "underWay", g.under_way());
    kv_i(&mut s, "repairCost", g.ship.repair_cost(), false);

    s.push_str(",\"port\":");
    match port {
        Some(p) => s.push_str(&p.to_string()),
        None => s.push_str("-1"),
    }

    // Standing at this quayside, and what it is worth. The discount is small on
    // purpose, so it has to be printed as a figure: a twelve percent cut nobody
    // states is a rounding error the player will read as noise in the index.
    let (favour, discount, invested, invest_cost, open, stocked) = match port {
        Some(p) => (
            g.markets.favour_of(p),
            g.markets.favour_discount(p),
            g.markets.invested_of(p),
            g.markets.investment_cost(p).unwrap_or(-1),
            g.markets.open_count(p),
            Markets::stocked_count(p),
        ),
        None => (0, 0, 0, -1, 0, 0),
    };
    kv_i(&mut s, "favour", favour, false);
    kv_i(&mut s, "favourDiscount", discount, false);
    kv_i(&mut s, "invested", invested, false);
    kv_i(&mut s, "investCost", invest_cost, false);
    kv_i(&mut s, "openGoods", open, false);
    kv_i(&mut s, "stockedGoods", stocked, false);

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
        s.push_str(",\"ceiling\":");
        s.push_str(&g.ship.ceiling(u).to_string());
        s.push_str(",\"cost\":");
        match g.ship.upgrade_cost(u) {
            Some(c) => s.push_str(&c.to_string()),
            None => s.push_str("-1"),
        }
        s.push('}');
    }
    s.push(']');

    // The shipyard. Empty everywhere but a capital, which is how the page knows
    // whether to draw one at all. Locked rows come through with their reason
    // attached rather than omitted, for the same reason a shut good does.
    s.push_str(",\"yard\":[");
    if let Some(p) = port {
        for (i, o) in g.yard(p).into_iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let spec = o.class.spec();
            s.push_str("{\"class\":");
            s.push_str(&o.class.index().to_string());
            s.push_str(",\"name\":");
            push_str_json(&mut s, spec.name);
            s.push_str(",\"blurb\":");
            push_str_json(&mut s, spec.blurb);
            s.push_str(",\"rig\":");
            push_str_json(&mut s, spec.rig);
            s.push_str(",\"price\":");
            s.push_str(&o.price.to_string());
            s.push_str(",\"tradeIn\":");
            s.push_str(&o.trade_in.to_string());
            s.push_str(",\"hold\":");
            s.push_str(&Ship::capacity_of(o.class).to_string());
            s.push_str(",\"maxGuns\":");
            s.push_str(&ship::guns_at(spec.gun_max).to_string());
            s.push_str(",\"bluewater\":");
            s.push_str(if o.class.is_bluewater() { "true" } else { "false" });
            // Stated on the offer, because it is the one thing about a big hull
            // that takes something away and the yard is the last place a reader
            // can act on it. Finding out at the harbour mouth is too late.
            s.push_str(",\"deep\":");
            s.push_str(if o.class.is_deep_draught() { "true" } else { "false" });
            s.push_str(",\"locked\":");
            match &o.locked {
                Some(why) => push_str_json(&mut s, why),
                None => s.push_str("null"),
            }
            s.push('}');
        }
    }
    s.push(']');

    // The hold, and if there is a market here, what it will pay.
    s.push_str(",\"market\":[");
    let mut first = true;
    for good in 0..GOODS.len() {
        let have = g.ship.hold[good];
        let (buy, sell, glut, cool, shut) = match port {
            Some(p) => (
                g.markets.buy_price(p, good).unwrap_or(-1),
                g.markets.sell_price(p, good).unwrap_or(-1),
                Markets::is_glutted(p, good),
                g.markets.cooldown_of(p, good),
                // Priced and shut, which is a different row from unpriced. The
                // page needs both facts because they read as the same blank
                // otherwise, and one of them is an invitation to invest.
                g.markets.buy_price(p, good).is_some() && !g.markets.is_open(p, good),
            ),
            None => (-1, -1, false, 0, false),
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
        // Months before this price starts climbing back. A depressed market
        // with no visible clock on it reads as a bad port rather than a worn
        // route, which is the opposite of what the cooldown is for.
        s.push_str(",\"cool\":");
        s.push_str(&cool.to_string());
        s.push_str(",\"shut\":");
        s.push_str(if shut { "true" } else { "false" });
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

    // The charted harbours this hull cannot enter. A subset of `known`, sent
    // alongside it rather than folded into it, because the page draws the same
    // list either way and the bar is a property of the ship rather than of the
    // port: buy a galleon and the list changes without a harbour moving.
    s.push_str(",\"barred\":[");
    let mut first = true;
    for i in 0..PORTS.len() {
        if !g.discovered(i) || !g.barred_port(i) {
            continue;
        }
        if !first {
            s.push(',');
        }
        first = false;
        s.push_str(&i.to_string());
    }
    s.push(']');

    // The errand on the table here, and the one already accepted. Both are
    // `null` far more often than not, which is why they are objects rather than
    // a spread of flat keys: an absent commission has no fields to give
    // sensible values to, and `-1` for a port index reads as a bug.
    s.push_str(",\"offer\":");
    write_commission(&mut s, g, g.offer(), port);
    s.push_str(",\"commission\":");
    write_commission(&mut s, g, g.commission(), port);

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
    // Said in words because the chart says it in colour. A reader who cannot tell
    // the two blues apart clicks the hex and is told.
    kv_b(&mut s, "deep", idx.map(|i| g.draws_deep(i)).unwrap_or(false));

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
            kv_b(&mut s, "barred", g.barred_port(p));
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

/// One commission, or `null`. Written from the page's point of view rather than
/// the simulation's: the panel needs to know whether it can draw a button, not
/// which end of the errand this port happens to be.
fn write_commission(s: &mut String, g: &Game, c: Option<&Commission>, here: Option<usize>) {
    let Some(c) = c else {
        s.push_str("null");
        return;
    };
    let have = g.ship.hold[c.good];
    s.push_str("{\"kind\":");
    push_str_json(s, if c.kind == Kind::Deliver { "deliver" } else { "collect" });
    s.push_str(",\"text\":");
    push_str_json(s, &c.wording());
    s.push_str(",\"good\":");
    s.push_str(&c.good.to_string());
    s.push_str(",\"goodName\":");
    push_str_json(s, GOODS[c.good]);
    s.push_str(",\"qty\":");
    s.push_str(&c.qty.to_string());
    s.push_str(",\"have\":");
    s.push_str(&have.to_string());
    s.push_str(",\"gold\":");
    s.push_str(&c.gold.to_string());
    s.push_str(",\"favour\":");
    s.push_str(&c.favour.to_string());
    // The far end, always, because that is the harbour the errand is pointing
    // the player at and the panel names it whichever way the parcel travels.
    s.push_str(",\"other\":");
    s.push_str(&c.other.to_string());
    s.push_str(",\"otherName\":");
    push_str_json(s, PORTS[c.other].name);
    s.push_str(",\"paidAt\":");
    s.push_str(&c.paid_at().to_string());
    s.push_str(",\"paidAtName\":");
    push_str_json(s, PORTS[c.paid_at()].name);
    s.push_str(",\"boughtAtName\":");
    push_str_json(s, PORTS[c.bought_at()].name);
    // Both halves of "can it be discharged", separately, so the page can say
    // which one is missing instead of greying a button with no reason.
    s.push_str(",\"atPayingPort\":");
    s.push_str(if here == Some(c.paid_at()) { "true" } else { "false" });
    s.push_str(",\"enough\":");
    s.push_str(if have >= c.qty { "true" } else { "false" });
    s.push('}');
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
    use std::sync::{Mutex, MutexGuard};

    /// The C ABI is global by nature: one process, one game, reached through
    /// functions that take no handle. That is right for the browser, where wasm
    /// is single-threaded and there is exactly one page. It is wrong for
    /// `cargo test`, which runs these in parallel threads, because `init` frees
    /// the previous `Game` while another thread may still hold a reference into
    /// it.
    ///
    /// This is not theoretical and it is not a CI quirk. It surfaced as a
    /// SIGSEGV *after* all 46 tests had reported ok, and it reproduces locally
    /// about once in sixty runs at `--test-threads=16`. It cannot affect the
    /// shipped binary, which has no threads at all. It still had to be fixed:
    /// a test suite that is itself undefined behaviour is not evidence of
    /// anything, and "it passed" from a racy harness means only that it passed
    /// this time.
    ///
    /// So every test that touches an exported function takes this guard, and
    /// the guard is the only thing here that seeds a game. A test that forgets
    /// it has no way to choose its seed, which turns the omission into
    /// something you notice while writing the test rather than one run in sixty
    /// on someone else's machine.
    static HELM: Mutex<()> = Mutex::new(());

    #[must_use]
    fn helm(seed: u32) -> MutexGuard<'static, ()> {
        // Poisoning is deliberately ignored. One failing test should report one
        // failure, not turn every later test in this module red behind it.
        let guard = HELM.lock().unwrap_or_else(|e| e.into_inner());
        init(seed);
        guard
    }

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
        // The atlas does not depend on the seed, but it does read the global,
        // so it takes the guard like everything else.
        let _helm = helm(10);
        write_atlas();
        let s = text().clone();
        parses(&s);
        assert!(s.contains("\"ports\""));
    }

    #[test]
    fn the_status_is_well_formed_at_sea_and_in_port() {
        let _helm = helm(11);
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
        let _helm = helm(12);
        write_look(0, 0);
        parses(&text().clone());
        write_look(-5, 999);
        parses(&text().clone());
    }

    #[test]
    fn the_render_buffer_is_the_length_it_says() {
        let _helm = helm(13);
        let len = render_len() as usize;
        assert_eq!(len, nav::CELLS * sim::STRIDE);
        let ptr = render_ptr();
        let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
        assert!(slice.iter().any(|b| *b == sim::CODE_SHIP));
    }

    #[test]
    fn orders_reject_nonsense_without_panicking() {
        let _helm = helm(14);
        assert_eq!(step(-1), 0);
        assert_eq!(step(6), 0);
        assert_eq!(set_course(-3), 0);
        assert_eq!(buy(-1, 5), 0);
        assert_eq!(sell(-1, 5), 0);
        assert_eq!(upgrade(9), 0);
        // A store index off the end of the enum, and a hire of nobody. Both
        // reach the game from a page that could be holding a stale build.
        assert_eq!(provision(7, 5), 0);
        assert_eq!(provision(0, 0), 0);
        assert_eq!(hire(0), 0);
        assert_eq!(discharge(0), 0);
        // In port at the start of a game, so there is nobody alongside to fire
        // on and the order should be refused rather than swallowed.
        assert_eq!(attack(), 0);
    }

    /// A field the page reads and the Rust side stops writing is a silent
    /// `undefined` in a template string, which renders as the word "undefined"
    /// and fails nothing. This is the cheapest place to catch that.
    #[test]
    fn the_status_carries_everything_the_page_reads() {
        let _helm = helm(15);
        write_status();
        let s = text().clone();
        parses(&s);
        for key in [
            "\"reputation\"",
            "\"standing\"",
            "\"merchants\"",
            "\"hunted\"",
            "\"merchantHere\"",
            "\"navy\"",
            "\"navyOut\"",
            "\"cool\"",
            "\"shut\"",
            "\"favour\"",
            "\"favourDiscount\"",
            "\"investCost\"",
            "\"openGoods\"",
            "\"stockedGoods\"",
            "\"shipClass\"",
            "\"shipName\"",
            "\"shipRig\"",
            "\"shipBluewater\"",
            "\"ceiling\"",
            "\"yard\"",
            "\"crew\"",
            "\"crewMin\"",
            "\"crewMax\"",
            "\"hireCost\"",
            "\"wages\"",
            "\"gunsWorked\"",
            "\"food\"",
            "\"water\"",
            "\"lumber\"",
            "\"foodPerDay\"",
            "\"waterPerDay\"",
            "\"daysOfStores\"",
            "\"stores\"",
            "\"mendable\"",
            "\"storeNames\"",
            "\"storePrices\"",
            "\"storePricesAfloat\"",
            "\"barred\"",
            "\"offer\"",
            "\"commission\"",
        ] {
            assert!(s.contains(key), "the status no longer carries {key}");
        }
    }
}
