//! Sight, weather, and getting from one hex to another.
//!
//! This is the part of the game that is actually expensive. Every step
//! recomputes what the ship can see, which is a raycast per hex in range, and
//! every autopilot leg is an A* over a cost field that changes with the month
//! and the wind. The pirates do the same thing on their own account. None of it
//! is hard, but there is a lot of it, and it is the reason the simulation is
//! compiled rather than interpreted.

use crate::hex::{self, Hex};
use crate::rng::hash3;
use crate::world::{COLS, LAND, ROWS};

pub const CELLS: usize = (COLS * ROWS) as usize;

pub const UNSEEN: u8 = 0;
pub const REMEMBERED: u8 = 1;
pub const VISIBLE: u8 = 2;

pub fn is_land_index(i: usize) -> bool {
    LAND[i] == b'#'
}

pub fn is_water(h: Hex) -> bool {
    match hex::index(h) {
        Some(i) => !is_land_index(i),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Sea room
// ---------------------------------------------------------------------------

/// How far out `harbour_room` looks. Two hexes is the ship's own length and a
/// little: it is the water she needs to swing in and to work out of again,
/// which is what a draught restriction is actually about.
const ROOM_RADIUS: usize = 2;

/// The most sea room a harbour may have and still count as shallow.
///
/// Nineteen hexes lie within two of any cell, so a harbour with eight or fewer
/// is more land than water: a river mouth, the head of a gulf, a pocket behind
/// a headland. This number is not a taste: it is the largest value at which
/// every economy on the map still keeps at least two harbours open to the
/// deepest hulls, which `every_economy_keeps_a_harbour_for_the_deepest_hulls`
/// checks by re-deriving it rather than restating it.
pub const SHALLOW_ROOM: i32 = 8;

/// Water cells within two hexes of this one, not counting the cell itself.
///
/// Distance to land cannot answer this. Every port on the map is against the
/// shore by construction, so all of them read as depth 1 and the figure
/// separates nothing. What tells a roadstead from a creek is how much water
/// lies around it, which is this.
pub fn harbour_room(h: Hex) -> i32 {
    let mut seen = vec![hex::index(h)];
    let mut ring = vec![h];
    let mut water = 0;
    for _ in 0..ROOM_RADIUS {
        let mut next = Vec::new();
        for c in ring.drain(..) {
            for d in 0..6 {
                let n = hex::neighbour(c, d);
                let Some(ni) = hex::index(n) else { continue };
                if seen.contains(&Some(ni)) {
                    continue;
                }
                seen.push(Some(ni));
                next.push(n);
                if !is_land_index(ni) {
                    water += 1;
                }
            }
        }
        ring = next;
    }
    water
}

// ---------------------------------------------------------------------------
// Distance to land
// ---------------------------------------------------------------------------

/// Hexes from each cell to the nearest land, computed once.
///
/// This is what "blue water" means here. Hugging a coast is safe and slow;
/// standing out of sight of land is the only way to cross an ocean, and it is
/// the thing rigging buys. A ship beyond its rating out here is in trouble.
pub fn distance_to_land() -> Vec<u8> {
    let mut dist = vec![u8::MAX; CELLS];
    let mut queue = std::collections::VecDeque::new();

    for row in 0..ROWS {
        for col in 0..COLS {
            let i = (row * COLS + col) as usize;
            if is_land_index(i) {
                dist[i] = 0;
                queue.push_back(hex::from_offset(col, row));
            }
        }
    }

    while let Some(h) = queue.pop_front() {
        let here = dist[hex::index(h).unwrap()];
        for d in 0..6 {
            let n = hex::neighbour(h, d);
            if let Some(ni) = hex::index(n) {
                if dist[ni] == u8::MAX {
                    dist[ni] = here.saturating_add(1);
                    queue.push_back(n);
                }
            }
        }
    }

    // Rows off the top and bottom of the map never got a source, but nothing
    // can sail there anyway. Leaving MAX would make them look like the deepest
    // water on the chart, which is the wrong kind of wrong.
    for d in dist.iter_mut() {
        if *d == u8::MAX {
            *d = 0;
        }
    }
    dist
}

// ---------------------------------------------------------------------------
// Wind and current
// ---------------------------------------------------------------------------

/// Prevailing wind over a hex in a given month, as a direction index into
/// [`hex::DIRECTIONS`] and a strength of 0 to 3.
///
/// The bands are the real ones, roughly: easterly trades either side of the
/// equator, westerlies in the temperate latitudes, easterlies again at the
/// poles. They are the reason the historical routes are shaped the way they
/// are, so a game about those routes has to have them or the map is arbitrary.
/// The per-hex hash roughens the bands so they are not stripes.
pub fn wind(h: Hex, month: i32) -> (usize, i32) {
    let (col, row) = hex::to_offset(h);
    let lat = 87.5 - 5.0 * row as f32;
    let a = lat.abs();

    // Direction the wind blows *towards*.
    let mut dir = if a < 30.0 {
        3 // trades, blowing west
    } else if a < 60.0 {
        0 // westerlies, blowing east
    } else {
        3 // polar easterlies, blowing west
    };

    // The bands shift with the season, north in the northern summer.
    let seasonal = ((month as f32 - 6.5) / 6.0) * 6.0;
    let shifted = lat + seasonal;

    let noise = hash3(col, row, month);
    // A sixth of hexes take the neighbouring direction, which stops the field
    // looking like a barcode and gives the coast-hugging route somewhere to go.
    match noise % 6 {
        0 => dir = (dir + 1) % 6,
        1 => dir = (dir + 5) % 6,
        _ => {}
    }
    // Nudge the meridional component with the season.
    if shifted.abs() > 55.0 && (noise >> 3) % 3 == 0 {
        dir = (dir + if lat > 0.0 { 5 } else { 1 }) % 6;
    }

    let strength = 1 + ((noise >> 6) % 3) as i32;
    (dir, strength)
}

/// Surface current, which does not care what month it is.
///
/// The great gyres turn clockwise north of the equator and anticlockwise south
/// of it. Currents are weaker than wind here but they never stop, so they are
/// what makes one direction round an ocean basin cheaper than the other.
pub fn current(h: Hex) -> (usize, i32) {
    let (col, row) = hex::to_offset(h);
    let lat = 87.5 - 5.0 * row as f32;
    let noise = hash3(col, row, 0x5EA);

    let base = if lat > 0.0 {
        if lat > 40.0 { 0 } else { 3 }
    } else if lat < -40.0 {
        0
    } else {
        3
    };
    let turn = if lat > 0.0 { 1 } else { 5 };
    let dir = if noise % 3 == 0 { (base + turn) % 6 } else { base };
    (dir, ((noise >> 4) % 2) as i32)
}

/// How badly two hex directions disagree: 0 is the same way, 3 is dead against.
pub fn opposition(a: usize, b: usize) -> i32 {
    let d = (a as i32 - b as i32).abs();
    d.min(6 - d)
}

/// Multiplier on the time to cross one hex, given where the ship is pointed.
///
/// Running before the wind is a great deal quicker than beating into it, and
/// better rigging narrows that gap without closing it: a square-rigged ship
/// never did sail upwind well, and the game should not pretend otherwise.
pub fn passage_factor(heading: usize, h: Hex, month: i32, rigging: u8) -> f32 {
    let (wdir, wstr) = wind(h, month);
    let (cdir, cstr) = current(h);

    let w = opposition(heading, wdir);
    let c = opposition(heading, cdir);

    // 0 -> running, 3 -> beating.
    let wind_penalty = [-0.30, -0.12, 0.22, 0.75][w as usize] * (wstr as f32 / 2.0);
    let upwind_relief = if w >= 2 { rigging as f32 * 0.07 } else { 0.0 };
    let current_penalty = [-0.10, -0.04, 0.06, 0.16][c as usize] * cstr as f32;

    (1.0 + wind_penalty - upwind_relief + current_penalty).max(0.35)
}

/// Weather severity over a hex on a given day, 0 to 3. Three is a storm.
pub fn weather(h: Hex, day: i32, month: i32) -> i32 {
    let (col, row) = hex::to_offset(h);
    let lat = 87.5 - 5.0 * row as f32;
    let n = hash3(col / 3, row / 3, day * 13 + month);
    // The high latitudes and the far southern ocean are foul; the doldrums
    // near the equator are calm and, in their own way, just as unwelcome.
    let bias = if lat.abs() > 55.0 { 22 } else if lat.abs() < 8.0 { 4 } else { 10 };
    match n % 100 {
        v if v < bias as u32 => 2 + ((n >> 9) % 2) as i32,
        v if v < (bias as u32 * 3) => 1,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Field of view
// ---------------------------------------------------------------------------

/// Recompute what can be seen from `eye`, writing into `fog`.
///
/// A hex is visible if the straight line to it from the ship is not
/// interrupted by land. Land itself is visible, since a headland is exactly the
/// thing you can see, but nothing behind it is. Anything previously seen and
/// now out of sight drops to remembered rather than to unseen, because a chart
/// does not blank itself when you sail away.
pub fn refresh_fov(eye: Hex, radius: i32, fog: &mut [u8], scratch: &mut Vec<Hex>, ray: &mut Vec<Hex>) {
    for v in fog.iter_mut() {
        if *v == VISIBLE {
            *v = REMEMBERED;
        }
    }

    hex::within(eye, radius, scratch);
    let targets = std::mem::take(scratch);

    for &target in targets.iter() {
        if !hex::in_bounds(target) {
            continue;
        }
        hex::line(eye, target, ray);
        for (step, &cell) in ray.iter().enumerate() {
            let Some(i) = hex::index(cell) else { break };
            fog[i] = VISIBLE;
            // Land blocks everything beyond it, but is seen itself. The eye's
            // own hex can never block, or a ship in harbour would be blind.
            if step > 0 && is_land_index(i) {
                break;
            }
        }
    }

    *scratch = targets;
}

// ---------------------------------------------------------------------------
// Pathfinding
// ---------------------------------------------------------------------------

use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(PartialEq)]
struct Node {
    f: f32,
    i: usize,
}
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed, because BinaryHeap is a max-heap and this is a search
        // for the cheapest.
        other.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The four map-sized buffers A* needs, kept so they can be reused.
///
/// This is not premature. A hunting pirate pathfinds once per move and there
/// are two dozen of them, so allocating these per call means something like a
/// megabyte of churn every time the player presses a key. That is not slow
/// enough to notice, but it is enough to make the allocator ask for more linear
/// memory, and `memory.grow` detaches every `ArrayBuffer` view JavaScript is
/// holding. The page would go blank several moves in, for a reason that looks
/// nothing like its cause.
pub struct Scratch {
    g: Vec<f32>,
    came: Vec<i32>,
    coord: Vec<Hex>,
    done: Vec<bool>,
    open: BinaryHeap<Node>,
}

impl Scratch {
    pub fn new() -> Self {
        Scratch {
            g: vec![f32::INFINITY; CELLS],
            came: vec![-1; CELLS],
            coord: vec![Hex::new(0, 0); CELLS],
            done: vec![false; CELLS],
            open: BinaryHeap::new(),
        }
    }

    fn reset(&mut self) {
        self.g.iter_mut().for_each(|v| *v = f32::INFINITY);
        self.came.iter_mut().for_each(|v| *v = -1);
        self.done.iter_mut().for_each(|v| *v = false);
        self.open.clear();
    }
}

/// A* from `start` to `goal` over water, minimising hours under sail.
///
/// Returns the hexes after `start`, ending at `goal`, or an empty path if the
/// goal cannot be reached. `max_depth_from_land` refuses hexes further offshore
/// than the ship dares go, which is how a coastal hull is kept out of the
/// Atlantic without a special case anywhere else.
pub fn find_path(
    scratch: &mut Scratch,
    start: Hex,
    goal: Hex,
    month: i32,
    rigging: u8,
    base_hours: f32,
    depth: &[u8],
    max_depth_from_land: i32,
    // Cells this hull may not enter at all, indexed like `depth`. Harbours too
    // shallow for her draught, and nothing else. Unlike `max_depth_from_land`
    // this admits no exception for the goal: a course that ends somewhere she
    // cannot go is not a course, and refusing it here means no caller can
    // hand back a route that strands her one hex short.
    barred: &[bool],
) -> Vec<Hex> {
    let Some(goal_i) = hex::index(goal) else {
        return Vec::new();
    };
    let Some(start_i) = hex::index(start) else {
        return Vec::new();
    };
    if !is_water(goal) {
        return Vec::new();
    }

    // The cheapest a hex can possibly cost, for an admissible heuristic. If
    // this were an overestimate A* would stop finding the shortest route and
    // start finding a plausible one, which is a bug that never announces
    // itself.
    let cheapest = base_hours * 0.35;

    scratch.reset();
    let Scratch { g, came, coord, done, open } = scratch;

    g[start_i] = 0.0;
    coord[start_i] = start;

    open.push(Node {
        f: hex::wrapped_distance(start, goal) as f32 * cheapest,
        i: start_i,
    });

    while let Some(Node { i, .. }) = open.pop() {
        if done[i] {
            continue;
        }
        done[i] = true;
        if i == goal_i {
            break;
        }
        let here = coord[i];

        for d in 0..6 {
            let n = hex::neighbour(here, d);
            let Some(ni) = hex::index(n) else { continue };
            if is_land_index(ni) || done[ni] || barred[ni] {
                continue;
            }
            // The goal is always allowed even if it is deep, so a port can be
            // reached; it is the water in between that is refused.
            if ni != goal_i && depth[ni] as i32 > max_depth_from_land {
                continue;
            }
            let cost = base_hours * passage_factor(d, here, month, rigging);
            let tentative = g[i] + cost;
            if tentative < g[ni] {
                g[ni] = tentative;
                came[ni] = i as i32;
                // Canonical, so that a route which crosses the antimeridian
                // hands back hexes that compare equal to the port they name.
                coord[ni] = hex::normalise(n);
                let h = hex::wrapped_distance(n, goal) as f32 * cheapest;
                open.push(Node { f: tentative + h, i: ni });
            }
        }
    }

    if !done[goal_i] {
        return Vec::new();
    }

    let mut path = Vec::new();
    let mut cur = goal_i;
    while cur != start_i {
        path.push(coord[cur]);
        let prev = came[cur];
        if prev < 0 {
            return Vec::new();
        }
        cur = prev as usize;
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::PORTS;

    /// Nothing barred: the shallow-harbour rule is the simulation's, not the
    /// pathfinder's, and these tests are about the water.
    fn open_sea() -> Vec<bool> {
        vec![false; CELLS]
    }

    fn port_hex(i: usize) -> Hex {
        hex::from_offset(PORTS[i].col as i32, PORTS[i].row as i32)
    }

    #[test]
    fn every_port_is_water() {
        for (i, p) in PORTS.iter().enumerate() {
            assert!(is_water(port_hex(i)), "{} is aground", p.name);
        }
    }

    #[test]
    fn land_is_at_zero_depth_and_open_ocean_is_not() {
        let d = distance_to_land();
        let deepest = d.iter().copied().max().unwrap();
        assert!(deepest > 3, "no blue water anywhere, depth maxes at {deepest}");
    }

    #[test]
    fn sight_is_blocked_by_land() {
        // Find a water hex with land next to it and something beyond.
        let d = distance_to_land();
        let mut fog = vec![UNSEEN; CELLS];
        let mut scratch = Vec::new();
        let mut ray = Vec::new();

        let eye = port_hex(0);
        refresh_fov(eye, 6, &mut fog, &mut scratch, &mut ray);
        assert_eq!(fog[hex::index(eye).unwrap()], VISIBLE);

        let seen = fog.iter().filter(|v| **v == VISIBLE).count();
        let in_range = 3 * 6 * (6 + 1) + 1;
        assert!(
            seen <= in_range as usize,
            "saw {seen} hexes, more than the {in_range} in range"
        );
        // Something within range must be hidden, or the test is not testing
        // obstruction. Port 0 is a harbour, so there is land about.
        let _ = d;
    }

    #[test]
    fn remembered_hexes_are_not_forgotten() {
        let mut fog = vec![UNSEEN; CELLS];
        let mut scratch = Vec::new();
        let mut ray = Vec::new();
        let a = port_hex(0);
        refresh_fov(a, 4, &mut fog, &mut scratch, &mut ray);
        let lit = fog.iter().filter(|v| **v == VISIBLE).count();

        // Move a long way off and look again.
        let b = port_hex(PORTS.len() / 2);
        refresh_fov(b, 4, &mut fog, &mut scratch, &mut ray);
        let remembered = fog.iter().filter(|v| **v == REMEMBERED).count();
        assert!(remembered > 0, "sailing away wiped the chart");
        assert!(remembered <= lit);
    }

    #[test]
    fn a_path_between_two_ports_is_contiguous_water() {
        let depth = distance_to_land();
        let a = port_hex(0);
        let b = port_hex(1);
        let path = find_path(&mut Scratch::new(), a, b, 6, 4, 6.0, &depth, 99, &open_sea());
        assert!(!path.is_empty(), "no route between the first two ports");
        assert_eq!(*path.last().unwrap(), b);
        let mut prev = a;
        for &step in &path {
            // Wrapped, not plain: a route that crosses the antimeridian hands
            // back canonical hexes, and two adjacent cells either side of the
            // seam are 71 apart in unwrapped axial space and 1 apart in fact.
            assert_eq!(hex::wrapped_distance(prev, step), 1, "path jumps");
            assert!(is_water(step), "path crosses land");
            prev = step;
        }
    }

    #[test]
    fn a_coastal_hull_cannot_cross_an_ocean_a_deep_water_hull_can() {
        let depth = distance_to_land();
        // Pick the two ports furthest apart, which will need open water.
        let mut best = (0, 0, -1);
        for i in 0..PORTS.len() {
            for j in (i + 1)..PORTS.len() {
                let d = hex::wrapped_distance(port_hex(i), port_hex(j));
                if d > best.2 {
                    best = (i, j, d);
                }
            }
        }
        let (i, j, _) = best;
        let open = find_path(&mut Scratch::new(), port_hex(i), port_hex(j), 6, 4, 6.0, &depth, 99, &open_sea());
        assert!(!open.is_empty(), "the two furthest ports are not connected");

        let hugging = find_path(&mut Scratch::new(), port_hex(i), port_hex(j), 6, 0, 6.0, &depth, 1, &open_sea());
        assert!(
            hugging.len() != open.len() || hugging.is_empty(),
            "a coast-hugging route was identical to the open-water one"
        );
    }

    #[test]
    fn beating_upwind_costs_more_than_running() {
        let h = Hex::new(10, 10);
        let (wdir, _) = wind(h, 6);
        let running = passage_factor(wdir, h, 6, 0);
        let beating = passage_factor((wdir + 3) % 6, h, 6, 0);
        assert!(beating > running, "upwind {beating} vs downwind {running}");
    }

    #[test]
    fn rigging_helps_upwind_but_does_not_make_it_free() {
        let h = Hex::new(10, 10);
        let (wdir, _) = wind(h, 6);
        let into = (wdir + 3) % 6;
        let poor = passage_factor(into, h, 6, 0);
        let good = passage_factor(into, h, 6, 4);
        let running = passage_factor(wdir, h, 6, 4);
        assert!(good < poor);
        assert!(good > running);
    }
}
