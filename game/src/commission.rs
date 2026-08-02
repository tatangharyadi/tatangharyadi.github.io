//! Commissions: the errands a factor puts to you when you come to anchor.
//!
//! The trade game rewards a groove. Find two ports whose prices disagree, run
//! the leg until the cooldown bites, wait, run it again. That is a real
//! strategy and the market model is built to make it a diminishing one, but a
//! player who has found one profitable circuit has very little reason to look
//! at the other sixty-odd harbours on the chart, and none at all to buy a good
//! they have never bought. The world gets no bigger the longer you play it.
//!
//! A commission is the counterweight. It is an offer, made on arrival, to pay
//! well above the going rate for a parcel of goods moved between here and
//! somewhere else. What makes it worth having is not the money: it is that the
//! *somewhere else* is chosen to be a port you have never called at, and the
//! parcel to be a good you have never bought. The reward is the excuse. The
//! detour is the point.
//!
//! Three things it deliberately is not.
//!
//! It has **no deadline and no penalty**. An optional errand that can punish
//! you is not optional, and a clock turns a nudge toward the unfamiliar into a
//! reason to refuse anything unfamiliar. Accept it and it waits; the day you
//! happen to reach that harbour with the cargo aboard, you are paid.
//!
//! It carries **no consigned cargo**. The factor does not load your hold, he
//! names a parcel and a price. The goods you carry are goods you bought, they
//! sit in `ship.hold` like any others and you may sell them anywhere you like
//! at whatever it costs you. That is a mechanical decision as much as a
//! flavourful one: cargo the player may not sell would need every reader of the
//! hold (`cargo`, `free_space`, the market table, the whole of `sell`) to learn
//! about a second kind of unit, in exchange for nothing the errand needs.
//!
//! There is **at most one at a time**. A board of six offers is a menu to
//! optimise over, which is the behaviour this exists to interrupt.

use crate::market::Markets;
use crate::sim::coin;
use crate::rng::Rng;
use crate::world::{GOODS, PORTS};

/// Which way the parcel travels. Both ends of both kinds are a port the player
/// must sail to, so the two differ less in the code than they do at the quay,
/// which is the intent: one mechanic, two sentences.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Buy the parcel here and land it at the far port. Paid there.
    Deliver,
    /// Fetch the parcel from the far port and bring it back here. Paid here.
    Collect,
}

#[derive(Clone, Debug)]
pub struct Commission {
    pub kind: Kind,
    /// The harbour whose factor made the offer.
    pub issued_at: usize,
    /// The other end. Always somewhere the player has charted, because a course
    /// cannot be laid to an unseen port and an unfulfillable errand is a lie.
    pub other: usize,
    pub good: usize,
    pub qty: i32,
    pub gold: i32,
    pub favour: i32,
}

impl Commission {
    /// Where the parcel is paid for.
    pub fn paid_at(&self) -> usize {
        match self.kind {
            Kind::Deliver => self.other,
            Kind::Collect => self.issued_at,
        }
    }

    /// Where the parcel has to be bought. Not enforced anywhere: the player may
    /// have the goods aboard already, or buy them at a third port entirely.
    /// This is what the offer is *written* around, so that it always names a
    /// good that can in fact be had at the far end of the errand.
    pub fn bought_at(&self) -> usize {
        match self.kind {
            Kind::Deliver => self.issued_at,
            Kind::Collect => self.other,
        }
    }

    /// The offer as the factor puts it. One sentence, because it is read in a
    /// panel and in the chronicle both.
    pub fn wording(&self) -> String {
        match self.kind {
            Kind::Deliver => format!(
                "Carry {} {} to {} for {} and their goodwill.",
                self.qty, GOODS[self.good], PORTS[self.other].name, coin(self.gold)
            ),
            Kind::Collect => format!(
                "Bring {} {} from {} for {} and their goodwill.",
                self.qty, GOODS[self.good], PORTS[self.other].name, coin(self.gold)
            ),
        }
    }
}

/// How much above the cost of the parcel the errand pays, as a percentage of
/// what the goods are worth where they are bought. Generous on purpose: the
/// player is being asked to give up the routing freedom that is the whole of
/// the trade game, and a margin they could beat by running their own circuit
/// would make the offer an insult.
const MARGIN_PERCENT: i32 = 70;

/// Added per hex between the two ports. A long errand is worth more than a
/// short one for the same reason a long voyage is: it costs days, wages and
/// stores.
const GOLD_PER_HEX: i32 = 18;

/// The band of separation an errand is drawn from, in hexes. The floor keeps
/// the factor from paying handsomely for a hop the player was making anyway;
/// the ceiling keeps him from naming a harbour a whole ocean away as though
/// that were a normal week's work.
const NEAR: i32 = 5;
const FAR: i32 = 30;

/// Parcel size. Small enough to fit the opening hull's forty units of hold
/// alongside its stores, because an errand the starting ship cannot physically
/// accept is an errand offered to the wrong player.
const QTY_MIN: i32 = 6;
const QTY_MAX: i32 = 18;

