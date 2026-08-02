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
//!
//! On top of the index there is a **cooldown**, which is what stops a wrecked
//! route from healing the moment you leave. A port that has just been flooded
//! with a good sits at the low price for a few months before it starts climbing
//! back, so coming straight back with a second hold of the same thing does not
//! merely earn less, it pushes the recovery further away.
//!
//! Two things are remembered per **port** rather than per good, and between
//! them they are the only reason two ports of the same economy are ever
//! different from each other.
//!
//! **Favour** is what the quayside thinks of you, and it rises with every gold
//! piece you move through the place, bought or sold alike. It buys a discount
//! on what the port charges you, and only that: a port that likes you sells
//! cheaper, it does not pay more. Putting it into `sell_price` would be
//! actively backwards, because the glut branch prices against
//! `BUY[good][econ]`, so a favour that leaked into that reference would make a
//! glutted port pay *better* the more you traded there.
//!
//! **Investment** is the counterweight. A port opens only a handful of its
//! goods to a stranger; the rest of what its economy carries is there, and
//! priced, and shut to you until you have put money into the place. Which
//! handful is open is drawn per port rather than per economy, so the ports that
//! all stock the same list no longer stock the same list *to you*. A locked
//! good is shown with its price and marked shut rather than hidden, because a
//! hidden good is indistinguishable from a good the economy never had, and that
//! is a silent failure rather than a mechanic.

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

/// Months a market sulks after a trade before the index starts moving back.
///
/// Two rather than more, and the reason is arithmetic rather than taste. The
/// index only recovers one point a month, so the largest possible crash already
/// takes ten months to heal on its own. A long dwell on top of that would not
/// make routes feel worn, it would make them feel dead, and the map has to keep
/// offering you somewhere to come back to. Two months is long enough that
/// turning straight round with a second cargo is visibly the wrong move and
/// short enough that the port is worth remembering.
const COOLDOWN_MONTHS: u8 = 2;

/// Favour points, and what a full book of them is worth off the asking price.
///
/// One point per 1000 gold traded means a full sixty points is sixty thousand
/// gold pushed across one quayside, which is many voyages rather than one lucky
/// cargo. Twelve percent is deliberately small. It is meant to be the reason to
/// come back to a port you already know rather than a reason never to leave it,
/// and the index swings fifty points either way, so a route that has gone stale
/// stays stale however well liked you are.
const FAVOUR_MAX: i32 = 60;
const FAVOUR_PER_1000_GOLD: i32 = 1;
const FAVOUR_DISCOUNT_MAX_PERCENT: i32 = 12;
/// Points of standing lost each month. See `drift`.
const FAVOUR_DECAY_PER_MONTH: i32 = 1;

/// How many of its economy's goods a port will sell to a stranger, and what the
/// first investment costs to open one more.
///
/// Three is enough that the opening move is never a dead port, and few enough
/// that the thinnest economies still hold something back. The cost rises by a
/// step each time, so opening a port's whole book is a long game: the fourth
/// good costs 5000, the fifth 10000, and so on.
const OPEN_AT_FIRST: i32 = 3;
const INVEST_STEP: i32 = 5_000;

pub struct Markets {
    /// Index per port per good, row-major over ports.
    index: Vec<i32>,
    /// Months left before the matching index may move back toward neutral.
    /// Same shape and same indexing as `index`.
    cooldown: Vec<u8>,
    /// Standing at each port, 0..=FAVOUR_MAX. Per port, not per good: it is
    /// what the harbour thinks of you, not what one trade did.
    favour: Vec<i32>,
    /// Goods opened at each port by investment, over and above OPEN_AT_FIRST.
    opened: Vec<i32>,
    /// Gold sunk into each port. Kept only so the status column can say what
    /// the standing cost; `opened` is what the mechanic reads.
    invested: Vec<i32>,
    goods: usize,
}

