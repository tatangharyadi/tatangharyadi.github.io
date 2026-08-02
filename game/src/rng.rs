//! A small deterministic generator.
//!
//! The crate has no dependencies, which rules out `rand`, and it targets
//! `wasm32-unknown-unknown`, where there is no operating system to ask for
//! entropy anyway. Both of those are fine here: a seeded generator is what a
//! simulation wants. Give the same seed and the same keystrokes and you get the
//! same voyage, which is the difference between a bug you can reproduce and a
//! bug you can only describe.
//!
//! xorshift64*, which is short, fast and has no pretensions. Nothing here is
//! cryptographic and nothing here should ever be used as though it were.

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u32) -> Self {
        // Any non-zero state will do; xorshift is stuck at zero forever.
        Rng(seed as u64 ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `0..n`. Biased by at most one part in 2^64 divided by n,
    /// which for the values this game uses is not worth a rejection loop.
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as u32
    }

    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent
    }

    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + self.below((hi - lo) as u32) as i32
    }
}

/// A stable hash of three numbers, for values that must be the same every time
/// they are asked for without being stored: the wind over a given hex in a
/// given month, for instance. 2592 hexes times twelve months is small enough to
/// tabulate, but deriving it means the table cannot drift out of step with
/// itself after a save and reload.
pub fn hash3(a: i32, b: i32, c: i32) -> u32 {
    let mut h = 0x811C_9DC5u32;
    for v in [a, b, c] {
        h ^= v as u32;
        h = h.wrapping_mul(0x0100_0193);
        h ^= h >> 15;
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_sequence() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn below_stays_in_range() {
        let mut r = Rng::new(3);
        for _ in 0..1000 {
            assert!(r.below(6) < 6);
        }
    }

    #[test]
    fn hash_is_stable_and_spread() {
        assert_eq!(hash3(1, 2, 3), hash3(1, 2, 3));
        assert_ne!(hash3(1, 2, 3), hash3(1, 2, 4));
    }
}
