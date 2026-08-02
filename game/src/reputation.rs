//! What the sea thinks of you.
//!
//! One signed number, and the argument for giving it a module of its own is
//! that it is the only thing in the simulation that is *read by* something other
//! than the code that writes it. Gold is spent, damage is repaired, cargo is
//! sold; reputation exists to be looked up by the pirates, who behave
//! differently toward a man they have heard of.
//!
//! It runs from [`FLOOR`] to [`CEILING`], starting at nought, and it moves in
//! two directions for two different reasons:
//!
//! * **up** for beating raiders, more for sinking one than for driving her off;
//! * **down** for firing on a merchant, whether or not the attempt succeeds,
//!   because the offence is the attack rather than the profit.
//!
//! It also **fades**. A point a month, toward nought, in either direction. That
//! is deliberate and it is the part that turns a score into a standing: fame is
//! not banked, it is maintained, and a reformed raider is eventually just
//! another sail. Without the fade the number is a ratchet and every long game
//! ends at one extreme or the other, where nothing reads it any differently.

pub const FLOOR: i32 = -100;
pub const CEILING: i32 = 100;

/// Points for driving a raider off, before her strength is counted.
pub const BEAT_PIRATE: i32 = 2;
/// Extra points for sinking her outright rather than letting her run.
pub const SINK_BONUS: i32 = 3;
/// What firing on a merchant costs, win or lose.
pub const RAID_MERCHANT: i32 = -12;

/// How far a strong opponent is worth beyond the flat rate. Pirate strengths run
/// 8 to 40, so this adds nothing to nought and up to three to the worst of them.
pub const STRENGTH_PER_POINT: i32 = 12;

/// How many points of reputation move a pirate's hunting range by one hex.
///
/// At `40` the full range of the score is worth two hexes either way against a
/// base of five, which is enough to feel in play without ever switching the
/// pirates off: a saint is still hunted from three hexes and a villain is not
/// hunted from across the ocean.
pub const HEXES_PER_POINT: i32 = 40;

/// What resisting a king's ship costs on top of whatever put her there.
pub const RESIST_NAVY: i32 = -6;
/// And what sinking one costs beyond merely driving her off.
pub const SINK_NAVY: i32 = -10;

/// The score at which the crown first takes an interest.
///
/// It sits at the bottom of the "under suspicion" band rather than deeper, and
/// the arithmetic is the reason: a raid is [`RAID_MERCHANT`] and the band starts
/// at fifteen, so a single merchant taken out of curiosity brings nobody, and
/// the second one does. That is the shape the mechanic wants. One mistake is a
/// mistake; twice is a habit, and a habit is what a navy is for.
pub const NAVY_FROM: i32 = -15;
/// Further points of infamy that buy the player one more hunter.
pub const NAVY_PER_SHIP: i32 = 20;
/// The most the crown will ever have at sea after one man at once.
///
/// Capped because the fleet is real ships on a real map and five of them
/// converging is already a situation with no good answer. Past that it stops
/// being pressure and becomes an unloseable board, and the player still has to
/// be able to run for it and mend their name.
pub const NAVY_MAX: usize = 5;

/// How many king's ships are out looking for you at this score.
pub fn navy_wanted(value: i32) -> usize {
    if value > NAVY_FROM {
        return 0;
    }
    let extra = (NAVY_FROM - value) / NAVY_PER_SHIP;
    (1 + extra as usize).min(NAVY_MAX)
}

/// Being taken and fined settles part of the account.
///
/// Without this the only way out of a bad name is to wait, one point a month,
/// which for a player at the floor is eight years of game time and in practice
/// a dead save. Answering for it in a prize court is the other way, and it is
/// deliberately not a cheap one: the loss that comes with it is half the purse
/// and the whole hold.
pub fn answered_for(value: i32) -> i32 {
    if value >= 0 {
        return value;
    }
    value / 2
}

/// Clamp a proposed value into the band.
pub fn clamp(value: i32) -> i32 {
    value.clamp(FLOOR, CEILING)
}