impl Markets {
    pub fn new() -> Self {
        Markets {
            index: vec![INDEX_NEUTRAL; PORTS.len() * GOODS.len()],
            cooldown: vec![0; PORTS.len() * GOODS.len()],
            favour: vec![0; PORTS.len()],
            opened: vec![0; PORTS.len()],
            invested: vec![0; PORTS.len()],
            goods: GOODS.len(),
        }
    }

    fn slot(&self, port: usize, good: usize) -> usize {
        port * self.goods + good
    }

    pub fn index_of(&self, port: usize, good: usize) -> i32 {
        self.index[self.slot(port, good)]
    }

    /// Months this market will stay where it is before it starts recovering.
    ///
    /// Exposed rather than kept private because a cooldown nobody can see is
    /// indistinguishable from the price simply being low. The status column
    /// prints it, so a reader can tell "this route is worn out" from "this route
    /// was never any good".
    pub fn cooldown_of(&self, port: usize, good: usize) -> i32 {
        self.cooldown[self.slot(port, good)] as i32
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
        let off = self.favour[port] * FAVOUR_DISCOUNT_MAX_PERCENT / FAVOUR_MAX;
        price = price * (100 - off) / 100;
        Some(price.max(1))
    }

    /// Standing at this port, 0..=FAVOUR_MAX.
    pub fn favour_of(&self, port: usize) -> i32 {
        self.favour[port]
    }

    /// What the discount is worth right now, as whole percent off the asking
    /// price. Exposed because a discount folded into a number is not a
    /// mechanic the player can see, and this one is small enough to miss.
    pub fn favour_discount(&self, port: usize) -> i32 {
        self.favour[port] * FAVOUR_DISCOUNT_MAX_PERCENT / FAVOUR_MAX
    }

    /// Gold sunk into this port so far.
    pub fn invested_of(&self, port: usize) -> i32 {
        self.invested[port]
    }

    /// How many goods this port's economy stocks at all.
    pub fn stocked_count(port: usize) -> i32 {
        match Self::economy(port) {
            Some(econ) => (0..GOODS.len()).filter(|&g| BUY[g][econ] >= 0).count() as i32,
            None => 0,
        }
    }

    /// How many of them are open to the player here.
    pub fn open_count(&self, port: usize) -> i32 {
        (OPEN_AT_FIRST + self.opened[port]).min(Self::stocked_count(port))
    }

