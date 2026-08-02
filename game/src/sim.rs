//! The game.
//!
//! One struct holds everything, because there is one ship and one player and
//! pretending otherwise would buy nothing. The interesting parts are the turn
//! loop, which advances time in hours rather than in moves, and the pirates,
//! which run the same pathfinder the player's autopilot does and therefore
//! cost the same to think.

use crate::commission::{self, Commission};
use crate::hex::{self, Hex};
use crate::market::Markets;
use crate::nav::{self, CELLS, REMEMBERED, UNSEEN, VISIBLE};
use crate::reputation;
use crate::rng::Rng;
use crate::ship::{self, guns_at, Class, Ship, Upgrade, CLASSES};
use crate::world::{GOODS, PORTS};

pub const CODE_SHALLOW: u8 = 0;
pub const CODE_DEEP: u8 = 1;
pub const CODE_LAND: u8 = 2;
pub const CODE_PORT: u8 = 3;
pub const CODE_SHIP: u8 = 4;
pub const CODE_PIRATE: u8 = 5;
pub const CODE_MERCHANT: u8 = 6;
pub const CODE_NAVY: u8 = 7;
// A hex holds one glyph, so before these existed a crowded hex looked exactly
// like a quiet one and the "in sight" count in the status column disagreed with
// what a reader could count on the chart. Traders cluster at ports, which is
// where every voyage starts, so the first thing a new player saw was a number
// that did not add up.
//
// The stack is not counted out, only marked. Three raiders and four are the
// same decision; one and more-than-one is not.
pub const CODE_PIRATES: u8 = 8;
pub const CODE_MERCHANTS: u8 = 9;
pub const CODE_NAVIES: u8 = 10;

/// Bytes per hex in the render buffer: terrain, then what is on it, then fog.
///
/// Terrain is sent separately from the mark rather than being the first three
/// values of one code, because the page fills the hex by terrain and draws the
/// mark on top of it. Packed into a single byte the two channels are exclusive:
/// a raider reported as a raider says nothing about the water he is sitting in,
/// and whether that water is shallow or deep is the whole question when you are
/// deciding to run for it. The terrain byte is written once per frame and the
/// overlays never touch it, so it always describes the sea floor and not the
/// traffic.
pub const STRIDE: usize = 3;
/// Offsets within one hex's slice.
pub const AT_TERRAIN: usize = 0;
pub const AT_MARK: usize = 1;
pub const AT_FOG: usize = 2;

const PIRATE_COUNT: usize = 24;
/// How close a pirate has to be before it stops wandering and starts hunting.
///
/// This is the base. What a given raider actually uses is
/// [`Game::hunt_range`], which reads your reputation and whether this
/// particular one has fought you before.
const HUNT_RANGE: i32 = 5;
/// Sailing steps a raider keeps standing toward where it last saw you.
///
/// Without this a pirate has no memory at all: `hunting` is recomputed from the
/// current distance on every step, so slipping one hex past the horizon makes
/// you cease to exist. Six steps at six and a half hours is a day and a half of
/// pursuit, which is long enough that outrunning one means outrunning it rather
/// than merely turning a corner.
const PIRATE_MEMORY: i32 = 6;
/// How many hexes off a hunted pirate keeps clear of the one who beat her.
const BEATEN_SHYNESS: i32 = 2;

const MERCHANT_COUNT: usize = 24;

/// Names for the raiders, so that the one who took your gold off Cape Verde is
/// recognisably the same sail when she comes back.
///
/// A named antagonist is the cheapest thing in this file and does more for the
/// feeling of being remembered than any of the arithmetic above it. There are
/// more names here than there are pirates, so a game never has to reuse one.
const PIRATE_NAMES: [&str; 32] = [
    "the Black Brig", "Salt Kate", "the Gull", "Ruy the Portuguese",
    "the Widow's Share", "Anselm Vloot", "the Cutwater", "Marta of Ceuta",
    "the Long Nine", "Iron Bartol", "the Sparrowhawk", "Old Dyer",
    "the Sea Hearse", "Juana la Roja", "the Grey Wolf", "Cass Ferrer",
    "the Broken Compass", "Hendrik Stoop", "the Carrion", "Nial Bright",
    "the Red Ensign", "Pieter Vos", "the Lame Dog", "Sancha Milan",
    "the Nightjar", "Otto Kranz", "the Splinter", "Lucia Bravo",
    "the Dry Well", "Yusuf Reis", "the Hollow Man", "Tam Skene",
];

/// Cargoes a merchant might be carrying, expressed as a range of units rather
/// than a table, because what she has is worth less than the fact that she has
/// it and will not give it up.
const NAVY_NAMES: [&str; 8] = [
    "the Constancy", "the Saint Elmo", "the Vigilant", "the Alcántara",
    "the Providence", "the Halberd", "the San Cristóbal", "the Diligence",
];

/// How far off a king's ship picks the player up.
///
/// Wider than a pirate's, and it ignores the shelf entirely, which together are
/// the real punishment for raiding merchants. A pirate works the offing and
/// leaves the coast alone, so an honest master always has somewhere safe to
/// trade. Bringing the navy out takes that away: they patrol the harbour
/// approaches precisely because that is where you were going to hide.
const NAVY_HUNT_RANGE: i32 = 8;
/// Guns a king's ship carries. Heavier than any raider afloat.
/// Who sails what.
///
/// Three fleets, three shapes, and the shape is the point: you can read the
/// threat off the hull before anyone fires. Weights are relative, not percents.
///
/// **Raiders** run small to medium. A pirate wants a ship that overhauls a
/// trader and outruns a king's ship, which is a Redonda or a Carrack, not a
/// first-rate. A galleon under the black flag exists and is meant to be the
/// worst thing you meet all game, at two chances in a hundred.
///
/// **Traders** carry the least armament that will discourage an opportunist,
/// because every gun is a ton of cargo they are not paid for. Most of them have
/// none at all.
///
/// **The crown** builds nothing small. Its business is standing up to raiders
/// and to you, so it starts where the pirates leave off.
const PIRATE_FLEET: [(Class, u32); 6] = [
    (Class::Balsa, 10),
    (Class::Latina, 20),
    (Class::Redonda, 28),
    (Class::Carrack, 30),
    (Class::Galleon, 10),
    (Class::HeavyGalleon, 2),
];

const MERCHANT_FLEET: [(Class, u32); 4] = [
    (Class::Balsa, 15),
    (Class::Latina, 30),
    (Class::Redonda, 25),
    (Class::Carrack, 30),
];

const NAVY_FLEET: [(Class, u32); 3] = [
    (Class::Carrack, 30),
    (Class::Galleon, 50),
    (Class::HeavyGalleon, 20),
];

/// Draw a hull from a fleet table.
fn pick_class(rng: &mut Rng, fleet: &[(Class, u32)]) -> Class {
    let total: u32 = fleet.iter().map(|(_, w)| w).sum();
    let mut roll = rng.below(total);
    for &(class, w) in fleet {
        if roll < w {
            return class;
        }
        roll -= w;
    }
    fleet[fleet.len() - 1].0
}

/// How many guns a hunter of this class runs out.
///
/// Fully gunned, because a raider or a king's ship has no cargo to protect and
/// no reason to leave a port empty. The spread is the crew: the same hull is
/// worth more or less depending on who is aboard.
fn hunter_guns(rng: &mut Rng, class: Class) -> i32 {
    let full = guns_at(class.spec().gun_max);
    let spread = (full / 4).max(1);
    (full - spread + rng.range(0, 2 * spread + 1)).max(2)
}
/// How far off they come over the horizon when they are sent for.
const NAVY_ARRIVES_AT: i32 = 12;

const MERCHANT_UNITS: (i32, i32) = (10, 60);
/// A merchant sails slower than a raider, which is what makes catching one
/// possible at all without giving the player a new kind of chase to learn.
const MERCHANT_STEP_HOURS: f32 = 8.0;
/// Consecutive blocked steps before a merchant gives up and picks a new port.
/// Merchants steer greedily rather than by pathfinder (see `move_merchants`),
/// so they do get caught in bays, and this is how they get out again.
const MERCHANT_STUCK_LIMIT: i32 = 6;
/// Hexes from land still counted as the coastal shelf, where raiders will not
/// give chase. Beyond it is the offing, and the offing is theirs.
const SHELF: i32 = 2;
/// How near a port has to be before it goes on the chart.
///
/// Deliberately shorter than the fog-of-war horizon, and they are not the same
/// question. A hex is five degrees, so a lookout who can make out water four
/// hexes off can see roughly from Lisbon to Naples, and letting that count as
/// charting meant eight harbours went on the chart before the ship had left
/// the quay, one line after the log said the chart was blank. Seeing that
/// there is sea over there is not the same as knowing whose harbour it is.
const CHART_RANGE: i32 = 2;
const HOURS_PER_DAY: f32 = 24.0;
const DAYS_PER_MONTH: i32 = 30;
const START_GOLD: i32 = 3_000;
const CHRONICLE_KEEP: usize = 64;

/// Give a merchant a fresh cargo and somewhere else to take it.
///
/// A free function taking the generator rather than a method on `Game`, because
/// the caller is already holding `&mut self.merchants[idx]` and a method would
/// want `&mut self` on top of it. Threading the one field it actually needs is
/// shorter than the dance required to borrow around that.
fn relade(rng: &mut Rng, m: &mut Merchant, ports: &[usize]) {
    m.cargo = rng.below(GOODS.len() as u32) as usize;
    // Bounded by the hull, so a Balsa is never found carrying a galleon's
    // freight. This is most of what a trader's class does: it decides whether
    // she is worth firing on.
    m.units = rng
        .range(MERCHANT_UNITS.0, MERCHANT_UNITS.1)
        .min(Ship::capacity_of(m.class));
    m.gold = rng.range(200, 2_400);
    m.stuck = 0;
    for _ in 0..8 {
        let next = ports[rng.below(ports.len() as u32) as usize];
        if next != m.bound {
            m.bound = next;
            return;
        }
    }
}

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
    /// The hull she is in, which is what the log names when you meet her and
    /// what her guns were drawn from.
    class: Class,
    strength: i32,
    /// Hours of sailing banked; pirates move on the same clock the player does
    /// rather than once per player move, so a fast ship really does outrun
    /// them.
    hours: f32,
    hunting: bool,
    name: &'static str,
    /// Where the player was last seen, and how many more sailing steps this
    /// raider will keep standing toward it. Together these are the whole of
    /// pirate memory: sight sets them, losing sight spends them down, and
    /// arriving at an empty hex gives up the chase.
    last_seen: Option<Hex>,
    memory: i32,
    /// Has fought the player and been driven off. Such a one keeps her distance.
    beaten: bool,
    /// Has fought the player at all, which is what lets the log name her.
    met: bool,
    /// A king's ship rather than a raider.
    ///
    /// The same struct carries both, and that is a considered choice rather than
    /// a shortcut. Everything that makes a hunter a hunter is shared: the sight
    /// check, the memory, the pathfinding, the engagement queue and the
    /// index-shifting bug that queue used to have. Forking a second nearly
    /// identical type would double all of it and halve the chance that a fix to
    /// one reaches the other. What actually differs is four decisions, and each
    /// of them reads this one flag where it is made.
    navy: bool,
}

/// An honest trader going about her business.
///
/// Merchants exist for one reason: reputation has to be able to fall, and it
/// cannot fall if there is nobody to wrong. They carry cargo and gold, they do
/// not fight back beyond a token, and they never hunt anybody. Sinking your
/// standing is meant to be easy and cheap, in the way that ruining a reputation
/// generally is.
struct Merchant {
    at: Hex,
    /// Her hull. A trader's class decides how much she carries and how little
    /// she can do about you, and nothing else.
    class: Class,
    /// The port she is making for.
    bound: usize,
    cargo: usize,
    units: i32,
    gold: i32,
    hours: f32,
    stuck: i32,
}

/// A sum in gold, grouped in threes, the way the page prints it.
pub(crate) fn coin(n: i32) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if n < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// What a yard is asking for one class, and why you may not have her.
///
/// Every class the port could ever build is listed, priced and reasoned about,
/// including the ones you cannot buy today. This is the same shown-but-locked
/// bargain the goods table makes: a price you cannot meet is a goal, and a
/// class that is simply absent is indistinguishable from one that never existed.
pub struct Offer {
    pub class: Class,
    /// Gold to hand over after the allowance for your present ship.
    pub price: i32,
    /// The allowance itself, so the yard can show its working.
    pub trade_in: i32,
    /// Why she cannot be bought, or `None` if she can.
    pub locked: Option<String>,
}

/// Which harbours are shut to a hull that draws too much water.
///
/// A harbour counts as shallow when it has `nav::SHALLOW_ROOM` or less water
/// within two hexes: a creek, a river mouth, the head of a gulf. Capitals are
/// exempt whatever the coastline says, and that is not a fudge. A capital is
/// where the yards are, so a capital that barred the hull it had just sold you
/// would be a trap rather than a constraint, and the exemption is also the
/// honest reading: a port big enough to build a galleon has water enough to
/// float one.
fn shallow_harbours() -> Vec<bool> {
    let mut out = vec![false; CELLS];
    for p in PORTS.iter() {
        if p.capital {
            continue;
        }
        let h = hex::from_offset(p.col as i32, p.row as i32);
        if nav::harbour_room(h) <= nav::SHALLOW_ROOM {
            if let Some(i) = hex::index(h) {
                out[i] = true;
            }
        }
    }
    out
}

/// Hexes between two harbours as the gull flies. Not a sailing distance: there
/// is land in the way of most pairs and a course round it is longer. Good
/// enough for pricing an errand, which is all it is used for.
fn port_distance(a: usize, b: usize) -> i32 {
    let ha = hex::from_offset(PORTS[a].col as i32, PORTS[a].row as i32);
    let hb = hex::from_offset(PORTS[b].col as i32, PORTS[b].row as i32);
    hex::distance(ha, hb)
}

pub struct Game {
    pub ship: Ship,
    pub gold: i32,
    pub at: Hex,
    pub hour: f32,
    pub day: i32,
    pub month: i32,
    pub year: i32,
    /// What the sea thinks of you. See the `reputation` module.
    pub reputation: i32,

    pub fog: Vec<u8>,
    depth: Vec<u8>,
    /// Harbours with too little sea room for a deep-draught hull, indexed like
    /// `depth`. Fixed at construction: it is a property of the coastline and
    /// nothing in play moves a headland.
    shallow_harbour: Vec<bool>,
    /// All false. Handed to the pathfinder for a hull that draws little enough
    /// to enter anywhere, which is most of them. Kept as a field rather than
    /// built on demand because `find_path` takes a slice and the alternative is
    /// allocating a bool per water cell on every leg a pirate sails.
    open_water: Vec<bool>,
    pub markets: Markets,
    pirates: Vec<Pirate>,
    merchants: Vec<Merchant>,
    discovered: Vec<bool>,
    /// Ports the player has actually come to anchor at, which is not what
    /// `discovered` records: that one is set by the lookout at range. A
    /// commission is drawn against this, because "somewhere you have never
    /// been" has to mean ashore.
    visited: Vec<bool>,
    /// Goods the player has ever bought, anywhere. The other half of what a
    /// commission steers by. Set in `buy` and never cleared.
    traded: Vec<bool>,
    /// The errand on the table at the port you are standing in. Drawn on
    /// arrival, thrown away the moment you weigh anchor: an offer nobody is
    /// there to make is not an offer.
    offer: Option<Commission>,
    /// The errand accepted. One at a time, on purpose. See `commission`.
    commission: Option<Commission>,
    chronicle: Vec<String>,
    rng: Rng,

    render: Vec<u8>,
    scratch: Vec<Hex>,
    ray: Vec<Hex>,
    pathfinder: nav::Scratch,

