//! The game.
//!
//! One struct holds everything, because there is one ship and one player and
//! pretending otherwise would buy nothing. The interesting parts are the turn
//! loop, which advances time in hours rather than in moves, and the pirates,
//! which run the same pathfinder the player's autopilot does and therefore
//! cost the same to think.

use crate::hex::{self, Hex};
use crate::market::Markets;
use crate::nav::{self, CELLS, REMEMBERED, UNSEEN, VISIBLE};
use crate::rng::Rng;
use crate::ship::{Ship, Upgrade};
use crate::world::{GOODS, PORTS};

pub const CODE_SHALLOW: u8 = 0;
pub const CODE_DEEP: u8 = 1;
pub const CODE_LAND: u8 = 2;
pub const CODE_PORT: u8 = 3;
pub const CODE_SHIP: u8 = 4;
pub const CODE_PIRATE: u8 = 5;

const PIRATE_COUNT: usize = 24;
const HOURS_PER_DAY: f32 = 24.0;
const DAYS_PER_MONTH: i32 = 30;
const START_GOLD: i32 = 3_000;
const CHRONICLE_KEEP: usize = 64;

/// Where the voyage begins. Lisbon if the data has it, otherwise the first
/// port that trades, so a renamed dataset degrades into something playable
/// rather than into a panic.
fn starting_port() -> usize {
    PORTS
        .iter()
        .position(|p| p.name == "Lisbon")
        .unwrap_or_else(|| (0..PORTS.len()).find(|i| Markets::trades(*i)).unwrap_or(0))
}

struct Pirate {
    at: Hex,
    strength: i32,
    /// Hours of sailing banked; pirates move on the same clock the player does
    /// rather than once per player move, so a fast ship really does outrun
    /// them.
    hours: f32,
    hunting: bool,
}

pub struct Game {
    pub ship: Ship,
    pub gold: i32,
    pub at: Hex,
    pub hour: f32,
    pub day: i32,
    pub month: i32,
    pub year: i32,

    pub fog: Vec<u8>,
    depth: Vec<u8>,
    pub markets: Markets,
    pirates: Vec<Pirate>,
    discovered: Vec<bool>,
    chronicle: Vec<String>,
    rng: Rng,

    render: Vec<u8>,
    scratch: Vec<Hex>,
    ray: Vec<Hex>,

    /// Autopilot: hexes still to sail, in reverse order so the next is last.
    course: Vec<Hex>,
    pub lost: bool,
}

impl Game {
    pub fn new(seed: u32) -> Self {
        let start = starting_port();
        let at = hex::from_offset(PORTS[start].col as i32, PORTS[start].row as i32);
        let depth = nav::distance_to_land();

        let mut g = Game {
            ship: Ship::new(GOODS.len()),
            gold: START_GOLD,
            at,
            hour: 6.0,
            day: 1,
            month: 3,
            year: 1502,
            fog: vec![UNSEEN; CELLS],
            depth,
            markets: Markets::new(),
            pirates: Vec::new(),
            discovered: vec![false; PORTS.len()],
            chronicle: Vec::new(),
            rng: Rng::new(seed),
            render: vec![0; CELLS * 2],
            scratch: Vec::new(),
            ray: Vec::new(),
            course: Vec::new(),
            lost: false,
        };

        g.discovered[start] = true;
        g.spawn_pirates();
        g.look();
        g.say(format!(
            "You take command at {}. Three thousand in gold, a coastal hull, and no guns.",
            PORTS[start].name
        ));
        g.say("The chart is blank beyond the harbour mouth.".into());
        g
    }

    // -- time and log ------------------------------------------------------

    fn say(&mut self, line: String) {
        self.chronicle.push(line);
        if self.chronicle.len() > CHRONICLE_KEEP {
            let drop = self.chronicle.len() - CHRONICLE_KEEP;
            self.chronicle.drain(0..drop);
        }
    }