    /// Where a good sits in this port's queue. Lower opens sooner.
    ///
    /// The port's own speciality is always first, because the thing a place is
    /// known for is precisely the thing it would show a stranger. Everything
    /// else is a hash of the pair, which is what makes two ports of one economy
    /// open different books while staying identical across runs: the layout of
    /// a harbour is a fact about the world, not about the dice this voyage
    /// happened to roll.
    fn queue_key(port: usize, good: usize) -> u32 {
        if PORTS[port].specialty == good as i16 {
            return 0;
        }
        let mut h = (port as u32)
            .wrapping_mul(0x9E37_79B9)
            ^ (good as u32).wrapping_mul(0x85EB_CA6B);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2545_F491);
        h ^= h >> 13;
        // Never zero, so nothing can tie with the speciality for first place.
        h | 1
    }

    /// True if the player may actually buy this good here.
    ///
    /// A port that does not stock the good at all is not "shut", it is empty,
    /// and this returns false for both. The caller distinguishes them by
    /// whether `buy_price` gave a price: priced and shut is an invitation to
    /// invest, unpriced is an economy that never carried the thing.
    pub fn is_open(&self, port: usize, good: usize) -> bool {
        let Some(econ) = Self::economy(port) else {
            return false;
        };
        if BUY[good][econ] < 0 {
            return false;
        }
        let mine = (Self::queue_key(port, good), good);
        let ahead = (0..GOODS.len())
            .filter(|&g| BUY[g][econ] >= 0 && (Self::queue_key(port, g), g) < mine)
            .count() as i32;
        ahead < self.open_count(port)
    }

    /// What it costs to open one more good here, or `None` if the whole book is
    /// already open.
    pub fn investment_cost(&self, port: usize) -> Option<i32> {
        if self.open_count(port) >= Self::stocked_count(port) {
            return None;
        }
        Some(INVEST_STEP * (self.opened[port] + 1))
    }

    /// Take the money and open one more good. The caller has already checked
    /// the strongbox.
    ///
    /// Investing also earns favour, because it is money across the same
    /// quayside as any other trade and a port that took five thousand gold off
    /// you has no reason to think less of you for it.
    pub fn invest(&mut self, port: usize, gold: i32) {
        self.opened[port] += 1;
        self.invested[port] += gold;
        self.earn_favour(port, gold);
    }

    fn earn_favour(&mut self, port: usize, gold: i32) {
        let gained = gold / 1000 * FAVOUR_PER_1000_GOLD;
        self.favour[port] = (self.favour[port] + gained).min(FAVOUR_MAX);
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
            // Glutted. Price against the cheapest economy that sells the good
            // anywhere, including this one.
            //
            // Against the *local* base would be the obvious reading and it is
            // wrong, because a few goods are glutted where they are dear and
            // cheap somewhere else: Cloth is shut out of the Mediterranean at
            // 60 and sold in SE Asia at 30. Paying a fraction of 60 for a good
            // obtainable at 30 leaves a margin, and once the speciality and
            // favour discounts stack on the buying end the margin closes to
            // nothing and carrying cloth to a cloth coast starts to pay. The
            // floor has to be the cheapest source or the invariant is a
            // coincidence about the price table rather than a rule.
            let price = cheapest_buy(good) * GLUT_PAYS_PERCENT / 100 * idx / 100;
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

    /// Move the index and restart the clock.
    ///
    /// The cooldown is set on every trade rather than only on a big one, which
    /// is what makes repeat visits bite: each fresh cargo pushes the recovery
    /// two months further out whether or not the index had any room left to
    /// fall. Pumping a port dry costs you the port for a season.
    fn shift(&mut self, port: usize, good: usize, points: i32) {
        let s = self.slot(port, good);
        self.index[s] = (self.index[s] + points).clamp(INDEX_FLOOR, INDEX_CEILING);
        self.cooldown[s] = COOLDOWN_MONTHS;
    }

    /// Record a purchase. Buying scarcity in makes the next unit dearer.
    pub fn on_buy(&mut self, port: usize, good: usize, gold: i32) {
        let points = (gold / 1000 * POINTS_PER_1000_GOLD).min(MAX_MOVE_PER_TRADE);
        self.shift(port, good, points);
        self.earn_favour(port, gold);
    }

    /// Record a sale. Landing cargo pushes the local price down.
    pub fn on_sell(&mut self, port: usize, good: usize, gold: i32) {
        let points = (gold / 1000 * POINTS_PER_1000_GOLD).min(MAX_MOVE_PER_TRADE);
        self.shift(port, good, -points);
        self.earn_favour(port, gold);
    }

    /// Put a port in your debt directly, in points rather than in gold.
    ///
    /// Every other way of earning standing runs through `earn_favour`, which is
    /// denominated in gold traded, because every other way of earning it *is* a
    /// trade. Discharging a commission is not: no goods change hands at a price
    /// and there is no sum for `earn_favour` to divide. The alternative was to
    /// invent a notional gold figure and hand it over, which would tie the size
    /// of the thank-you to the going rate for the parcel rather than to the
    /// favour the errand was worth.
    ///
    /// Clamped at the same ceiling, so this is not a way round `FAVOUR_MAX`.
    pub fn oblige(&mut self, port: usize, points: i32) {
        if port >= self.favour.len() || points <= 0 {
            return;
        }
        self.favour[port] = (self.favour[port] + points).min(FAVOUR_MAX);
    }

    /// Called on the first of the month. Every index creeps back toward
    /// neutral, so a route you wrecked will recover if you leave it alone, and
    /// the map keeps offering you somewhere to come back to.
    ///
    /// A market still inside its cooldown spends the month sulking instead: the
    /// counter comes down and the price does not move. Recovery therefore begins
    /// `COOLDOWN_MONTHS` after the *last* trade rather than after the first.
    pub fn drift(&mut self) {
        // Standing fades with the same month. A factor remembers a good customer
        // and forgets a former one, and without this the discount was a ratchet:
        // trade hard at one port for a season and it stayed the cheapest place
        // on the map for the rest of the game whether you ever returned or not.
        //
        // One point a month is deliberately slower than the earning rate, which
        // is a point per thousand gold traded. Anyone still calling loses nothing
        // they notice; the decay only bites on abandonment, which is the whole
        // thing it is meant to price. Sixty points is therefore five years of
        // being away before a full house of favour is a stranger again.
        for f in self.favour.iter_mut() {
            *f = (*f - FAVOUR_DECAY_PER_MONTH).max(0);
        }
        for (v, cool) in self.index.iter_mut().zip(self.cooldown.iter_mut()) {
            if *cool > 0 {
                *cool -= 1;
                continue;
            }
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

    /// The cooldown, stated as the thing a player would notice: the month after
    /// a sale the price has not begun to come back.
    #[test]
    fn a_market_sulks_before_it_starts_recovering() {
        let mut m = Markets::new();
        let (good, _, wanted) = glut_example();
        m.on_sell(wanted, good, 9_000);
        let sunk = m.index_of(wanted, good);
        assert!(sunk < INDEX_NEUTRAL, "the sale did not move the index");
        assert_eq!(m.cooldown_of(wanted, good), COOLDOWN_MONTHS as i32);

        for month in 1..=COOLDOWN_MONTHS {
            m.drift();
            assert_eq!(
                m.index_of(wanted, good),
                sunk,
                "the price moved {month} month(s) in, while still on cooldown"
            );
        }
        m.drift();
        assert!(
            m.index_of(wanted, good) > sunk,
            "the price never started coming back"
        );
    }

    /// Coming straight back with a second cargo pushes the recovery further out
    /// rather than merely earning less. This is the whole point of the cooldown
    /// and it is the half that a "price goes down when you sell" test misses.
    #[test]
    fn flooding_the_same_port_again_restarts_the_clock() {
        let mut m = Markets::new();
        let (good, _, wanted) = glut_example();
        m.on_sell(wanted, good, 9_000);
        m.drift();
        assert_eq!(m.cooldown_of(wanted, good), COOLDOWN_MONTHS as i32 - 1);
        m.on_sell(wanted, good, 1_000);
        assert_eq!(
            m.cooldown_of(wanted, good),
            COOLDOWN_MONTHS as i32,
            "a second cargo did not restart the cooldown"
        );
    }

    /// The combined figure, which is the one that decides whether the map still
    /// works. Cooldown and drift compose, so what one voyage costs you is the
    /// dwell plus one month per point, and the question is not whether the
    /// cooldown is short but whether the total is a length of time a player
    /// would plausibly sail out and back in.
    ///
    /// One hold sold is at most `MAX_MOVE_PER_TRADE`, so the honest answer for a
    /// single visit is a year and no worse. Bottoming a market right out takes
    /// five such visits and then genuinely does cost you the port for years,
    /// which is the intended shape: the punishment is for pumping, not trading.
    #[test]
    fn one_voyage_never_costs_a_port_more_than_a_year() {
        let mut m = Markets::new();
        m.on_sell(0, 0, 1_000_000);
        let sunk = INDEX_NEUTRAL - m.index_of(0, 0);
        assert_eq!(sunk, MAX_MOVE_PER_TRADE, "one sale should move the full cap");

        let months = sunk + COOLDOWN_MONTHS as i32;
        assert!(months <= 12, "one voyage costs the port {months} months");
        for _ in 0..months {
            m.drift();
        }
        assert_eq!(m.index_of(0, 0), INDEX_NEUTRAL, "it did not heal in {months}");
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

    /// The invariant above again, at the most favourable prices the game can
    /// ever quote. This is the one that would actually break: the glut branch
    /// pays a fixed fraction of a *base* price, while favour and speciality
    /// both cut the price a player pays, so the two sides converge. If a future
    /// discount pushes them past each other, carrying pepper to a pepper coast
    /// starts to pay and the whole economy inverts.
    #[test]
    fn carrying_a_good_to_where_it_grows_cannot_pay_even_at_full_favour() {
        let mut m = Markets::new();
        for port in 0..PORTS.len() {
            m.favour[port] = FAVOUR_MAX;
        }
        assert_eq!(m.favour_discount(0), FAVOUR_DISCOUNT_MAX_PERCENT);
        for (good, glutted, _) in glut_examples() {
            let paid_here = m.sell_price(glutted, good).unwrap();
            let cheapest_anywhere = (0..PORTS.len())
                .filter_map(|p| m.buy_price(p, good))
                .min()
                .expect("a good nobody sells");
            assert!(
                paid_here < cheapest_anywhere,
                "{} costs at least {} at full favour and a glutted port pays {}",
                GOODS[good],
                cheapest_anywhere,
                paid_here
            );
        }
    }

    /// Favour is buy-side only. Stated as a test because the reason is a
    /// two-step argument about the glut reference price and would otherwise
    /// survive exactly as long as the comment explaining it.
    #[test]
    fn favour_makes_buying_cheaper_and_leaves_selling_alone() {
        let mut m = Markets::new();
        let (good, _, wanted) = glut_example();
        let port = (0..PORTS.len())
            .find(|&p| m.buy_price(p, good).map_or(false, |v| v > 20))
            .expect("no port sells this dearly enough to see a discount");
        let buy_before = m.buy_price(port, good).unwrap();
        let sell_before = m.sell_price(wanted, good).unwrap();

        m.favour[port] = FAVOUR_MAX;
        m.favour[wanted] = FAVOUR_MAX;

        assert!(
            m.buy_price(port, good).unwrap() < buy_before,
            "full favour did not cut the asking price"
        );
        assert_eq!(
            m.sell_price(wanted, good).unwrap(),
            sell_before,
            "favour moved what the port pays, which it must never do"
        );
    }

    #[test]
    fn favour_is_earned_by_trading_and_stops_at_the_cap() {
        let mut m = Markets::new();
        assert_eq!(m.favour_of(0), 0, "a stranger starts with no standing");
        m.on_buy(0, 0, 5_000);
        assert_eq!(m.favour_of(0), 5);
        m.on_sell(0, 1, 3_000);
        assert_eq!(m.favour_of(0), 8, "selling is commerce too");
        for _ in 0..50 {
            m.on_sell(0, 1, 100_000);
        }
        assert_eq!(m.favour_of(0), FAVOUR_MAX);
    }

    /// Standing fades, floors at nothing, and fades slower than trading earns.
    ///
    /// The last clause is the one that matters and is asserted rather than
    /// argued: a single thousand-gold trade has to be worth more than the month
    /// it sits inside, or the mechanic would tax the player for playing instead
    /// of for staying away.
    #[test]
    fn favour_decays_month_by_month_and_stops_at_nothing() {
        let mut m = Markets::new();
        m.on_buy(0, 0, 10_000);
        assert_eq!(m.favour_of(0), 10);
        m.drift();
        assert_eq!(m.favour_of(0), 9, "a month away costs a point");
        for _ in 0..20 {
            m.drift();
        }
        assert_eq!(m.favour_of(0), 0, "standing runs out rather than going negative");

        let mut kept = Markets::new();
        kept.on_buy(1, 0, 1_000);
        kept.drift();
        assert_eq!(
            kept.favour_of(1),
            0,
            "the smallest trade exactly offsets the month, and nothing is owed"
        );
        kept.on_buy(1, 0, 2_000);
        kept.drift();
        assert_eq!(kept.favour_of(1), 1, "trading outpaces the decay");
    }

    /// The point of the whole mechanic: two ports of one economy are no longer
    /// the same shop. Asserted over the real map rather than a contrived pair,
    /// because if the hash ever degenerates this is the only thing that notices.
    #[test]
    fn two_ports_of_the_same_economy_open_different_books() {
        let m = Markets::new();
        let mut differ = 0;
        for a in 0..PORTS.len() {
            for b in (a + 1)..PORTS.len() {
                if Markets::economy(a).is_none() || Markets::economy(a) != Markets::economy(b) {
                    continue;
                }
                if Markets::stocked_count(a) <= OPEN_AT_FIRST {
                    continue;
                }
                if (0..GOODS.len()).any(|g| m.is_open(a, g) != m.is_open(b, g)) {
                    differ += 1;
                }
            }
        }
        assert!(
            differ > 0,
            "every port of every economy opens an identical book"
        );
    }

    /// Progressive reveal, from both ends: a stranger is never shown the whole
    /// book, and is never shown an empty one either.
    #[test]
    fn a_first_arrival_sees_some_of_the_book_and_not_all_of_it() {
        let m = Markets::new();
        for port in 0..PORTS.len() {
            let stocked = Markets::stocked_count(port);
            if stocked == 0 {
                continue;
            }
            let open = (0..GOODS.len()).filter(|&g| m.is_open(port, g)).count() as i32;
            assert_eq!(open, m.open_count(port), "{} miscounts its open goods", PORTS[port].name);
            assert!(open > 0, "{} shows a stranger nothing", PORTS[port].name);
            assert!(
                open <= stocked,
                "{} opens more than it stocks",
                PORTS[port].name
            );
            if stocked > OPEN_AT_FIRST {
                assert!(
                    open < stocked,
                    "{} holds nothing back on a first arrival",
                    PORTS[port].name
                );
            }
        }
    }

    /// A port's speciality is the thing it is known for, so it is never the
    /// good you have to buy your way into.
    #[test]
    fn a_speciality_is_always_open_from_the_first_arrival() {
        let m = Markets::new();
        for port in 0..PORTS.len() {
            let s = PORTS[port].specialty;
            if s < 0 || m.buy_price(port, s as usize).is_none() {
                continue;
            }
            assert!(
                m.is_open(port, s as usize),
                "{} keeps its own speciality shut",
                PORTS[port].name
            );
        }
    }

    #[test]
    fn investing_opens_one_more_good_each_time_and_then_runs_out() {
        let mut m = Markets::new();
        let port = (0..PORTS.len())
            .max_by_key(|&p| Markets::stocked_count(p))
            .unwrap();
        let stocked = Markets::stocked_count(port);
        assert!(stocked > OPEN_AT_FIRST, "no port stocks enough to test");

        let mut cost = m.investment_cost(port).expect("nothing to open");
        assert_eq!(cost, INVEST_STEP);
        let mut opened = m.open_count(port);
        while let Some(c) = m.investment_cost(port) {
            assert!(c >= cost, "investment got cheaper");
            cost = c;
            m.invest(port, c);
            assert_eq!(m.open_count(port), opened + 1, "investment opened nothing");
            opened += 1;
        }
        assert_eq!(m.open_count(port), stocked, "the book never fully opened");
        assert!(m.invested_of(port) > 0);
    }

    /// Opening the book must not also open a good the economy never carried.
    #[test]
    fn investment_never_conjures_a_good_the_economy_does_not_stock() {
        let mut m = Markets::new();
        for port in 0..PORTS.len() {
            for _ in 0..GOODS.len() {
                if let Some(c) = m.investment_cost(port) {
                    m.invest(port, c);
                }
            }
            for good in 0..GOODS.len() {
                if m.is_open(port, good) {
                    assert!(
                        m.buy_price(port, good).is_some(),
                        "{} opened {} with no price",
                        PORTS[port].name,
                        GOODS[good]
                    );
                }
            }
        }
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
