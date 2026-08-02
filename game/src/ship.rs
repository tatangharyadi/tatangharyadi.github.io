//! Ships: what class of vessel you are in, and the three things you spend money
//! on to make that vessel better.
//!
//! There are two decisions here and they are deliberately different in kind.
//!
//! **What you buy at the yard is a class.** A Latina is not a small Galleon. It
//! is a different hull with a different rig, built in different places, and no
//! amount of money turns one into the other. A class fixes the *ceilings*: how
//! big the hull can ever be, how far offshore the rig can ever take you, how
//! many guns the deck will ever bear. Outgrowing a class is the reason to go
//! back to a shipyard.
//!
//! **What you buy at the shipwright is a tier**, and this is the older system,
//! unchanged. Upgrades are not one "level" number. Each buys a different kind of
//! freedom and each costs you something else, so the interesting question is
//! which of the three to buy next rather than whether to buy.
//!
//! * **Hull** carries cargo. More cargo is more profit per voyage and a slower,
//!   clumsier ship that pirates catch.
//! * **Rigging** is speed and, past a point, the ability to stand out of sight
//!   of land at all. A coastal hull in blue water is a wreck waiting to happen.
//! * **Guns** are the argument you make to pirates. They also eat hold space,
//!   so an armed trader is a smaller trader.
//!
//! The two systems meet at exactly one place: `ceiling()`. Every tier is still
//! 0..=4 and every table below is still indexed by tier; the class only says how
//! far up each of the three you are allowed to climb. That is why adding classes
//! did not disturb `fit()`, the costs, or any of the arithmetic that reads them.
//!
//! **The third thing a ship has is people, and the people need feeding.** Crew
//! is not an upgrade tier: it is a number between the class's minimum and its
//! maximum that you move up by signing hands on and that the sea moves down. It
//! is deliberately not gated the way tiers are. Being short-handed makes a ship
//! slow and makes her guns quiet; it never makes her refuse to sail, because a
//! crew that starves at sea would otherwise leave the player becalmed forever
//! with no order left that could help. Every penalty here is a multiplier, and
//! `manning()` is the one place that decides how steep.
//!
//! Food, water and lumber ride in the hold and are counted against the same
//! capacity as cargo, so provisioning for a long leg is paid for in the profit
//! you could have carried instead. That is the whole point of them.

/// The highest tier any class allows. Individual classes stop lower.
pub const MAX_TIER: u8 = 4;

/// What a hull of each tier will hold, in units, before guns are deducted.
const HULL_CAPACITY: [i32; 5] = [40, 90, 160, 260, 400];

/// How far from land each rigging tier dares to sail. Hexes.
///
/// This is the gate on the long ocean crossings, and so on the whole second
/// half of the map. Sailing beyond your rating is possible and is how ships are
/// lost. Because a class caps rigging, it is really the class that decides
/// this: only the two galleons reach tier 4, and so only the two galleons can
/// stand across an ocean. Everything smaller is a coasting vessel that has to
/// follow the land around.
const BLUEWATER_RATING: [i32; 5] = [1, 2, 4, 7, 99];

/// The rating at and above which a ship is considered blue-water: able to leave
/// soundings entirely rather than feel its way along a coast.
pub const BLUEWATER: i32 = 99;

/// Guns carried at each tier, and the hold they displace.
const GUN_COUNT: [i32; 5] = [0, 6, 14, 28, 48];
const GUN_HOLD_COST: [i32; 5] = [0, 8, 20, 40, 70];

/// What signing one hand on costs at the quayside, and what keeping them costs
/// every month afterwards.
///
/// The advance is the smaller number and the wage is the one that hurts, which
/// is the right way round: a full fighting complement is cheap to gather and
/// expensive to keep, so carrying one is a decision you make voyage by voyage
/// rather than once.
pub const HIRE_ADVANCE: i32 = 25;
pub const MONTHLY_WAGE: i32 = 8;

/// How many hands one unit of each store keeps alive for a day.
///
/// Water is the one that runs out. A barrel waters five and feeds nobody, so a
/// long leg is provisioned two-thirds in water by volume, which is both the
/// historical shape of the problem and the reason a hold that looked ample at
/// the quay is not.
pub const HANDS_PER_FOOD: i32 = 10;
pub const HANDS_PER_WATER: i32 = 5;