/// One month's fade, toward nought from wherever it is.
pub fn fade(value: i32) -> i32 {
    match value {
        v if v > 0 => v - 1,
        v if v < 0 => v + 1,
        v => v,
    }
}

/// What is worth saying in the log when the number crosses into a new band.
///
/// The bands are asymmetric on purpose. Going up is a slow accumulation and
/// deserves few names; going down happens twelve points at a time and wants
/// more, so that a player who raids one merchant out of curiosity sees the
/// consequence before it is a habit.
pub fn standing(value: i32) -> &'static str {
    match value {
        v if v >= 60 => "a scourge of pirates",
        v if v >= 25 => "well spoken of",
        v if v > -15 => "unremarked",
        v if v > -40 => "under suspicion",
        v if v > -70 => "a known raider",
        _ => "hunted by every honest flag",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_score_never_leaves_its_band() {
        assert_eq!(clamp(500), CEILING);
        assert_eq!(clamp(-500), FLOOR);
        assert_eq!(clamp(7), 7);
    }

    #[test]
    fn fame_fades_toward_nothing_from_either_side() {
        assert_eq!(fade(5), 4);
        assert_eq!(fade(-5), -4);
        assert_eq!(fade(0), 0);
    }

    /// The fade has to actually terminate at nought rather than oscillate, which
    /// is the one way a two-armed `match` like this goes wrong.
    #[test]
    fn the_fade_settles_at_nothing_and_stays() {
        for start in [CEILING, FLOOR, 1, -1] {
            let mut v = start;
            for _ in 0..(CEILING - FLOOR + 10) {
                v = fade(v);
            }
            assert_eq!(v, 0, "a score of {start} did not settle");
        }
    }

    /// Every reachable value has a name, and the bands run in the right order.
    #[test]
    fn every_score_has_a_standing_and_they_are_ordered() {
        let mut seen = Vec::new();
        for v in FLOOR..=CEILING {
            let s = standing(v);
            if seen.last().map_or(true, |l| *l != s) {
                seen.push(s);
            }
        }
        assert_eq!(
            seen,
            vec![
                "hunted by every honest flag",
                "a known raider",
                "under suspicion",
                "unremarked",
                "well spoken of",
                "a scourge of pirates",
            ],
            "the bands are out of order or one is unreachable"
        );
    }

    /// The pressure has to arrive gradually and it has to stop. A step function
    /// that jumped straight to the cap, or one that never capped, would both be
    /// the same thing in play: a board with no way back.
    #[test]
    fn the_crown_takes_an_interest_by_degrees_and_then_stops() {
        assert_eq!(navy_wanted(0), 0, "an honest master is left alone");
        assert_eq!(navy_wanted(NAVY_FROM + 1), 0, "they came a point early");
        assert_eq!(navy_wanted(NAVY_FROM), 1, "the first offence brought nobody");
        assert_eq!(navy_wanted(FLOOR), NAVY_MAX, "the worst man alive is not hunted");

        let mut last = 0;
        for v in (FLOOR..=CEILING).rev() {
            let n = navy_wanted(v);
            assert!(n >= last, "the fleet shrank as the player got worse at {v}");
            assert!(n <= NAVY_MAX, "the cap does not hold at {v}");
            last = n;
        }
    }

    /// One raid is a mistake and brings nobody; two is a habit and brings the
    /// crown. This is the whole balance of the thing in one assertion.
    #[test]
    fn one_raid_is_forgiven_and_the_second_is_not() {
        assert_eq!(navy_wanted(clamp(RAID_MERCHANT)), 0);
        assert!(navy_wanted(clamp(RAID_MERCHANT * 2)) >= 1);
    }

    /// Answering for it has to actually move the number toward nought and has to
    /// terminate, or "surrender and pay" is a loop rather than a way out.
    #[test]
    fn answering_for_it_is_a_way_back() {
        let mut v = FLOOR;
        for _ in 0..12 {
            let next = answered_for(v);
            assert!(next > v || next == 0, "a fine at {v} settled nothing");
            v = next;
        }
        assert_eq!(v, 0, "no number of prize courts ever cleared the name");
        assert_eq!(answered_for(40), 40, "an honest master was fined for it");
    }
}
