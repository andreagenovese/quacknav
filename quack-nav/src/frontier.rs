//! Frontiers: where the known floor meets the unknown, and how to get there.
//!
//! The model is the one every mapping robot vacuum uses: a global planner
//! on the map, a reactive layer for what the map does not know. "Map
//! everything" loops over this module: find the free cells that touch
//! unknown ones, group them, pick the nearest group by path cost, and
//! hand back the path. When no group is reachable, the map is as complete
//! as the duck can make it from where it can stand.
//!
//! Costs: known floor is cheap, unknown floor is dear — in mapped space the
//! shortest known route wins, and unknown is crossed only to reach a
//! frontier (the depth sensor inks free floor only along beams that reach
//! a wall, so a fresh map is a fan of streaks with unknown between them;
//! a planner that refused unknown would reach nothing). Walls, and the
//! host's **extra walls** — obstacles the sensor met that the map has not
//! inked, drop edges — are inflated by [`INFLATE_M`] and impassable. Cells
//! here are the map grid's: `(row, col)` with row 0 at `y_min`.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::map::{Cell, Grid};

/// Keep paths this far from a wall — the body is ~10 cm wide. Kept small
/// on purpose: the sensor's wedge is narrow and a fresh map has little
/// floor to spare; the mapping step's own wall margin does the rest.
pub const INFLATE_M: f64 = 0.15;
/// The inflation the planner runs with: the body's half-width, 0.10
/// (`QK_INFLATE_M`). Passability is physics; keeping away from walls
/// where there is room is the graded cost's job (`COMFORT_M`). With 0.15
/// the 0.54 m passage beside the twin's stairwell had 0.13 m of plannable
/// floor and a doorway of 0.42–0.50 m was always at the limit (the user's
/// rule, 2026-09-16: shrink the margins, the sensor decides).
pub fn inflate_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("QK_INFLATE_M")
            .ok()
            .and_then(|v| v.parse().ok())
            // 0.12 since the evening of the 16th: at the body's own 0.10
            // the route brushed door jambs (the user's eye).
            .unwrap_or(0.12)
    })
}
/// The least inflation that still fits the body (0.19 m wide): the
/// planner's last resort when the usual margin leaves no way out.
pub const SQUEEZE_INFLATE_M: f64 = 0.10;
/// Frontier cells this close to a sensor-seen obstacle are not frontiers:
/// the sensor has looked, the map's gap there is a wall it did not ink.
pub const SENSOR_KILL_M: f64 = 0.30;
/// A drop on the books (radius [`DROP_WALL_M`] or more) kills less: the
/// floor beyond the edge is dead, but the passage beside a stairwell is
/// not — twenty-five drops around the twin's stairwell, each killing
/// 0.42 m, erased the whole entrance of the 0.54 m passage and the south
/// of the flat was never tried (MuJoCo run 70).
pub const DROP_KILL_M: f64 = 0.08;
pub const DROP_WALL_M: f64 = 0.10;
/// A lane cell yields to a booked drop this near it: a walked cell is
/// floor, but not the rim itself — booked rim points are off by up to
/// 10 cm. Wide enough to keep the axis of the 0.44 m passage east of the
/// twin's stairwell open (0.22 m from its rim): the duck is to pass there
/// as the human did (the user's rule, 2026-09-16), the sensor judging.
pub const LANE_YIELDS_TO_DROP_M: f64 = 0.12;
/// The margin the planner keeps around a booked drop, on top of its own
/// radius: the body's half-width, never the wall margin. A drop's radius
/// already says how far the rim may be off; adding the wall's 0.15 made
/// 0.27 m a side, and the 0.54 m passage beside the twin's stairwell was
/// passable only if every rim point was booked to the centimetre — two
/// booked 6 and 11 cm inside it sealed the way, and the duck took a 10 m
/// detour (house1, 2026-09-15). The sensor guard, not the planner, keeps
/// the body off the edge. `QK_DROP_INFLATE` to measure.
/// 0.10: with the drop's own 0.12 that is 0.22 m — the guard's own lane
/// to a side, so the 0.44 m passage east of the twin's stairwell is
/// plannable down its axis with no help from anyone's trail (the user's
/// rule, 2026-09-16: the duck reasons freely, it only needs more
/// confidence). The leg simulator keeps 0.20 on an ordinary leg and 0.15
/// on a passage leg; a refusal by the books widens the planner round
/// that drop and steps back, so the 82-refusal deadlock of full7 cannot
/// recur.
/// 0.05 (0.17 with the rim's radius) since the afternoon of the 16th:
/// at 0.22 the plannable strip beside the 0.55 m passage ran 10–15 cm
/// from the west wall, the duck followed it onto the wall and the guard
/// said "no room" 25 times (frozen4). The route now runs 21 cm from the
/// wall; the leg simulator's 0.20 from a rim still holds on an
/// ordinary leg.
const DROP_INFLATE_DEFAULT: f64 = 0.05;
fn drop_inflate() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("QK_DROP_INFLATE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DROP_INFLATE_DEFAULT)
    })
}
/// How far short of a frontier the duck stands to look at it.
pub const VIEW_BACK_M: f64 = 0.5;
/// How far from its own cell the planner looks for passable floor to
/// start from, after the usual 0.35 m failed.
const START_REACH_M: f64 = 0.75;
const START_REACH_FAR_M: f64 = 1.5;
/// A frontier group smaller than this is a crack in the map, not a place
/// to go.
pub const MIN_FRONTIER_CELLS: usize = 8;
/// Path cost of a known free cell and of an unknown one. Unknown is not a
/// wall — the planner has always been willing to cross it — but at three
/// times the price it will walk a long way round to avoid a gap the duck
/// simply has not looked at yet, and a house mapped through a 45° wedge is
/// full of those. Measured on the twin (2026-09-13): the route the duck is
/// handed at the start of a journey is 1.46x the straight line, where the
/// same endpoints on a settled map cost 1.16-1.26x and the walls
/// themselves allow 1.19x. `QK_COST_UNKNOWN` to measure the price.
const COST_FREE: u32 = 10;
/// A lane cell — floor the body has stood on, or a saved drive's trail —
/// may cost less than a free cell (`QK_COST_LANE`, e.g. 5): where the
/// house has been walked, the planner follows the walk instead of
/// cutting the corner of a stairwell the guard then refuses (loc9,
/// 2026-09-16: 400 s at the north mouth, the human had never taken that
/// corner). Off by default (a free cell's price): with it on, the route
/// followed the human into the 0.44 m passage east of the stairwell,
/// which the passage law forbids the body (lane2) — the human's walk is
/// evidence of floor, not of a passage the duck's guards accept.
const COST_LANE_DEFAULT: u32 = COST_FREE;
fn cost_lane() -> u32 {
    static V: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("QK_COST_LANE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(COST_LANE_DEFAULT)
    })
}
const COST_UNKNOWN_DEFAULT: u32 = 30;
fn cost_unknown() -> u32 {
    static V: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("QK_COST_UNKNOWN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(COST_UNKNOWN_DEFAULT)
    })
}
/// Room costs less than a wall's side. A free cell within [`COMFORT_M`] of
/// the nearest wall or booked obstacle pays extra, from [`COST_HUG`] at the
/// inflation's edge down to nothing at [`COMFORT_M`]: with every free cell
/// at the same price the planner cut every corner at exactly the margin,
/// 15 cm from the wall, in the middle of an empty room (the user, watching
/// house1, 2026-09-15: "the route should pass close only when there is
/// little room"). In a passage narrower than twice [`COMFORT_M`] every
/// cell pays, so the shortest way wins there as before; the human driver
/// kept a median 0.38 m from the nearest obstacle. `QK_COST_HUG=0`
/// restores the flat price.
const COMFORT_M: f64 = 0.50;
const COST_HUG_DEFAULT: u32 = 30;
fn cost_hug() -> u32 {
    static V: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("QK_COST_HUG")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(COST_HUG_DEFAULT)
    })
}
/// Frontier cells counted toward a group's worth, at most: beyond this a
/// group is "a whole open side" and distance decides again.
pub const GAIN_CAP_CELLS: usize = 40;

