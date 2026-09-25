//! Golden routes: the planner's route — Dijkstra's, and the one pulled taut
//! from it — on fixed scenes, compared with the route saved the last time
//! someone looked at it and agreed. A change to the costmap, the prices or
//! the pull shows here as a failing test, with the scene named, rather than
//! three twin runs later as a duck that brushes a door jamb.
//!
//! When a change is meant to move a route, look at the new one and save it:
//!
//!     UPDATE_GOLDEN=1 cargo test -p quack-nav --test golden_routes
//!
//! and say in the commit why the route moved. The files are in
//! `tests/golden/`, one per scene, the routes in metres to the millimetre.
//!
//! The planner reads a few `QK_*` knobs from the environment; the golden
//! routes are the defaults', so run these without any set.

use std::path::PathBuf;

use quack_nav::frontier::{ExtraWall, inflate_m, path_to_both};
use quack_nav::map::{Cell, Grid};

/// The map's own pitch.
const CELL: f64 = 0.05;
/// A rim point on the books, as the explorer books a hole's.
const DROP_R: f64 = 0.10;

struct Scene {
    name: &'static str,
    grid: Grid,
    drops: Vec<ExtraWall>,
    start: (f64, f64),
    goal: (f64, f64),
}

/// A `w` × `h` metre floor, all of it `fill`, with a wall round the edge.
fn floor(w: f64, h: f64, fill: Cell) -> Grid {
    let (cols, rows) = ((w / CELL).round() as usize, (h / CELL).round() as usize);
    let mut g = Grid { rows, cols, x_min: 0.0, y_min: 0.0, cell_m: CELL, cells: vec![fill; rows * cols] };
    wall(&mut g, 0.0, 0.0, w, CELL);
    wall(&mut g, 0.0, h - CELL, w, h);
    wall(&mut g, 0.0, 0.0, CELL, h);
    wall(&mut g, w - CELL, 0.0, w, h);
    g
}

/// Every cell whose centre is inside the rectangle becomes `cell`.
fn paint(g: &mut Grid, x0: f64, y0: f64, x1: f64, y1: f64, cell: Cell) {
    for row in 0..g.rows {
        for col in 0..g.cols {
            let (x, y) = ((col as f64 + 0.5) * g.cell_m + g.x_min, (row as f64 + 0.5) * g.cell_m + g.y_min);
            if x >= x0 && x < x1 && y >= y0 && y < y1 {
                g.cells[row * g.cols + col] = cell;
            }
        }
    }
}

fn wall(g: &mut Grid, x0: f64, y0: f64, x1: f64, y1: f64) {
    paint(g, x0, y0, x1, y1, Cell::Wall);
}

