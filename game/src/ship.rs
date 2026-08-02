//! Ships, and the three things you spend money on to make them better.
//!
//! Upgrades are deliberately not one "level" number. Each one buys a different
//! kind of freedom and each one costs you something else, so the interesting
//! question is which of the three to buy next rather than whether to buy.
//!
//! * **Hull** carries cargo. More cargo is more profit per voyage and a slower,
//!   clumsier ship that pirates catch.
//! * **Rigging** is speed and, past a point, the ability to stand out of sight
//!   of land at all. A coastal hull in blue water is a wreck waiting to happen.
//! * **Guns** are the argument you make to pirates. They also eat hold space,
//!   so an armed trader is a smaller trader.

pub const MAX_TIER: u8 = 4;

/// What a hull of each tier will hold, in units, before guns are deducted.
const HULL_CAPACITY: [i32; 5] = [40, 90, 160, 260, 400];

/// How far from land each rigging tier dares to sail. Hexes.
///
/// This is the gate on the long ocean crossings, and so on the whole second
/// half of the map: the Atlantic and Indian crossings need tier 2, the Pacific
/// needs tier 3. Sailing beyond your rating is possible and is how ships are
/// lost.
const BLUEWATER_RATING: [i32; 5] = [1, 2, 4, 7, 99];

/// Guns carried at each tier, and the hold they displace.
const GUN_COUNT: [i32; 5] = [0, 6, 14, 28, 48];
const GUN_HOLD_COST: [i32; 5] = [0, 8, 20, 40, 70];

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

pub struct Ship {
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
}

impl Ship {
    pub fn new(goods: usize) -> Self {
        Ship {
            hull: 0,
            rigging: 0,
            guns: 0,
            damage: 0,
            hold: vec![0; goods],
            paid: vec![0; goods],
        }
    }

    pub fn capacity(&self) -> i32 {
        HULL_CAPACITY[self.hull as usize] - GUN_HOLD_COST[self.guns as usize]
    }

    pub fn cargo(&self) -> i32 {
        self.hold.iter().sum()
    }

    pub fn free_space(&self) -> i32 {
        (self.capacity() - self.cargo()).max(0)
    }

    pub fn bluewater_rating(&self) -> i32 {
        BLUEWATER_RATING[self.rigging as usize]
    }

    pub fn gun_count(&self) -> i32 {
        GUN_COUNT[self.guns as usize]
    }

    /// Hours to cross one hex in still air, before wind and current.
    ///
    /// A bigger hull is slower; better rigging more than makes up for it, which
    /// is why the two are worth buying in a particular order rather than in
    /// whatever order you can afford.
    pub fn base_hours(&self) -> f32 {
        let laden = 1.0 + 0.10 * self.hull as f32;
        let rig = 1.0 + 0.28 * self.rigging as f32;
        6.0 * laden / rig
    }

    pub fn tier_of(&self, u: Upgrade) -> u8 {
        match u {
            Upgrade::Hull => self.hull,
            Upgrade::Rigging => self.rigging,
            Upgrade::Guns => self.guns,
        }
    }

    /// Gold to buy the next tier, or `None` if it is already at the top.
    pub fn upgrade_cost(&self, u: Upgrade) -> Option<i32> {
        let next = self.tier_of(u) + 1;
        if next > MAX_TIER {
            return None;
        }
        let table = match u {
            Upgrade::Hull => &HULL_COST,
            Upgrade::Rigging => &RIG_COST,
            Upgrade::Guns => &GUN_COST,
        };
        Some(table[next as usize])
    }

    /// Fit the next tier. Returns false if it is already at the top, or if
    /// adding guns would displace cargo that is already aboard: you cannot
    /// have the shipwright cut gunports through a full hold.
    pub fn fit(&mut self, u: Upgrade) -> bool {
        let next = self.tier_of(u) + 1;
        if next > MAX_TIER {
            return false;
        }
        match u {
            Upgrade::Hull => self.hull = next,
            Upgrade::Rigging => self.rigging = next,
            Upgrade::Guns => {
                let after = HULL_CAPACITY[self.hull as usize]
                    - GUN_HOLD_COST[next as usize];
                if self.cargo() > after {
                    return false;
                }
                self.guns = next;
            }
        }
        true
    }

    pub fn repair_cost(&self) -> i32 {
        self.damage * 40
    }
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
        let small = Ship::new(1);
        let mut big = Ship::new(1);
        big.fit(Upgrade::Hull);
        big.fit(Upgrade::Rigging);
        assert!(big.base_hours() < small.base_hours());
    }

    #[test]
    fn upgrades_run_out_at_the_top_tier() {
        let mut s = Ship::new(1);
        for _ in 0..MAX_TIER {
            assert!(s.fit(Upgrade::Rigging));
        }
        assert!(!s.fit(Upgrade::Rigging));
        assert_eq!(s.upgrade_cost(Upgrade::Rigging), None);
    }
}