/// Units of lumber that mend one point of damage, and the hands it takes to
/// work them. Below the class minimum there is nobody to spare for carpentry.
pub const LUMBER_PER_POINT: i32 = 1;

/// Cost in gold to move from the tier below to this one.
const HULL_COST: [i32; 5] = [0, 2_400, 7_200, 19_000, 46_000];
const RIG_COST: [i32; 5] = [0, 3_000, 9_500, 24_000, 58_000];
const GUN_COST: [i32; 5] = [0, 1_800, 5_600, 15_000, 38_000];

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Upgrade {
    Hull,
    Rigging,
    Guns,
}

impl Upgrade {
    pub fn from_index(i: i32) -> Option<Upgrade> {
        match i {
            0 => Some(Upgrade::Hull),
            1 => Some(Upgrade::Rigging),
            2 => Some(Upgrade::Guns),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Upgrade::Hull => "hull",
            Upgrade::Rigging => "rigging",
            Upgrade::Guns => "guns",
        }
    }
}

// -- stores ----------------------------------------------------------------

/// The three things a ship carries that are not cargo.
///
/// They are an enum rather than three more entries in the goods table on
/// purpose. A good has a price that moves with what you have done to the
/// market, a port that may not deal in it and a glut that punishes you for
/// bringing too much; none of that should ever apply to the crew's drinking
/// water. Stores cost the same everywhere and are sold by the chandler, not
/// the exchange.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Store {
    Food = 0,
    Water = 1,
    Lumber = 2,
}

pub const STORES: [Store; 3] = [Store::Food, Store::Water, Store::Lumber];

impl Store {
    pub fn from_index(i: i32) -> Option<Store> {
        STORES.get(i as usize).copied()
    }

    pub fn name(self) -> &'static str {
        match self {
            Store::Food => "food",
            Store::Water => "water",
            Store::Lumber => "lumber",
        }
    }

    /// Gold a unit costs at the chandler's. Water is the cheapest thing on the
    /// quay and the crew drink twice as much of it as they eat, so the two come
    /// out close to the same cost per day and differ mostly in the room they
    /// take up.
    pub fn price(self) -> i32 {
        match self {
            Store::Food => 6,
            Store::Water => 3,
            Store::Lumber => 15,
        }
    }

    /// What the same unit costs out of another ship's hold: half as much again.
    /// She is doing you a favour and she knows it.
    ///
    /// This lives here rather than at the call site because the page has to
    /// print the figure before the player commits to the order, and a premium
    /// written out in both Rust and JavaScript is a premium that will drift.
    pub fn price_afloat(self) -> i32 {
        (self.price() * 3 + 1) / 2
    }
}

// -- classes ---------------------------------------------------------------

/// A class of vessel. Six of them, ordered smallest to largest, which several
/// things below rely on: the yard lists them in this order and the class index
/// crossing the wasm boundary is this discriminant.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Class {
    Balsa = 0,
    Latina = 1,
    Redonda = 2,
    Carrack = 3,
    Galleon = 4,
    HeavyGalleon = 5,
}

pub const CLASSES: [Class; 6] = [
    Class::Balsa,
    Class::Latina,
    Class::Redonda,
    Class::Carrack,
    Class::Galleon,
    Class::HeavyGalleon,
];