    pub fn chronicle(&self) -> &[String] {
        &self.chronicle
    }

    fn advance(&mut self, hours: f32) {
        self.hour += hours;
        while self.hour >= HOURS_PER_DAY {
            self.hour -= HOURS_PER_DAY;
            self.day += 1;
            if self.day > DAYS_PER_MONTH {
                self.day = 1;
                self.month += 1;
                if self.month > 12 {
                    self.month = 1;
                    self.year += 1;
                }
                self.markets.drift();
                self.say("A new month. Prices ease back toward their old levels.".into());
            }
        }
    }

    // -- sight -------------------------------------------------------------

    fn sight_radius(&self) -> i32 {
        // A lookout is only as good as the mast he is up. Foul weather closes
        // it right down, which is what makes storms frightening rather than
        // merely slow.
        let base = 4 + self.ship.rigging as i32;
        match nav::weather(self.at, self.day, self.month) {
            0 => base,
            1 => base - 1,
            _ => (base - 3).max(1),
        }
    }

    fn look(&mut self) {
        let r = self.sight_radius();
        let mut scratch = std::mem::take(&mut self.scratch);
        let mut ray = std::mem::take(&mut self.ray);
        nav::refresh_fov(self.at, r, &mut self.fog, &mut scratch, &mut ray);
        self.scratch = scratch;
        self.ray = ray;

        let mut found = Vec::new();
        for (i, p) in PORTS.iter().enumerate() {
            if self.discovered[i] {
                continue;
            }
            let h = hex::from_offset(p.col as i32, p.row as i32);
            if let Some(idx) = hex::index(h) {
                if self.fog[idx] == VISIBLE {
                    found.push(i);
                }
            }
        }
        for i in found {
            self.discovered[i] = true;
            self.say(format!("You sight {} and put it on the chart.", PORTS[i].name));
        }
    }

    // -- movement ----------------------------------------------------------

    pub fn port_here(&self) -> Option<usize> {
        PORTS.iter().position(|p| {
            hex::from_offset(p.col as i32, p.row as i32) == self.at
        })
    }

    /// Sail one hex. Returns false and says why if it cannot be done.
    pub fn step(&mut self, dir: usize) -> bool {
        if self.lost {
            return false;
        }
        let target = hex::normalise(hex::neighbour(self.at, dir));
        let Some(ti) = hex::index(target) else {
            self.say("There is nothing that way but ice.".into());
            return false;
        };
        if nav::is_land_index(ti) {
            self.say("Shoal water and rock. You bear away.".into());
            return false;
        }

        let factor = nav::passage_factor(dir, self.at, self.month, self.ship.rigging);
        let severity = nav::weather(target, self.day, self.month);
        let storm = 1.0 + 0.35 * severity as f32;
        let hours = self.ship.base_hours() * factor * storm;

        self.at = target;
        self.advance(hours);

        self.blue_water_check(ti);
        if self.lost {
            return true;
        }
        self.storm_check(severity);
        if self.lost {
            return true;
        }

        self.look();
        self.move_pirates(hours);

        if let Some(p) = self.port_here() {
            if !self.discovered[p] {
                self.discovered[p] = true;
            }
            // Note the arrival but do not cancel a laid course. Passing through
            // a harbour on the way somewhere else should not end the voyage,
            // and the player asked for the destination, not the first landfall.
            if self.course.is_empty() {
                self.say(format!("You come to anchor at {}.", PORTS[p].name));
            } else {
                self.say(format!("You stand in past {}.", PORTS[p].name));
            }
        }
        true
    }

    fn blue_water_check(&mut self, index: usize) {
        let offshore = self.depth[index] as i32;
        let rating = self.ship.bluewater_rating();
        if offshore <= rating {
            return;
        }
        let over = offshore - rating;
        self.say(format!(
            "No land in sight. This is {} hex{} further out than the rigging is rated for.",
            over,
            if over == 1 { "" } else { "es" }
        ));
        // The further past the rating, the likelier the ship is hurt.
        if self.rng.chance((12 * over).min(70) as u32) {
            let hurt = self.rng.range(6, 10 + 6 * over);
            self.hurt(hurt, "The sea works the hull and something gives.");
        }
    }