/// One group of frontier cells and the way to its nearest cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Frontier {
    pub cells: usize,
    /// World coordinates of the group's centroid.
    pub centroid: (f64, f64),
    /// World coordinates of the group cell the path reaches first.
    pub target: (f64, f64),
    /// Where to stand and look at it: [`VIEW_BACK_M`] short of `target`
    /// along the path, on known floor. A frontier sits by definition
    /// against walls and furniture; walking onto it puts the beak on
    /// them, while a stand a little short of it maps it just as well.
    pub stand: (f64, f64),
    /// Path from the start to `target`, world coordinates, start excluded;
    /// `stand` is one of its points (or the start).
    pub path: Vec<(f64, f64)>,
    /// Path length, metres.
    pub distance_m: f64,
    /// Path cost (free cells cheap, unknown dear) — what "nearest" means.
    pub cost: u32,
    /// What the group is worth going for: path cost per frontier cell,
    /// cells counted up to [`GAIN_CAP_CELLS`]. Lower is better — a wide
    /// opening a room away beats a sliver behind the nearest chair.
    pub score: f64,
}

/// A world point the planner must treat as a wall, with the radius to
/// keep clear of it (on top of [`INFLATE_M`]).
pub type ExtraWall = ((f64, f64), f64);

/// A traversability mask: where the body may plan to go, and at what cost.
struct Costmap {
    rows: usize,
    cols: usize,
    /// 0 = impassable.
    cost: Vec<u32>,
}

impl Costmap {
    /// `lanes` are points the body has stood on: their cells (and the
    /// four neighbours) stay passable whatever the inflation and the
    /// extra walls say — the body was there, at the body's width, which
    /// is the one fact about passability the map cannot argue with. A
    /// drop the sensor saw on a bed and the wall margin sealed the door
    /// the duck had walked in through, for a quarter of an hour (MuJoCo
    /// run 70). A cell the map inks as wall is not a lane: the map may be
    /// right and the pose wrong.
    fn build(grid: &Grid, extra_walls: &[ExtraWall], inflate_m: f64, lanes: &[(f64, f64)]) -> Self {
        let r = (inflate_m / grid.cell_m).ceil() as isize;
        let mut cost = vec![0u32; grid.rows * grid.cols];
        let mut lane = vec![false; grid.rows * grid.cols];
        for &(lx, ly) in lanes {
            let Some((row, col)) = to_cell(grid, lx, ly) else { continue };
            for (dr, dc) in [(0isize, 0isize), (-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (rr, cc) = (row as isize + dr, col as isize + dc);
                if rr >= 0 && cc >= 0 && (rr as usize) < grid.rows && (cc as usize) < grid.cols {
                    lane[rr as usize * grid.cols + cc as usize] = true;
                }
            }
        }
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let base = match grid.cell(row, col) {
                    Some(Cell::Wall) => continue,
                    Some(Cell::Free) => COST_FREE,
                    _ => cost_unknown(),
                };
                if lane[row * grid.cols + col] {
                    let (wx, wy) = to_world(grid, (row, col));
                    let rim_near = extra_walls.iter().any(|((bx, by), radius)| {
                        *radius >= DROP_WALL_M && ((wx - bx).powi(2) + (wy - by).powi(2)).sqrt() < LANE_YIELDS_TO_DROP_M
                    });
                    if !rim_near {
                        cost[row * grid.cols + col] = cost_lane().max(1);
                        continue;
                    }
                }
                let mut clear = true;
                'scan: for dr in -r..=r {
                    for dc in -r..=r {
                        if dr * dr + dc * dc > r * r {
                            continue;
                        }
                        let (rr, cc) = (row as isize + dr, col as isize + dc);
                        if rr < 0 || cc < 0 {
                            continue;
                        }
                        if grid.cell(rr as usize, cc as usize) == Some(Cell::Wall) {
                            clear = false;
                            break 'scan;
                        }
                    }
                }
                if clear {
                    let (wx, wy) = to_world(grid, (row, col));
                    clear = !extra_walls.iter().any(|((bx, by), radius)| {
                        let margin = if *radius >= DROP_WALL_M { inflate_m.min(drop_inflate()) } else { inflate_m };
                        ((wx - bx).powi(2) + (wy - by).powi(2)).sqrt() < radius + margin
                    });
                }
                if clear {
                    cost[row * grid.cols + col] = base;
                }
            }
        }
        if cost_hug() > 0 {
            // Distance (in cells) from every passable cell to the nearest
            // impassable one — walls, their inflation, the booked
            // obstacles — by two chamfer passes; then the graded price.
            let (rows, cols) = (grid.rows, grid.cols);
            let far = (rows + cols) as u32;
            let mut dist: Vec<u32> = cost.iter().map(|c| if *c == 0 { 0 } else { far }).collect();
            for row in 0..rows {
                for col in 0..cols {
                    let i = row * cols + col;
                    if row > 0 {
                        dist[i] = dist[i].min(dist[i - cols] + 1);
                    }
                    if col > 0 {
                        dist[i] = dist[i].min(dist[i - 1] + 1);
                    }
                }
            }
            for row in (0..rows).rev() {
                for col in (0..cols).rev() {
                    let i = row * cols + col;
                    if row + 1 < rows {
                        dist[i] = dist[i].min(dist[i + cols] + 1);
                    }
                    if col + 1 < cols {
                        dist[i] = dist[i].min(dist[i + 1] + 1);
                    }
                }
            }
            let band = (COMFORT_M - inflate_m).max(grid.cell_m) / grid.cell_m;
            // A lane pays the graded price like any cell: exempt, the
            // trail a blind journey left along the stairwell's rim was
            // the cheapest floor there, and the route hugged the rim
            // instead of the middle between it and the wall (the user's
            // question, 2026-09-21). The lane's own discount, when on,
            // still applies.
            for i in 0..rows * cols {
                if cost[i] == 0 {
                    continue;
                }
                let d = dist[i] as f64 - 1.0;
                if d < band {
                    cost[i] += (f64::from(cost_hug()) * (1.0 - d / band)).round() as u32;
                }
            }
        }
        Self {
            rows: grid.rows,
            cols: grid.cols,
            cost,
        }
    }

    fn at(&self, row: usize, col: usize) -> u32 {
        if row < self.rows && col < self.cols {
            self.cost[row * self.cols + col]
        } else {
            0
        }
    }
}

