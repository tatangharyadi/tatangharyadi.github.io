//! Hexagonal grid mathematics.
//!
//! Follows the conventions in <https://www.redblobgames.com/grids/hexagons/>.
//! Three coordinate systems are in play and each earns its place:
//!
//! * **Offset (odd-r)** is how the world is stored, because the map is a
//!   rectangle and a rectangle of hexes indexes naturally as rows and columns.
//! * **Axial** is how it is passed around, because it is two numbers.
//! * **Cube** is how arithmetic is done, because `q + r + s == 0` makes
//!   distance, rounding and interpolation fall out cleanly.
//!
//! Offset coordinates cannot be safely added or subtracted: the direction
//! vectors depend on whether the row is odd or even. That is the entire reason
//! the conversions below exist, and the reason nothing outside this module is
//! allowed to do arithmetic on a column and a row.
//!
//! The world wraps east to west, because it is a globe and sailing west from
//! the Americas should reach Asia. It does not wrap north to south, because
//! this game does not model crossing the pole.

use crate::world::{COLS, ROWS};

/// Axial coordinate. `s` is not stored because `s == -q - r`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Hex {
    pub q: i32,
    pub r: i32,
}

impl Hex {
    pub const fn new(q: i32, r: i32) -> Self {
        Hex { q, r }
    }

    pub const fn s(self) -> i32 {
        -self.q - self.r
    }
}

/// The six neighbours of a hex, in axial deltas, starting due east and going
/// anticlockwise. The order is fixed because the keyboard bindings and the
/// wind directions both index into it.
///
/// Index 0 is E, 1 is NE, 2 is NW, 3 is W, 4 is SW, 5 is SE.
pub const DIRECTIONS: [Hex; 6] = [
    Hex::new(1, 0),
    Hex::new(1, -1),
    Hex::new(0, -1),
    Hex::new(-1, 0),
    Hex::new(-1, 1),
    Hex::new(0, 1),
];

pub const DIR_NAMES: [&str; 6] = ["east", "north-east", "north-west", "west", "south-west", "south-east"];

pub fn neighbour(h: Hex, dir: usize) -> Hex {
    let d = DIRECTIONS[dir % 6];
    Hex::new(h.q + d.q, h.r + d.r)
}

/// Distance in hexes. This is the cube distance written in axial terms: half
/// the sum of the three absolute cube differences.
pub fn distance(a: Hex, b: Hex) -> i32 {
    let dq = a.q - b.q;
    let dr = a.r - b.r;
    let ds = a.s() - b.s();
    (dq.abs() + dr.abs() + ds.abs()) / 2
}

/// Distance in hexes, taking the shorter way round the globe.
///
/// [`distance`] measures in unwrapped axial space, which is right for line of
/// sight because a sight line never crosses the seam twice. It is wrong for
/// "how far is Nagasaki", where the answer may be to sail west. Adding a whole
/// map width to a column adds the same amount to `q`, so the three candidates
/// below are the only ones worth checking.
pub fn wrapped_distance(a: Hex, b: Hex) -> i32 {
    let direct = distance(a, b);
    let east = distance(a, Hex::new(b.q + COLS, b.r));
    let west = distance(a, Hex::new(b.q - COLS, b.r));
    direct.min(east).min(west)
}

// ---------------------------------------------------------------------------
// Offset conversions
// ---------------------------------------------------------------------------

/// Odd-r offset to axial. Odd rows are shifted right by half a hex.
pub fn from_offset(col: i32, row: i32) -> Hex {
    Hex::new(col - (row - (row & 1)) / 2, row)
}

/// Axial to odd-r offset. The column is wrapped into the map; the row is not,
/// so a row outside the map stays outside and the caller has to notice.
pub fn to_offset(h: Hex) -> (i32, i32) {
    let col = h.q + (h.r - (h.r & 1)) / 2;
    (col.rem_euclid(COLS), h.r)
}

/// The canonical `Hex` for the cell this one names.
///
/// The world wraps east to west, so `q` and `q + 72` are the same place, and
/// two `Hex` values that name one cell will still compare unequal. Everything
/// that goes through [`index`] is safe because indexing wraps, but anything
/// that *stores* or *compares* a position is not: a ship that sailed west out
/// of the Atlantic and round to Japan would stop recognising the harbour it was
/// sitting in.
///
/// So: axial arithmetic runs unwrapped, because sight lines and distances need
/// it to, and every stored position is put through here first. Idempotent, and
/// the round trip is exactly what [`to_offset`] already does to the column.
pub fn normalise(h: Hex) -> Hex {
    let (col, row) = to_offset(h);
    from_offset(col, row)
}