/// Everything that distinguishes one class from another.
///
/// The three `start` values are what a freshly bought ship of the class comes
/// fitted with; the three `max` values are how far the shipwright will take her.
/// A class whose start equals its max is finished the day you buy it.
pub struct Spec {
    pub name: &'static str,
    /// One line the yard shows next to the price. The player is choosing
    /// between six of these at once, so each has to say what the ship is *for*.
    pub blurb: &'static str,
    /// The rig, in the plain words the request used. Lateen sails are
    /// three-cornered; square sails are four.
    pub rig: &'static str,
    pub hull_start: u8,
    pub hull_max: u8,
    pub rig_start: u8,
    pub rig_max: u8,
    pub gun_start: u8,
    pub gun_max: u8,
    /// How kindly she handles, as a multiplier on speed. Above 1.0 is nimble.
    pub handling: f32,
    /// Gold at the yard, before any allowance for the ship you sail in on.
    pub price: i32,
    /// Which economies build her, as a bitmask over `world::ECONOMIES`.
    pub yards: u8,
    /// Gold that must already be invested *in this port* before her yard will
    /// take the order. Zero for everything but the two galleons.
    pub needs_invested: i32,
    /// Hands needed to work her properly, and the most she will berth.
    ///
    /// The minimum is not a bar on sailing, it is the figure everything else is
    /// measured against: at or above it she sails and fights at her rated
    /// speed, below it both fall off. The maximum is a hard cap, because there
    /// is only so much deck.
    pub crew_min: i32,
    pub crew_max: i32,
}

/// Economy indices, as a bitmask, matching the order of `world::ECONOMIES`.
const MED: u8 = 1 << 0;
const NEU: u8 = 1 << 1;
const AME: u8 = 1 << 2;
const WAF: u8 = 1 << 3;
const EAF: u8 = 1 << 4;
const ARA: u8 = 1 << 5;
const SEA: u8 = 1 << 6;
const FAR: u8 = 1 << 7;

/// The classes, in `Class` order.
///
/// Two shapes are worth reading off this table rather than hunting for in the
/// code. First, **rig follows geography**: the lateen craft are built where the
/// inland seas are, around the Mediterranean and the Arabian and African coasts,
/// and the square-rigged ocean ships come out of the Atlantic yards. That is
/// what makes where you are matter when you want a different ship. Second,
/// **only the galleons are blue-water**, because only they reach rigging tier 4.
/// A Redonda is fast and a Carrack is roomy and neither of them can cross an
/// ocean, which is the whole reason to want a galleon at all.
pub const SPECS: [Spec; 6] = [
    Spec {
        name: "Balsa",
        blurb: "A coasting boat. Yours because it was cheap.",
        rig: "three-point sails",
        hull_start: 0, hull_max: 1,
        rig_start: 0, rig_max: 1,
        gun_start: 0, gun_max: 1,
        handling: 1.00,
        price: 1_200,
        yards: MED | NEU | AME | WAF | EAF | ARA | SEA | FAR,
        needs_invested: 0,
        crew_min: 3, crew_max: 12,
    },
    Spec {
        name: "Latina",
        blurb: "A small craft, easy to work through the inland seas.",
        rig: "three-point sails",
        hull_start: 1, hull_max: 2,
        rig_start: 1, rig_max: 2,
        gun_start: 0, gun_max: 1,
        handling: 1.12,
        price: 7_000,
        yards: MED | WAF | EAF | ARA,
        needs_invested: 0,
        crew_min: 6, crew_max: 24,
    },
    Spec {
        name: "Redonda",
        blurb: "A small craft built for speed, and for very little else.",
        rig: "four-point sails",
        hull_start: 1, hull_max: 2,
        rig_start: 2, rig_max: 3,
        gun_start: 0, gun_max: 2,
        handling: 1.30,
        price: 14_000,
        yards: MED | NEU | AME | WAF,
        needs_invested: 0,
        crew_min: 8, crew_max: 30,
    },
    Spec {
        name: "Carrack",
        blurb: "A deep hold and a gun deck. The workhorse of the long coasts.",
        rig: "four-point sails",
        hull_start: 2, hull_max: 3,
        rig_start: 2, rig_max: 3,
        gun_start: 1, gun_max: 3,
        handling: 0.95,
        price: 34_000,
        yards: MED | NEU | AME | SEA,
        needs_invested: 0,
        crew_min: 15, crew_max: 60,
    },
    Spec {
        name: "Galleon",
        blurb: "The first ship here that will stand across an open ocean.",
        rig: "four-point sails",
        hull_start: 3, hull_max: 4,
        rig_start: 3, rig_max: 4,
        gun_start: 1, gun_max: 4,
        handling: 0.90,
        price: 78_000,
        yards: NEU | AME | FAR,
        needs_invested: 15_000,
        crew_min: 25, crew_max: 100,
    },
    Spec {
        name: "Heavy Galleon",
        blurb: "Fill her with guns for the ultimate warship, or with cargo and go quietly.",
        rig: "four-point sails",
        hull_start: 4, hull_max: 4,
        rig_start: 3, rig_max: 4,
        gun_start: 2, gun_max: 4,
        handling: 0.85,
        price: 165_000,
        yards: MED | NEU,
        needs_invested: 40_000,
        crew_min: 35, crew_max: 130,
    },
];