/// A frontier cell that is not within [`SENSOR_KILL_M`] of something the
/// sensor met: the map may show floor touching unknown there because a
/// wall was not inked, but the sensor has already said what is there.
fn is_live_frontier(grid: &Grid, extra_walls: &[ExtraWall], row: usize, col: usize) -> bool {
    if !is_frontier(grid, row, col) {
        return false;
    }
    let (wx, wy) = to_world(grid, (row, col));
    !extra_walls
        .iter()
        .any(|((bx, by), r)| {
            let kill = if *r >= DROP_WALL_M { DROP_KILL_M } else { SENSOR_KILL_M };
            ((wx - bx).powi(2) + (wy - by).powi(2)).sqrt() < r + kill
        })
}

fn is_frontier(grid: &Grid, row: usize, col: usize) -> bool {
    if grid.cell(row, col) != Some(Cell::Free) {
        return false;
    }
    for dr in -1isize..=1 {
        for dc in -1isize..=1 {
            if dr == 0 && dc == 0 {
                continue;
            }
            let (rr, cc) = (row as isize + dr, col as isize + dc);
            if rr < 0 || cc < 0 {
                continue;
            }
            if grid.cell(rr as usize, cc as usize) == Some(Cell::Unknown) {
                return true;
            }
        }
    }
    false
}

fn to_cell(grid: &Grid, x: f64, y: f64) -> Option<(usize, usize)> {
    let col = ((x - grid.x_min) / grid.cell_m).floor();
    let row = ((y - grid.y_min) / grid.cell_m).floor();
    if col < 0.0 || row < 0.0 || row as usize >= grid.rows || col as usize >= grid.cols {
        return None;
    }
    Some((row as usize, col as usize))
}

fn to_world(grid: &Grid, (row, col): (usize, usize)) -> (f64, f64) {
    (
        grid.x_min + (col as f64 + 0.5) * grid.cell_m,
        grid.y_min + (row as f64 + 0.5) * grid.cell_m,
    )
}

/// The nearest passable cell to `(x, y)` within `radius_m` — the robot's
/// own cell is often unknown or inside the wall inflation, because the
/// sensor looks ahead, not down.
fn start_cell(grid: &Grid, map: &Costmap, x: f64, y: f64, radius_m: f64) -> Option<(usize, usize)> {
    let (row, col) = to_cell(grid, x, y)?;
    let r = (radius_m / grid.cell_m).ceil() as isize;
    let mut best: Option<((usize, usize), isize)> = None;
    for dr in -r..=r {
        for dc in -r..=r {
            let (rr, cc) = (row as isize + dr, col as isize + dc);
            if rr < 0 || cc < 0 || map.at(rr as usize, cc as usize) == 0 {
                continue;
            }
            let d = dr * dr + dc * dc;
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some(((rr as usize, cc as usize), d));
            }
        }
    }
    best.map(|(c, _)| c)
}

/// Every reachable frontier group from `(x, y)`, most worth going for
/// first (see [`Frontier::score`]).
/// `blocked` are world points (with a radius) to leave alone — spots the
/// duck already visited or failed to reach; a block removes the group's
/// cells inside it, not the group. `extra_walls` are what the sensor met
/// that the map has not inked: impassable, like walls.
pub fn frontiers(
    grid: &Grid,
    x: f64,
    y: f64,
    blocked: &[((f64, f64), f64)],
    extra_walls: &[ExtraWall],
) -> Vec<Frontier> {
    frontiers_with(grid, x, y, blocked, extra_walls, inflate_m(), &[])
}

fn touches_unknown(grid: &Grid, row: usize, col: usize) -> bool {
    for dr in -1isize..=1 {
        for dc in -1isize..=1 {
            let (rr, cc) = (row as isize + dr, col as isize + dc);
            if rr >= 0 && cc >= 0 && grid.cell(rr as usize, cc as usize) == Some(Cell::Unknown) {
                return true;
            }
        }
    }
    false
}