pub fn in_bounds(h: Hex) -> bool {
    h.r >= 0 && h.r < ROWS
}

/// Index into the row-major world arrays, or `None` off the top or bottom.
pub fn index(h: Hex) -> Option<usize> {
    if !in_bounds(h) {
        return None;
    }
    let (col, row) = to_offset(h);
    Some((row * COLS + col) as usize)
}

// ---------------------------------------------------------------------------
// Rounding and lines
// ---------------------------------------------------------------------------

/// Round fractional cube coordinates to the nearest hex.
///
/// Rounding each of the three axes independently can break the `q + r + s == 0`
/// constraint, so the component that moved furthest is recomputed from the
/// other two rather than rounded.
pub fn cube_round(fq: f32, fr: f32) -> Hex {
    let fs = -fq - fr;
    let mut q = fq.round();
    let mut r = fr.round();
    let s = fs.round();

    let dq = (q - fq).abs();
    let dr = (r - fr).abs();
    let ds = (s - fs).abs();

    if dq > dr && dq > ds {
        q = -r - s;
    } else if dr > ds {
        r = -q - s;
    }
    Hex::new(q as i32, r as i32)
}

/// The hexes on a straight line from `a` to `b`, inclusive of both.
///
/// Used for line of sight. The nudge on the start point keeps the line off
/// exact hex edges, where the rounding would otherwise pick arbitrarily and
/// sight lines would flicker between symmetric cases.
pub fn line(a: Hex, b: Hex, out: &mut Vec<Hex>) {
    out.clear();
    let n = distance(a, b);
    if n == 0 {
        out.push(a);
        return;
    }
    let aq = a.q as f32 + 1e-6;
    let ar = a.r as f32 + 1e-6;
    let bq = b.q as f32;
    let br = b.r as f32;
    let step = 1.0 / n as f32;
    for i in 0..=n {
        let t = step * i as f32;
        out.push(cube_round(aq + (bq - aq) * t, ar + (br - ar) * t));
    }
}

/// Every hex within `radius` of `centre`, including the centre.
pub fn within(centre: Hex, radius: i32, out: &mut Vec<Hex>) {
    out.clear();
    for dq in -radius..=radius {
        let lo = (-radius).max(-dq - radius);
        let hi = radius.min(-dq + radius);
        for dr in lo..=hi {
            out.push(Hex::new(centre.q + dq, centre.r + dr));
        }
    }
}

// ---------------------------------------------------------------------------
// Pixels
// ---------------------------------------------------------------------------

/// Centre of a hex in pixels, pointy-top.
///
/// The width of a pointy-top hex is `sqrt(3) * size` and its vertical spacing
/// is `3/2 * size`, which is where the two constants come from.
pub fn to_pixel(h: Hex, size: f32) -> (f32, f32) {
    const SQRT3: f32 = 1.732_050_8;
    let x = size * (SQRT3 * h.q as f32 + SQRT3 / 2.0 * h.r as f32);
    let y = size * (1.5 * h.r as f32);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_round_trips() {
        for row in 0..ROWS {
            for col in 0..COLS {
                let (c, r) = to_offset(from_offset(col, row));
                assert_eq!((c, r), (col, row), "at {col},{row}");
            }
        }
    }

    #[test]
    fn neighbours_are_one_away() {
        let h = Hex::new(3, -2);
        for d in 0..6 {
            assert_eq!(distance(h, neighbour(h, d)), 1);
        }
    }

    #[test]
    fn cube_constraint_holds_after_rounding() {
        let h = cube_round(1.4, -0.7);
        assert_eq!(h.q + h.r + h.s(), 0);
    }

    #[test]
    fn a_line_is_contiguous_and_ends_where_asked() {
        let a = Hex::new(0, 0);
        let b = Hex::new(4, -2);
        let mut out = Vec::new();
        line(a, b, &mut out);
        assert_eq!(out.first().copied(), Some(a));
        assert_eq!(out.last().copied(), Some(b));
        assert_eq!(out.len() as i32, distance(a, b) + 1);
        for pair in out.windows(2) {
            assert_eq!(distance(pair[0], pair[1]), 1);
        }
    }

    #[test]
    fn within_counts_the_centred_hex_number() {
        let mut out = Vec::new();
        for radius in 0..6 {
            within(Hex::new(0, 0), radius, &mut out);
            // 1, 7, 19, 37 ... = 3r(r+1) + 1
            assert_eq!(out.len() as i32, 3 * radius * (radius + 1) + 1);
        }
    }
}