impl Class {
    pub fn from_index(i: i32) -> Option<Class> {
        CLASSES.get(i as usize).copied()
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn spec(self) -> &'static Spec {
        &SPECS[self.index()]
    }

    pub fn name(self) -> &'static str {
        self.spec().name
    }

    /// Is this class built anywhere in the given economy? `econ` is an index
    /// into `world::ECONOMIES`; a port that does not trade has -1 and gets
    /// nothing, which is correct because it has no yard either.
    pub fn built_in(self, econ: i8) -> bool {
        if econ < 0 || econ >= 8 {
            return false;
        }
        self.spec().yards & (1 << econ) != 0
    }

    /// Can she leave soundings? True only where the class reaches rigging 4.
    pub fn is_bluewater(self) -> bool {
        BLUEWATER_RATING[self.spec().rig_max as usize] >= BLUEWATER
    }
}

// -- the ship --------------------------------------------------------------

pub struct Ship {
    pub class: Class,
    pub hull: u8,
    pub rigging: u8,
    pub guns: u8,
    /// Damage taken, as a percentage. At 100 the ship is lost.
    pub damage: i32,
    /// Units of each good in the hold, indexed by good.
    pub hold: Vec<i32>,
    /// What each good in the hold cost, so profit can be reported honestly
    /// rather than as gross takings.
    pub paid: Vec<i32>,
    /// Hands aboard. Never negative; at nought the ship is adrift and lost.
    pub crew: i32,
    /// Stores, in the same units the hold is measured in. These are not goods:
    /// they have no market price that moves, they are bought and sold at the
    /// quayside at a fixed rate, and no port pays a premium for them. Keeping
    /// them out of `hold` is what stops the market machinery from ever pricing
    /// the crew's water as a speculation.
    pub food: i32,
    pub water: i32,
    pub lumber: i32,
}

impl Ship {
    /// The ship you begin with: a Balsa, which is a boat rather than a ship and
    /// is meant to feel like one. The first real decision in the game is which
    /// class to trade up to, and starting in the smallest hull is what makes
    /// that a decision rather than a formality.
    pub fn new(goods: usize) -> Self {
        Ship::of_class(Class::Balsa, goods)
    }

    pub fn of_class(class: Class, goods: usize) -> Self {
        let s = class.spec();
        Ship {
            class,
            hull: s.hull_start,
            rigging: s.rig_start,
            guns: s.gun_start,
            damage: 0,
            hold: vec![0; goods],
            paid: vec![0; goods],
            crew: s.crew_min,
            food: 0,
            water: 0,
            lumber: 0,
        }
    }

    pub fn capacity(&self) -> i32 {
        HULL_CAPACITY[self.hull as usize] - GUN_HOLD_COST[self.guns as usize]
    }

    pub fn cargo(&self) -> i32 {
        self.hold.iter().sum()
    }

    pub fn store(&self, s: Store) -> i32 {
        match s {
            Store::Food => self.food,
            Store::Water => self.water,
            Store::Lumber => self.lumber,
        }
    }

    pub fn store_mut(&mut self, s: Store) -> &mut i32 {
        match s {
            Store::Food => &mut self.food,
            Store::Water => &mut self.water,
            Store::Lumber => &mut self.lumber,
        }
    }

    /// Hold taken by food, water and lumber together.
    pub fn stores(&self) -> i32 {
        self.food + self.water + self.lumber
    }

    /// Everything aboard that takes up room, which is cargo and stores alike.
    ///
    /// This, not `cargo()`, is what any check about whether something will fit
    /// should read. `cargo()` remains the answer to the different question of
    /// what is aboard to sell.
    pub fn stowed(&self) -> i32 {
        self.cargo() + self.stores()
    }

    pub fn free_space(&self) -> i32 {
        (self.capacity() - self.stowed()).max(0)
    }