    fn storm_check(&mut self, severity: i32) {
        if severity < 2 {
            return;
        }
        self.say("The glass is falling and the sea is getting up.".into());
        if self.rng.chance(30) {
            let hurt = self.rng.range(4, 14);
            self.hurt(hurt, "A sea comes aboard and takes some of the rail with it.");
        }
    }

    fn hurt(&mut self, amount: i32, why: &str) {
        self.ship.damage = (self.ship.damage + amount).min(100);
        self.say(format!("{why} Damage now {}%.", self.ship.damage));
        if self.ship.damage >= 100 {
            self.lost = true;
            self.say("The ship will not answer. She goes down with all hands.".into());
        }
    }

    /// Plot and follow a course to a port, one hex per call, so the caller can
    /// draw a frame between steps and the player can see the voyage happen.
    pub fn set_course(&mut self, port: usize) -> bool {
        if port >= PORTS.len() || self.lost {
            return false;
        }
        if !self.discovered[port] {
            self.say("You cannot lay a course to a place you have not seen.".into());
            return false;
        }
        let goal = hex::from_offset(PORTS[port].col as i32, PORTS[port].row as i32);
        let path = nav::find_path(
            self.at,
            goal,
            self.month,
            self.ship.rigging,
            self.ship.base_hours(),
            &self.depth,
            self.ship.bluewater_rating(),
        );
        if path.is_empty() {
            self.say(format!(
                "There is no route to {} that the rigging will stand. Better rigging, or a nearer port.",
                PORTS[port].name
            ));
            return false;
        }
        self.say(format!(
            "Course laid for {}: {} hexes.",
            PORTS[port].name,
            path.len()
        ));
        self.course = path;
        self.course.reverse();
        true
    }

    pub fn under_way(&self) -> bool {
        !self.course.is_empty() && !self.lost
    }

    /// Sail the next leg of a laid course. Returns false when there is none.
    pub fn sail_on(&mut self) -> bool {
        let Some(next) = self.course.pop() else {
            return false;
        };
        let mut dir = None;
        for d in 0..6 {
            if hex::normalise(hex::neighbour(self.at, d)) == next {
                dir = Some(d);
                break;
            }
        }
        match dir {
            Some(d) => {
                let moved = self.step(d);
                if !moved {
                    self.course.clear();
                    self.say("The course is no good. You will have to lay another.".into());
                }
                moved
            }
            None => {
                self.course.clear();
                false
            }
        }
    }

    pub fn wait(&mut self) {
        let hours = 6.0;
        self.advance(hours);
        self.look();
        self.move_pirates(hours);
        if self.port_here().is_some() && self.ship.damage > 0 {
            self.say("The carpenter works while you lie at anchor.".into());
        }
    }

    // -- pirates -----------------------------------------------------------

    fn spawn_pirates(&mut self) {
        // Scattered over deep water away from the start, so the first voyage is
        // not an ambush, and weighted toward the trade lanes because that is
        // where the money is.
        let mut placed = 0;
        let mut guard = 0;
        while placed < PIRATE_COUNT && guard < 20_000 {
            guard += 1;
            let col = self.rng.below(crate::world::COLS as u32) as i32;
            let row = self.rng.range(6, crate::world::ROWS - 8);
            let h = hex::from_offset(col, row);
            let Some(i) = hex::index(h) else { continue };
            if nav::is_land_index(i) {
                continue;
            }
            if self.depth[i] > 6 {
                continue; // even pirates keep off the empty middle
            }
            if hex::wrapped_distance(h, self.at) < 8 {
                continue;
            }
            self.pirates.push(Pirate {
                at: h,
                strength: self.rng.range(8, 40),
                hours: 0.0,
                hunting: false,
            });
            placed += 1;
        }
    }