/// How many free cells touch unknown at all, reachable or not: zero means
/// the map is finished; some, with nothing reachable, means the duck is
/// sealed in or the map has moved under it.
pub fn frontier_cells(grid: &Grid) -> usize {
    let mut n = 0;
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            if grid.cell(row, col) == Some(Cell::Free) && touches_unknown(grid, row, col) {
                n += 1;
            }
        }
    }
    n
}

/// The size of the largest frontier group on the map, reachable or not:
/// what "big frontiers remain" means for the map as a whole, where
/// [`frontiers_with`] only sees what the duck can walk to from here. The
/// sensor's word counts here too: the ring of floor around a stairwell
/// touches unknown for ever, and the drops on the books are what say so.
pub fn largest_frontier(grid: &Grid, extra_walls: &[ExtraWall]) -> usize {
    let n = grid.rows * grid.cols;
    let mut seen = vec![false; n];
    let mut best = 0;
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            let idx = row * grid.cols + col;
            if seen[idx] || !is_live_frontier(grid, extra_walls, row, col) {
                continue;
            }
            seen[idx] = true;
            let mut size = 0;
            let mut stack = vec![(row, col)];
            while let Some((r0, c0)) = stack.pop() {
                size += 1;
                for dr in -1isize..=1 {
                    for dc in -1isize..=1 {
                        let (rr, cc) = (r0 as isize + dr, c0 as isize + dc);
                        if rr < 0 || cc < 0 || rr as usize >= grid.rows || cc as usize >= grid.cols {
                            continue;
                        }
                        let (rr, cc) = (rr as usize, cc as usize);
                        let j = rr * grid.cols + cc;
                        if seen[j] || !is_live_frontier(grid, extra_walls, rr, cc) {
                            continue;
                        }
                        seen[j] = true;
                        stack.push((rr, cc));
                    }
                }
            }
            best = best.max(size);
        }
    }
    best
}

/// [`frontiers`] with the wall inflation of one's choosing and the
/// walked lanes (see [`Costmap::build`]):
/// [`SQUEEZE_INFLATE_M`] is the body's own half-width, for getting out
/// of a pocket the usual margin says has no way out.
pub fn frontiers_with(
    grid: &Grid,
    x: f64,
    y: f64,
    blocked: &[((f64, f64), f64)],
    extra_walls: &[ExtraWall],
    inflate_m: f64,
    lanes: &[(f64, f64)],
) -> Vec<Frontier> {
    let map = Costmap::build(grid, extra_walls, inflate_m, lanes);
    // The robot's own cell may be inside the inflation, or inked as wall
    // when it walked into something low; the nearest passable floor is
    // where it came from, so look wider before giving up.
    let Some(start) = start_cell(grid, &map, x, y, 0.35)
        .or_else(|| start_cell(grid, &map, x, y, START_REACH_M))
    else {
        return Vec::new();
    };
    // Dijkstra over the costmap: cost, hop count and parent per cell.
    let n = grid.rows * grid.cols;
    let mut cost = vec![u32::MAX; n];
    let mut hops = vec![0u32; n];
    let mut parent = vec![usize::MAX; n];
    let mut heap = BinaryHeap::new();
    let start_idx = start.0 * grid.cols + start.1;
    cost[start_idx] = 0;
    heap.push(Reverse((0u32, start_idx)));
    while let Some(Reverse((c, here))) = heap.pop() {
        if c > cost[here] {
            continue;
        }
        let (row, col) = (here / grid.cols, here % grid.cols);
        for (dr, dc) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
            let (rr, cc) = (row as isize + dr, col as isize + dc);
            if rr < 0 || cc < 0 {
                continue;
            }
            let (rr, cc) = (rr as usize, cc as usize);
            let step = map.at(rr, cc);
            if step == 0 {
                continue;
            }
            let idx = rr * grid.cols + cc;
            let next = c + step;
            if next < cost[idx] {
                cost[idx] = next;
                hops[idx] = hops[here] + 1;
                parent[idx] = here;
                heap.push(Reverse((next, idx)));
            }
        }
    }
    // Reachable frontier cells, grouped by 8-connectivity.
    let mut seen = vec![false; n];
    let mut groups: Vec<Vec<(usize, usize)>> = Vec::new();
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            let idx = row * grid.cols + col;
            if seen[idx] || cost[idx] == u32::MAX || !is_live_frontier(grid, extra_walls, row, col) {
                continue;
            }
            let mut group = Vec::new();
            let mut stack = vec![(row, col)];
            seen[idx] = true;
            while let Some((r0, c0)) = stack.pop() {
                group.push((r0, c0));
                for dr in -1isize..=1 {
                    for dc in -1isize..=1 {
                        let (rr, cc) = (r0 as isize + dr, c0 as isize + dc);
                        if rr < 0 || cc < 0 {
                            continue;
                        }
                        let (rr, cc) = (rr as usize, cc as usize);
                        if rr >= grid.rows || cc >= grid.cols {
                            continue;
                        }
                        let j = rr * grid.cols + cc;
                        if seen[j] || cost[j] == u32::MAX || !is_live_frontier(grid, extra_walls, rr, cc) {
                            continue;
                        }
                        seen[j] = true;
                        stack.push((rr, cc));
                    }
                }
            }
            groups.push(group);
        }
    }
    let near = |(px, py): (f64, f64), (bx, by): (f64, f64), r: f64| {
        ((px - bx).powi(2) + (py - by).powi(2)).sqrt() < r
    };
    let mut out: Vec<Frontier> = groups
        .into_iter()
        .filter(|g| g.len() >= MIN_FRONTIER_CELLS)
        .filter_map(|g| {
            let (sx, sy) = g.iter().fold((0.0, 0.0), |(ax, ay), &c| {
                let (wx, wy) = to_world(grid, c);
                (ax + wx, ay + wy)
            });
            let centroid = (sx / g.len() as f64, sy / g.len() as f64);
            let nearest = *g
                .iter()
                .filter(|&&c| {
                    let w = to_world(grid, c);
                    !blocked.iter().any(|(b, r)| near(w, *b, *r))
                })
                .min_by_key(|&&(r, c)| cost[r * grid.cols + c])?;
            let target = to_world(grid, nearest);
            let target_idx = nearest.0 * grid.cols + nearest.1;
            let mut path = Vec::new();
            let mut idx = target_idx;
            while idx != start_idx {
                path.push(to_world(grid, (idx / grid.cols, idx % grid.cols)));
                idx = parent[idx];
            }
            path.reverse();
            let back = (VIEW_BACK_M / grid.cell_m).round() as usize;
            let stand = match path.len().checked_sub(back + 1) {
                Some(i) => path[i],
                None => (x, y),
            };
            Some(Frontier {
                cells: g.len(),
                centroid,
                target,
                stand,
                distance_m: f64::from(hops[target_idx]) * grid.cell_m,
                cost: cost[target_idx],
                score: f64::from(cost[target_idx]) / (g.len().min(GAIN_CAP_CELLS) as f64),
                path,
            })
        })
        .collect();
    out.sort_by(|a, b| a.score.total_cmp(&b.score));
    out
}