    pub fn crew_min(&self) -> i32 {
        self.class.spec().crew_min
    }

    pub fn crew_max(&self) -> i32 {
        self.class.spec().crew_max
    }

    /// How well manned she is, as a fraction of her class minimum, capped at
    /// one. Extra hands beyond the minimum buy fighting strength, not speed.
    pub fn manning(&self) -> f32 {
        let min = self.crew_min().max(1) as f32;
        (self.crew as f32 / min).clamp(0.0, 1.0)
    }

    /// Units of food and water the crew get through in a day.
    ///
    /// Rounded up, so a single hand still drinks something and the last barrel
    /// still empties. A crew of nought consumes nothing, which matters because
    /// a ship can reach that state and must not then be charged for it.
    pub fn food_per_day(&self) -> i32 {
        (self.crew + HANDS_PER_FOOD - 1) / HANDS_PER_FOOD
    }

    pub fn water_per_day(&self) -> i32 {
        (self.crew + HANDS_PER_WATER - 1) / HANDS_PER_WATER
    }

    /// Days the present stores will last at the present head count, or a very
    /// large number if there is nobody left to feed.
    pub fn days_of_stores(&self) -> i32 {
        let food = match self.food_per_day() {
            0 => i32::MAX,
            n => self.food / n,
        };
        let water = match self.water_per_day() {
            0 => i32::MAX,
            n => self.water / n,
        };
        food.min(water)
    }

    pub fn bluewater_rating(&self) -> i32 {
        BLUEWATER_RATING[self.rigging as usize]
    }

    pub fn gun_count(&self) -> i32 {
        GUN_COUNT[self.guns as usize]
    }

    /// Guns there are actually hands to serve.
    ///
    /// A gun deck is a labour problem before it is a firepower one. The rated
    /// figure is what she carries and is what the yard sells you; this is what
    /// she can fire today, and it is the one combat reads.
    pub fn guns_worked(&self) -> i32 {
        (self.gun_count() as f32 * self.manning()) as i32
    }

    /// Wages owed at the turn of the month.
    pub fn wages(&self) -> i32 {
        self.crew * MONTHLY_WAGE
    }

    /// The most damage the lumber aboard could mend, ignoring who is free to
    /// swing a hammer.
    pub fn mendable(&self) -> i32 {
        (self.lumber / LUMBER_PER_POINT).min(self.damage)
    }

    /// Hours to cross one hex in still air, before wind and current.
    ///
    /// A bigger hull is slower; better rigging more than makes up for it, which
    /// is why the two are worth buying in a particular order rather than in
    /// whatever order you can afford. Handling is the class's own contribution
    /// and is why a Redonda with the same rig as a Carrack still beats her.
    /// A short-handed ship is slower, and this is how much.
    ///
    /// It is a multiplier on hours rather than a bar on sailing, and it is
    /// bounded: an empty ship is not infinitely slow, she is two and a half
    /// times slow. That bound is the whole reason starvation cannot strand
    /// anyone. Full crew is 1.0 and costs nothing.
    pub fn short_handed(&self) -> f32 {
        1.0 / (0.4 + 0.6 * self.manning())
    }

    pub fn base_hours(&self) -> f32 {
        let laden = 1.0 + 0.10 * self.hull as f32;
        let rig = 1.0 + 0.28 * self.rigging as f32;
        6.0 * laden * self.short_handed() / (rig * self.class.spec().handling)
    }

    pub fn tier_of(&self, u: Upgrade) -> u8 {
        match u {
            Upgrade::Hull => self.hull,
            Upgrade::Rigging => self.rigging,
            Upgrade::Guns => self.guns,
        }
    }

    /// How far the class will let this upgrade be taken.
    pub fn ceiling(&self, u: Upgrade) -> u8 {
        let s = self.class.spec();
        match u {
            Upgrade::Hull => s.hull_max,
            Upgrade::Rigging => s.rig_max,
            Upgrade::Guns => s.gun_max,
        }
    }