/// Standing granted at the paying port on completion. See `Markets::oblige`.
const FAVOUR_MIN: i32 = 2;
const FAVOUR_MAX: i32 = 5;

/// One arrival in this many carries an offer. Not every landfall, or the
/// chronicle becomes a job board and the errands stop reading as luck.
pub const OFFER_CHANCE_PERCENT: u32 = 35;

/// Everything `draw` needs to know about the player, gathered by the caller so
/// this module can stay a pure function of the world and the dice. `visited`
/// and `traded` are the two facts the whole feature turns on and neither
/// existed before it: `discovered` means *sighted from the masthead*, which is
/// not the same as having been ashore, and nothing anywhere recorded which
/// goods a player had ever bought.
pub struct Ledger<'a> {
    pub visited: &'a [bool],
    pub discovered: &'a [bool],
    pub traded: &'a [bool],
    /// Ports this hull may not enter. A commission to a harbour she draws too
    /// much for is the same trap as a course laid to one.
    pub barred: &'a dyn Fn(usize) -> bool,
    pub distance: &'a dyn Fn(usize, usize) -> i32,
}

/// Weight given to a candidate the player has never called at, against 1 for
/// one they have. Not exclusion: a run of unlucky rolls, or a late game with
/// nowhere left unvisited, must still produce an offer rather than silence.
const UNVISITED_WEIGHT: i32 = 8;
/// The same, for a good never bought.
const UNTRADED_WEIGHT: i32 = 6;

/// Draw an offer for a player standing in `here`, or `None` if the world cannot
/// supply one: no charted port in the band that this hull may enter, or no good
/// tradeable at both ends of the errand.
///
/// The dice come from the voyage's `Rng` rather than from `hash3`, and that is
/// the opposite of the choice `market::queue_key` makes one file over. It is
/// deliberate and the distinction is the useful one: which goods a harbour
/// opens is a fact about the harbour, true before the player was born and the
/// same in every game seeded alike. What a factor happens to need this week is
/// not a fact about anything. It is weather.
pub fn draw(
    rng: &mut Rng,
    markets: &Markets,
    here: usize,
    ledger: &Ledger,
) -> Option<Commission> {
    if !Markets::trades(here) {
        return None;
    }

    let other = pick_port(rng, here, ledger)?;
    let kind = if rng.chance(50) { Kind::Deliver } else { Kind::Collect };

    // Written around where the parcel is bought, so the offer never names a
    // good the far port has never heard of.
    let source = match kind {
        Kind::Deliver => here,
        Kind::Collect => other,
    };
    let good = pick_good(rng, markets, source, ledger)?;
    let unit = markets.buy_price(source, good)?;

    let qty = rng.range(QTY_MIN, QTY_MAX + 1);
    let hexes = (ledger.distance)(here, other);
    let gold = qty * unit * (100 + MARGIN_PERCENT) / 100 + hexes * GOLD_PER_HEX;
    let favour = rng.range(FAVOUR_MIN, FAVOUR_MAX + 1);

    Some(Commission { kind, issued_at: here, other, good, qty, gold, favour })
}

/// The far end. Charted, enterable, in the distance band, and heavily weighted
/// toward one the player has never been ashore at.
fn pick_port(rng: &mut Rng, here: usize, ledger: &Ledger) -> Option<usize> {
    let eligible = |p: usize| {
        p != here
            && ledger.discovered[p]
            && Markets::trades(p)
            && !(ledger.barred)(p)
            && {
                let d = (ledger.distance)(here, p);
                (NEAR..=FAR).contains(&d)
            }
    };
    weighted_pick(rng, PORTS.len(), |p| {
        if !eligible(p) {
            0
        } else if ledger.visited[p] {
            1
        } else {
            UNVISITED_WEIGHT
        }
    })
}

/// The parcel. Has to be openly on sale where the errand expects it bought,
/// and is weighted toward one the player has never bought anywhere.
fn pick_good(rng: &mut Rng, markets: &Markets, source: usize, ledger: &Ledger) -> Option<usize> {
    weighted_pick(rng, GOODS.len(), |g| {
        if !markets.is_open(source, g) || markets.buy_price(source, g).is_none() {
            0
        } else if ledger.traded[g] {
            1
        } else {
            UNTRADED_WEIGHT
        }
    })
}

/// Pick an index in `0..n` in proportion to `weight`, or `None` if every weight
/// is zero.
fn weighted_pick(rng: &mut Rng, n: usize, weight: impl Fn(usize) -> i32) -> Option<usize> {
    let total: i32 = (0..n).map(&weight).sum();
    if total <= 0 {
        return None;
    }
    let mut roll = rng.below(total as u32) as i32;
    for i in 0..n {
        roll -= weight(i);
        if roll < 0 {
            return Some(i);
        }
    }
    None
}
