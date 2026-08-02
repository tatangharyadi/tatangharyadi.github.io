//! Prices, and why carrying pepper to a pepper coast ruins you.
//!
//! Every port belongs to one of eight economies, and the base price of each
//! good is a property of the economy rather than the port. On top of that each
//! port keeps a price index per good, which is what makes a route wear out:
//! sell enough cloves in one harbour and the cloves market there stops being
//! worth the voyage, and stays that way until it drifts back.
//!
//! The source table marks two different kinds of dash and they mean opposite
//! things, which is the most useful thing in the whole dataset:
//!
//! * a dash under **buy** means no port of that economy stocks the good, so
//!   there is nothing to load;
//! * a dash under **sell** means *every* port of that economy already sells it.
//!   The good is local. Bringing more is bringing coal to Newcastle, and the
//!   price reflects it.
//!
//! That second case is the whole game. A hold full of nutmeg is a fortune in
//! Lisbon and near worthless in Amboina, and the distance between those two
//! facts is what you are being paid for.

use crate::world::{BUY, ECONOMIES, GOODS, PORTS, SELL};

/// Index points, where 100 is the neutral price.
pub const INDEX_NEUTRAL: i32 = 100;
const INDEX_FLOOR: i32 = 50;
const INDEX_CEILING: i32 = 150;

/// How far the index moves per 1000 gold traded, and the most one visit can
/// move it. Trading in size is worth less per unit than trading in dribs, which
/// is the pressure that pushes you to find a second port rather than pumping
/// the first one dry.
const POINTS_PER_1000_GOLD: i32 = 1;
const MAX_MOVE_PER_TRADE: i32 = 10;

/// What a port pays for a good it already produces, as a percentage of what it
/// would charge to sell you the same thing. Well under half, so it is a loss
/// you can see coming and still make by accident.
const GLUT_PAYS_PERCENT: i32 = 35;

/// A port's own speciality is cheap at the quayside.
const SPECIALTY_DISCOUNT_PERCENT: i32 = 80;

pub struct Markets {
    /// Index per port per good, row-major over ports.
    index: Vec<i32>,
    goods: usize,
}

impl Markets {
    pub fn new() -> Self {
        Markets {
            index: vec![INDEX_NEUTRAL; PORTS.len() * GOODS.len()],
            goods: GOODS.len(),
        }
    }

    fn slot(&self, port: usize, good: usize) -> usize {
        port * self.goods + good
    }

    pub fn index_of(&self, port: usize, good: usize) -> i32 {
        self.index[self.slot(port, good)]
    }

    fn economy(port: usize) -> Option<usize> {
        let e = PORTS[port].econ;
        if e < 0 {
            None
        } else {
            Some(e as usize)
        }
    }

    pub fn trades(port: usize) -> bool {
        Self::economy(port).is_some()
    }

    pub fn economy_name(port: usize) -> &'static str {
        match Self::economy(port) {
            Some(e) => ECONOMIES[e],
            None => "no market",
        }
    }

    /// Gold to buy one unit here, or `None` if the port does not stock it.
    pub fn buy_price(&self, port: usize, good: usize) -> Option<i32> {
        let econ = Self::economy(port)?;
        let base = BUY[good][econ];
        if base < 0 {
            return None;
        }
        let mut price = base as i32 * self.index_of(port, good) / 100;
        if PORTS[port].specialty == good as i16 {
            price = price * SPECIALTY_DISCOUNT_PERCENT / 100;
        }
        Some(price.max(1))
    }

    /// Gold this port pays for one unit, or `None` if it will not take it.
    ///
    /// A port whose economy produces the good will still take it, and will pay
    /// badly for it. That is not a refusal, it is a bad trade, and the
    /// difference matters: the game should let you make the mistake.
    pub fn sell_price(&self, port: usize, good: usize) -> Option<i32> {
        let econ = Self::economy(port)?;
        let base = SELL[good][econ];
        let idx = self.index_of(port, good);
        if base < 0 {
            // Glutted. Price against what it costs here, if it is sold here at
            // all, otherwise against the cheapest economy that does sell it.
            let reference = if BUY[good][econ] >= 0 {
                BUY[good][econ] as i32
            } else {
                cheapest_buy(good)
            };
            let price = reference * GLUT_PAYS_PERCENT / 100 * idx / 100;
            return Some(price.max(1));
        }
        Some((base as i32 * idx / 100).max(1))
    }

    /// True if this port produces the good itself, so selling here is a loss.
    pub fn is_glutted(port: usize, good: usize) -> bool {
        match Self::economy(port) {
            Some(econ) => SELL[good][econ] < 0,
            None => false,
        }
    }

    fn shift(&mut self, port: usize, good: usize, points: i32) {
        let s = self.slot(port, good);
        self.index[s] = (self.index[s] + points).clamp(INDEX_FLOOR, INDEX_CEILING);
    }

    /// Record a purchase. Buying scarcity in makes the next unit dearer.
    pub fn on_buy(&mut self, port: usize, good: usize, gold: i32) {
        let points = (gold / 1000 * POINTS_PER_1000_GOLD).min(MAX_MOVE_PER_TRADE);
        self.shift(port, good, points);
    }

    /// Record a sale. Landing cargo pushes the local price down.
    pub fn on_sell(&mut self, port: usize, good: usize, gold: i32) {
        let points = (gold / 1000 * POINTS_PER_1000_GOLD).min(MAX_MOVE_PER_TRADE);
        self.shift(port, good, -points);
    }

    /// Called on the first of the month. Every index creeps back toward
    /// neutral, so a route you wrecked will recover if you leave it alone, and
    /// the map keeps offering you somewhere to come back to.
    pub fn drift(&mut self) {
        for v in self.index.iter_mut() {
            if *v > INDEX_NEUTRAL {
                *v -= 1;
            } else if *v < INDEX_NEUTRAL {
                *v += 1;
            }
        }
    }
}