/// The cheapest path from `(x, y)` to `goal`, on the same costmap the
/// frontier planner uses — known floor cheap, unknown dear, walls and the
/// `extra_walls` impassable, the walked `lanes` always passable. World
/// coordinates, the start excluded; `None` when the goal cannot be
/// reached, and the goal cell is snapped to the nearest passable floor
/// within [`START_REACH_M`] so a target against a wall still works.
pub fn path_to(
    grid: &Grid,
    x: f64,
    y: f64,
    goal: (f64, f64),
    extra_walls: &[ExtraWall],
    inflate_m: f64,
    lanes: &[(f64, f64)],
) -> Option<Vec<(f64, f64)>> {
    let map = Costmap::build(grid, extra_walls, inflate_m, lanes);
    // A start deep inside the inflation — a rim sealed with the body
    // beside it (rimD1, 2026-09-20: "no way to the goal" from 0.42 m
    // of the seal point, for good) — still gets out: the nearest
    // passable cell as far as `START_REACH_FAR_M`.
    let start = start_cell(grid, &map, x, y, 0.35)
        .or_else(|| start_cell(grid, &map, x, y, START_REACH_M))
        .or_else(|| start_cell(grid, &map, x, y, START_REACH_FAR_M))?;
    let end = start_cell(grid, &map, goal.0, goal.1, 0.35)
        .or_else(|| start_cell(grid, &map, goal.0, goal.1, START_REACH_M))?;
    let mut path = dijkstra(grid, &map, start, end)?;
    path.reverse();
    if pull_route() {
        path = pull(&map, grid, &path, extra_walls);
    }
    Some(path)
}


/// Dijkstra over the costmap from `start` to `end` (cells), the path in
/// world coordinates from the cell after `start` to `end`.
fn dijkstra(grid: &Grid, map: &Costmap, start: (usize, usize), end: (usize, usize)) -> Option<Vec<(f64, f64)>> {
    let n = grid.rows * grid.cols;
    let (mut cost, mut parent) = (vec![u32::MAX; n], vec![usize::MAX; n]);
    let mut heap = BinaryHeap::new();
    let start_idx = start.0 * grid.cols + start.1;
    let end_idx = end.0 * grid.cols + end.1;
    cost[start_idx] = 0;
    heap.push(Reverse((0u32, start_idx)));
    while let Some(Reverse((c, here))) = heap.pop() {
        if here == end_idx {
            break;
        }
        if c > cost[here] {
            continue;
        }
        let (row, col) = (here / grid.cols, here % grid.cols);
        for (dr, dc) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
            let (rr, cc) = (row as isize + dr, col as isize + dc);
            if rr < 0 || cc < 0 {
                continue;
            }
            let (rr, cc) = (rr as usize, cc as usize);
            let step = map.at(rr, cc);
            if step == 0 {
                continue;
            }
            let idx = rr * grid.cols + cc;
            let next = c + step;
            if next < cost[idx] {
                cost[idx] = next;
                parent[idx] = here;
                heap.push(Reverse((next, idx)));
            }
        }
    }
    if cost[end_idx] == u32::MAX {
        return None;
    }
    let mut path = Vec::new();
    let mut idx = end_idx;
    while idx != start_idx {
        path.push(to_world(grid, (idx / grid.cols, idx % grid.cols)));
        idx = parent[idx];
    }
    path.reverse();
    Some(path)
}

/// Is `route` still walkable over the first `ahead_m` of it — every
/// point on passable cells of a costmap built now?
pub fn route_passable(
    grid: &Grid,
    route: &[(f64, f64)],
    ahead_m: f64,
    extra_walls: &[ExtraWall],
    inflate_m: f64,
    lanes: &[(f64, f64)],
) -> bool {
    let map = Costmap::build(grid, extra_walls, inflate_m, lanes);
    let mut along = 0.0;
    let mut prev: Option<(f64, f64)> = None;
    for p in route {
        if let Some(q) = prev {
            along += (p.0 - q.0).hypot(p.1 - q.1);
        }
        prev = Some(*p);
        if along > ahead_m {
            break;
        }
        if !to_cell(grid, p.0, p.1).is_some_and(|(r, c)| map.at(r, c) > 0) {
            return false;
        }
    }
    true
}

/// Both routes: Dijkstra's own and the pulled one (the same when pulling
/// is off) — for an overlay to draw the two.
pub fn path_to_both(
    grid: &Grid,
    x: f64,
    y: f64,
    goal: (f64, f64),
    extra_walls: &[ExtraWall],
    inflate_m: f64,
    lanes: &[(f64, f64)],
) -> Option<(Vec<(f64, f64)>, Vec<(f64, f64)>)> {
    let map = Costmap::build(grid, extra_walls, inflate_m, lanes);
    let start = start_cell(grid, &map, x, y, 0.35).or_else(|| start_cell(grid, &map, x, y, START_REACH_M))?;
    let end = start_cell(grid, &map, goal.0, goal.1, 0.35)
        .or_else(|| start_cell(grid, &map, goal.0, goal.1, START_REACH_M))?;
    let raw = dijkstra(grid, &map, start, end)?;
    let pulled = if pull_route() { pull(&map, grid, &raw, extra_walls) } else { raw.clone() };
    Some((raw, pulled))
}