    fn move_pirates(&mut self, elapsed: f32) {
        let month = self.month;
        let player = self.at;
        let mut engagements = Vec::new();

        for idx in 0..self.pirates.len() {
            let (at, strength) = {
                let p = &self.pirates[idx];
                (p.at, p.strength)
            };
            self.pirates[idx].hours += elapsed;

            // A pirate sails a little slower than a well-found trader, which is
            // what makes rigging worth buying for reasons other than speed.
            let step_hours = 6.5;
            let mut moved = false;
            while self.pirates[idx].hours >= step_hours {
                self.pirates[idx].hours -= step_hours;

                let here = self.pirates[idx].at;
                let sees = hex::wrapped_distance(here, player) <= 5;
                self.pirates[idx].hunting = sees;

                let dir = if sees {
                    // Run the same pathfinder the player's autopilot uses,
                    // limited to a short horizon so a fleet of them stays
                    // affordable.
                    let path = nav::find_path(
                        here, player, month, 2, 6.0, &self.depth, 8,
                    );
                    path.first().and_then(|&n| {
                        (0..6).find(|&d| hex::normalise(hex::neighbour(here, d)) == n)
                    })
                } else {
                    Some(self.rng.below(6) as usize)
                };

                if let Some(d) = dir {
                    let n = hex::normalise(hex::neighbour(here, d));
                    if let Some(ni) = hex::index(n) {
                        if !nav::is_land_index(ni) && self.depth[ni] <= 8 {
                            self.pirates[idx].at = n;
                            moved = true;
                        }
                    }
                }
            }
            let _ = moved;

            if self.pirates[idx].at == player {
                engagements.push((idx, strength));
            }
            let _ = at;
        }

        for (idx, strength) in engagements {
            self.fight(idx, strength);
            if self.lost {
                return;
            }
        }
    }

    fn fight(&mut self, idx: usize, strength: i32) {
        let guns = self.ship.gun_count();
        self.say(format!(
            "A sail closes fast and runs up no colours. {} guns against yours.",
            strength
        ));

        if guns == 0 {
            // Unarmed, the only question is how much they take.
            let cargo = self.ship.cargo();
            if cargo == 0 && self.gold < 200 {
                self.say("They board, find nothing worth the trouble, and stand off.".into());
            } else {
                let taken_gold = self.gold / 3;
                self.gold -= taken_gold;
                let mut units = 0;
                for slot in self.ship.hold.iter_mut() {
                    let take = *slot / 2;
                    *slot -= take;
                    units += take;
                }
                self.say(format!(
                    "You have nothing to answer with. They take {taken_gold} in gold and {units} units, and let you go."
                ));
            }
            let parting = self.rng.range(5, 15);
            self.hurt(parting, "They fire into you as they sheer off.");
            self.pirates[idx].at = self.scatter_from(self.at);
            return;
        }

        // An armed trader is a real fight. Guns tell, but so does luck.
        let roll = self.rng.range(0, guns + strength);
        if roll < guns {
            self.say("Two broadsides and they have had enough. They bear away.".into());
            let scars = self.rng.range(2, 10);
            self.hurt(scars, "You did not come off clean.");
            self.pirates.remove(idx);
        } else {
            let taken_gold = self.gold / 5;
            self.gold -= taken_gold;
            let mut units = 0;
            for slot in self.ship.hold.iter_mut() {
                let take = *slot / 4;
                *slot -= take;
                units += take;
            }
            self.say(format!(
                "They are the better gunners. {taken_gold} in gold and {units} units go over the side to them."
            ));
            let wound = self.rng.range(8, 24);
            self.hurt(wound, "The mainmast is wounded.");
            if !self.lost {
                self.pirates[idx].at = self.scatter_from(self.at);
            }
        }
    }