    /// Gold to buy the next tier, or `None` if the class will take no more.
    pub fn upgrade_cost(&self, u: Upgrade) -> Option<i32> {
        let next = self.tier_of(u) + 1;
        if next > self.ceiling(u) {
            return None;
        }
        let table = match u {
            Upgrade::Hull => &HULL_COST,
            Upgrade::Rigging => &RIG_COST,
            Upgrade::Guns => &GUN_COST,
        };
        Some(table[next as usize])
    }

    /// Fit the next tier. Returns false if the class is already at its ceiling,
    /// or if adding guns would displace cargo that is already aboard: you cannot
    /// have the shipwright cut gunports through a full hold.
    pub fn fit(&mut self, u: Upgrade) -> bool {
        let next = self.tier_of(u) + 1;
        if next > self.ceiling(u) {
            return false;
        }
        match u {
            Upgrade::Hull => self.hull = next,
            Upgrade::Rigging => self.rigging = next,
            Upgrade::Guns => {
                let after = HULL_CAPACITY[self.hull as usize] - GUN_HOLD_COST[next as usize];
                if self.stowed() > after {
                    return false;
                }
                self.guns = next;
            }
        }
        true
    }

    /// What a hull of `class`, as the yard would hand her over, will hold.
    ///
    /// The yard needs this before the ship exists, to refuse a sale that would
    /// leave cargo on the quay. It is the one place capacity is computed for a
    /// ship that is not `self`.
    pub fn capacity_of(class: Class) -> i32 {
        let s = class.spec();
        HULL_CAPACITY[s.hull_start as usize] - GUN_HOLD_COST[s.gun_start as usize]
    }

    /// What the yard allows against the ship you sail in on.
    ///
    /// Half of what her class cost new, and nothing for the damage you have not
    /// repaired. Trading up is meant to be expensive but not punitive, and a
    /// battered ship is meant to be worth mending before you sell her.
    pub fn trade_in(&self) -> i32 {
        let sound = (100 - self.damage.clamp(0, 100)) as i64;
        ((self.class.spec().price as i64 * sound) / 200) as i32
    }

    pub fn repair_cost(&self) -> i32 {
        self.damage * 40
    }
}