/// `QK_PULL_ROUTE=0` leaves Dijkstra's staircase as it is. On by default:
/// the route is pulled taut between its corners — from each point the
/// farthest one ahead reachable in a straight line over passable cells
/// is the next corner — and re-laid at the cell's spacing along those
/// straight runs, so the duck turns at doorways and the stairwell, not
/// at every step of a grid (the user, 2026-09-16: "too broken up, make
/// it squarer, it turns fewer times").
fn pull_route() -> bool {
    std::env::var("QK_PULL_ROUTE").map(|v| v != "0").unwrap_or(true)
}
/// A straight run may stray this far from the Dijkstra route it
/// replaces: the staircase is smoothed, the route is not redrawn — a
/// diagonal across the room cut corners and brushed walls Dijkstra had
/// kept away from (the user's eye, 2026-09-16). `QK_PULL_DEVIATION_M`.
fn pull_deviation_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("QK_PULL_DEVIATION_M").ok().and_then(|v| v.parse().ok()).unwrap_or(0.20)
    })
}

/// A straight run keeps Dijkstra's distance from the booked drops: its
/// nearest approach to any drop is no closer than that of the route it
/// replaces (by [`PULL_DROP_SLACK_M`]). Dijkstra keeps the drop's
/// inflation and the graded band; the pull, allowed 0.20 m of
/// deviation, took the route to the rim's edge beside the stairwell
/// (the user's eye, 2026-09-21: "keep our route as close as possible to
/// Dijkstra's, which is the truth"). Pinning the pulled route to a cell
/// of Dijkstra's near drops instead made the aim zigzag along the
/// staircase (paper twin, known world: 7/30 against 30/30).
const PULL_DROP_SLACK_M: f64 = 0.02;

fn nearest_drop_m(x: f64, y: f64, drops: &[ExtraWall]) -> f64 {
    drops
        .iter()
        .filter(|(_, radius)| *radius >= DROP_WALL_M)
        .map(|((bx, by), _)| (bx - x).hypot(by - y))
        .fold(f64::INFINITY, f64::min)
}

/// Every sample of the segment `a`–`b` lies within `dev` of some point of
/// `along` (the route it replaces), and the segment comes no nearer a
/// booked drop than `along` does.
fn line_near(a: (f64, f64), b: (f64, f64), along: &[(f64, f64)], dev: f64, cell_m: f64, drops: &[ExtraWall]) -> bool {
    let n = ((b.0 - a.0).hypot(b.1 - a.1) / cell_m).ceil().max(1.0) as usize;
    let along_min = along.iter().map(|p| nearest_drop_m(p.0, p.1, drops)).fold(f64::INFINITY, f64::min);
    (0..=n).all(|i| {
        let t = i as f64 / n as f64;
        let (x, y) = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
        along.iter().any(|p| (p.0 - x).hypot(p.1 - y) <= dev)
            && (along_min.is_infinite() || nearest_drop_m(x, y, drops) >= along_min - PULL_DROP_SLACK_M)
    })
}

/// The cost of the straight segment from `a` to `b` (world) over `map`,
/// one cell's price per cell's length, or `None` where it crosses an
/// impassable cell. A shortcut is taken only when it costs no more than
/// the route it replaces: not through the comfort band by a wall, not
/// off a lane.
fn line_cost(map: &Costmap, grid: &Grid, a: (f64, f64), b: (f64, f64)) -> Option<f64> {
    let len = (b.0 - a.0).hypot(b.1 - a.1);
    let n = (len / (grid.cell_m * 0.5)).ceil().max(1.0) as usize;
    let mut total = 0.0;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let (x, y) = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
        let (r, c) = to_cell(grid, x, y)?;
        let v = map.at(r, c);
        if v == 0 {
            return None;
        }
        total += f64::from(v);
    }
    // Sampled twice per cell: the price per cell of length.
    Some(total / 2.0)
}

fn pull(map: &Costmap, grid: &Grid, path: &[(f64, f64)], drops: &[ExtraWall]) -> Vec<(f64, f64)> {
    if path.len() < 3 {
        return path.to_vec();
    }
    let cost_at = |p: (f64, f64)| to_cell(grid, p.0, p.1).map(|(r, c)| f64::from(map.at(r, c))).unwrap_or(0.0);
    let mut corners = vec![path[0]];
    let mut i = 0;
    while i + 1 < path.len() {
        // The farthest point ahead whose straight line costs no more than
        // the route it replaces; at least the next.
        let mut j = path.len() - 1;
        loop {
            if j <= i + 1 {
                break;
            }
            let along: f64 = path[i + 1..=j].iter().map(|p| cost_at(*p)).sum();
            if line_cost(map, grid, path[i], path[j]).is_some_and(|c| c <= along + 0.5)
                && line_near(path[i], path[j], &path[i..=j], pull_deviation_m(), grid.cell_m, drops)
            {
                break;
            }
            j -= 1;
        }
        corners.push(path[j]);
        i = j;
    }
    // Re-laid densely along the straight runs.
    let mut out = vec![corners[0]];
    for w in corners.windows(2) {
        let (a, b) = (w[0], w[1]);
        let n = ((b.0 - a.0).hypot(b.1 - a.1) / grid.cell_m).round().max(1.0) as usize;
        for k in 1..=n {
            let t = k as f64 / n as f64;
            out.push((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)));
        }
    }
    out
}