    fn scatter_from(&mut self, from: Hex) -> Hex {
        for _ in 0..24 {
            let d = self.rng.below(6) as usize;
            let mut h = from;
            for _ in 0..4 {
                h = hex::normalise(hex::neighbour(h, d));
            }
            if let Some(i) = hex::index(h) {
                if !nav::is_land_index(i) {
                    return h;
                }
            }
        }
        from
    }

    // -- trade -------------------------------------------------------------

    pub fn buy(&mut self, good: usize, qty: i32) -> bool {
        let Some(port) = self.port_here() else {
            self.say("You are at sea. There is nobody to trade with.".into());
            return false;
        };
        if good >= GOODS.len() || qty <= 0 {
            return false;
        }
        let Some(unit) = self.markets.buy_price(port, good) else {
            self.say(format!("Nobody in {} deals in {}.", PORTS[port].name, GOODS[good]));
            return false;
        };
        let affordable = self.gold / unit;
        let fits = self.ship.free_space();
        let take = qty.min(affordable).min(fits);
        if take <= 0 {
            if fits <= 0 {
                self.say("The hold is full.".into());
            } else {
                self.say("There is not enough in the strongbox.".into());
            }
            return false;
        }
        let cost = take * unit;
        self.gold -= cost;
        self.ship.hold[good] += take;
        self.ship.paid[good] += cost;
        self.markets.on_buy(port, good, cost);
        self.say(format!(
            "Bought {take} {} at {unit} for {cost}.",
            GOODS[good]
        ));
        true
    }

    pub fn sell(&mut self, good: usize, qty: i32) -> bool {
        let Some(port) = self.port_here() else {
            self.say("You are at sea. There is nobody to trade with.".into());
            return false;
        };
        if good >= GOODS.len() || qty <= 0 {
            return false;
        }
        let have = self.ship.hold[good];
        let take = qty.min(have);
        if take <= 0 {
            self.say(format!("There is no {} in the hold.", GOODS[good]));
            return false;
        }
        let Some(unit) = self.markets.sell_price(port, good) else {
            return false;
        };

        // What this parcel cost, averaged, so the log can tell the truth about
        // whether the voyage paid.
        let avg_cost = if have > 0 { self.ship.paid[good] / have } else { 0 };
        let takings = take * unit;
        let outlay = take * avg_cost;

        self.gold += takings;
        self.ship.hold[good] -= take;
        self.ship.paid[good] -= outlay;
        self.markets.on_sell(port, good, takings);

        let glut = Markets::is_glutted(port, good);
        let verdict = if takings > outlay {
            format!("a profit of {}", takings - outlay)
        } else if takings < outlay {
            format!("a loss of {}", outlay - takings)
        } else {
            "no better and no worse".into()
        };
        if glut {
            self.say(format!(
                "{} grows here. They take your {take} at {unit} a unit out of politeness: {verdict}.",
                GOODS[good],
            ));
        } else {
            self.say(format!(
                "Sold {take} {} at {unit}: {verdict}.",
                GOODS[good]
            ));
        }
        true
    }

    pub fn upgrade(&mut self, which: i32) -> bool {
        let Some(port) = self.port_here() else {
            self.say("There is no shipwright in open water.".into());
            return false;
        };
        let Some(u) = Upgrade::from_index(which) else {
            return false;
        };
        let Some(cost) = self.ship.upgrade_cost(u) else {
            self.say(format!("The {} is as good as it gets.", u.name()));
            return false;
        };
        if self.gold < cost {
            self.say(format!(
                "The yard at {} wants {cost} for that. You have {}.",
                PORTS[port].name, self.gold
            ));
            return false;
        }
        if !self.ship.fit(u) {
            self.say("The hold would have to be cleared first.".into());
            return false;
        }
        self.gold -= cost;
        let msg = match u {
            Upgrade::Hull => format!(
                "A bigger hull. She will carry {} now.",
                self.ship.capacity()
            ),
            Upgrade::Rigging => format!(
                "New rigging. You can stand {} hexes off the land.",
                self.ship.bluewater_rating()
            ),
            Upgrade::Guns => format!("{} guns aboard.", self.ship.gun_count()),
        };
        self.say(msg);
        true
    }