/// Guns carried at a given tier, for a hull that is not the player's.
///
/// Everyone else's ship is a class and a gun tier and nothing else: there is no
/// hold to track and no upgrade path to walk. This is the whole of the
/// arithmetic they need, and it keeps the gun table private to this module.
pub fn guns_at(tier: u8) -> i32 {
    GUN_COUNT[(tier as usize).min(MAX_TIER as usize)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guns_eat_the_hold() {
        let mut s = Ship::new(4);
        let bare = s.capacity();
        assert!(s.fit(Upgrade::Guns));
        assert!(s.capacity() < bare);
    }

    #[test]
    fn cannot_fit_guns_over_a_full_hold() {
        let mut s = Ship::new(4);
        s.hold[0] = s.capacity();
        assert!(!s.fit(Upgrade::Guns), "should refuse while the hold is full");
    }

    #[test]
    fn rigging_makes_the_ship_quicker_despite_a_bigger_hull() {
        let small = Ship::of_class(Class::Carrack, 1);
        let mut big = Ship::of_class(Class::Carrack, 1);
        big.fit(Upgrade::Hull);
        big.fit(Upgrade::Rigging);
        assert!(big.base_hours() < small.base_hours());
    }

    #[test]
    fn upgrades_stop_at_the_class_ceiling_not_the_global_one() {
        let mut s = Ship::new(1);
        assert!(s.ceiling(Upgrade::Rigging) < MAX_TIER, "a Balsa is not a galleon");
        while s.rigging < s.ceiling(Upgrade::Rigging) {
            assert!(s.fit(Upgrade::Rigging));
        }
        assert!(!s.fit(Upgrade::Rigging));
        assert_eq!(s.upgrade_cost(Upgrade::Rigging), None);
    }

    #[test]
    fn every_class_can_reach_its_own_ceilings() {
        for c in CLASSES {
            let mut s = Ship::of_class(c, 1);
            for u in [Upgrade::Hull, Upgrade::Rigging, Upgrade::Guns] {
                assert!(
                    s.tier_of(u) <= s.ceiling(u),
                    "{} starts above its own {} ceiling",
                    c.name(),
                    u.name()
                );
                while s.tier_of(u) < s.ceiling(u) {
                    assert!(s.fit(u), "{} stuck below its {} ceiling", c.name(), u.name());
                }
            }
            assert!(s.capacity() > 0, "{} has no hold once fully gunned", c.name());
        }
    }

    #[test]
    fn only_the_galleons_can_leave_soundings() {
        let blue: Vec<&str> = CLASSES
            .iter()
            .filter(|c| c.is_bluewater())
            .map(|c| c.name())
            .collect();
        assert_eq!(blue, vec!["Galleon", "Heavy Galleon"]);
    }

    #[test]
    fn bigger_classes_cost_more_and_carry_more() {
        for pair in CLASSES.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!(
                a.spec().price < b.spec().price,
                "{} should cost less than {}",
                a.name(),
                b.name()
            );
            // Not strictly greater: a Redonda is a Latina that trades her hold
            // for sail area, so the two carry the same and the price buys speed.
            assert!(
                Ship::capacity_of(a) <= Ship::capacity_of(b),
                "{} should not out-carry {}",
                a.name(),
                b.name()
            );
        }
    }

    #[test]
    fn every_class_is_built_somewhere_and_the_small_ones_widely() {
        for c in CLASSES {
            let yards = (0..8).filter(|e| c.built_in(*e)).count();
            assert!(yards > 0, "{} is built nowhere", c.name());
        }
        assert!(
            Class::Balsa.built_in(7) && !Class::HeavyGalleon.built_in(7),
            "the Far East should sell a boat but not a first-rate"
        );
        assert!(!Class::Balsa.built_in(-1), "a landfall has no yard");
    }

    #[test]
    fn only_the_galleons_need_a_port_invested_in() {
        for c in CLASSES {
            let gated = c.spec().needs_invested > 0;
            assert_eq!(gated, c.is_bluewater(), "{} gates wrongly", c.name());
        }
    }

    #[test]
    fn a_damaged_ship_is_worth_less_at_the_yard() {
        let mut s = Ship::of_class(Class::Carrack, 1);
        let sound = s.trade_in();
        s.damage = 50;
        assert!(s.trade_in() < sound);
        s.damage = 100;
        assert_eq!(s.trade_in(), 0);
    }

    /// The crew figures have to be ordered and the ladder has to climb, or a
    /// bigger hull would be cheaper to man than a smaller one.
    #[test]
    fn every_class_berths_more_than_it_needs_and_more_than_the_last() {
        let mut last = (0, 0);
        for c in CLASSES {
            let s = c.spec();
            assert!(s.crew_min > 0, "a {} sails itself", c.name());
            assert!(
                s.crew_max >= s.crew_min * 2,
                "a {} has no room to sign anyone on",
                c.name()
            );
            assert!(
                s.crew_min >= last.0 && s.crew_max > last.1,
                "a {} needs no more hands than the hull below her",
                c.name()
            );
            last = (s.crew_min, s.crew_max);
        }
    }

    /// A full complement is the ordinary case and must cost nothing.
    #[test]
    fn a_ship_at_her_minimum_is_a_ship_at_full_speed() {
        for c in CLASSES {
            let mut s = Ship::of_class(c, 4);
            assert_eq!(s.crew, s.crew_min(), "she does not start manned");
            assert_eq!(s.manning(), 1.0);
            assert_eq!(s.short_handed(), 1.0, "a manned {} is slowed", c.name());
            assert_eq!(s.guns_worked(), s.gun_count());
            s.crew = s.crew_max();
            assert_eq!(s.manning(), 1.0, "extra hands are not extra speed");
        }
    }

    /// Rounding up means one hand still eats, which is the only sane direction
    /// for a ration to round in.
    #[test]
    fn a_single_hand_still_draws_a_ration() {
        let mut s = Ship::of_class(Class::Galleon, 4);
        s.crew = 1;
        assert_eq!(s.food_per_day(), 1);
        assert_eq!(s.water_per_day(), 1);
        s.crew = 0;
        assert_eq!(s.food_per_day(), 0);
        assert_eq!(s.days_of_stores(), i32::MAX, "an empty ship never runs out");
    }
}
