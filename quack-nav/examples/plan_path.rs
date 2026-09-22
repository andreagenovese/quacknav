//! The route `go_to` would take, printed as JSON — for drawing it.
//!
//!     cargo run -p quack-nav --example plan_path -- <frame.json> <x> <y> <goal_x> <goal_y>

use quack_nav::frontier::{INFLATE_M, path_to};
use quack_nav::map::MapFrame;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(a.len() >= 5, "usage: plan_path <frame.json> <x> <y> <goal_x> <goal_y>");
    let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&a[0])?)?;
    let frame: MapFrame = serde_json::from_value(value.get("frame").cloned().unwrap_or(value))?;
    let grid = frame.grid()?;
    let (x, y): (f64, f64) = (a[1].parse()?, a[2].parse()?);
    let goal: (f64, f64) = (a[3].parse()?, a[4].parse()?);
    let path = path_to(&grid, x, y, goal, &[], INFLATE_M, &[]).unwrap_or_default();
    let metres: f64 = path.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).sum();
    println!("{}", serde_json::json!({"goal": [goal.0, goal.1], "planned": path, "planned_m": metres}));
    Ok(())
}