/// A hole's rim on the books: points every `DROP_R` round the rectangle,
/// the way the explorer books what the cliff guard saw.
fn hole(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<ExtraWall> {
    let mut out = Vec::new();
    let steps = |a: f64, b: f64| ((b - a) / DROP_R).round() as usize;
    for i in 0..=steps(x0, x1) {
        let x = x0 + i as f64 * DROP_R;
        out.push(((x, y0), DROP_R));
        out.push(((x, y1), DROP_R));
    }
    for i in 1..steps(y0, y1) {
        let y = y0 + i as f64 * DROP_R;
        out.push(((x0, y), DROP_R));
        out.push(((x1, y), DROP_R));
    }
    out
}

fn scenes() -> Vec<Scene> {
    let mut v = Vec::new();

    // Two 2 × 2 m rooms, a 0.6 m door between them off the room's axis:
    // the route must turn at the door, not cut its jamb.
    let mut g = floor(4.0, 2.0, Cell::Free);
    wall(&mut g, 1.95, 0.0, 2.05, 2.0);
    paint(&mut g, 1.95, 1.2, 2.05, 1.8, Cell::Free);
    v.push(Scene { name: "doorway", grid: g, drops: vec![], start: (0.6, 0.5), goal: (3.4, 0.5) });

    // A 1.3 m hall with a stairwell in it (0.40 × 0.70 m, as house2's):
    // 0.55 m of floor between the hole and the north wall, the only way
    // through. The route goes down that passage and keeps Dijkstra's
    // distance from the rim.
    let g = floor(4.0, 1.3, Cell::Free);
    v.push(Scene { name: "passage_by_hole", grid: g, drops: hole(1.8, 0.05, 2.2, 0.70), start: (0.5, 0.5), goal: (3.5, 0.5) });

    // An L of 0.8 m corridor: one corner, taken once.
    let mut g = floor(3.0, 3.0, Cell::Free);
    wall(&mut g, 0.85, 0.85, 3.0, 3.0);
    v.push(Scene { name: "corridor_l", grid: g, drops: vec![], start: (2.6, 0.45), goal: (0.45, 2.6) });

    // A 4 × 3 m room with a 0.6 m box in the middle and the start and goal
    // either side of it: open floor, the graded price keeping off the box.
    let mut g = floor(4.0, 3.0, Cell::Free);
    wall(&mut g, 1.7, 1.2, 2.3, 1.8);
    v.push(Scene { name: "open_box", grid: g, drops: vec![], start: (0.5, 1.5), goal: (3.5, 1.5) });

    // Unknown floor between two known strips: dear, but the only way.
    let mut g = floor(4.0, 2.0, Cell::Free);
    paint(&mut g, 1.2, 0.05, 2.8, 1.95, Cell::Unknown);
    v.push(Scene { name: "across_unknown", grid: g, drops: vec![], start: (0.5, 1.0), goal: (3.5, 1.0) });

    // A low obstacle on the books (radius 0.05, the sensor's) in a doorway
    // of 0.8 m: the route passes beside it, not through it.
    let mut g = floor(4.0, 2.0, Cell::Free);
    wall(&mut g, 1.95, 0.0, 2.05, 2.0);
    paint(&mut g, 1.95, 0.6, 2.05, 1.4, Cell::Free);
    v.push(Scene { name: "obstacle_in_door", grid: g, drops: vec![((2.0, 0.85), 0.05)], start: (0.6, 1.0), goal: (3.4, 1.0) });

    v
}

fn rounded(route: &[(f64, f64)]) -> serde_json::Value {
    serde_json::Value::Array(
        route
            .iter()
            .map(|(x, y)| serde_json::json!([(x * 1000.0).round() / 1000.0, (y * 1000.0).round() / 1000.0]))
            .collect(),
    )
}

#[test]
fn routes_match_the_golden_files() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut failed = Vec::new();
    for s in scenes() {
        let (raw, pulled) = path_to_both(&s.grid, s.start.0, s.start.1, s.goal, &s.drops, inflate_m(), &[])
            .unwrap_or_else(|| panic!("{}: no route at all", s.name));
        let got = serde_json::json!({
            "scene": s.name,
            "start": [s.start.0, s.start.1],
            "goal": [s.goal.0, s.goal.1],
            "raw": rounded(&raw),
            "pulled": rounded(&pulled),
        });
        let path = dir.join(format!("{}.json", s.name));
        if update {
            std::fs::create_dir_all(&dir).expect("golden dir");
            std::fs::write(&path, serde_json::to_string_pretty(&got).expect("json") + "\n").expect("write golden");
            continue;
        }
        let want: serde_json::Value = match std::fs::read_to_string(&path) {
            Ok(t) => serde_json::from_str(&t).expect("golden json"),
            Err(_) => {
                failed.push(format!("{}: no golden file (UPDATE_GOLDEN=1 to create it)", s.name));
                continue;
            }
        };
        for key in ["raw", "pulled"] {
            if got[key] != want[key] {
                let (g, w) = (got[key].as_array().map_or(0, Vec::len), want[key].as_array().map_or(0, Vec::len));
                let first = got[key]
                    .as_array()
                    .zip(want[key].as_array())
                    .and_then(|(g, w)| g.iter().zip(w).position(|(a, b)| a != b));
                failed.push(format!(
                    "{}: the {key} route moved ({g} points, was {w}; first difference at point {})",
                    s.name,
                    first.map_or("beyond the shorter".into(), |i| i.to_string())
                ));
            }
        }
    }
    assert!(
        failed.is_empty(),
        "golden routes changed — look, and if it is meant, UPDATE_GOLDEN=1:\n  {}",
        failed.join("\n  ")
    );
}

/// The golden scenes themselves obey the rules the property tests state in
/// general: no route sample in a wall, none inside a booked rim.
#[test]
fn golden_scenes_keep_the_rules() {
    for s in scenes() {
        let (_, pulled) = path_to_both(&s.grid, s.start.0, s.start.1, s.goal, &s.drops, inflate_m(), &[])
            .unwrap_or_else(|| panic!("{}: no route", s.name));
        for w in pulled.windows(2) {
            for k in 0..=4 {
                let t = f64::from(k) / 4.0;
                let (x, y) = (w[0].0 + t * (w[1].0 - w[0].0), w[0].1 + t * (w[1].1 - w[0].1));
                let (col, row) = ((x / CELL) as usize, (y / CELL) as usize);
                assert_ne!(s.grid.cell(row, col), Some(Cell::Wall), "{}: route through a wall at ({x:.2}, {y:.2})", s.name);
                for ((dx, dy), r) in &s.drops {
                    assert!((dx - x).hypot(dy - y) >= *r, "{}: route inside a booked rim at ({x:.2}, {y:.2})", s.name);
                }
            }
        }
    }
}