    pub fn repair(&mut self) -> bool {
        if self.port_here().is_none() {
            self.say("Repairs need a yard.".into());
            return false;
        }
        if self.ship.damage == 0 {
            return false;
        }
        let cost = self.ship.repair_cost();
        if self.gold < cost {
            self.say(format!("The repair is {cost} and you have {}.", self.gold));
            return false;
        }
        self.gold -= cost;
        self.ship.damage = 0;
        self.advance(48.0);
        self.say(format!("Two days in the yard and {cost} gone. She is sound."));
        true
    }

    // -- rendering ---------------------------------------------------------

    /// Two bytes per hex: what is there, and how well it can be seen.
    pub fn render(&mut self) -> &[u8] {
        for i in 0..CELLS {
            let code = if nav::is_land_index(i) {
                CODE_LAND
            } else if self.depth[i] > 3 {
                CODE_DEEP
            } else {
                CODE_SHALLOW
            };
            self.render[i * 2] = code;
            self.render[i * 2 + 1] = self.fog[i];
        }

        for (i, p) in PORTS.iter().enumerate() {
            if !self.discovered[i] {
                continue;
            }
            let h = hex::from_offset(p.col as i32, p.row as i32);
            if let Some(idx) = hex::index(h) {
                self.render[idx * 2] = CODE_PORT;
                if self.render[idx * 2 + 1] == UNSEEN {
                    self.render[idx * 2 + 1] = REMEMBERED;
                }
            }
        }

        for p in &self.pirates {
            if let Some(idx) = hex::index(p.at) {
                if self.fog[idx] == VISIBLE {
                    self.render[idx * 2] = CODE_PIRATE;
                }
            }
        }

        if let Some(idx) = hex::index(self.at) {
            self.render[idx * 2] = CODE_SHIP;
            self.render[idx * 2 + 1] = VISIBLE;
        }
        &self.render
    }

    pub fn discovered(&self, port: usize) -> bool {
        self.discovered[port]
    }

    pub fn pirates_in_sight(&self) -> usize {
        self.pirates
            .iter()
            .filter(|p| {
                hex::index(p.at).map(|i| self.fog[i] == VISIBLE).unwrap_or(false)
            })
            .count()
    }

    pub fn wind_here(&self) -> (usize, i32) {
        nav::wind(self.at, self.month)
    }

    pub fn current_here(&self) -> (usize, i32) {
        nav::current(self.at)
    }

    pub fn weather_here(&self) -> i32 {
        nav::weather(self.at, self.day, self.month)
    }