    /// Autopilot: hexes still to sail, in reverse order so the next is last.
    course: Vec<Hex>,
    /// An `enforce()` owed once it is safe to resize `pirates`. See `arrest`.
    pending_enforce: bool,
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
            reputation: 0,
            fog: vec![UNSEEN; CELLS],
            depth,
            shallow_harbour: shallow_harbours(),
            open_water: vec![false; CELLS],
            markets: Markets::new(),
            pirates: Vec::new(),
            merchants: Vec::new(),
            discovered: vec![false; PORTS.len()],
            visited: vec![false; PORTS.len()],
            traded: vec![false; GOODS.len()],
            offer: None,
            commission: None,
            chronicle: Vec::new(),
            rng: Rng::new(seed),
            render: vec![0; CELLS * STRIDE],
            scratch: Vec::new(),
            ray: Vec::new(),
            pathfinder: nav::Scratch::new(),
            course: Vec::new(),
            pending_enforce: false,
            lost: false,
        };

        // Eight days of both, at the three hands a Balsa starts with. Enough
        // that the first voyage is about trade rather than about the purser,
        // and little enough that the second one is not. It is also sixteen of
        // her forty units of hold, which is the trade-off stated in the plainest
        // way the game has: this is what provisions cost you.
        g.ship.food = 8;
        g.ship.water = 8;

        g.discovered[start] = true;
        g.visited[start] = true;
        g.spawn_pirates();
        g.spawn_merchants();
        g.look();
        g.say(format!(
            "You take command at {}. Three thousand in gold, a coastal hull, no guns, and {} hands.",
            PORTS[start].name,
            g.ship.crew
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
            // Inside the loop, not after it. A single order can be two days
            // long: `repair` is forty-eight hours and a fair wind can carry a
            // course several days in one call. Feeding the crew once per call
            // rather than once per day would make the long orders free.
            self.ration();
            if self.day > DAYS_PER_MONTH {
                self.day = 1;
                self.month += 1;
                if self.month > 12 {
                    self.month = 1;
                    self.year += 1;
                }
                self.markets.drift();
                self.say("A new month. Prices ease back toward their old levels.".into());
                self.pay_wages();
                let was = reputation::standing(self.reputation);
                self.reputation = reputation::fade(self.reputation);
                let now = reputation::standing(self.reputation);
                if now != was {
                    self.say(format!("Talk of you has died down. You are {now}."));
                }
                // The fade is the only thing that ever sends the navy home, so
                // the recall has to be checked on the same tick that moves the
                // number, not only when an offence pushes it the other way.
                self.enforce();
            }
        }
    }

    // -- crew, stores and wages --------------------------------------------

    /// One day's food and water, and the consequence of not having it.
    ///
    /// Short of either, one hand dies; short of both, two. That is the "one by
    /// one" the rule is meant to be, and it is slow enough that a player who
    /// reads the chronicle has several days to make for land. The warnings
    /// below exist so that nobody meets this mechanic for the first time by
    /// losing to it.
    fn ration(&mut self) {
        if self.ship.crew <= 0 || self.lost {
            return;
        }
        let (need_food, need_water) = (self.ship.food_per_day(), self.ship.water_per_day());
        let mut short = 0;
        if self.ship.food >= need_food {
            self.ship.food -= need_food;
        } else {
            self.ship.food = 0;
            short += 1;
        }
        if self.ship.water >= need_water {
            self.ship.water -= need_water;
        } else {
            self.ship.water = 0;
            short += 1;
        }

        if short == 0 {
            // Warn on the way down rather than at the bottom. Both figures are
            // exact rather than approximate because the player can act on
            // them: three days is a course change, one day is an emergency.
            match self.ship.days_of_stores() {
                3 => self.say("Three days of stores left. The purser says so twice.".into()),
                1 => self.say("One day of stores left. Make land or go hungry.".into()),
                _ => {}
            }
            return;
        }

        let what = match (self.ship.food, self.ship.water) {
            (0, 0) => "There is neither food nor water aboard",
            (0, _) => "The food is gone",
            _ => "The water is gone",
        };
        let died = short.min(self.ship.crew);
        self.ship.crew -= died;
        let who = if died == 1 { "a hand" } else { "two hands" };
        self.say(format!(
            "{what}. You lose {who}. {} left aboard.",
            self.ship.crew
        ));
        if self.ship.crew == 0 {
            self.lost = true;
            self.say("There is nobody left to steer. She drifts away under bare poles.".into());
        }
    }

    /// Wages, at the turn of the month.
    ///
    /// Hands you cannot pay do not stay. They are floored at one, so being
    /// broke costs you almost the whole crew and never the ship: an empty ship
    /// is a loss, and losing on a bookkeeping tick you could not see coming is
    /// not a fair way to lose. One hand is enough to sail badly and reach a
    /// port, which is all the guarantee needs to be.
    fn pay_wages(&mut self) {
        let owed = self.ship.wages();
        if owed <= 0 || self.lost {
            return;
        }
        if self.gold >= owed {
            self.gold -= owed;
            self.say(format!(
                "Wages for {} hands: {}.",
                self.ship.crew,
                coin(owed)
            ));
            return;
        }
        let short = owed - self.gold;
        self.gold = 0;
        let unpaid = ((short + ship::MONTHLY_WAGE - 1) / ship::MONTHLY_WAGE)
            .min(self.ship.crew - 1)
            .max(0);
        self.ship.crew -= unpaid;
        self.say(format!(
            "You cannot make the wage bill. {unpaid} hands go looking for a better berth. {} stay.",
            self.ship.crew
        ));
    }

    /// Hands lost to shot and splinters. Never takes the last one: a fight is
    /// survivable by definition, and the ship is already lost at 100% damage
    /// if it was not.
    fn kill_crew(&mut self, n: i32) {
        let died = n.min((self.ship.crew - 1).max(0));
        if died <= 0 {
            return;
        }
        self.ship.crew -= died;
        let who = if died == 1 { "One hand is" } else { "Hands are" };
        self.say(format!(
            "{who} killed in the exchange: {died} lost, {} still aboard.",
            self.ship.crew
        ));
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
                // Both conditions, not either. The fog test is what stops a
                // port being charted through a headland; the range test is what
                // stops the whole western Mediterranean being charted at once.
                if self.fog[idx] == VISIBLE
                    && hex::wrapped_distance(self.at, h) <= CHART_RANGE
                {
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
        self.port_at(self.at)
    }

    /// The port on a given hex, if there is one. `port_here` in the general
    /// case, which the draught rule needs because it asks about a harbour the
    /// ship is not in and is refusing to enter.
    pub fn port_at(&self, h: Hex) -> Option<usize> {
        PORTS
            .iter()
            .position(|p| hex::from_offset(p.col as i32, p.row as i32) == h)
    }

    /// Would the hull she is in now be turned away from this harbour?
    ///
    /// Asked of a port rather than a hex because that is what the page has to
    /// print: a destination the player is about to choose. The answer moves
    /// when the ship does, which is why it is computed rather than stored.
    pub fn barred_port(&self, port: usize) -> bool {
        if !self.ship.class.is_deep_draught() {
            return false;
        }
        let h = hex::from_offset(PORTS[port].col as i32, PORTS[port].row as i32);
        hex::index(h).is_some_and(|i| self.shallow_harbour[i])
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
        // The bar, checked on the hex about to be entered rather than on
        // arrival, so a deep hull is turned back at the harbour mouth instead
        // of being told she has run aground after the fact.
        if self.ship.class.is_deep_draught() && self.shallow_harbour[ti] {
            self.say(format!(
                "{} is too shallow for her draught. You stand off.",
                self.port_at(target).map_or("That harbour", |p| PORTS[p].name)
            ));
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
        self.move_merchants(hours);
        self.move_pirates(hours);

        // Any errand on the table belonged to the port just left. Cleared on
        // every leg rather than only on departure, so there is no way to carry
        // an offer out to sea and accept it there.
        self.offer = None;

        if let Some(p) = self.port_here() {
            if !self.discovered[p] {
                self.discovered[p] = true;
            }
            self.visited[p] = true;
            // Note the arrival but do not cancel a laid course. Passing through
            // a harbour on the way somewhere else should not end the voyage,
            // and the player asked for the destination, not the first landfall.
            if self.course.is_empty() {
                self.say(format!("You come to anchor at {}.", PORTS[p].name));
                // Only to a ship that has actually stopped. A hull standing in
                // past a harbour under a laid course has nobody ashore.
                self.offer_commission(p);
            } else {
                self.say(format!("You stand in past {}.", PORTS[p].name));
            }
        }
        true
    }

    // -- commissions -------------------------------------------------------

    /// Roll for an errand at the port just anchored in. See `commission`.
    fn offer_commission(&mut self, port: usize) {
        if self.commission.is_some() || !self.rng.chance(commission::OFFER_CHANCE_PERCENT) {
            return;
        }
        // Borrowed out of `self` before the draw, because `Ledger` holds
        // references into the same struct the generator needs `&mut` of.
        let deep = self.ship.class.is_deep_draught();
        let shallow: Vec<bool> = (0..PORTS.len())
            .map(|p| {
                let h = hex::from_offset(PORTS[p].col as i32, PORTS[p].row as i32);
                deep && hex::index(h).is_some_and(|i| self.shallow_harbour[i])
            })
            .collect();
        let ledger = commission::Ledger {
            visited: &self.visited,
            discovered: &self.discovered,
            traded: &self.traded,
            barred: &|p: usize| shallow[p],
            distance: &port_distance,
        };
        let Some(offer) = commission::draw(&mut self.rng, &self.markets, port, &ledger) else {
            return;
        };
        self.say(format!("A factor comes aboard. {}", offer.wording()));
        self.offer = Some(offer);
    }

    pub fn offer(&self) -> Option<&Commission> {
        self.offer.as_ref()
    }

    pub fn commission(&self) -> Option<&Commission> {
        self.commission.as_ref()
    }

    /// Take the errand on the table. Nothing is paid and nothing is loaded: the
    /// only thing that changes is that the game now remembers what you agreed
    /// to and where you agreed to do it.
    pub fn accept_commission(&mut self) -> bool {
        let Some(c) = self.offer.take() else {
            self.say("There is no errand on the table.".into());
            return false;
        };
        if self.commission.is_some() {
            self.say("You are already under commission.".into());
            self.offer = Some(c);
            return false;
        }
        self.say(match c.kind {
            commission::Kind::Deliver => format!(
                "You take the commission. {} {} for {}, and {} on the quay there.",
                c.qty, GOODS[c.good], PORTS[c.other].name, coin(c.gold)
            ),
            commission::Kind::Collect => format!(
                "You take the commission. {} {} out of {}, and {} on your return.",
                c.qty, GOODS[c.good], PORTS[c.other].name, coin(c.gold)
            ),
        });
        self.commission = Some(c);
        true
    }

    /// Hand over the parcel and be paid. Refuses rather than part-pays: the
    /// factor asked for a number and half of it is not the errand.
    pub fn settle_commission(&mut self) -> bool {
        let Some(c) = self.commission.clone() else {
            self.say("You are under no commission.".into());
            return false;
        };
        let Some(here) = self.port_here() else {
            self.say("You are at sea. There is nobody to pay you.".into());
            return false;
        };
        if here != c.paid_at() {
            self.say(format!("That parcel is wanted in {}.", PORTS[c.paid_at()].name));
            return false;
        }
        if self.ship.hold[c.good] < c.qty {
            self.say(format!(
                "The hold is {} {} short of the commission.",
                c.qty - self.ship.hold[c.good],
                GOODS[c.good]
            ));
            return false;
        }

        // The parcel leaves the hold at what it cost, so the ledger of outlay
        // stays honest for whatever else of that good is still aboard: the
        // errand pays a fee, it is not a sale at a wonderful price.
        let have = self.ship.hold[c.good];
        let avg_cost = if have > 0 { self.ship.paid[c.good] / have } else { 0 };
        self.ship.hold[c.good] -= c.qty;
        self.ship.paid[c.good] -= avg_cost * c.qty;

        self.gold += c.gold;
        self.markets.oblige(here, c.favour);
        self.commission = None;
        self.say(format!(
            "The commission is discharged at {}. {} in gold, and they will remember it.",
            PORTS[here].name, coin(c.gold)
        ));
        true
    }

    /// Give up the errand. No penalty: it was optional when it was offered and
    /// nothing about accepting it made it less so.
    pub fn abandon_commission(&mut self) -> bool {
        if self.commission.take().is_none() {
            self.say("You are under no commission.".into());
            return false;
        }
        self.say("You send word that the commission is off.".into());
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

    /// Move the reputation and say so only when it means something new.
    ///
    /// Announcing every point would bury the chronicle in arithmetic nobody
    /// asked for. Announcing only the crossings makes the number legible: you
    /// hear about your standing on the day it changes, and the exact figure is
    /// in the status column for anyone who wants it.
    fn credit(&mut self, points: i32) {
        if points == 0 {
            return;
        }
        let was = reputation::standing(self.reputation);
        self.reputation = reputation::clamp(self.reputation + points);
        let now = reputation::standing(self.reputation);
        if now != was {
            self.say(format!("Word travels. You are {now}."));
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
        self.lay_course(goal, PORTS[port].name)
    }

    /// Lay a course to any charted hex, which is what clicking the map does.
    ///
    /// A course to open water is worth having and not only a convenience: the
    /// whole point of better rigging is the offing it opens up, and a reader
    /// who can only ever aim at a harbour never sees that. The one thing this
    /// will not do is aim at somewhere never seen, for the same reason
    /// `set_course` will not: a master cannot plot a route through water he has
    /// no chart of.
    pub fn set_course_hex(&mut self, col: i32, row: i32) -> bool {
        if self.lost {
            return false;
        }
        let goal = hex::from_offset(col, row);
        let Some(idx) = hex::index(goal) else {
            return false;
        };
        if self.fog[idx] == UNSEEN {
            self.say("You cannot lay a course through water you have never seen.".into());
            return false;
        }
        if nav::is_land_index(idx) {
            self.say("That is dry land.".into());
            return false;
        }
        if goal == self.at {
            self.say("You are already there.".into());
            return false;
        }
        // Named if the hex happens to be a harbour, because a course laid by
        // clicking the chart can land on one and every refusal below prints this
        // string. "The open sea is too shallow for her draught" is a sentence no
        // reader can act on.
        let what = self.port_at(goal).map_or("the open sea", |p| PORTS[p].name);
        self.lay_course(goal, what)
    }

    fn lay_course(&mut self, goal: hex::Hex, what: &str) -> bool {
        // Hoisted, because the pathfinder's scratch is borrowed mutably for the
        // length of the call and `self.ship.base_hours()` is a borrow of self.
        let (from, month) = (self.at, self.month);
        let (rig, hours, rating) = (
            self.ship.rigging,
            self.ship.base_hours(),
            self.ship.bluewater_rating(),
        );
        // Hoisted for the same reason: a field borrow sits alongside the
        // mutable one, where a `self.barred()` method would borrow all of self.
        let barred: &[bool] = if self.ship.class.is_deep_draught() {
            &self.shallow_harbour
        } else {
            &self.open_water
        };
        // Said here rather than left to the empty path below, because "no route
        // the rigging will stand" is the wrong answer to a draught problem and
        // would send a reader off to buy sails they do not need.
        if hex::index(goal).is_some_and(|i| barred[i]) {
            self.say(format!("{what} is too shallow for her draught. She would sit on the mud."));
            return false;
        }
        let path = nav::find_path(
            &mut self.pathfinder,
            from,
            goal,
            month,
            rig,
            hours,
            &self.depth,
            rating,
            barred,
        );
        if path.is_empty() {
            self.say(format!(
                "There is no route to {what} that the rigging will stand. Better rigging, or a nearer mark."
            ));
            return false;
        }
        self.say(format!("Course laid for {what}: {} hexes.", path.len()));
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
        self.move_merchants(hours);
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
            let class = pick_class(&mut self.rng, &PIRATE_FLEET);
            let strength = hunter_guns(&mut self.rng, class);
            self.pirates.push(Pirate {
                at: h,
                class,
                strength,
                hours: 0.0,
                hunting: false,
                name: PIRATE_NAMES[placed % PIRATE_NAMES.len()],
                last_seen: None,
                memory: 0,
                beaten: false,
                met: false,
                navy: false,
            });
            placed += 1;
        }
    }

    /// How far off this particular raider will pick you up.
    ///
    /// Three things move it, and between them they are what makes reputation a
    /// mechanic rather than a line in the status column. A known pirate-killer
    /// is given a wider berth; a man who preys on merchants is a rich mark and
    /// gets chased from further out; and one who has already lost to you keeps
    /// clear on her own account.
    /// A king's ship reads none of it. She is not weighing a prize against the
    /// risk, she has orders, and she has already lost to you once if she is
    /// still out here. Hence a flat range and no shyness.
    fn hunt_range(&self, p: &Pirate) -> i32 {
        if p.navy {
            return NAVY_HUNT_RANGE;
        }
        let notoriety = self.reputation / reputation::HEXES_PER_POINT;
        let shyness = if p.beaten { BEATEN_SHYNESS } else { 0 };
        (HUNT_RANGE - notoriety - shyness).max(1)
    }

    // -- the navy ----------------------------------------------------------

    pub fn navy_out(&self) -> usize {
        self.pirates.iter().filter(|p| p.navy).count()
    }

    /// Bring the crown's strength into line with what the player has earned.
    ///
    /// Called after every offence and again each month, which is what ties the
    /// fleet to the score in both directions: raiding sends for more of them,
    /// and the monthly fade eventually sends them home. The recall is as
    /// important as the summons. A hunt that could only grow would mean the
    /// first two merchants you took decided the rest of the game, and the
    /// reputation number would have no reason to be a number rather than a flag.
    fn enforce(&mut self) {
        let wanted = reputation::navy_wanted(self.reputation);
        let mut out = self.navy_out();

        while out > wanted {
            // Recall the furthest off first, so the one bearing down on you is
            // the last to be called home. Being let off has to feel like the
            // weather clearing, not like a ship blinking out of the next hex.
            let Some(idx) = self
                .pirates
                .iter()
                .enumerate()
                .filter(|(_, p)| p.navy)
                .max_by_key(|(_, p)| hex::wrapped_distance(p.at, self.at))
                .map(|(i, _)| i)
            else {
                break;
            };
            let name = self.pirates[idx].name;
            self.pirates.remove(idx);
            self.say(format!("{name} has given you up and stood away for home."));
            out -= 1;
        }

        while out < wanted {
            let Some(at) = self.navy_station() else { break };
            let name = NAVY_NAMES[(self.rng.below(NAVY_NAMES.len() as u32)) as usize];
            let class = pick_class(&mut self.rng, &NAVY_FLEET);
            let strength = hunter_guns(&mut self.rng, class);
            self.pirates.push(Pirate {
                at,
                class,
                strength,
                hours: 0.0,
                hunting: false,
                name,
                last_seen: None,
                memory: 0,
                beaten: false,
                met: false,
                navy: true,
            });
            self.say(format!(
                "{name} has been sent for. There is a king's ship looking for you."
            ));
            out += 1;
        }
    }

    /// Somewhere over the horizon to send a king's ship from.
    ///
    /// Not on top of the player, because arriving inside her sight is a jump
    /// scare rather than a hunt, and the whole point of the memory work is that
    /// pursuit should be something you watch coming.
    fn navy_station(&mut self) -> Option<Hex> {
        for _ in 0..400 {
            let col = self.rng.below(crate::world::COLS as u32) as i32;
            let row = self.rng.range(4, crate::world::ROWS - 6);
            let h = hex::from_offset(col, row);
            let Some(i) = hex::index(h) else { continue };
            if nav::is_land_index(i) || self.depth[i] > 8 {
                continue;
            }
            let d = hex::wrapped_distance(h, self.at);
            if d < NAVY_ARRIVES_AT || d > NAVY_ARRIVES_AT * 2 {
                continue;
            }
            return Some(h);
        }
        None
    }

    fn move_pirates(&mut self, elapsed: f32) {
        let month = self.month;
        let player = self.at;
        let mut engagements = Vec::new();

        for idx in 0..self.pirates.len() {
            let strength = self.pirates[idx].strength;
            self.pirates[idx].hours += elapsed;

            // A pirate sails a little slower than a well-found trader, which is
            // what makes rigging worth buying for reasons other than speed.
            let step_hours = 6.5;
            while self.pirates[idx].hours >= step_hours {
                self.pirates[idx].hours -= step_hours;

                let here = self.pirates[idx].at;
                // Close enough to see, and far enough out to dare. Raiders work
                // the open sea, not the harbour approaches, so a ship on the
                // coastal shelf is left alone.
                //
                // This is a balance fix with a real measurement behind it. When
                // hunting ignored the shelf, a run of twelve hexes out of Lisbon
                // drew two boardings, and an unarmed trader loses a third of her
                // gold each time. Compounded over an ordinary voyage that is
                // three thousand gold down to single figures: not a hard game, an
                // arithmetically unplayable one. Tying the danger to the offing
                // instead gives a safe coastal trade to start from and makes the
                // blue-water rigging worth buying for what it earns, not just for
                // where it lets you go.
                let range = self.hunt_range(&self.pirates[idx]);
                // The shelf clause protects an honest master from raiders and
                // does nothing for a wanted one. A king's ship works the
                // approaches by design, so the safe coastal trade the paragraph
                // above buys you is exactly what piracy costs you.
                let inshore_safe = !self.pirates[idx].navy && self.offshore() <= SHELF;
                let sees = hex::wrapped_distance(here, player) <= range && !inshore_safe;

                // Memory. Sight refreshes it; losing sight spends it down one
                // step at a time; running out of it forgets where you were.
                // This is what stops a raider losing you the instant you cross
                // the horizon, which is how she behaved before and which meant
                // that outrunning a pirate never actually required speed.
                if sees {
                    self.pirates[idx].last_seen = Some(player);
                    self.pirates[idx].memory = PIRATE_MEMORY;
                } else if self.pirates[idx].memory > 0 {
                    self.pirates[idx].memory -= 1;
                    if self.pirates[idx].memory == 0 {
                        self.pirates[idx].last_seen = None;
                    }
                }

                // Arriving at the remembered hex and finding nothing there ends
                // the chase honestly, rather than letting her sit on the spot
                // burning down a counter.
                if !sees && self.pirates[idx].last_seen == Some(here) {
                    self.pirates[idx].last_seen = None;
                    self.pirates[idx].memory = 0;
                }

                let goal = if sees { Some(player) } else { self.pirates[idx].last_seen };
                self.pirates[idx].hunting = goal.is_some();

                let dir = if let Some(goal) = goal {
                    // Run the same pathfinder the player's autopilot uses,
                    // limited to a short horizon so a fleet of them stays
                    // affordable.
                    // Nothing barred: a raider's draught is not modelled, and a
                    // chase that stopped at a creek mouth would be a rule the
                    // player could see the effect of without ever being told it.
                    let path = nav::find_path(
                        &mut self.pathfinder, here, goal, month, 2, 6.0, &self.depth, 8,
                        &self.open_water,
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
                        }
                    }
                }
            }

            if self.pirates[idx].at == player {
                engagements.push((idx, strength));
            }
        }

        // Descending, because a won fight removes the pirate from the vector
        // and every index above it shifts down. Two of them can reach the same
        // hex on the same tick, and with `panic = "abort"` an out-of-bounds
        // index here is not a bug report, it is a blank page.
        for (idx, strength) in engagements.into_iter().rev() {
            self.fight(idx, strength);
            if self.lost {
                return;
            }
        }

        // Safe now: nothing above holds an index into `pirates` any more.
        if self.pending_enforce {
            self.pending_enforce = false;
            self.enforce();
        }
    }

    /// Being brought to by a king's ship.
    ///
    /// Split out rather than folded into `fight` with more branches, because
    /// almost nothing about it is the same fight. There is no prize in it for
    /// either side: they are not after your cargo, they are after you, and the
    /// two outcomes are answering for it or adding to it. Winning is the worse
    /// of the two in the long run, which is the point.
    fn arrest(&mut self, idx: usize, strength: i32) {
        let guns = self.ship.guns_worked();
        let name = self.pirates[idx].name;
        let hull = self.pirates[idx].class.name();
        self.pirates[idx].met = true;
        self.say(format!(
            "{name}, a {hull}, runs up the king's colours and fires a gun to \
             windward. {strength} guns, and they are hailing you to bring to."
        ));

        if guns == 0 || self.rng.range(0, guns + strength) >= guns {
            // Taken. Half the purse and the whole hold, and the account is
            // partly settled: there has to be a way back other than waiting out
            // eight years of monthly fade, and standing trial is it.
            let fine = self.gold / 2;
            self.gold -= fine;
            let mut units = 0;
            for slot in self.ship.hold.iter_mut() {
                units += *slot;
                *slot = 0;
            }
            let was = reputation::standing(self.reputation);
            self.reputation = reputation::answered_for(self.reputation);
            let now = reputation::standing(self.reputation);
            self.say(format!(
                "You are boarded and taken in. The court has {} in gold and \
                 {units} units condemned, and lets you keep the ship.",
                coin(fine)
            ));
            if now != was {
                self.say(format!("You leave the prize court {now}."));
            }
            let wound = self.rng.range(4, 14);
            self.hurt(wound, "They fired into your rigging to bring you to.");
            let toll = self.rng.range(0, 2);
            self.kill_crew(toll);
            if !self.lost {
                self.pirates[idx].at = self.scatter_from(self.at);
                self.pirates[idx].last_seen = None;
                self.pirates[idx].memory = 0;
            }
            // Deferred, not called. `enforce` adds and removes ships, and this
            // runs from inside a loop that is holding indices into that same
            // vector: recalling a ship below the one being fought would shift
            // every index above it and the next engagement would read the wrong
            // pirate or read off the end. With `panic = "abort"` that is a blank
            // page, not a stack trace. The caller flushes this once it is done
            // indexing.
            self.pending_enforce = true;
            return;
        }

        // Fought clear of the navy, which is a heavier crime than whatever put
        // them on you, and it is meant to be: the escape that costs nothing
        // would make the whole fleet a nuisance rather than a consequence.
        let roll = self.rng.range(0, guns + strength);
        if roll * 4 < guns {
            self.say(format!(
                "You rake {name} and she settles by the head. You have sunk a king's ship."
            ));
            self.pirates.remove(idx);
            self.credit(reputation::RESIST_NAVY + reputation::SINK_NAVY);
        } else {
            self.say(format!(
                "You fight clear of {name} and run. That will be reported."
            ));
            self.pirates[idx].beaten = true;
            self.pirates[idx].last_seen = None;
            self.pirates[idx].memory = 0;
            self.pirates[idx].at = self.scatter_from(self.at);
            self.credit(reputation::RESIST_NAVY);
        }
        let scars = self.rng.range(6, 18);
        self.hurt(scars, "They were gunners by trade.");
        let toll = self.rng.range(2, 5 + strength / 6);
        self.kill_crew(toll);
        // Deferred for the same reason as the branch above, and unconditionally:
        // the flag costs nothing to set when the game is already lost, and
        // guarding it here was the only asymmetry between the two outcomes.
        self.pending_enforce = true;
    }

    fn fight(&mut self, idx: usize, strength: i32) {
        if self.pirates[idx].navy {
            return self.arrest(idx, strength);
        }
        // Guns she can serve, not guns she carries. A gun deck with nobody on
        // it is a hold full of iron, and a ship that has starved her way down
        // to a handful of hands fights like the unarmed trader she has become.
        let guns = self.ship.guns_worked();
        let name = self.pirates[idx].name;
        // Recognition. The second meeting reads differently from the first, and
        // this one line is most of what makes the memory above land as memory
        // rather than as a pathfinding change nobody can see.
        let hull = self.pirates[idx].class.name();
        if self.pirates[idx].met {
            self.say(format!(
                "{name} again. You know that {hull}. {strength} guns against yours."
            ));
        } else {
            self.say(format!(
                "A {hull} closes fast and runs up no colours. {strength} guns against yours."
            ));
        }
        self.pirates[idx].met = true;

        if guns == 0 {
            if self.ship.gun_count() > 0 {
                self.say("The guns are run out and there is nobody to serve them.".into());
            }
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
                    "You have nothing to answer with. They take {} in gold and {units} units, and let you go.",
                    coin(taken_gold)
                ));
            }
            let parting = self.rng.range(5, 15);
            self.hurt(parting, "They fire into you as they sheer off.");
            let toll = self.rng.range(0, 2);
            self.kill_crew(toll);
            self.pirates[idx].at = self.scatter_from(self.at);
            // Both sides break off. Without this the memory above would let a
            // raider who has just robbed an unarmed ship turn straight round and
            // do it again, which is not a hard game, it is the same boarding on
            // a loop.
            self.pirates[idx].last_seen = None;
            self.pirates[idx].memory = 0;
            return;
        }

        // An armed trader is a real fight. Guns tell, but so does luck.
        let roll = self.rng.range(0, guns + strength);
        if roll < guns {
            // A win, and how decisive it was decides whether she goes down or
            // gets away. This split is not decoration. If every win removed the
            // pirate, the only raiders left alive would be the ones who beat
            // you, nothing could ever remember losing to you, and a long
            // successful game would empty the ocean, since the fleet is spawned
            // once and never replenished. Letting most wins end in a chase
            // broken off gives the memory above somebody to belong to.
            let decisive = roll * 4 < guns;
            let earned = reputation::BEAT_PIRATE
                + strength / reputation::STRENGTH_PER_POINT;
            if decisive {
                self.say(format!(
                    "You hull {name} twice below the waterline and she goes down."
                ));
                self.pirates.remove(idx);
                self.credit(earned + reputation::SINK_BONUS);
            } else {
                self.say(format!(
                    "Two broadsides and {name} has had enough. She bears away."
                ));
                self.pirates[idx].beaten = true;
                self.pirates[idx].last_seen = None;
                self.pirates[idx].memory = 0;
                self.pirates[idx].at = self.scatter_from(self.at);
                self.credit(earned);
            }
            let scars = self.rng.range(2, 10);
            self.hurt(scars, "You did not come off clean.");
            // Winning costs people too, and it costs more against a heavier
            // ship. This is the figure that makes a run of victories something
            // you have to pay for at the next quayside.
            let toll = self.rng.range(0, 2 + strength / 10);
            self.kill_crew(toll);
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
                "They are the better gunners. {} in gold and {units} units go over the side to them.",
                coin(taken_gold)
            ));
            let wound = self.rng.range(8, 24);
            self.hurt(wound, "The mainmast is wounded.");
            let toll = self.rng.range(1, 3 + strength / 6);
            self.kill_crew(toll);
            if !self.lost {
                self.pirates[idx].at = self.scatter_from(self.at);
                self.pirates[idx].last_seen = None;
                self.pirates[idx].memory = 0;
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

    // -- merchants ---------------------------------------------------------

    fn trading_ports(&self) -> Vec<usize> {
        (0..PORTS.len()).filter(|p| Markets::trades(*p)).collect()
    }

    fn spawn_merchants(&mut self) {
        let ports = self.trading_ports();
        if ports.len() < 2 {
            return;
        }
        for _ in 0..MERCHANT_COUNT {
            let from = ports[self.rng.below(ports.len() as u32) as usize];
            let at = hex::from_offset(PORTS[from].col as i32, PORTS[from].row as i32);
            let class = pick_class(&mut self.rng, &MERCHANT_FLEET);
            let mut m = Merchant {
                at,
                class,
                bound: from,
                cargo: 0,
                units: 0,
                gold: 0,
                hours: 0.0,
                stuck: 0,
            };
            relade(&mut self.rng, &mut m, &ports);
            self.merchants.push(m);
        }
    }

    /// Sail the merchant fleet.
    ///
    /// These steer greedily toward their destination rather than by pathfinder,
    /// and that is a deliberate cost decision rather than laziness. A full turn
    /// serialises in about 46µs, of which the simulation step is a quarter of a
    /// microsecond; the pirates can afford A* because only the ones actually
    /// hunting ever call it, which is rarely more than one. Two dozen merchants
    /// pathfinding on every step would be two dozen A* calls per player move and
    /// would dominate the frame for no gain the player could see. Greedy steering
    /// walks them into bays, which is what `stuck` is for: enough failed steps
    /// and the master gives up on that port and picks another.
    fn move_merchants(&mut self, elapsed: f32) {
        let ports = self.trading_ports();
        if ports.is_empty() {
            return;
        }
        for idx in 0..self.merchants.len() {
            self.merchants[idx].hours += elapsed;
            while self.merchants[idx].hours >= MERCHANT_STEP_HOURS {
                self.merchants[idx].hours -= MERCHANT_STEP_HOURS;

                let here = self.merchants[idx].at;
                let bound = self.merchants[idx].bound;
                let goal =
                    hex::from_offset(PORTS[bound].col as i32, PORTS[bound].row as i32);
                if here == goal {
                    relade(&mut self.rng, &mut self.merchants[idx], &ports);
                    continue;
                }

                // The neighbour that gets closest and is water she can float in.
                let mut best: Option<(i32, Hex)> = None;
                for d in 0..6 {
                    let n = hex::normalise(hex::neighbour(here, d));
                    let Some(ni) = hex::index(n) else { continue };
                    if nav::is_land_index(ni) || self.depth[ni] > 6 {
                        continue;
                    }
                    let cost = hex::wrapped_distance(n, goal);
                    if best.map_or(true, |(b, _)| cost < b) {
                        best = Some((cost, n));
                    }
                }
                match best {
                    Some((cost, n)) if cost < hex::wrapped_distance(here, goal) => {
                        self.merchants[idx].at = n;
                        self.merchants[idx].stuck = 0;
                    }
                    _ => {
                        self.merchants[idx].stuck += 1;
                        if self.merchants[idx].stuck >= MERCHANT_STUCK_LIMIT {
                            relade(&mut self.rng, &mut self.merchants[idx], &ports);
                        }
                    }
                }
            }
        }
    }

    /// The merchant on this hex, if there is one.
    pub fn merchant_here(&self) -> Option<usize> {
        self.merchants.iter().position(|m| m.at == self.at)
    }

    /// Fire on a merchant. This is the only order in the game that is a crime.
    ///
    /// Deliberate rather than automatic, which is the whole difference between
    /// this and meeting a pirate: bumping into an honest trader must not cost
    /// you your standing, so it takes its own order and its own key. The
    /// reputation goes whether or not it works, because the offence is the
    /// attack.
    pub fn attack(&mut self) -> bool {
        if self.lost {
            return false;
        }
        let Some(idx) = self.merchant_here() else {
            self.say("There is nobody alongside to fire on.".into());
            return false;
        };
        let guns = self.ship.guns_worked();
        if guns == 0 {
            if self.ship.gun_count() > 0 {
                self.say("You have guns and not the hands to serve one of them.".into());
            } else {
                self.say("You have no guns. It would be a boarding with bare hands.".into());
            }
            return false;
        }

        let units = self.merchants[idx].units;
        let cargo = self.merchants[idx].cargo;
        let purse = self.merchants[idx].gold;
        self.say(format!(
            "You run out the guns at a {}, flying no flag but her own, and she is carrying {} {}.",
            self.merchants[idx].class.name(),
            units,
            GOODS[cargo]
        ));
        self.credit(reputation::RAID_MERCHANT);
        // Word goes out on the offence, not on the outcome, and it goes out now
        // rather than at the end of the month. Being seen to run out your guns
        // at a trader is the thing that is reported; whether you were any good
        // at it is a detail for the court.
        self.enforce();

        // A merchant is not defenceless, only badly armed: she fights the guns
        // her class was launched with and never buys more, because every gun is
        // a ton of cargo she is not paid for. The rest is the crew defending
        // their own freight, which is why a full hold fights harder.
        let escort = guns_at(self.merchants[idx].class.spec().gun_start) + 4 + units / 8;
        let roll = self.rng.range(0, guns + escort);
        if roll >= guns {
            self.say("She fights her guns better than you expected and hauls off.".into());
            let wound = self.rng.range(4, 16);
            self.hurt(wound, "You take the worst of the exchange.");
            let toll = self.rng.range(1, 4);
            self.kill_crew(toll);
            return true;
        }

        let taken = units.min(self.ship.free_space());
        self.ship.hold[cargo] += taken;
        self.gold += purse;
        self.merchants[idx].units -= taken;
        self.merchants[idx].gold = 0;
        self.say(format!(
            "She strikes. You take {taken} {} and {purse} in gold out of her, and leave her the hull.",
            GOODS[cargo]
        ));
        if taken < units {
            self.say("The rest goes over the side. Your hold would not hold it.".into());
        }

        // Looted cargo cost nothing, so the profit report has to know that or it
        // would call the whole sale a loss against an outlay that never happened.
        let scars = self.rng.range(2, 12);
        self.hurt(scars, "Not without damage.");
        let toll = self.rng.range(0, 3);
        self.kill_crew(toll);
        if !self.lost {
            let away = self.scatter_from(self.at);
            self.merchants[idx].at = away;
        }
        true
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
        // Priced but shut. The distinction matters enough to spend a different
        // sentence on it: the first refusal above is the world telling you the
        // good is not here, this one is the harbour telling you it is not here
        // *for you yet*, and those ask for opposite responses from the player.
        if !self.markets.is_open(port, good) {
            self.say(format!(
                "{} is traded in {} behind closed doors. Buy your way in first.",
                GOODS[good], PORTS[port].name
            ));
            return false;
        }
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
        // Remembered for the life of the voyage. A commission steers toward
        // goods this has never been set for, so it must record the trade rather
        // than the stock: selling the parcel on does not make it unfamiliar.
        self.traded[good] = true;
        self.markets.on_buy(port, good, cost);
        self.say(format!(
            "Bought {take} {} at {} for {}.",
            GOODS[good],
            coin(unit),
            coin(cost)
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
                "{} grows here. They take your {take} at {} a unit out of politeness: {verdict}.",
                GOODS[good],
                coin(unit),
            ));
        } else {
            self.say(format!(
                "Sold {take} {} at {}: {verdict}.",
                GOODS[good],
                coin(unit)
            ));
        }
        true
    }

    // -- crew and stores at the quayside -----------------------------------

    /// Sign hands on. Any port, not only a capital.
    ///
    /// The yard that sells hulls is in nine places; the crimp who finds you a
    /// crew is in all seventy. That asymmetry is deliberate. Provisions and
    /// hands are what you need to *leave* a port, and a rule that let you into
    /// a harbour you could not victual out of would be a trap rather than a
    /// difficulty.
    pub fn hire(&mut self, n: i32) -> bool {
        if self.lost || n <= 0 {
            return false;
        }
        if self.port_here().is_none() {
            self.say("Hands are signed on at a quayside, not at sea.".into());
            return false;
        }
        let berths = self.ship.crew_max() - self.ship.crew;
        if berths <= 0 {
            self.say(format!(
                "A {} berths {} and you have them.",
                self.ship.class.name(),
                self.ship.crew_max()
            ));
            return false;
        }
        let affordable = self.gold / ship::HIRE_ADVANCE;
        let take = n.min(berths).min(affordable);
        if take <= 0 {
            self.say(format!(
                "The advance is {} a head and you have {}.",
                coin(ship::HIRE_ADVANCE),
                coin(self.gold)
            ));
            return false;
        }
        let cost = take * ship::HIRE_ADVANCE;
        self.gold -= cost;
        self.ship.crew += take;
        self.say(format!(
            "{take} hands sign on for {} in advance. {} aboard, of {} berths.",
            coin(cost),
            self.ship.crew,
            self.ship.crew_max()
        ));
        true
    }

    /// Pay hands off, which is how a wage bill gets smaller.
    ///
    /// Never takes the last one. A ship with nobody aboard is lost, and losing
    /// by pressing the discharge key would be a cruelty rather than a
    /// consequence.
    pub fn discharge(&mut self, n: i32) -> bool {
        if self.lost || n <= 0 {
            return false;
        }
        if self.port_here().is_none() {
            self.say("Hands are paid off ashore, not at sea.".into());
            return false;
        }
        let take = n.min((self.ship.crew - 1).max(0));
        if take <= 0 {
            self.say("There is one hand left and she needs one hand.".into());
            return false;
        }
        self.ship.crew -= take;
        self.say(format!(
            "{take} hands paid off and gone ashore. {} left.",
            self.ship.crew
        ));
        true
    }

    /// Buy or sell food, water and lumber. Positive to buy, negative to sell.
    ///
    /// Selling back exists as a way out of the one corner this feature could
    /// otherwise paint a player into: a hold full of water, no gold, and
    /// nothing the market will take. It pays half, which is enough to be a
    /// rescue and not enough to be a trade.
    ///
    /// Alongside a merchant the same order works at a premium, because she is
    /// selling out of her own lockers and has no reason to be generous.
    pub fn provision(&mut self, kind: i32, qty: i32) -> bool {
        if self.lost || qty == 0 {
            return false;
        }
        let Some(store) = ship::Store::from_index(kind) else {
            return false;
        };
        let port = self.port_here();
        let afloat = port.is_none();
        if afloat && self.merchant_here().is_none() {
            self.say("There is no chandler at sea and nobody alongside.".into());
            return false;
        }

        let unit = if afloat {
            store.price_afloat()
        } else {
            store.price()
        };

        if qty < 0 {
            if afloat {
                self.say("She has her own stores and no room for yours.".into());
                return false;
            }
            let take = (-qty).min(self.ship.store(store));
            if take <= 0 {
                self.say(format!("There is no {} aboard to sell.", store.name()));
                return false;
            }
            let paid = take * unit / 2;
            *self.ship.store_mut(store) -= take;
            self.gold += paid;
            self.say(format!(
                "Sold {take} {} back to the chandler for {}.",
                store.name(),
                coin(paid)
            ));
            return true;
        }

        let affordable = self.gold / unit;
        let fits = self.ship.free_space();
        let take = qty.min(affordable).min(fits);
        if take <= 0 {
            if fits <= 0 {
                self.say("The hold is full. Stores take the same room as cargo.".into());
            } else {
                self.say(format!(
                    "{} is {} a unit and you have {}.",
                    store.name(),
                    coin(unit),
                    coin(self.gold)
                ));
            }
            return false;
        }
        let cost = take * unit;
        self.gold -= cost;
        *self.ship.store_mut(store) += take;
        let whom = if afloat { " out of her own lockers" } else { "" };
        self.say(format!(
            "Took aboard {take} {} at {} a unit{whom}: {}.",
            store.name(),
            coin(unit),
            coin(cost)
        ));
        true
    }

    /// Mend the ship with lumber and hands, wherever she happens to be.
    ///
    /// This is the counterpart to `repair`, not a cheaper version of it. The
    /// yard wants gold and gives you a whole ship back in two days; the
    /// carpenter wants timber and people and gives you back only as much as
    /// there is timber for. What it buys is the ability to mend at sea after a
    /// fight, which is the whole reason to carry lumber at all.
    pub fn mend(&mut self) -> bool {
        if self.lost {
            return false;
        }
        if self.ship.damage == 0 {
            self.say("She is sound. There is nothing to mend.".into());
            return false;
        }
        if self.ship.crew < self.ship.crew_min() {
            self.say(format!(
                "It takes {} hands to work the timber and you have {}.",
                self.ship.crew_min(),
                self.ship.crew
            ));
            return false;
        }
        let points = self.ship.mendable();
        if points <= 0 {
            self.say("There is no lumber aboard.".into());
            return false;
        }
        self.ship.lumber -= points * ship::LUMBER_PER_POINT;
        self.ship.damage -= points;
        // Carpentry is a day's work whatever the size of the hole, and the day
        // costs food and water like any other. That is what stops mending from
        // being free.
        self.advance(HOURS_PER_DAY);
        self.say(format!(
            "A day of hammering and {points} points of damage go. She is at {}%.",
            self.ship.damage
        ));
        true
    }

    /// Put money into the harbour and have one more of its goods opened to you.
    ///
    /// Deliberately not a choice of *which* good. The port decides what it lets
    /// a new partner see next, which is both the truthful reading and the one
    /// that keeps two ports of an economy distinct: if the player picked, every
    /// port would converge on the same book in the same order.
    pub fn invest(&mut self) -> bool {
        let Some(port) = self.port_here() else {
            self.say("You are at sea. There is nobody to invest with.".into());
            return false;
        };
        let Some(cost) = self.markets.investment_cost(port) else {
            self.say(format!(
                "{} has no more to show you. Their whole book is open.",
                PORTS[port].name
            ));
            return false;
        };
        if self.gold < cost {
            self.say(format!(
                "A share in {} costs {}.",
                PORTS[port].name,
                coin(cost)
            ));
            return false;
        }
        let before: Vec<usize> = (0..GOODS.len())
            .filter(|&g| self.markets.is_open(port, g))
            .collect();
        self.gold -= cost;
        self.markets.invest(port, cost);
        let opened = (0..GOODS.len())
            .find(|g| self.markets.is_open(port, *g) && !before.contains(g));
        match opened {
            Some(g) => self.say(format!(
                "Invested {} in {}. They open the {} trade to you.",
                coin(cost),
                PORTS[port].name,
                GOODS[g]
            )),
            None => self.say(format!(
                "Invested {} in {}.",
                coin(cost),
                PORTS[port].name
            )),
        }
        true
    }

    // -- the shipyard ------------------------------------------------------
    //
    // Sums here are grouped in threes, because the page prints the same sums
    // beside them through `toLocaleString` and a bare `33400` next to a
    // `33,400` reads as a different number.

    /// Everything on offer at this port, largest last.
    ///
    /// Empty at a port with no yard, which is most of them, and that emptiness
    /// is what the page reads to decide whether to draw a shipyard at all.
    pub fn yard(&self, port: usize) -> Vec<Offer> {
        if !PORTS[port].capital {
            return Vec::new();
        }
        let econ = PORTS[port].econ;
        let invested = self.markets.invested_of(port);
        let allowance = self.ship.trade_in();
        CLASSES
            .iter()
            .filter(|c| c.built_in(econ))
            .map(|&class| {
                let spec = class.spec();
                let price = (spec.price - allowance).max(0);
                let locked = if class == self.ship.class {
                    Some("You are in one.".into())
                } else if spec.needs_invested > invested {
                    Some(format!(
                        "The yard builds these to order for its own partners. {} invested here, of {}.",
                        coin(invested),
                        coin(spec.needs_invested)
                    ))
                } else if self.gold < price {
                    Some(format!("{} gold, and you have {}.", coin(price), coin(self.gold)))
                } else if self.ship.stowed() > Ship::capacity_of(class) {
                    // Stores count. They come across with the ship, so a hold
                    // that will not take them is the same refusal as a hold
                    // that will not take the cargo, and saying otherwise here
                    // would tip the crew's water onto the quay.
                    Some(format!(
                        "She holds {} and you have {} aboard, stores and all. Sell down first.",
                        Ship::capacity_of(class),
                        self.ship.stowed()
                    ))
                } else {
                    None
                };
                Offer { class, price, trade_in: allowance, locked }
            })
            .collect()
    }

    /// Trade the ship you are in for one of `which`.
    ///
    /// The hold does not come with you. A new hull is a new hold, so anything
    /// aboard has to fit the ship you are buying, and the yard refuses rather
    /// than quietly tipping your cargo onto the quay. That mirrors `fit()`
    /// refusing to cut gunports through a full hold, and for the same reason:
    /// the game never destroys goods the player paid for without being asked.
    pub fn buy_ship(&mut self, which: i32) -> bool {
        let Some(port) = self.port_here() else {
            self.say("There is no shipyard in open water.".into());
            return false;
        };
        let Some(class) = Class::from_index(which) else {
            return false;
        };
        if !PORTS[port].capital {
            self.say(format!(
                "{} has no shipyard. Only a capital builds ships.",
                PORTS[port].name
            ));
            return false;
        }
        let Some(offer) = self.yard(port).into_iter().find(|o| o.class == class) else {
            self.say(format!(
                "No yard in {} builds a {}.",
                crate::world::ECONOMIES[PORTS[port].econ.max(0) as usize],
                class.name()
            ));
            return false;
        };
        if let Some(why) = offer.locked {
            self.say(why);
            return false;
        }

        // Carry the cargo, the stores and the people across by hand. The hold
        // vectors are per-ship and the new one starts empty, so this is the
        // only thing that survives the sale besides the money.
        //
        // Crew comes across capped at the new berths and is *not* topped up to
        // the new minimum. Buying a Carrack with eight hands leaves you
        // undermanned and slow until you have been to the crimp, which is the
        // honest outcome and, more to the point, is not a lock: the alternative
        // of refusing the sale until you had fifteen hands would be
        // unreachable, since a Balsa berths twelve.
        let hold = core::mem::take(&mut self.ship.hold);
        let paid = core::mem::take(&mut self.ship.paid);
        let (crew, food, water_aboard, lumber) = (
            self.ship.crew,
            self.ship.food,
            self.ship.water,
            self.ship.lumber,
        );
        self.gold -= offer.price;
        self.ship = Ship::of_class(class, GOODS.len());
        self.ship.hold = hold;
        self.ship.paid = paid;
        self.ship.crew = crew.min(self.ship.crew_max());
        self.ship.food = food;
        self.ship.water = water_aboard;
        self.ship.lumber = lumber;
        if crew > self.ship.crew {
            self.say(format!(
                "{} hands more than she berths are paid off on the quay.",
                crew - self.ship.crew
            ));
        }
        if self.ship.crew < self.ship.crew_min() {
            self.say(format!(
                "She wants {} hands and you have {}. She will be slow until you find more.",
                self.ship.crew_min(),
                self.ship.crew
            ));
        }

        let spec = class.spec();
        let water = if class.is_bluewater() {
            "She will stand across an open ocean."
        } else {
            "She is a coasting ship; keep the land in sight."
        };
        self.say(format!(
            "A {} out of {}, {}, {} guns, {} in the hold. {water}",
            spec.name,
            PORTS[port].name,
            spec.rig,
            self.ship.gun_count(),
            self.ship.capacity(),
        ));
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
                "The yard at {} wants {} for that. You have {}.",
                PORTS[port].name,
                coin(cost),
                coin(self.gold)
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
            self.say(format!(
                "The repair is {} and you have {}.",
                coin(cost),
                coin(self.gold)
            ));
            return false;
        }
        self.gold -= cost;
        self.ship.damage = 0;
        self.advance(48.0);
        self.say(format!(
            "Two days in the yard and {} gone. She is sound.",
            coin(cost)
        ));
        true
    }

    // -- rendering ---------------------------------------------------------

    /// [`STRIDE`] bytes per hex: the terrain, what is on it, and how well it can
    /// be seen.
    ///
    /// The mark starts out as the terrain and is overwritten by anything more
    /// interesting standing on it. That keeps a quiet hex to one glyph while
    /// giving the page both channels where they differ.
    pub fn render(&mut self) -> &[u8] {
        for i in 0..CELLS {
            let code = if nav::is_land_index(i) {
                CODE_LAND
            } else if self.draws_deep(i) {
                CODE_DEEP
            } else {
                CODE_SHALLOW
            };
            self.render[i * STRIDE + AT_TERRAIN] = code;
            self.render[i * STRIDE + AT_MARK] = code;
            self.render[i * STRIDE + AT_FOG] = self.fog[i];
        }

        for (i, p) in PORTS.iter().enumerate() {
            if !self.discovered[i] {
                continue;
            }
            let h = hex::from_offset(p.col as i32, p.row as i32);
            if let Some(idx) = hex::index(h) {
                self.render[idx * STRIDE + AT_MARK] = CODE_PORT;
                if self.render[idx * STRIDE + AT_FOG] == UNSEEN {
                    self.render[idx * STRIDE + AT_FOG] = REMEMBERED;
                }
            }
        }

        // Merchants first, then pirates over the top of them. Where both are on
        // one hex the raider is the one you need to know about.
        for m in &self.merchants {
            if let Some(idx) = hex::index(m.at) {
                if self.fog[idx] == VISIBLE {
                    self.render[idx * STRIDE + AT_MARK] = match self.render[idx * STRIDE + AT_MARK] {
                        CODE_MERCHANT | CODE_MERCHANTS => CODE_MERCHANTS,
                        _ => CODE_MERCHANT,
                    };
                }
            }
        }

        // A raider arriving on a hex that already holds one marks the stack. Only
        // raiders count toward that: a pirate standing over a trader is a raider
        // on his own, and the trader is not the news.
        for p in &self.pirates {
            if let Some(idx) = hex::index(p.at) {
                if self.fog[idx] == VISIBLE {
                    let crowded = matches!(
                        self.render[idx * STRIDE + AT_MARK],
                        CODE_PIRATE | CODE_NAVY | CODE_PIRATES | CODE_NAVIES
                    );
                    self.render[idx * STRIDE + AT_MARK] = match (p.navy, crowded) {
                        (false, false) => CODE_PIRATE,
                        (false, true) => CODE_PIRATES,
                        (true, false) => CODE_NAVY,
                        (true, true) => CODE_NAVIES,
                    };
                }
            }
        }

        if let Some(idx) = hex::index(self.at) {
            self.render[idx * STRIDE + AT_MARK] = CODE_SHIP;
            self.render[idx * STRIDE + AT_FOG] = VISIBLE;
        }
        &self.render
    }

    pub fn discovered(&self, port: usize) -> bool {
        self.discovered[port]
    }

    /// Raiders only. A king's ship in sight is a different problem and gets its
    /// own line, because "three strange sail" and "two raiders and a frigate"
    /// call for opposite decisions.
    pub fn pirates_in_sight(&self) -> usize {
        self.in_sight(|p| !p.navy)
    }

    pub fn navy_in_sight(&self) -> usize {
        self.in_sight(|p| p.navy)
    }

    fn in_sight(&self, want: impl Fn(&Pirate) -> bool) -> usize {
        self.pirates
            .iter()
            .filter(|p| {
                want(p)
                    && hex::index(p.at).map(|i| self.fog[i] == VISIBLE).unwrap_or(false)
            })
            .count()
    }

    pub fn merchants_in_sight(&self) -> usize {
        self.merchants
            .iter()
            .filter(|m| {
                hex::index(m.at).map(|i| self.fog[i] == VISIBLE).unwrap_or(false)
            })
            .count()
    }

    /// True while at least one raider is standing toward you, whether or not she
    /// can currently see you. This is the memory made visible: the warning stays
    /// up for a day and a half after you slip over the horizon.
    pub fn hunted(&self) -> bool {
        self.pirates.iter().any(|p| p.hunting)
    }

    pub fn standing(&self) -> &'static str {
        reputation::standing(self.reputation)
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

    /// Whether a hex draws as deep water, by the same test [`Game::render`] uses.
    ///
    /// The chart tells shallow from deep by fill, and fill is a colour, so the
    /// "there" panel has to be able to say it in words too. Sharing the test with
    /// the renderer rather than restating the threshold is the point: a panel
    /// that disagreed with the tile it describes would be worse than silence.
    pub fn draws_deep(&self, index: usize) -> bool {
        !nav::is_land_index(index) && self.depth[index] > 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- commissions -------------------------------------------------------

    /// A game with the whole chart known and nothing but the start port called
    /// at, which is the state the exploration bias is supposed to act on.
    fn charted_but_untravelled() -> Game {
        let mut g = Game::new(1);
        g.discovered.iter_mut().for_each(|d| *d = true);
        g.visited.iter_mut().for_each(|v| *v = false);
        let start = g.port_here().unwrap();
        g.visited[start] = true;
        g
    }

    fn draw_many(g: &mut Game, here: usize, n: usize) -> Vec<Commission> {
        let (visited, discovered, traded) =
            (g.visited.clone(), g.discovered.clone(), g.traded.clone());
        let ledger = commission::Ledger {
            visited: &visited,
            discovered: &discovered,
            traded: &traded,
            barred: &|_| false,
            distance: &port_distance,
        };
        (0..n)
            .filter_map(|_| commission::draw(&mut g.rng, &g.markets, here, &ledger))
            .collect()
    }

    fn a_trading_port(not: usize) -> usize {
        (0..PORTS.len())
            .find(|&p| p != not && Markets::trades(p))
            .expect("no port trades")
    }

    /// The whole reason the feature exists, asserted as the statistical claim it
    /// actually is rather than as the presence of a field.
    ///
    /// A commission that named ports uniformly would be a lottery with extra
    /// steps: the player would take the ones on their existing circuit and
    /// ignore the rest, and the map would stay exactly as small as it was. The
    /// bias is the feature. If this drops to a coin flip the errands are still
    /// generated, still payable and still worth gold, and the thing they were
    /// built for has quietly stopped happening, with nothing else to notice.
    #[test]
    fn commissions_point_at_harbours_the_player_has_never_seen_the_inside_of() {
        let mut g = charted_but_untravelled();
        let here = g.port_here().unwrap();
        // A quarter of the chart called at, so there is a large pool of both
        // kinds to choose between and a uniform draw would land near 25%.
        for p in (0..PORTS.len()).step_by(4) {
            g.visited[p] = true;
        }
        let drawn = draw_many(&mut g, here, 400);
        assert!(drawn.len() > 300, "the world could barely supply an errand: {}", drawn.len());
        let fresh = drawn.iter().filter(|c| !g.visited[c.other]).count();
        assert!(
            fresh * 10 > drawn.len() * 8,
            "only {fresh} of {} errands named an unvisited port; the bias is gone",
            drawn.len()
        );
    }

    /// The same claim for the other half of the ask: the parcel should be
    /// something the player has never bought.
    #[test]
    fn commissions_name_goods_the_player_has_never_bought() {
        let mut g = charted_but_untravelled();
        let here = g.port_here().unwrap();
        for good in (0..GOODS.len()).step_by(3) {
            g.traded[good] = true;
        }
        let drawn = draw_many(&mut g, here, 400);
        let fresh = drawn.iter().filter(|c| !g.traded[c.good]).count();
        assert!(
            fresh * 10 > drawn.len() * 8,
            "only {fresh} of {} errands named an unbought good",
            drawn.len()
        );
    }

    /// The bias is a weight and not a filter, so a player who has been
    /// everywhere still gets offered work. The alternative is a late game where
    /// the factors go silent, which reads as a bug.
    #[test]
    fn a_player_who_has_been_everywhere_is_still_offered_errands() {
        let mut g = charted_but_untravelled();
        g.visited.iter_mut().for_each(|v| *v = true);
        g.traded.iter_mut().for_each(|t| *t = true);
        let here = g.port_here().unwrap();
        assert!(!draw_many(&mut g, here, 40).is_empty());
    }

    /// An errand to a harbour this hull cannot enter is the same trap as a
    /// course laid to one, and worse, because it is a trap with a price on it.
    #[test]
    fn no_errand_names_a_harbour_the_hull_is_barred_from() {
        let mut g = charted_but_untravelled();
        let here = g.port_here().unwrap();
        let banned = a_trading_port(here);
        let (visited, discovered, traded) =
            (g.visited.clone(), g.discovered.clone(), g.traded.clone());
        let ledger = commission::Ledger {
            visited: &visited,
            discovered: &discovered,
            traded: &traded,
            barred: &|p: usize| p == banned,
            distance: &port_distance,
        };
        for _ in 0..400 {
            if let Some(c) = commission::draw(&mut g.rng, &g.markets, here, &ledger) {
                assert_ne!(c.other, banned, "offered an errand to a barred harbour");
            }
        }
    }

    /// An unseen port cannot be laid a course to, so naming one would be an
    /// errand the player is unable to start.
    #[test]
    fn no_errand_names_an_uncharted_harbour() {
        let mut g = Game::new(1);
        let here = g.port_here().unwrap();
        let known: Vec<bool> = g.discovered.clone();
        for _ in 0..200 {
            let drawn = draw_many(&mut g, here, 1);
            for c in drawn {
                assert!(known[c.other], "offered an errand to a port not on the chart");
            }
        }
    }

    /// Carrying the parcel to the right quay pays the gold and the standing,
    /// takes the goods and ends the errand.
    #[test]
    fn discharging_a_commission_pays_gold_and_standing() {
        let mut g = charted_but_untravelled();
        let here = g.port_here().unwrap();
        let c = draw_many(&mut g, here, 200)
            .into_iter()
            .find(|c| c.kind == commission::Kind::Collect)
            .expect("no collect errand in two hundred draws");
        let (good, qty, gold, favour) = (c.good, c.qty, c.gold, c.favour);
        g.commission = Some(c);

        g.ship.hold[good] = qty + 3;
        g.ship.paid[good] = 0;
        let purse = g.gold;
        let standing = g.markets.favour_of(here);

        assert!(g.settle_commission());
        assert_eq!(g.gold, purse + gold);
        assert_eq!(g.ship.hold[good], 3, "the parcel did not leave the hold");
        assert_eq!(g.markets.favour_of(here), standing + favour);
        assert!(g.commission().is_none());
    }

    /// Short of the parcel is a refusal, not a part payment, and the errand
    /// survives it: the player can go and buy the rest.
    #[test]
    fn a_short_hold_cannot_discharge_a_commission() {
        let mut g = charted_but_untravelled();
        let here = g.port_here().unwrap();
        let c = draw_many(&mut g, here, 200)
            .into_iter()
            .find(|c| c.kind == commission::Kind::Collect)
            .expect("no collect errand in two hundred draws");
        let (good, qty) = (c.good, c.qty);
        g.commission = Some(c);
        g.ship.hold[good] = qty - 1;
        let purse = g.gold;

        assert!(!g.settle_commission());
        assert_eq!(g.gold, purse);
        assert!(g.commission().is_some(), "a refusal must not eat the errand");
        assert_eq!(g.ship.hold[good], qty - 1, "a refusal must not eat the cargo");
    }

    /// The right cargo at the wrong quay is refused, and the refusal says where
    /// it is wanted. A player standing in the harbour they bought the goods at
    /// would otherwise have no way to tell a bug from a rule.
    #[test]
    fn the_parcel_is_only_paid_for_at_the_port_that_wants_it() {
        let mut g = charted_but_untravelled();
        let here = g.port_here().unwrap();
        let c = draw_many(&mut g, here, 200)
            .into_iter()
            .find(|c| c.kind == commission::Kind::Deliver)
            .expect("no deliver errand in two hundred draws");
        // A deliver errand is paid at the far end, and the ship is still here.
        let (good, qty, other) = (c.good, c.qty, c.other);
        g.commission = Some(c);
        g.ship.hold[good] = qty;

        assert!(!g.settle_commission());
        let said = g.chronicle().last().unwrap();
        assert!(said.contains(PORTS[other].name), "the refusal did not say where: {said:?}");
    }

    /// One at a time. The offer is not even drawn while an errand is running,
    /// so the port panel never shows a second one to weigh against the first.
    #[test]
    fn a_ship_already_under_commission_is_offered_no_other() {
        let mut g = charted_but_untravelled();
        let here = g.port_here().unwrap();
        let c = draw_many(&mut g, here, 200).into_iter().next().expect("no errand");
        g.commission = Some(c);
        for _ in 0..200 {
            g.offer_commission(here);
        }
        assert!(g.offer().is_none());
    }

    /// Standing in past a harbour under a laid course is not coming ashore, so
    /// nobody rows out with an errand.
    #[test]
    fn no_factor_boards_a_ship_that_did_not_stop() {
        let mut g = charted_but_untravelled();
        let here = g.port_here().unwrap();
        for _ in 0..200 {
            g.offer_commission(here);
            if g.offer().is_some() {
                break;
            }
        }
        assert!(g.offer().is_some(), "no errand in two hundred landfalls");
        // Any leg of sailing clears it, which is what the passing-through case
        // relies on: the offer is thrown away before the arrival block runs.
        for dir in 0..6 {
            if g.step(dir) {
                break;
            }
        }
        assert!(g.offer().is_none(), "an offer followed the ship out of harbour");
    }

    /// Abandoning costs nothing. An optional errand that can punish you is not
    /// optional, and this is the line that says so in code.
    #[test]
    fn abandoning_a_commission_costs_nothing() {
        let mut g = charted_but_untravelled();
        let here = g.port_here().unwrap();
        let c = draw_many(&mut g, here, 200).into_iter().next().expect("no errand");
        g.commission = Some(c);
        let (purse, standing, rep) = (g.gold, g.markets.favour_of(here), g.reputation);

        assert!(g.abandon_commission());
        assert!(g.commission().is_none());
        assert_eq!(g.gold, purse);
        assert_eq!(g.markets.favour_of(here), standing);
        assert_eq!(g.reputation, rep);
    }

    /// `discovered` is set by the lookout at range and `visited` only by an
    /// anchor going down. If the two were the same field the bias would point
    /// at every harbour the player had ever sailed within sight of, which is
    /// most of the ones they have already worked.
    #[test]
    fn sighting_a_harbour_is_not_the_same_as_calling_at_it() {
        let g = Game::new(1);
        let charted = g.discovered.iter().filter(|d| **d).count();
        let called = g.visited.iter().filter(|v| **v).count();
        assert_eq!(called, 1, "a new game has been ashore exactly once");
        assert!(charted >= called, "the lookout cannot see fewer ports than were entered");
    }

    /// Buying records the good for good, even if the parcel is sold on at the
    /// next quay. The bias steers by what the player has *tried*, not by what
    /// happens to be in the hold.
    #[test]
    fn selling_a_good_on_does_not_make_it_unfamiliar_again() {
        let mut g = Game::new(1);
        let port = g.port_here().unwrap();
        let good = (0..GOODS.len())
            .find(|&x| g.markets.is_open(port, x) && g.markets.buy_price(port, x).is_some())
            .expect("nothing on sale at the start port");
        assert!(!g.traded[good]);
        assert!(g.buy(good, 1));
        assert!(g.traded[good]);
        g.sell(good, 99);
        assert!(g.traded[good], "the ledger forgot a good the player has bought");
    }

    #[test]
    fn sums_are_grouped_the_way_the_page_groups_them() {
        assert_eq!(coin(0), "0");
        assert_eq!(coin(999), "999");
        assert_eq!(coin(1_000), "1,000");
        assert_eq!(coin(33_400), "33,400");
        assert_eq!(coin(1_234_567), "1,234,567");
        assert_eq!(coin(-2_500), "-2,500");
    }

    fn port_named(name: &str) -> usize {
        PORTS.iter().position(|p| p.name == name).expect(name)
    }

    /// Every water cell a ship of the given offing may occupy, reachable from
    /// Lisbon. The graph is undirected, so anything it can reach it can leave.
    fn sea_within(g: &Game, offing: i32) -> Vec<bool> {
        let start = hex::index(g.at).unwrap();
        let mut seen = vec![false; CELLS];
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            let h = hex::from_offset(i as i32 % crate::world::COLS, i as i32 / crate::world::COLS);
            for d in 0..6 {
                let Some(j) = hex::index(hex::normalise(hex::neighbour(h, d))) else {
                    continue;
                };
                if seen[j] || nav::is_land_index(j) || g.depth[j] as i32 > offing {
                    continue;
                }
                seen[j] = true;
                stack.push(j);
            }
        }
        seen
    }

    fn ports_outside(g: &Game, offing: i32) -> Vec<&'static str> {
        let sea = sea_within(g, offing);
        PORTS
            .iter()
            .enumerate()
            .filter(|(i, _)| Markets::trades(*i))
            .filter(|(_, p)| {
                !sea[hex::index(hex::from_offset(p.col as i32, p.row as i32)).unwrap()]
            })
            .map(|(_, p)| p.name)
            .collect()
    }

    /// Days under sail from one port to another, or `None` if this ship cannot
    /// make the passage at all.
    ///
    /// The hours are summed from the same `passage_factor` the voyage itself
    /// uses, over the route the same pathfinder returns, so this is a
    /// measurement of the game rather than a second model of it.
    fn leg_days(g: &Game, s: &Ship, from: Hex, to: Hex) -> Option<f32> {
        let mut scratch = nav::Scratch::new();
        let path = nav::find_path(
            &mut scratch,
            from,
            to,
            g.month,
            s.rigging,
            s.base_hours(),
            &g.depth,
            s.bluewater_rating(),
            if s.class.is_deep_draught() { &g.shallow_harbour } else { &g.open_water },
        );
        if path.is_empty() {
            return None;
        }
        let mut hours = 0.0;
        let mut at = from;
        for step in &path {
            let dir = (0..6).find(|&d| hex::normalise(hex::neighbour(at, d)) == *step)?;
            hours += s.base_hours() * nav::passage_factor(dir, at, g.month, s.rigging);
            at = *step;
        }
        Some(hours / HOURS_PER_DAY)
    }

    /// The longest hop this ship would ever have to make between one trading
    /// port and the next nearest one she can reach: the emptiest stretch of the
    /// map, from her point of view.
    ///
    /// Only the nearest handful of candidates by hex distance are tried, which
    /// can only make the answer larger than the truth. An overestimate here
    /// makes the test that reads it stricter, never laxer, which is the right
    /// direction for a bound to be wrong in.
    fn worst_hop(g: &Game, s: &Ship) -> (f32, &'static str) {
        let mut worst = (0.0f32, "nowhere");
        for (i, p) in PORTS.iter().enumerate() {
            if !Markets::trades(i) {
                continue;
            }
            let from = hex::from_offset(p.col as i32, p.row as i32);
            let mut near: Vec<usize> = (0..PORTS.len())
                .filter(|&j| j != i && Markets::trades(j))
                .collect();
            near.sort_by_key(|&j| {
                let h = hex::from_offset(PORTS[j].col as i32, PORTS[j].row as i32);
                hex::wrapped_distance(from, h)
            });
            let nearest = near
                .iter()
                .take(8)
                .filter_map(|&j| {
                    let to = hex::from_offset(PORTS[j].col as i32, PORTS[j].row as i32);
                    leg_days(g, s, from, to)
                })
                .fold(f32::INFINITY, f32::min);
            assert!(
                nearest.is_finite(),
                "a {} at {} can reach none of its neighbours",
                s.class.name(),
                p.name
            );
            if nearest > worst.0 {
                worst = (nearest, p.name);
            }
        }
        worst
    }

    /// Provisions cannot strand a ship, the way a rigging ceiling cannot.
    ///
    /// This is the counterpart of `no_class_is_shut_out_of_any_market`, and it
    /// guards the same failure in the new mechanic: a class whose hold is too
    /// small to carry the food and water her own crew need to reach the next
    /// harbour would be a ship you could sail into a corner of the map and
    /// starve in, with no order left that helps.
    ///
    /// The margin is a factor of three, not a hair's breadth, because the hop
    /// measured here is the best case: a fair wind, no detour round a hunting
    /// squadron, and a player who leaves with a hold full of nothing but
    /// stores. Any real voyage carries cargo as well.
    #[test]
    fn every_class_can_provision_the_emptiest_stretch_of_sea() {
        let g = Game::new(1);
        for c in CLASSES {
            let mut s = Ship::of_class(c, GOODS.len());
            while s.fit(Upgrade::Rigging) {}
            let per_day = s.food_per_day() + s.water_per_day();
            assert!(per_day > 0, "a {} eats nothing", c.name());
            let endurance = s.capacity() / per_day;
            let (hop, where_) = worst_hop(&g, &s);
            assert!(
                endurance as f32 >= hop * 3.0,
                "a {} carries {endurance} days of stores and the worst hop, out of {where_}, is {hop:.1} days",
                c.name()
            );
        }
    }

    /// The same guarantee for the boat you actually start in: not fully rigged,
    /// not fully provisioned, and carrying what `Game::new` puts aboard.
    #[test]
    fn the_opening_boat_leaves_harbour_with_days_in_hand() {
        let g = Game::new(1);
        let (hop, where_) = worst_hop(&g, &g.ship);
        assert!(
            g.ship.days_of_stores() as f32 >= hop * 2.0,
            "she sails with {} days of stores and the worst hop, out of {where_}, is {hop:.1} days",
            g.ship.days_of_stores()
        );
    }

    /// Starving is a slow, legible loss and never a silent one.
    #[test]
    fn an_empty_hold_kills_the_crew_one_at_a_time() {
        let mut g = Game::new(1);
        g.ship.crew = 4;
        g.ship.food = 0;
        g.ship.water = 0;
        let before = g.ship.crew;
        g.advance(HOURS_PER_DAY);
        assert_eq!(g.ship.crew, before - 2, "short of both, two hands a day");
        assert!(
            g.chronicle().iter().any(|l| l.contains("lose")),
            "the chronicle says nothing about it"
        );
        g.advance(HOURS_PER_DAY);
        assert_eq!(g.ship.crew, 0);
        assert!(g.lost, "an empty ship is not lost");
    }

    /// A long order is charged for every day it covers, not once per call.
    #[test]
    fn one_order_of_several_days_eats_for_all_of_them() {
        let mut g = Game::new(1);
        g.ship.crew = 10;
        g.ship.food = 40;
        g.ship.water = 40;
        let (food, water) = (g.ship.food, g.ship.water);
        g.advance(HOURS_PER_DAY * 3.0);
        assert_eq!(g.ship.food, food - 3 * g.ship.food_per_day());
        assert_eq!(g.ship.water, water - 3 * g.ship.water_per_day());
    }

    /// Being unable to make the wage bill costs you the crew and never the ship.
    #[test]
    fn an_empty_strongbox_never_takes_the_last_hand() {
        let mut g = Game::new(1);
        g.ship.crew = g.ship.crew_max();
        g.gold = 0;
        g.ship.food = 999;
        g.ship.water = 999;
        g.advance(HOURS_PER_DAY * (DAYS_PER_MONTH as f32 + 1.0));
        assert_eq!(g.ship.crew, 1, "somebody has to steer");
        assert!(!g.lost, "a wage bill is not a shipwreck");
    }

    /// The ordinary case, which the floor test above does not reach: some of
    /// the wage bill is met and the rest walks off. Called directly rather
    /// than through a month of sailing, because the arithmetic is the point
    /// and a pirate on the way would blur it.
    #[test]
    fn a_part_paid_crew_loses_exactly_the_hands_it_cannot_pay() {
        let mut g = Game::new(1);
        g.ship.crew = 12;
        // Twelve hands at eight apiece is ninety-six, of which forty is in
        // hand. Fifty-six short buys off seven berths, rounding against you.
        g.gold = 40;
        assert_eq!(g.ship.wages(), 96);
        g.pay_wages();
        assert_eq!(g.gold, 0, "the strongbox is emptied first");
        assert_eq!(g.ship.crew, 5, "56 short at 8 a head is seven berths");
    }

    /// However bad a fight goes, it leaves somebody alive to sail home.
    #[test]
    fn a_fight_never_kills_the_whole_crew() {
        let mut g = Game::new(1);
        g.ship.crew = 2;
        g.kill_crew(50);
        assert_eq!(g.ship.crew, 1);
        assert!(!g.lost);
    }

    /// Short-handed is slow and never stopped, which is the whole reason
    /// starvation cannot become a stall.
    #[test]
    fn a_short_handed_ship_is_slower_and_still_sails() {
        let mut s = Ship::of_class(Class::Carrack, GOODS.len());
        let manned = s.base_hours();
        s.crew = 1;
        let thin = s.base_hours();
        assert!(thin > manned, "losing the crew cost her nothing");
        assert!(thin.is_finite() && thin < manned * 3.0, "she is becalmed");
        assert_eq!(s.guns_worked(), 0, "one hand fires a whole gun deck");
    }

    /// Stores are charged against the hold like anything else aboard.
    #[test]
    fn stores_take_the_same_room_as_cargo() {
        let mut s = Ship::of_class(Class::Latina, GOODS.len());
        let empty = s.free_space();
        s.water = 20;
        assert_eq!(s.free_space(), empty - 20);
        assert_eq!(s.cargo(), 0, "water is not cargo");
    }

    /// The way out of the corner: a full hold, an empty purse and nothing the
    /// market wants is still recoverable, because the chandler buys back.
    #[test]
    fn stores_can_be_sold_back_when_there_is_nothing_else_left() {
        let mut g = Game::new(1);
        g.gold = 0;
        g.ship.water = g.ship.free_space() + g.ship.water;
        assert_eq!(g.ship.free_space(), 0);
        assert!(g.provision(ship::Store::Water as i32, -10));
        assert!(g.gold > 0, "she got nothing for it");
        assert!(g.ship.free_space() > 0, "the hold is still full");
    }

    /// Hands and stores survive a change of hull, and surplus hands do not.
    #[test]
    fn a_new_hull_keeps_the_crew_it_can_berth() {
        let mut g = Game::new(1);
        g.ship.crew = g.ship.crew_max();
        g.ship.food = 5;
        g.ship.lumber = 3;
        g.gold = 500_000;
        let port = port_named("Lisbon");
        g.at = hex::from_offset(PORTS[port].col as i32, PORTS[port].row as i32);
        assert!(g.buy_ship(Class::Carrack as i32), "the yard refused");
        assert_eq!(g.ship.food, 5, "the stores went over the side");
        assert_eq!(g.ship.lumber, 3);
        assert_eq!(g.ship.crew, 12, "the hands did not come across");
        assert!(
            g.ship.crew < g.ship.crew_min(),
            "she should be undermanned and sailable, not refused"
        );
    }

    /// Lumber mends at sea; without hands it does not.
    #[test]
    fn mending_needs_both_timber_and_people() {
        let mut g = Game::new(1);
        g.ship.damage = 20;
        g.ship.lumber = 8;
        g.ship.crew = 1;
        assert!(!g.mend(), "one hand rebuilt a ship");
        g.ship.crew = g.ship.crew_min();
        assert!(g.mend());
        assert_eq!(g.ship.damage, 12, "eight lumber mends eight points");
        assert_eq!(g.ship.lumber, 0);
    }

    /// A trader alongside sells stores, and dearer than a quayside would.
    #[test]
    fn a_merchant_alongside_sells_stores_at_a_premium() {
        let mut g = Game::new(1);
        let quay = g.gold;
        assert!(g.provision(ship::Store::Food as i32, 10), "no chandler in Lisbon");
        let ashore = quay - g.gold;

        // The fleet starts alongside their quays, so sail a while and catch one
        // in open water: the premium is for the inconvenience of buying at sea.
        let mut g = Game::new(1);
        let mut days = 0;
        let afloat = loop {
            g.move_merchants(HOURS_PER_DAY);
            days += 1;
            let found = g.merchants.iter().map(|m| m.at).find(|&h| {
                let i = hex::index(h).unwrap();
                !PORTS
                    .iter()
                    .any(|p| hex::index(hex::from_offset(p.col as i32, p.row as i32)) == Some(i))
            });
            if let Some(h) = found {
                break h;
            }
            assert!(days < 20, "no trader ever left harbour");
        };
        g.at = afloat;
        let purse = g.gold;
        assert!(g.provision(ship::Store::Food as i32, 10), "she would not sell");
        assert!(
            purse - g.gold > ashore,
            "a trader at sea sold at the quayside price"
        );
    }

    /// No class ceiling strands a market.
    ///
    /// This is the way ship classes could quietly brick the game: a hull whose
    /// rigging cannot be worked up far enough to round some cape, leaving
    /// harbours it can never reach and, worse, a harbour it can enter but not
    /// leave. Fully rigged, every class in the game reaches every port that
    /// trades, so the choice of hull is a choice about speed, hold and guns
    /// rather than about which half of the world you are allowed to see.
    ///
    /// This is about rigging and offing only. Draught is a separate limit with
    /// a deliberately weaker guarantee, since a rule that barred nothing would
    /// not be a rule; see `every_economy_keeps_a_harbour_for_the_deepest_hulls`.
    #[test]
    fn no_class_is_shut_out_of_any_market() {
        let g = Game::new(1);
        for c in CLASSES {
            let mut s = crate::ship::Ship::of_class(c, GOODS.len());
            while s.fit(Upgrade::Rigging) {}
            let missed = ports_outside(&g, s.bluewater_rating());
            assert!(
                missed.is_empty(),
                "a fully rigged {} cannot reach {:?}",
                c.name(),
                missed
            );
        }
    }

    /// The draught rule bars somewhere, and never everywhere.
    ///
    /// Two claims, and both are load-bearing in opposite directions. A rule
    /// that shut no harbour would be a paragraph of comment doing nothing, and
    /// a rule that shut a whole economy would make the largest hulls a trap
    /// rather than a trade-off: the galleon is the ship you buy to cross an
    /// ocean, and arriving to find nowhere to sell is not a choice you made.
    ///
    /// Two open harbours per economy rather than one, because one would leave a
    /// region with a single quayside where every price you move is a price you
    /// then have to trade against.
    #[test]
    fn every_economy_keeps_a_harbour_for_the_deepest_hulls() {
        let g = Game::new(1);
        let mut open = [0; 8];
        let mut barred = 0;
        for (i, p) in PORTS.iter().enumerate() {
            if !Markets::trades(i) {
                continue;
            }
            let h = hex::from_offset(p.col as i32, p.row as i32);
            if g.shallow_harbour[hex::index(h).unwrap()] {
                assert!(!p.capital, "{} is a capital and must never be barred", p.name);
                barred += 1;
            } else {
                open[p.econ as usize] += 1;
            }
        }
        assert!(barred > 0, "no harbour is shallow, so the rule does nothing");
        for (e, n) in open.iter().enumerate() {
            assert!(
                *n >= 2,
                "{} keeps only {n} harbour(s) open to a deep hull",
                crate::world::ECONOMIES[e]
            );
        }
    }

    /// A deep hull is turned back at the harbour mouth, and a shallow one is not.
    ///
    /// Driven through `step` rather than by reading the mask, because the mask
    /// being right and the refusal being wired are two different facts and only
    /// the second one is what a player meets.
    #[test]
    fn a_deep_draught_hull_is_refused_where_a_coastal_one_is_not() {
        let mut g = Game::new(1);
        let (approach, dir) = (0..PORTS.len())
            .filter_map(|i| {
                let h = hex::from_offset(PORTS[i].col as i32, PORTS[i].row as i32);
                let idx = hex::index(h)?;
                if !g.shallow_harbour[idx] {
                    return None;
                }
                // Approach from any water hex beside her.
                (0..6).find_map(|d| {
                    let n = hex::normalise(hex::neighbour(h, d));
                    let ni = hex::index(n)?;
                    if nav::is_land_index(ni) || g.shallow_harbour[ni] {
                        return None;
                    }
                    // The direction that takes her back in.
                    let back = (0..6)
                        .find(|&b| hex::normalise(hex::neighbour(n, b)) == hex::normalise(h))?;
                    Some((n, back))
                })
            })
            .next()
            .expect("no shallow harbour with water beside it");

        g.at = approach;
        g.ship = Ship::of_class(Class::Balsa, GOODS.len());
        assert!(g.step(dir), "a coastal hull may enter");

        g.at = approach;
        g.ship = Ship::of_class(Class::Galleon, GOODS.len());
        assert!(!g.step(dir), "a galleon may not");
        assert!(
            g.chronicle().last().unwrap().contains("draught"),
            "she was refused without being told why: {:?}",
            g.chronicle().last()
        );
    }

    /// A course clicked on the chart names the harbour it is refused by.
    ///
    /// `set_course_hex` knows only a hex, so it used to hand the refusal the
    /// words "the open sea" whatever it was pointed at. A port is a water cell,
    /// so clicking a shallow harbour produced "the open sea is too shallow for
    /// her draught": true of nothing, and no help at all to someone deciding
    /// which hull to buy.
    #[test]
    fn a_course_clicked_on_a_shallow_harbour_names_it() {
        let mut g = Game::new(1);
        let port = (0..PORTS.len())
            .find(|&i| {
                let h = hex::from_offset(PORTS[i].col as i32, PORTS[i].row as i32);
                hex::index(h).is_some_and(|idx| g.shallow_harbour[idx]) && h != g.at
            })
            .expect("no shallow harbour");
        let h = hex::from_offset(PORTS[port].col as i32, PORTS[port].row as i32);
        g.fog[hex::index(h).unwrap()] = nav::REMEMBERED;
        g.ship = Ship::of_class(Class::Galleon, GOODS.len());

        assert!(!g.set_course_hex(PORTS[port].col as i32, PORTS[port].row as i32));
        let said = g.chronicle().last().unwrap().clone();
        assert!(said.contains(PORTS[port].name), "she was refused by an unnamed place: {said:?}");
        assert!(!said.contains("open sea"), "still the old wording: {said:?}");
    }

    /// The boat you start in is the one exception, and it is the point of her.
    ///
    /// A Balsa off the stocks holds the coast at one hex, which is enough for
    /// the old world and not enough for the Atlantic. Working her rigging up a
    /// single tier opens the whole map, so the first crossing is something you
    /// earn in the first hour rather than a wall. Sailing beyond the rating is
    /// allowed in any case; see `ship::Ship::bluewater_rating`.
    #[test]
    fn the_opening_boat_can_work_the_old_world_but_not_yet_cross() {
        let g = Game::new(1);
        let new = crate::ship::Ship::of_class(Class::Balsa, GOODS.len());
        let missed = ports_outside(&g, new.bluewater_rating());
        assert!(!missed.is_empty(), "nothing is left to sail for");
        for name in &missed {
            assert_eq!(
                crate::world::ECONOMIES[PORTS[port_named(name)].econ as usize],
                "Americas",
                "{name} is out of reach and is not across an ocean"
            );
        }
    }

    #[test]
    fn every_economy_has_a_capital_and_no_landfall_does() {
        let mut seen = [false; 8];
        for p in PORTS.iter() {
            if p.capital {
                assert!(p.econ >= 0, "{} is a capital but does not trade", p.name);
                seen[p.econ as usize] = true;
            }
        }
        for (i, ok) in seen.iter().enumerate() {
            assert!(*ok, "{} has nowhere to buy a ship", crate::world::ECONOMIES[i]);
        }
    }

    #[test]
    fn only_a_capital_has_a_shipyard() {
        let g = Game::new(3);
        for (i, p) in PORTS.iter().enumerate() {
            assert_eq!(
                !g.yard(i).is_empty(),
                p.capital,
                "{} disagrees with its own capital flag",
                p.name
            );
        }
    }

    /// The regional answer, asserted rather than described. Lisbon is a
    /// Mediterranean yard and builds the lateen craft; London is a northern one
    /// and does not. If the tables in `ship.rs` are ever flattened into one list
    /// this is what notices.
    #[test]
    fn a_yard_sells_only_what_its_own_region_builds() {
        let g = Game::new(3);
        let names = |port: usize| -> Vec<&'static str> {
            g.yard(port).iter().map(|o| o.class.name()).collect()
        };
        let lisbon = names(port_named("Lisbon"));
        let london = names(port_named("London"));
        assert!(lisbon.contains(&"Latina"), "the Mediterranean builds lateen craft");
        assert!(!london.contains(&"Latina"), "the north does not");
        assert!(london.contains(&"Galleon"), "the north builds ocean ships");
        assert!(!lisbon.contains(&"Galleon"), "and Lisbon does not");
        assert_ne!(lisbon, london, "two yards should not stock the same shelf");
    }

    #[test]
    fn the_biggest_hulls_need_the_port_invested_in() {
        let mut g = Game::new(3);
        let london = port_named("London");
        g.gold = 1_000_000;
        let locked = |g: &Game| {
            g.yard(london)
                .into_iter()
                .find(|o| o.class == Class::Galleon)
                .expect("London builds galleons")
                .locked
        };
        assert!(locked(&g).is_some(), "money alone should not buy a galleon");
        while g.markets.invested_of(london) < Class::Galleon.spec().needs_invested {
            let cost = g.markets.investment_cost(london).expect("more to invest in");
            g.markets.invest(london, cost);
        }
        assert!(locked(&g).is_none(), "an invested partner should get her galleon");
    }

    #[test]
    fn buying_a_ship_carries_the_cargo_across() {
        let mut g = Game::new(3);
        g.gold = 1_000_000;
        g.ship.hold[0] = 20;
        g.ship.paid[0] = 400;
        let lisbon = port_named("Lisbon");
        g.at = hex::from_offset(PORTS[lisbon].col as i32, PORTS[lisbon].row as i32);
        assert!(g.buy_ship(Class::Latina.index() as i32));
        assert_eq!(g.ship.class, Class::Latina);
        assert_eq!(g.ship.hold[0], 20, "the cargo was left on the quay");
        assert_eq!(g.ship.paid[0], 400, "what it cost was forgotten");
    }

    #[test]
    fn the_yard_refuses_a_hull_too_small_for_the_hold() {
        let mut g = Game::new(3);
        g.gold = 1_000_000;
        let lisbon = port_named("Lisbon");
        g.at = hex::from_offset(PORTS[lisbon].col as i32, PORTS[lisbon].row as i32);
        assert!(g.buy_ship(Class::Carrack.index() as i32));
        g.ship.hold[0] = g.ship.capacity();
        let before = g.gold;
        assert!(!g.buy_ship(Class::Balsa.index() as i32), "should refuse the downgrade");
        assert_eq!(g.ship.class, Class::Carrack);
        assert_eq!(g.gold, before, "a refused sale should cost nothing");
    }

    #[test]
    fn a_ship_cannot_be_bought_where_there_is_no_yard() {
        let mut g = Game::new(3);
        g.gold = 1_000_000;
        let seville = port_named("Seville");
        assert!(!PORTS[seville].capital);
        g.at = hex::from_offset(PORTS[seville].col as i32, PORTS[seville].row as i32);
        assert!(!g.buy_ship(Class::Latina.index() as i32));
        assert_eq!(g.ship.class, Class::Balsa);
    }

    /// The player starts in the smallest hull there is, so the first shipyard
    /// visit is a real decision. If that ever stops being true, the opening
    /// purse and the price of a Latina both need looking at again.
    #[test]
    fn the_voyage_begins_in_a_boat_that_cannot_cross_an_ocean() {
        let g = Game::new(1);
        assert_eq!(g.ship.class, Class::Balsa);
        assert!(!g.ship.class.is_bluewater());
    }

    /// Reachability, which is the one way this feature could quietly brick the
    /// game: a blue-water hull the player can never legally arrive at.
    #[test]
    fn a_blue_water_yard_is_reachable_by_a_coasting_ship() {
        let g = Game::new(1);
        let coastal = crate::ship::Ship::of_class(Class::Carrack, GOODS.len()).bluewater_rating();
        for (i, p) in PORTS.iter().enumerate() {
            if !p.capital {
                continue;
            }
            let sells_blue = g.yard(i).iter().any(|o| o.class.is_bluewater());
            if !sells_blue {
                continue;
            }
            assert!(
                g.depth[hex::index(hex::from_offset(p.col as i32, p.row as i32)).unwrap()]
                    <= coastal as u8,
                "{} sells ocean ships but lies beyond a coasting ship's offing",
                p.name
            );
        }
    }

    #[test]
    fn a_new_game_starts_in_a_port_and_can_see_out_of_it() {
        let g = Game::new(1);
        assert!(g.port_here().is_some(), "the voyage does not start in a port");
        let lit = g.fog.iter().filter(|v| **v == VISIBLE).count();
        assert!(lit > 1, "the ship is blind in harbour");
        let dark = g.fog.iter().filter(|v| **v == UNSEEN).count();
        assert!(dark > CELLS / 2, "the chart starts too full");
    }

    /// The opening used to chart eight harbours from the quayside at Lisbon,
    /// one line after the log announced that the chart was blank, because port
    /// discovery rode on the fog horizon and a hex is five degrees across.
    #[test]
    fn a_new_game_charts_only_its_own_neighbourhood() {
        for seed in 1..40u32 {
            let g = Game::new(seed);
            // The real invariant is the distance one below. This cap is only a
            // tripwire, and it is not 1 or 2 because western Europe genuinely
            // is that crowded at five degrees to the hex: five harbours lie
            // within two hexes of Lisbon, and a master sailing out of Lisbon
            // would know all five. The bug was charting Naples, not Seville.
            let charted = g.discovered.iter().filter(|d| **d).count();
            assert!(
                charted <= 8,
                "seed {seed} charted {charted} ports before the ship had moved"
            );
            for (i, p) in PORTS.iter().enumerate() {
                if !g.discovered[i] {
                    continue;
                }
                let h = hex::from_offset(p.col as i32, p.row as i32);
                assert!(
                    hex::wrapped_distance(g.at, h) <= CHART_RANGE,
                    "{} was charted from {} hexes away",
                    p.name,
                    hex::wrapped_distance(g.at, h)
                );
            }
        }
    }

    /// Raiders keep off the coastal shelf. Without this an unarmed trader loses
    /// a third of her gold per boarding often enough that three thousand gold
    /// compounds down to single figures over an ordinary voyage.
    /// The claim is about *chasing*, and it is checked on every tick rather than
    /// once at the end, because "not hunting when the music stopped" is a much
    /// weaker statement than "never hunted at all".
    ///
    /// It used to assert, on one seed, that the player's gold was untouched, and
    /// that assertion was quietly unsound. A pirate that is not hunting wanders
    /// at random, and a random walk of well over a hundred steps starting one hex
    /// away will sometimes blunder onto the player and board them. It passed only
    /// because that seed happened to walk the other way, and it broke the moment
    /// an unrelated change shifted the generator stream. A blunder is not a
    /// chase. So the gold check is gone and the chase check is absolute: eleven
    /// seeds, every tick, no raider ever hunts and no raider ever remembers.
    /// That is stronger than what it replaced, which checked one seed once.
    ///
    /// Losing the gold line loses nothing real. This setup parks a raider one hex
    /// from a ship that then sits still for six months of game time, and four
    /// seeds in eleven end in a boarding. That is not the shelf failing, it is a
    /// random walk of a hundred and forty steps starting adjacent, and a
    /// stationary trader moored beside a pirate deserves what she gets. The
    /// playability claim in the paragraph above rests on nobody giving chase,
    /// which is what is actually asserted here.
    #[test]
    fn pirates_do_not_hunt_a_ship_on_the_shelf() {
        for seed in 1..12u32 {
            let mut g = Game::new(seed);
            assert!(
                g.offshore() <= SHELF,
                "a port should sit on the shelf, not in the offing"
            );
            g.pirates.clear();
            // Well inside HUNT_RANGE, and given plenty of time to close.
            g.pirates.push(Pirate {
                at: hex::normalise(hex::neighbour(g.at, 0)),
                class: Class::Carrack,
                strength: 10,
                hours: 0.0,
                hunting: true,
                name: "the Test Sail",
                last_seen: None,
                memory: 0,
                beaten: false,
                met: false,
                navy: false,
            });
            for _ in 0..40 {
                g.move_pirates(24.0);
                if g.pirates.is_empty() {
                    break;
                }
                assert!(!g.pirates[0].hunting, "seed {seed}: a raider gave chase inshore");
                assert!(
                    g.pirates[0].last_seen.is_none(),
                    "seed {seed}: a raider inshore remembered where the player was"
                );
            }
        }
    }

    /// Two pirates on one hex used to be an out-of-bounds index, because a won
    /// fight removes an element and the queued indices above it went stale.
    /// Under `panic = "abort"` that is a blank page and a console trap, so it
    /// gets a test of its own rather than a comment.
    /// The chart says how many, not just what.
    ///
    /// This is the bug the stack codes exist for: the status column counts
    /// hulls and the chart draws hexes, so before these a reader who counted
    /// the glyphs got a smaller number than the one printed beside them and had
    /// no way to tell which was lying.
    #[test]
    fn a_crowded_hex_draws_a_different_mark_from_a_lone_one() {
        let mut g = Game::new(1);
        let water = (0..6)
            .map(|d| hex::normalise(hex::neighbour(g.at, d)))
            .find(|&h| hex::index(h).is_some_and(|i| !nav::is_land_index(i)))
            .expect("nowhere to float a trader");
        let idx = hex::index(water).unwrap();
        let trader = |at| Merchant {
            at,
            class: Class::Carrack,
            bound: 0,
            cargo: 0,
            units: 1,
            gold: 100,
            hours: 0.0,
            stuck: 0,
        };

        g.merchants.clear();
        g.pirates.clear();
        g.look();
        assert_eq!(g.fog[idx], VISIBLE, "the test hex is not in sight");

        g.merchants.push(trader(water));
        assert_eq!(g.render()[idx * STRIDE + AT_MARK], CODE_MERCHANT);

        g.merchants.push(trader(water));
        assert_eq!(g.render()[idx * STRIDE + AT_MARK], CODE_MERCHANTS, "two traders, one mark");

        // A raider over them is still the news, and alone until a second joins.
        g.pirates.push(Pirate {
            at: water,
            class: Class::Carrack,
            strength: 10,
            hours: 0.0,
            hunting: true,
            name: "the Test Sail",
            last_seen: None,
            memory: 0,
            beaten: false,
            met: false,
            navy: false,
        });
        assert_eq!(g.render()[idx * STRIDE + AT_MARK], CODE_PIRATE, "a raider over traders is one raider");
    }

    #[test]
    fn two_pirates_on_the_same_hex_do_not_break_the_engagement() {
        for seed in 1..40u32 {
            let mut g = Game::new(seed);
            // Armed, so `fight` can take the branch that removes a pirate, and
            // rich enough that losing does not end the run early.
            g.ship.guns = 3;
            g.gold = 100_000;
            g.pirates.clear();
            for _ in 0..3 {
                g.pirates.push(Pirate {
                    at: g.at,
                    class: Class::Carrack,
                    strength: 10,
                    hours: 0.0,
                    hunting: true,
                    name: "the Test Sail",
                    last_seen: None,
                    memory: 0,
                    beaten: false,
                    met: false,
                    navy: false,
                });
            }
            g.move_pirates(0.0);
            assert!(g.pirates.len() <= 3);
        }
    }

    /// Put the player well out in the offing with one raider inside her hunting
    /// range but not on top of her. Used by the memory tests below, which need
    /// the same setup and differ only in what happens next.
    ///
    /// The gap matters. An adjacent raider closes on the first step and the
    /// engagement scatters her and wipes the very memory under test, so the
    /// spacing here is what makes these tests about seeing rather than fighting.
    fn a_raider_that_has_seen_you(seed: u32) -> Option<Game> {
        const GAP: i32 = 3;
        let mut g = Game::new(seed);
        g.pirates.clear();
        g.merchants.clear();

        // Somewhere well off the land, so the SHELF clause does not veto the
        // chase before the memory is ever exercised.
        let deep = |g: &Game, h: Hex| {
            hex::index(h).is_some_and(|i| {
                !nav::is_land_index(i) && i32::from(g.depth[i]) > SHELF + 2
            })
        };
        let mut here = None;
        for i in 0..nav::CELLS {
            let h = hex::from_offset(
                i as i32 % crate::world::COLS,
                i as i32 / crate::world::COLS,
            );
            if deep(&g, h) {
                here = Some(h);
                break;
            }
        }
        g.at = here?;

        let mut spot = None;
        for i in 0..nav::CELLS {
            let h = hex::from_offset(
                i as i32 % crate::world::COLS,
                i as i32 / crate::world::COLS,
            );
            if hex::wrapped_distance(h, g.at) == GAP && deep(&g, h) {
                spot = Some(h);
                break;
            }
        }
        g.pirates.push(Pirate {
            at: spot?,
            class: Class::Carrack,
            strength: 10,
            hours: 0.0,
            hunting: false,
            name: "the Test Sail",
            last_seen: None,
            memory: 0,
            beaten: false,
            met: false,
            navy: false,
        });
        g.gold = 1_000_000;
        g.ship.guns = 0;
        Some(g)
    }

    /// The heart of it. A raider that loses sight of you keeps standing toward
    /// where you were instead of forgetting you the instant you cross the
    /// horizon, which is what she did before.
    #[test]
    fn a_raider_remembers_where_she_last_saw_you() {
        let Some(mut g) = a_raider_that_has_seen_you(3) else { return };
        g.move_pirates(7.0);
        assert!(g.pirates[0].hunting, "she never picked the player up at all");
        let remembered = g.pirates[0].last_seen.expect("nothing was remembered");
        assert_eq!(remembered, g.at, "she remembered somewhere the player was not");

        // Vanish. Teleporting the player is not something the game can do, which
        // is exactly why it is the right way to test the memory in isolation:
        // it separates "she cannot see him" from "he sailed away".
        g.at = hex::from_offset(0, 0);
        g.pirates[0].hours = 0.0;
        g.move_pirates(7.0);
        assert!(
            g.pirates[0].hunting,
            "she gave up the moment the player was out of sight"
        );
        assert_eq!(
            g.pirates[0].last_seen,
            Some(remembered),
            "she is chasing somewhere other than where she last saw him"
        );
    }

    /// And the other half: memory is finite. A chase that finds nothing ends.
    #[test]
    fn a_chase_that_finds_nothing_is_given_up() {
        let Some(mut g) = a_raider_that_has_seen_you(4) else { return };
        g.move_pirates(7.0);
        assert!(g.pirates[0].hunting);
        g.at = hex::from_offset(0, 0);
        // Well past PIRATE_MEMORY steps' worth of hours.
        for _ in 0..(PIRATE_MEMORY + 4) {
            g.pirates[0].hours = 0.0;
            g.move_pirates(7.0);
        }
        assert!(!g.pirates[0].hunting, "she is still chasing a ship long gone");
        assert_eq!(g.pirates[0].last_seen, None, "she never forgot the spot");
    }

    /// Notoriety, which is what makes the reputation number a mechanic rather
    /// than a line in the status column: the same raider in the same water picks
    /// the player up from further off or nearer depending on what she has heard.
    #[test]
    fn reputation_changes_how_far_off_a_raider_picks_you_up() {
        let Some(mut g) = a_raider_that_has_seen_you(5) else { return };
        g.reputation = 0;
        let neutral = g.hunt_range(&g.pirates[0]);

        g.reputation = reputation::CEILING;
        let feared = g.hunt_range(&g.pirates[0]);
        g.reputation = reputation::FLOOR;
        let marked = g.hunt_range(&g.pirates[0]);

        assert!(feared < neutral, "a pirate-killer is hunted just as hard");
        assert!(marked > neutral, "a known raider is no more of a target");

        g.reputation = 0;
        g.pirates[0].beaten = true;
        assert!(
            g.hunt_range(&g.pirates[0]) < neutral,
            "one that has already lost to the player is no shyer for it"
        );
    }

    /// Beating a raider has to leave somebody alive to remember it, or the
    /// reputation loop has no one to read it and the ocean drains over a long
    /// game. Most wins drive her off; only a decisive one sinks her.
    #[test]
    fn most_wins_drive_a_raider_off_rather_than_sinking_her() {
        let (mut sunk, mut driven) = (0, 0);
        for seed in 1..80u32 {
            let mut g = Game::new(seed);
            g.gold = 1_000_000;
            g.ship.guns = 4; // top tier, so wins are common enough to count
            g.ship.damage = 0;
            let before = g.pirates.len();
            g.fight(0, 10);
            if g.lost {
                continue;
            }
            if g.pirates.len() < before {
                sunk += 1;
            } else if g.pirates[0].beaten {
                driven += 1;
                assert!(g.reputation > 0, "driving one off earned nothing");
            }
        }
        assert!(sunk > 0, "no fight in eighty ever sank a raider");
        assert!(driven > sunk, "sinkings outnumber chases broken off: {sunk} to {driven}");
    }

    /// The fleet is spawned once and never replenished, so if every win removed
    /// a pirate a successful game would end in an empty ocean. This is the test
    /// that stops that regressing.
    ///
    /// A third rather than most of them, and the measurement is honest about
    /// why: this is sixty consecutive fights at the top gun tier against the
    /// weakest raiders in the game, which no real voyage looks like, and about a
    /// quarter of wins are decisive. Even that leaves eleven sail at large.
    /// Before the split it would have left none.
    #[test]
    fn a_long_run_of_victories_does_not_empty_the_ocean() {
        let mut g = Game::new(21);
        g.gold = 1_000_000;
        g.ship.guns = 4;
        for _ in 0..60 {
            if g.pirates.is_empty() || g.lost {
                break;
            }
            g.ship.damage = 0;
            g.fight(0, 8);
        }
        assert!(
            g.pirates.len() >= PIRATE_COUNT / 3,
            "sixty fights left only {} raiders of {PIRATE_COUNT}",
            g.pirates.len()
        );
    }

    /// Reputation up for pirates, down for merchants, and the second one does
    /// not need the attack to succeed.
    #[test]
    fn firing_on_a_merchant_costs_your_standing_win_or_lose() {
        let mut wins = 0;
        let mut losses = 0;
        for seed in 1..60u32 {
            let mut g = Game::new(seed);
            g.ship.guns = 3;
            g.ship.damage = 0;
            let Some(idx) = (0..g.merchants.len()).next() else { continue };
            g.at = g.merchants[idx].at;
            let before = g.reputation;
            let gold_before = g.gold;
            if !g.attack() {
                continue;
            }
            assert_eq!(
                g.reputation,
                before + reputation::RAID_MERCHANT,
                "seed {seed}: the attack was free"
            );
            if g.gold > gold_before {
                wins += 1;
            } else {
                losses += 1;
            }
        }
        assert!(wins > 0 && losses > 0, "raiding is not a gamble: {wins} won, {losses} lost");
    }

    /// An unarmed ship cannot commit the crime, and neither can one with nobody
    /// alongside. Both refusals have to leave the standing alone.
    #[test]
    fn an_attack_that_cannot_happen_costs_nothing() {
        let mut g = Game::new(30);
        g.merchants.clear();
        assert!(!g.attack(), "fired on an empty sea");
        assert_eq!(g.reputation, 0);

        let mut g = Game::new(31);
        g.ship.guns = 0;
        if !g.merchants.is_empty() {
            g.at = g.merchants[0].at;
            assert!(!g.attack(), "an unarmed ship raided somebody");
            assert_eq!(g.reputation, 0);
        }
    }

    /// Merchants are the reason reputation can fall, so they have to actually be
    /// out there and actually be moving. A fleet that spawns and then sits in
    /// harbour is a fleet nobody ever meets.
    #[test]
    fn the_merchant_fleet_sails() {
        let mut g = Game::new(40);
        assert_eq!(g.merchants.len(), MERCHANT_COUNT);
        let before: Vec<Hex> = g.merchants.iter().map(|m| m.at).collect();
        for _ in 0..20 {
            g.move_merchants(24.0);
        }
        let moved = g
            .merchants
            .iter()
            .zip(&before)
            .filter(|(m, was)| m.at != **was)
            .count();
        assert!(
            moved > MERCHANT_COUNT / 2,
            "only {moved} of {MERCHANT_COUNT} traders ever left harbour"
        );
        for m in &g.merchants {
            let i = hex::index(m.at).expect("a trader sailed off the world");
            assert!(!nav::is_land_index(i), "a trader is aground");
        }
    }

    /// The crown answers an offence rather than a score in the abstract: the
    /// fleet has to appear as a consequence of something the player did, on the
    /// turn they did it, not at some later monthly tick.
    #[test]
    fn raiding_traders_brings_the_navy_out() {
        let mut raided = 0;
        for seed in 1..40u32 {
            let mut g = Game::new(seed);
            g.ship.guns = 3;
            g.ship.damage = 0;
            g.gold = 100_000;
            if g.merchants.is_empty() {
                continue;
            }
            // First offence. A mistake, and nobody is sent.
            g.at = g.merchants[0].at;
            if !g.attack() || g.lost {
                continue;
            }
            assert_eq!(g.navy_out(), 0, "seed {seed}: one raid brought the fleet out");

            // Second. A habit, and it is answered.
            let Some(next) = (0..g.merchants.len()).find(|&i| g.merchants[i].units > 0) else {
                continue;
            };
            g.at = g.merchants[next].at;
            if !g.attack() || g.lost {
                continue;
            }
            assert!(g.navy_out() >= 1, "seed {seed}: the second raid went unanswered");
            raided += 1;
        }
        assert!(raided > 10, "only {raided} runs got as far as a second raid");
    }

    /// And the fleet has to grow with the score rather than sitting at one.
    #[test]
    fn the_worse_it_gets_the_more_of_them_come() {
        let mut g = Game::new(50);
        let mut seen = Vec::new();
        for score in [0, -20, -40, -60, -80, -100] {
            g.reputation = score;
            g.enforce();
            seen.push(g.navy_out());
        }
        assert_eq!(seen[0], 0, "an honest master was hunted");
        for w in seen.windows(2) {
            assert!(w[1] >= w[0], "the fleet shrank as the player got worse: {seen:?}");
        }
        assert!(
            *seen.last().unwrap() > seen[1],
            "the worst score brought no more ships than a middling one: {seen:?}"
        );
        assert_eq!(*seen.last().unwrap(), reputation::NAVY_MAX, "the cap is not reached");
    }

    /// A whole squadron alongside at once, which is the case where the recall
    /// and the engagement loop are both touching `pirates` on the same tick.
    ///
    /// Standing trial halves the score, and halving it from the floor recalls
    /// three of the five ships. If that recall happened while the loop was
    /// still holding indices into the vector, the ships left to fight would be
    /// at indices that no longer mean what they meant, and with `panic =
    /// "abort"` the page would simply stop. Every seed here reaches the
    /// squadron, so a regression is a failure rather than a run that quietly
    /// never got there.
    #[test]
    fn a_squadron_alongside_at_once_does_not_lose_its_place() {
        for seed in 0..40 {
            let mut g = Game::new(seed);
            g.reputation = reputation::FLOOR;
            g.enforce();
            let out = g.navy_out();
            assert_eq!(out, reputation::NAVY_MAX, "seed {seed}: the squadron is short");

            // Alongside, all of them, and armed enough that either outcome of
            // the arrest is reachable.
            g.ship.guns = 2;
            let at = g.at;
            for p in g.pirates.iter_mut().filter(|p| p.navy) {
                p.at = at;
                p.hours = 100.0;
            }
            g.move_pirates(0.0);

            if g.lost {
                continue;
            }
            assert_eq!(
                g.navy_out(),
                reputation::navy_wanted(g.reputation),
                "seed {seed}: the fleet does not match the score after the boarding"
            );
        }
    }

    /// The recall matters as much as the summons. Without it the first two
    /// merchants taken would decide the rest of the game.
    #[test]
    fn mending_your_name_sends_them_home() {
        let mut g = Game::new(51);
        g.reputation = reputation::FLOOR;
        g.enforce();
        assert!(g.navy_out() > 0, "nobody was sent for a man at the floor");
        g.reputation = 0;
        g.enforce();
        assert_eq!(g.navy_out(), 0, "they stayed out after the name was clear");
    }

    /// The real punishment for piracy is not the number, it is that the coast
    /// stops being safe. Raiders leave the shelf alone; the navy is there
    /// precisely because that is where you would otherwise hide.
    #[test]
    fn the_navy_works_the_coast_that_raiders_leave_alone() {
        let mut g = Game::new(52);
        assert!(g.offshore() <= SHELF, "a port should sit on the shelf");
        g.reputation = reputation::FLOOR;
        g.pirates.clear();
        g.merchants.clear();
        g.pirates.push(Pirate {
            at: hex::normalise(hex::neighbour(g.at, 0)),
            class: Class::Carrack,
            strength: 40,
            hours: 0.0,
            hunting: false,
            name: "the Test Frigate",
            last_seen: None,
            memory: 0,
            beaten: false,
            met: false,
            navy: true,
        });
        g.ship.guns = 0;
        g.move_pirates(7.0);
        assert!(
            g.pirates.is_empty() || g.pirates[0].hunting || g.pirates[0].last_seen.is_some(),
            "a king's ship ignored a wanted man in her own harbour approaches"
        );
    }

    /// Fighting clear of the crown has to be worse than being taken by it, or
    /// the fleet is a nuisance rather than a consequence and the way back is
    /// never worth taking.
    #[test]
    fn resisting_the_crown_costs_more_than_answering_for_it() {
        let mut fought = 0;
        let mut taken = 0;
        for seed in 60..120u32 {
            let mut g = Game::new(seed);
            g.reputation = -50;
            g.gold = 100_000;
            g.ship.damage = 0;
            g.ship.guns = 4;
            g.pirates.clear();
            g.pirates.push(Pirate {
                at: g.at,
                class: Class::Carrack,
                strength: 40,
                hours: 0.0,
                hunting: true,
                name: "the Test Frigate",
                last_seen: None,
                memory: 0,
                beaten: false,
                met: false,
                navy: true,
            });
            let before = g.reputation;
            g.arrest(0, 40);
            if g.lost {
                continue;
            }
            if g.reputation < before {
                fought += 1;
            } else if g.reputation > before {
                taken += 1;
                assert!(g.gold < 100_000, "seed {seed}: the court charged nothing");
            }
        }
        assert!(fought > 0 && taken > 0, "one outcome never happened: {fought} fought, {taken} taken");
    }

    /// Fame is maintained rather than banked. Without the fade every long game
    /// pins at one extreme and nothing reads the number differently again.
    #[test]
    fn a_standing_fades_if_it_is_not_kept_up() {
        let mut g = Game::new(41);
        g.reputation = 40;
        // A year of months.
        g.advance(HOURS_PER_DAY * DAYS_PER_MONTH as f32 * 12.0);
        assert!(g.reputation < 40, "a reputation earned once lasted for ever");
        assert!(g.reputation >= 0, "the fade overshot past nothing");
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
            .find(|&x| g.markets.buy_price(port, x).is_some() && g.markets.is_open(port, x))
            .expect("the starting port sells nothing at all");
        let gold_before = g.gold;
        assert!(g.buy(good, 5));
        assert_eq!(g.ship.hold[good], 5);
        assert!(g.gold < gold_before);
    }

    /// The opening move, checked as a whole rather than as its parts. Gating
    /// the market is the one change that could make the first five minutes
    /// worse, and a port that opens three goods a player cannot afford is the
    /// same dead start as a port that opens none.
    #[test]
    fn the_first_port_is_playable_on_the_opening_purse() {
        let g = Game::new(4);
        let port = g.port_here().expect("the voyage does not start in a port");
        let open: Vec<usize> = (0..GOODS.len())
            .filter(|&x| g.markets.is_open(port, x))
            .collect();
        assert!(open.len() >= 2, "the first port opens almost nothing");
        assert!(
            open.iter()
                .any(|&x| g.markets.buy_price(port, x).unwrap() <= g.gold),
            "nothing the first port opens is affordable on the opening purse"
        );
    }

    #[test]
    fn buying_a_shut_good_is_refused_and_investing_opens_one() {
        let mut g = Game::new(4);
        let port = g.port_here().unwrap();
        let shut = (0..GOODS.len())
            .find(|&x| g.markets.buy_price(port, x).is_some() && !g.markets.is_open(port, x))
            .expect("the first port holds nothing back");
        g.gold = 1_000_000;
        assert!(!g.buy(shut, 1), "a shut good was sold anyway");
        assert_eq!(g.ship.hold[shut], 0);

        let before = g.markets.open_count(port);
        assert!(g.invest());
        assert_eq!(g.markets.open_count(port), before + 1);
        assert!(g.markets.favour_of(port) > 0, "investing earned no standing");
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
            .find(|&x| g.markets.buy_price(port, x).is_some() && g.markets.is_open(port, x))
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
        assert_eq!(buf[at * STRIDE + AT_MARK], CODE_SHIP);
        assert_eq!(buf[at * STRIDE + AT_FOG], VISIBLE);
        assert_eq!(buf.len(), CELLS * STRIDE);
    }

    /// The point of the third byte: a mark never erases the sea under it.
    ///
    /// The ship is the easiest case to reach from a test, and it is also the one
    /// that matters most, because "what am I floating on" is the question every
    /// course decision starts from.
    #[test]
    fn the_terrain_byte_survives_whatever_stands_on_it() {
        let mut g = Game::new(10);
        let at = hex::index(g.at).unwrap();
        let buf = g.render();
        assert_eq!(buf[at * STRIDE + AT_MARK], CODE_SHIP);

        // Every hex, everywhere: the terrain channel only ever holds terrain, so
        // the hex under the ship still reports the water she is floating in.
        for i in 0..CELLS {
            assert!(
                matches!(
                    buf[i * STRIDE + AT_TERRAIN],
                    CODE_SHALLOW | CODE_DEEP | CODE_LAND
                ),
                "cell {i} terrain byte held {}",
                buf[i * STRIDE + AT_TERRAIN]
            );
        }
    }
}