fn cheapest_buy(good: usize) -> i32 {
    let mut best = i32::MAX;
    for e in 0..ECONOMIES.len() {
        let v = BUY[good][e];
        if v >= 0 {
            best = best.min(v as i32);
        }
    }
    if best == i32::MAX {
        1
    } else {
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every good that is produced in one economy and wanted in another, with
    /// the port that produces it and the port that pays best for it.
    ///
    /// The second one is deliberately the *best* market rather than the first
    /// one found. The source's sell prices span a factor of thirty for a single
    /// good, so "a glut pays less than some other port" is not true and should
    /// not be asserted: Olive Oil fetches 2 in one economy, which is worse than
    /// the glut price anywhere. What is true is the claim below.
    fn glut_examples() -> Vec<(usize, usize, usize)> {
        let m = Markets::new();
        let mut out = Vec::new();
        for good in 0..GOODS.len() {
            let mut glut = None;
            let mut best: Option<(usize, i32)> = None;
            for port in 0..PORTS.len() {
                if !Markets::trades(port) {
                    continue;
                }
                if Markets::is_glutted(port, good) {
                    glut.get_or_insert(port);
                } else if let Some(p) = m.sell_price(port, good) {
                    if best.map_or(true, |(_, b)| p > b) {
                        best = Some((port, p));
                    }
                }
            }
            if let (Some(g), Some((b, _))) = (glut, best) {
                out.push((good, g, b));
            }
        }
        assert!(
            !out.is_empty(),
            "no good is produced in one economy and wanted in another"
        );
        out
    }

    fn glut_example() -> (usize, usize, usize) {
        glut_examples()[0]
    }

    #[test]
    fn selling_where_it_is_produced_pays_far_less_than_where_it_is_wanted() {
        let m = Markets::new();
        for (good, glutted, wanted) in glut_examples() {
            let bad = m.sell_price(glutted, good).unwrap();
            let paid = m.sell_price(wanted, good).unwrap();
            assert!(
                bad < paid,
                "{} pays {} at a glutted port and {} at the best market",
                GOODS[good],
                bad,
                paid
            );
        }
    }

    /// The mechanic, stated as strongly as it is true: you cannot make money
    /// carrying a good to a coast that grows it, whatever you paid for it,
    /// because a glutted port pays less than the cheapest place it is sold.
    #[test]
    fn carrying_a_good_to_where_it_grows_cannot_pay() {
        let m = Markets::new();
        for (good, glutted, _) in glut_examples() {
            let paid_here = m.sell_price(glutted, good).unwrap();
            let cheapest_anywhere = (0..PORTS.len())
                .filter_map(|p| m.buy_price(p, good))
                .min()
                .expect("a good nobody sells");
            assert!(
                paid_here < cheapest_anywhere,
                "{} costs at least {} and a glutted port pays {}",
                GOODS[good],
                cheapest_anywhere,
                paid_here
            );
        }
    }

    #[test]
    fn a_glutted_port_is_an_outright_loss_if_you_bought_it_there() {
        let m = Markets::new();
        let (good, glutted, _) = glut_example();
        if let Some(cost) = m.buy_price(glutted, good) {
            let back = m.sell_price(glutted, good).unwrap();
            assert!(back < cost, "buying and selling on the spot should lose");
        }
    }

    #[test]
    fn selling_pushes_the_local_price_down_and_it_recovers() {
        let mut m = Markets::new();
        let (good, _, wanted) = glut_example();
        let before = m.sell_price(wanted, good).unwrap();
        m.on_sell(wanted, good, 9_000);
        let after = m.sell_price(wanted, good).unwrap();
        assert!(after < before, "landing cargo should depress the price");
        for _ in 0..40 {
            m.drift();
        }
        assert_eq!(m.index_of(wanted, good), INDEX_NEUTRAL);
    }

    #[test]
    fn the_index_never_leaves_its_band() {
        let mut m = Markets::new();
        for _ in 0..500 {
            m.on_sell(0, 0, 1_000_000);
        }
        assert_eq!(m.index_of(0, 0), 50);
        for _ in 0..500 {
            m.on_buy(0, 0, 1_000_000);
        }
        assert_eq!(m.index_of(0, 0), 150);
    }

    #[test]
    fn a_port_with_no_economy_has_no_prices() {
        if let Some(p) = (0..PORTS.len()).find(|p| !Markets::trades(*p)) {
            let m = Markets::new();
            assert!(m.buy_price(p, 0).is_none());
            assert!(m.sell_price(p, 0).is_none());
        }
    }
}