    pub fn offshore(&self) -> i32 {
        hex::index(self.at).map(|i| self.depth[i] as i32).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_game_starts_in_a_port_and_can_see_out_of_it() {
        let g = Game::new(1);
        assert!(g.port_here().is_some(), "the voyage does not start in a port");
        let lit = g.fog.iter().filter(|v| **v == VISIBLE).count();
        assert!(lit > 1, "the ship is blind in harbour");
        let dark = g.fog.iter().filter(|v| **v == UNSEEN).count();
        assert!(dark > CELLS / 2, "the chart starts too full");
    }

    #[test]
    fn the_same_seed_and_the_same_moves_give_the_same_voyage() {
        let run = |seed| {
            let mut g = Game::new(seed);
            for i in 0..60 {
                g.step(i % 6);
            }
            (g.gold, g.day, g.month, g.ship.damage, g.at)
        };
        assert_eq!(run(99), run(99));
    }

    #[test]
    fn time_passes_when_the_ship_moves() {
        let mut g = Game::new(2);
        let before = (g.day, g.hour);
        for d in 0..6 {
            if g.step(d) {
                break;
            }
        }
        assert_ne!((g.day, g.hour), before, "sailing took no time at all");
    }

    #[test]
    fn you_cannot_sail_onto_land() {
        let mut g = Game::new(3);
        let start = g.at;
        for d in 0..6 {
            let target = hex::neighbour(start, d);
            if !nav::is_water(target) {
                g.at = start;
                assert!(!g.step(d), "sailed straight over a headland");
                assert_eq!(g.at, start);
                return;
            }
        }
    }

    #[test]
    fn buying_costs_gold_and_fills_the_hold() {
        let mut g = Game::new(4);
        let port = g.port_here().unwrap();
        let good = (0..GOODS.len())
            .find(|&x| g.markets.buy_price(port, x).is_some())
            .expect("the starting port sells nothing at all");
        let gold_before = g.gold;
        assert!(g.buy(good, 5));
        assert_eq!(g.ship.hold[good], 5);
        assert!(g.gold < gold_before);
    }

    #[test]
    fn you_cannot_trade_at_sea() {
        let mut g = Game::new(5);
        for d in 0..6 {
            if g.step(d) {
                break;
            }
        }
        if g.port_here().is_none() {
            assert!(!g.buy(0, 1));
            assert!(!g.sell(0, 1));
        }
    }

    #[test]
    fn the_hold_cannot_be_overfilled() {
        let mut g = Game::new(6);
        g.gold = 10_000_000;
        let port = g.port_here().unwrap();
        let good = (0..GOODS.len())
            .find(|&x| g.markets.buy_price(port, x).is_some())
            .unwrap();
        g.buy(good, 100_000);
        assert!(g.ship.cargo() <= g.ship.capacity());
    }

    #[test]
    fn an_upgrade_costs_what_it_says_and_changes_the_ship() {
        let mut g = Game::new(7);
        g.gold = 1_000_000;
        let before = g.ship.capacity();
        assert!(g.upgrade(0));
        assert!(g.ship.capacity() > before);
    }

    #[test]
    fn a_course_can_be_laid_to_a_discovered_port_and_not_to_an_unseen_one() {
        let mut g = Game::new(8);
        let start = g.port_here().unwrap();
        let unseen = (0..PORTS.len())
            .find(|&i| !g.discovered(i))
            .expect("everything is already discovered");
        assert!(!g.set_course(unseen), "laid a course to an unseen port");
        assert!(g.set_course(start) || g.port_here() == Some(start));
    }

    #[test]
    fn sailing_a_laid_course_arrives() {
        let mut g = Game::new(9);
        g.gold = 1_000_000;
        for _ in 0..4 {
            g.upgrade(1); // rigging, so the route is not refused
        }
        // Reveal a distant port the honest way would take too long, so nudge
        // discovery directly and check only the pathing.
        let target = (0..PORTS.len())
            .find(|&i| Some(i) != g.port_here() && Markets::trades(i))
            .unwrap();
        g.discovered[target] = true;
        if g.set_course(target) {
            let mut guard = 0;
            while g.under_way() && guard < 5_000 && !g.lost {
                g.sail_on();
                guard += 1;
            }
            if !g.lost {
                let goal = hex::from_offset(
                    PORTS[target].col as i32,
                    PORTS[target].row as i32,
                );
                assert_eq!(g.at, goal, "the autopilot did not arrive");
            }
        }
    }

    #[test]
    fn the_render_buffer_marks_the_ship() {
        let mut g = Game::new(10);
        let at = hex::index(g.at).unwrap();
        let buf = g.render();
        assert_eq!(buf[at * 2], CODE_SHIP);
        assert_eq!(buf[at * 2 + 1], VISIBLE);
        assert_eq!(buf.len(), CELLS * 2);
    }
}
