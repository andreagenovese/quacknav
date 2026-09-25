//! The planner's safety rules as properties: on rooms generated at random —
//! boxes, holes with their rims on the books, low obstacles, unknown floor —
//! whatever route comes out obeys them. The examples in `frontier.rs` show a
//! rule holding in the scene that taught it; these say it holds everywhere
//! proptest could think of, and a failure comes back shrunk to the smallest
//! room that breaks it.
//!
//! The rules:
//!
//! 1. No route sample in a wall cell.
//! 2. No route sample inside a booked rim or a booked obstacle.
//! 3. The pulled route comes no nearer a rim than Dijkstra's did (less the
//!    pull's 2 cm of slack): straightening a route never walks it to the edge.
//! 4. The pulled route strays at most the pull's 0.20 m from Dijkstra's.
//! 5. Both end where the goal was snapped to.
//!
//! Run with the planner's defaults (no `QK_*` knobs set).

use proptest::prelude::*;
use quack_nav::frontier::{ExtraWall, inflate_m, path_to_both};
use quack_nav::map::{Cell, Grid};

const CELL: f64 = 0.05;
const DROP_R: f64 = 0.10;
const OBSTACLE_R: f64 = 0.05;
/// The pull's allowances, `frontier::pull_deviation_m` and
/// `PULL_DROP_SLACK_M`, at their defaults.
const PULL_DEVIATION_M: f64 = 0.20;
const PULL_DROP_SLACK_M: f64 = 0.02;
/// Route points sit at cell centres and the re-laid runs at cell spacing:
/// half a cell of tolerance for sampling where the planner sampled elsewhere.
const SAMPLING: f64 = CELL / 2.0;

#[derive(Debug, Clone)]
struct Room {
    w: f64,
    h: f64,
    boxes: Vec<(f64, f64, f64, f64)>,
    holes: Vec<(f64, f64, f64, f64)>,
    obstacles: Vec<(f64, f64)>,
    unknown: Option<(f64, f64, f64, f64)>,
    start: (f64, f64),
    goal: (f64, f64),
}

/// A rectangle inside a `w` × `h` room, `min`–`max` metres a side.
fn rect(w: f64, h: f64, min: f64, max: f64) -> impl Strategy<Value = (f64, f64, f64, f64)> {
    (0.0..1.0f64, 0.0..1.0f64, min..max, min..max).prop_map(move |(fx, fy, sw, sh)| {
        let (sw, sh) = (sw.min(w - 0.2), sh.min(h - 0.2));
        let x0 = 0.1 + fx * (w - 0.2 - sw);
        let y0 = 0.1 + fy * (h - 0.2 - sh);
        (x0, y0, x0 + sw, y0 + sh)
    })
}

fn room() -> impl Strategy<Value = Room> {
    (2.0..5.0f64, 2.0..4.0f64).prop_flat_map(|(w, h)| {
        let point = move || (0.15..w - 0.15, 0.15..h - 0.15);
        (
            prop::collection::vec(rect(w, h, 0.2, 0.9), 0..5),
            prop::collection::vec(rect(w, h, 0.3, 0.8), 0..3),
            prop::collection::vec(point(), 0..6),
            prop::option::of(rect(w, h, 0.4, 1.5)),
            point(),
            point(),
        )
            .prop_map(move |(boxes, holes, obstacles, unknown, start, goal)| Room {
                w,
                h,
                boxes,
                holes,
                obstacles,
                unknown,
                start,
                goal,
            })
    })
}

fn paint(g: &mut Grid, (x0, y0, x1, y1): (f64, f64, f64, f64), cell: Cell) {
    for row in 0..g.rows {
        for col in 0..g.cols {
            let (x, y) = ((col as f64 + 0.5) * CELL, (row as f64 + 0.5) * CELL);
            if x >= x0 && x < x1 && y >= y0 && y < y1 {
                g.cells[row * g.cols + col] = cell;
            }
        }
    }
}