/// The point on `path` about `lookahead_m` along it — the next leg's aim.
pub fn waypoint(path: &[(f64, f64)], lookahead_m: f64, cell_m: f64) -> Option<(f64, f64)> {
    let steps = (lookahead_m / cell_m).round().max(1.0) as usize;
    path.get(steps.min(path.len().checked_sub(1)?)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A room drawn as text: `#` wall, `.` free, ` ` unknown; row 0 at the
    /// bottom (y_min), like the wire.
    fn room(rows: &[&str], cell_m: f64) -> Grid {
        let cols = rows[0].len();
        let mut cells = Vec::new();
        for line in rows.iter().rev() {
            for ch in line.chars() {
                cells.push(match ch {
                    '#' => Cell::Wall,
                    '.' => Cell::Free,
                    _ => Cell::Unknown,
                });
            }
        }
        Grid {
            rows: rows.len(),
            cols,
            x_min: 0.0,
            y_min: 0.0,
            cell_m,
            cells,
        }
    }

    #[test]
    fn in_a_wide_room_the_route_keeps_away_from_the_walls() {
        // A 2 m wide, 4 m long hall (cells 0.1 m), start and goal both
        // 0.25 m from the south wall: the shortest line hugs that wall,
        // the graded price walks out to the middle.
        let mut rows: Vec<String> = vec!["#".repeat(42)];
        for _ in 0..20 {
            rows.push(format!("#{}#", ".".repeat(40)));
        }
        rows.push("#".repeat(42));
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        let g = room(&refs, 0.1);
        let path = path_to(&g, 0.5, 0.35, (3.6, 0.35), &[], INFLATE_M, &[]).expect("a way across the hall");
        let mid: Vec<&(f64, f64)> = path.iter().filter(|(x, _)| *x > 1.2 && *x < 2.9).collect();
        assert!(!mid.is_empty());
        let nearest = mid.iter().map(|(_, y)| y.min(2.2 - y)).fold(f64::INFINITY, f64::min);
        assert!(nearest >= 0.40, "the route ran {nearest:.2} m from the wall in a 2 m hall: {path:?}");
    }

    #[test]
    fn a_walked_lane_is_preferred_over_bare_floor() {
        // An open hall; a lane runs along the south wall's comfort band.
        // The straight route across the middle is shortest; the lane is
        // cheaper per cell, so the route bends onto it.
        let mut rows: Vec<String> = vec!["#".repeat(42)];
        for _ in 0..20 {
            rows.push(format!("#{}#", ".".repeat(40)));
        }
        rows.push("#".repeat(42));
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        let g = room(&refs, 0.1);
        let lane: Vec<(f64, f64)> = (5..=36).map(|i| (i as f64 * 0.1 + 0.05, 0.75)).collect();
        // The preference is a knob (off by default): the test sets it.
        unsafe { std::env::set_var("QK_COST_LANE", "5") };
        let free = path_to(&g, 0.5, 1.15, (3.6, 1.15), &[], INFLATE_M, &[]).unwrap();
        let laned = path_to(&g, 0.5, 1.15, (3.6, 1.15), &[], INFLATE_M, &lane).unwrap();
        let on_lane = |p: &Vec<(f64, f64)>| p.iter().filter(|(x, y)| *x > 1.0 && *x < 3.0 && (y - 0.75).abs() < 0.12).count();
        assert_eq!(on_lane(&free), 0, "{free:?}");
        assert!(on_lane(&laned) >= 10, "{laned:?}");
    }

    #[test]
    fn a_lane_beside_a_rim_does_not_open_the_cells_next_to_it() {
        // An open room with a booked drop (a rim point) in its middle and
        // a lane running 0.15 m from it: open along the lane, even beside
        // the rim; a lane 0.05 m from the rim yields to it.
        let rows: Vec<String> = std::iter::once("#".repeat(42))
            .chain((0..20).map(|_| format!("#{}#", ".".repeat(40))))
            .chain(std::iter::once("#".repeat(42)))
            .collect();
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        let g = room(&refs, 0.1);
        let rim = ((2.0, 1.0), DROP_WALL_M);
        let lane: Vec<(f64, f64)> = (5..=36).map(|i| (i as f64 * 0.1 + 0.05, 1.15)).collect();
        let map = Costmap::build(&g, &[rim], INFLATE_M, &lane);
        let at = |x: f64, y: f64| {
            let (r, c) = to_cell(&g, x, y).unwrap();
            map.at(r, c)
        };
        assert!(at(1.05, 1.15) > 0, "along the lane the cells are open");
        assert!(at(2.05, 1.15) > 0, "0.15 m from the rim the lane still opens the cell (the 0.44 m passage's axis)");
        let close: Vec<(f64, f64)> = (5..=36).map(|i| (i as f64 * 0.1 + 0.05, 1.05)).collect();
        let map2 = Costmap::build(&g, &[rim], INFLATE_M, &close);
        let (r, c) = to_cell(&g, 2.05, 1.05).unwrap();
        assert_eq!(map2.at(r, c), 0, "0.05 m from the rim the lane yields");
    }

    #[test]
    fn the_route_is_pulled_taut_between_its_corners() {
        // An L-shaped way round a wall stub: Dijkstra's staircase becomes
        // two straight runs and one corner.
        let rows: Vec<String> = std::iter::once("#".repeat(42))
            .chain((0..20).map(|r| if r < 10 { format!("#{}{}#", ".".repeat(20), "#".repeat(20)) } else { format!("#{}#", ".".repeat(40)) }))
            .chain(std::iter::once("#".repeat(42)))
            .collect();
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        let g = room(&refs, 0.1);
        let path = path_to(&g, 0.5, 0.5, (3.5, 0.5), &[], INFLATE_M, &[]).expect("a way round the stub");
        // Corners: where the heading changes by more than a few degrees.
        let mut turns = 0;
        for w in path.windows(3) {
            let a = (w[1].1 - w[0].1).atan2(w[1].0 - w[0].0);
            let b = (w[2].1 - w[1].1).atan2(w[2].0 - w[1].0);
            if ((b - a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI).abs() > 0.15 {
                turns += 1;
            }
        }
        assert!(turns <= 4, "{turns} turns: {path:?}");
    }

    #[test]
    fn in_a_narrow_passage_the_route_still_goes_through() {
        // A 0.5 m wide corridor (cells 0.05 m) between two rooms: every
        // cell pays the same graded price, so the way through is found
        // and is straight.
        let mut rows: Vec<String> = Vec::new();
        for r in 0..30 {
            let line: String = (0..60)
                .map(|c| {
                    let corridor = (20..40).contains(&c);
                    let lane = (10..20).contains(&r);
                    if !corridor || lane { '.' } else { '#' }
                })
                .collect();
            rows.push(line);
        }
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        let g = room(&refs, 0.05);
        let path = path_to(&g, 0.5, 0.75, (2.5, 0.75), &[], INFLATE_M, &[]).expect("a way through the 0.5 m passage");
        assert!(path.len() as f64 * 0.05 < 2.6, "the passage was not taken straight: {} cells", path.len());
    }

    #[test]
    fn the_cheapest_reachable_frontier_comes_first_with_a_path() {
        let g = room(
            &[
                "      ........      ",
                "    ..........#...  ",
                "  ..................",
                "  ..................",
                "  ..................",
                "  ..................",
                "  ..................",
                "  ..................",
                "  ..................",
                "      ........      ",
            ],
            0.1,
        );
        let fs = frontiers(&g, 0.6, 0.5, &[], &[]);
        assert!(fs.len() >= 2, "{fs:?}");
        let first = &fs[0];
        assert!(first.cost <= fs[1].cost);
        assert!(first.cells >= MIN_FRONTIER_CELLS);
        assert!(!first.path.is_empty());
        for (px, py) in &first.path {
            assert_ne!(g.at(*px, *py), Some(Cell::Wall), "{px},{py}");
        }
        let wp = waypoint(&first.path, 0.25, 0.1).unwrap();
        assert_ne!(g.at(wp.0, wp.1), Some(Cell::Wall));
    }

    #[test]
    fn unknown_floor_costs_more_than_known() {
        // The same strip, once all known and once with an unknown middle:
        // the frontier at its end costs more to reach across the unknown.
        let known = room(&["                    ", "....................", "                    "], 0.1);
        let mixed = room(&["                    ", "..        ..........", "                    "], 0.1);
        let fk = frontiers(&known, 0.15, 0.15, &[], &[]);
        let fm = frontiers(&mixed, 0.15, 0.15, &[], &[]);
        let far_k = fk
            .iter()
            .max_by_key(|f| f.path.len())
            .expect("a frontier on the known strip");
        let far_m = fm
            .iter()
            .max_by_key(|f| f.path.len())
            .expect("a frontier on the mixed strip");
        assert_eq!(far_k.cost, far_k.path.len() as u32 * COST_FREE);
        assert!(
            far_m.cost > far_m.path.len() as u32 * COST_FREE,
            "{far_m:?}"
        );
        assert!(
            far_m.cost <= far_m.path.len() as u32 * COST_UNKNOWN_DEFAULT,
            "{far_m:?}"
        );
    }

    #[test]
    fn walls_are_inflated_and_blocks_take_cells_not_groups() {
        let g = room(
            &[
                "  ....#....  ",
                "  ....#....  ",
                "  ....#....  ",
                "  ....#....  ",
                "  .... ....  ",
                "  ....#....  ",
                "  ....#....  ",
                "  ....#....  ",
                "  ....#....  ",
            ],
            0.1,
        );
        let fs = frontiers(&g, 0.35, 0.45, &[], &[]);
        assert!(!fs.is_empty());
        assert!(
            fs.iter().all(|f| f.centroid.0 < 0.6),
            "nothing beyond the wall is reachable: {fs:?}"
        );
        let blocked = fs.iter().map(|f| (f.centroid, 5.0)).collect::<Vec<_>>();
        assert!(frontiers(&g, 0.35, 0.45, &blocked, &[]).is_empty());
        let partial = vec![(fs[0].target, 0.12)];
        let rest = frontiers(&g, 0.35, 0.45, &partial, &[]);
        assert!(!rest.is_empty(), "the far end of the frontier survives");
        assert!(rest.iter().all(|f| f.target != fs[0].target));
    }

    #[test]
    fn an_extra_wall_is_kept_clear_of() {
        let g = room(
            &[
                "          ",
                "..........",
                "..........",
                "..........",
                "          ",
            ],
            0.1,
        );
        let before = frontiers(&g, 0.15, 0.25, &[], &[]);
        assert!(!before.is_empty());
        let reach_before = before.iter().map(|f| f.path.len()).max().unwrap();
        // A sensor-seen obstacle in the middle of the strip (5 cm, plus
        // the planner's own inflation): no path passes within 20 cm of
        // it, and the strip beyond it is cut off.
        let after = frontiers(&g, 0.15, 0.25, &[], &[((0.5, 0.25), 0.05)]);
        assert!(after.iter().all(|f| {
            f.path
                .iter()
                .all(|(px, py)| ((px - 0.5).powi(2) + (py - 0.25).powi(2)).sqrt() >= 0.2)
        }));
        let reach_after = after.iter().map(|f| f.path.len()).max().unwrap_or(0);
        assert!(
            reach_after < reach_before,
            "{reach_after} vs {reach_before}"
        );
    }

    #[test]
    fn a_closed_room_has_no_frontier() {
        let g = room(
            &[
                "############",
                "#..........#",
                "#..........#",
                "#..........#",
                "#..........#",
                "#..........#",
                "#..........#",
                "#..........#",
                "############",
            ],
            0.1,
        );
        assert!(frontiers(&g, 0.6, 0.45, &[], &[]).is_empty());
    }

    #[test]
    fn a_duck_outside_the_grid_finds_nothing() {
        let g = room(&["........", "........"], 0.1);
        assert!(frontiers(&g, -5.0, -5.0, &[], &[]).is_empty());
    }

    #[test]
    fn a_frontier_across_unknown_floor_is_reachable() {
        let g = room(
            &[
                "                ",
                "                ",
                "....     .......",
                "....     .......",
                "                ",
                "                ",
            ],
            0.1,
        );
        let fs = frontiers(&g, 0.15, 0.25, &[], &[]);
        assert!(
            !fs.is_empty(),
            "the far streak must be reachable across unknown"
        );
        assert!(fs.iter().any(|f| f.target.0 > 0.8), "{fs:?}");
    }
}
