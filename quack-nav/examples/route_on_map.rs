//! The route a journey would plan on a saved map with its book, for a
//! sequence of stops: which way it goes and how long it is, under the
//! planner's knobs of the moment (`QK_INFLATE_M`, `QK_COST_HUG`, ...). What
//! the paper twin cannot say about a real house: on house2's map a layered
//! costmap (tried and removed) sent g4 through the passage beside the
//! stairwell (11.3 m) where the linear one went round (13.0 m), and on
//! MuJoCo the passage cost the journey its time budget, three rounds in
//! three.
//!
//!     cargo run -p quack-nav --example route_on_map -- \
//!         <frame.json> <ground.json> <map name> x0,y0 x1,y1 [x2,y2 ...]
//!
//! `frame.json` from `maploc/examples/dump_frame`; `ground.json` the drop
//! book (the `<name>` list of [x, y, r] and `<name>.lanes`). Drops are
//! planned at the blind journey's radius, 0.12 m, as `planner_walls` has
//! them. Prints one line per leg of the tour, and the route's nearest
//! approach to a booked rim.

use quack_nav::frontier::{inflate_m, path_to};
use quack_nav::map::MapFrame;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(a.len() >= 5, "usage: route_on_map <frame.json> <ground.json> <name> x0,y0 x1,y1 ...");
    let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&a[0])?)?;
    let frame: MapFrame = serde_json::from_value(value.get("frame").cloned().unwrap_or(value))?;
    // `QK_ORACLE_WALLS` / `QK_ORACLE_HOLES`: the map drawn from the truth.
    let frame = match quack_nav::oracle::oracle() {
        Some(o) => o.apply(frame),
        None => frame,
    };
    let grid = frame.grid()?;
    let ground: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&a[1])?)?;
    let name = &a[2];
    let triple = |v: &serde_json::Value| -> Option<(f64, f64, f64)> { Some((v.get(0)?.as_f64()?, v.get(1)?.as_f64()?, v.get(2)?.as_f64()?)) };
    let drops: Vec<((f64, f64), f64)> = ground[name]
        .as_array()
        .map(|v| v.iter().filter_map(triple).map(|(x, y, r)| ((x, y), if r >= 0.10 { 0.12 } else { r })).collect())
        .unwrap_or_default();
    let lanes: Vec<(f64, f64)> = ground[format!("{name}.lanes")]
        .as_array()
        .map(|v| v.iter().filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?))).collect())
        .unwrap_or_default();
    let stops: Vec<(f64, f64)> = a[3..]
        .iter()
        .map(|s| {
            let (x, y) = s.split_once(',').expect("x,y");
            (x.parse().expect("x"), y.parse().expect("y"))
        })
        .collect();
    for w in stops.windows(2) {
        let (from, to) = (w[0], w[1]);
        match path_to(&grid, from.0, from.1, to, &drops, inflate_m(), &lanes) {
            Some(path) => {
                let metres: f64 = path.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum();
                let rim = path
                    .iter()
                    .flat_map(|p| drops.iter().filter(|(_, r)| *r >= 0.10).map(move |(d, r)| (d.0 - p.0).hypot(d.1 - p.1) - r))
                    .fold(f64::INFINITY, f64::min);
                println!("{from:?} -> {to:?}: {metres:.2} m, nearest rim {rim:.2} m, {} points", path.len());
            }
            None => println!("{from:?} -> {to:?}: no way"),
        }
    }
    Ok(())
}