fn build(r: &Room) -> (Grid, Vec<ExtraWall>) {
    let (cols, rows) = ((r.w / CELL).round() as usize, (r.h / CELL).round() as usize);
    let mut g = Grid { rows, cols, x_min: 0.0, y_min: 0.0, cell_m: CELL, cells: vec![Cell::Free; rows * cols] };
    if let Some(u) = r.unknown {
        paint(&mut g, u, Cell::Unknown);
    }
    for b in &r.boxes {
        paint(&mut g, *b, Cell::Wall);
    }
    for edge in [(0.0, 0.0, r.w, CELL), (0.0, r.h - CELL, r.w, r.h), (0.0, 0.0, CELL, r.h), (r.w - CELL, 0.0, r.w, r.h)] {
        paint(&mut g, edge, Cell::Wall);
    }
    let mut books: Vec<ExtraWall> = Vec::new();
    for &(x0, y0, x1, y1) in &r.holes {
        // A hole reads as floor on the map (it does, measured): only its
        // rim on the books keeps the planner out.
        let n = |a: f64, b: f64| ((b - a) / DROP_R).ceil() as usize;
        for i in 0..=n(x0, x1) {
            let x = (x0 + i as f64 * DROP_R).min(x1);
            books.push(((x, y0), DROP_R));
            books.push(((x, y1), DROP_R));
        }
        for i in 1..n(y0, y1) {
            let y = y0 + i as f64 * DROP_R;
            books.push(((x0, y), DROP_R));
            books.push(((x1, y), DROP_R));
        }
    }
    books.extend(r.obstacles.iter().map(|p| (*p, OBSTACLE_R)));
    (g, books)
}

/// Samples along a route, a quarter cell apart.
fn samples(route: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = route.first().copied().into_iter().collect::<Vec<_>>();
    for w in route.windows(2) {
        let n = ((w[1].0 - w[0].0).hypot(w[1].1 - w[0].1) / (CELL / 4.0)).ceil().max(1.0) as usize;
        for k in 1..=n {
            let t = k as f64 / n as f64;
            out.push((w[0].0 + t * (w[1].0 - w[0].0), w[0].1 + t * (w[1].1 - w[0].1)));
        }
    }
    out
}

fn nearest_rim(p: (f64, f64), books: &[ExtraWall]) -> f64 {
    books
        .iter()
        .filter(|(_, r)| *r >= DROP_R)
        .map(|((x, y), r)| (x - p.0).hypot(y - p.1) - r)
        .fold(f64::INFINITY, f64::min)
}

proptest! {
    // 256 rooms by default; `PROPTEST_CASES=5000` for a long hunt.
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(256),
        ..ProptestConfig::default()
    })]

    #[test]
    fn every_route_keeps_the_rules(r in room()) {
        let (g, books) = build(&r);
        let Some((raw, pulled)) = path_to_both(&g, r.start.0, r.start.1, r.goal, &books, inflate_m(), &[]) else {
            // No way there is an answer, not a breach.
            return Ok(());
        };
        if raw.is_empty() {
            // Start and goal in one cell: nothing to walk.
            return Ok(());
        }
        for (name, route) in [("raw", &raw), ("pulled", &pulled)] {
            for p in samples(route) {
                let (col, row) = ((p.0 / CELL) as usize, (p.1 / CELL) as usize);
                // 1. walls
                prop_assert_ne!(g.cell(row, col), Some(Cell::Wall), "{} route in a wall at {:?}", name, p);
                // 2. rims and obstacles, at their bare radius
                for ((x, y), rad) in &books {
                    prop_assert!((x - p.0).hypot(y - p.1) >= rad - SAMPLING,
                        "{} route inside a booked point {:?} r {} at {:?}", name, (x, y), rad, p);
                }
            }
        }
        // 3. straightening never walks nearer a rim
        let raw_min = raw.iter().map(|p| nearest_rim(*p, &books)).fold(f64::INFINITY, f64::min);
        if raw_min.is_finite() {
            let pulled_min = samples(&pulled).iter().map(|p| nearest_rim(*p, &books)).fold(f64::INFINITY, f64::min);
            prop_assert!(pulled_min >= raw_min - PULL_DROP_SLACK_M - SAMPLING,
                "pulled route {:.3} m from a rim, Dijkstra's {:.3}", pulled_min, raw_min);
        }
        // 4. never far from Dijkstra's
        for p in &pulled {
            let off = raw.iter().map(|q| (q.0 - p.0).hypot(q.1 - p.1)).fold(f64::INFINITY, f64::min);
            prop_assert!(off <= PULL_DEVIATION_M + SAMPLING, "pulled point {:?} {:.3} m off Dijkstra's route", p, off);
        }
        // 5. the same end
        let (a, b) = (raw.last().unwrap(), pulled.last().unwrap());
        prop_assert!((a.0 - b.0).hypot(a.1 - b.1) < 1e-9, "the routes end apart: {:?} vs {:?}", a, b);
    }
}
