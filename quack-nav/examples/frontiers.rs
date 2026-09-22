//! Explain the frontier finder on a saved `map.frame`.
//!
//!     cargo run -p quack-nav --example frontiers -- <frame.json> [x,y,r ...]
//!
//! Extra `x,y,r` arguments are local obstacles (world metres, radius) the
//! planner must keep clear of, as the explore job records them.
//!
//! The file is what `sim-maploc/mapdump.py` (or any `robot.map` client)
//! saves: either the frame object itself or `{"frame": {...}}`. Prints the
//! grid's make-up, the raw frontier cells, the groups and which of them
//! are reachable from the frame's own pose — the numbers to look at when
//! "map everything" says it is done and the room says otherwise.

use quack_nav::frontier::{MIN_FRONTIER_CELLS, frontiers};
use quack_nav::map::{Cell, MapFrame};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: frontiers <frame.json> [x,y,r ...]"))?;
    let walls: Vec<((f64, f64), f64)> = args
        .map(|a| {
            let v: Vec<f64> = a.split(',').map(|s| s.parse()).collect::<Result<_, _>>()?;
            anyhow::ensure!(v.len() == 3, "expected x,y,r, got {a}");
            Ok(((v[0], v[1]), v[2]))
        })
        .collect::<anyhow::Result<_>>()?;
    let text = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let frame: MapFrame = serde_json::from_value(value.get("frame").cloned().unwrap_or(value))?;
    let grid = frame.grid()?;
    let (unknown, free, wall) = grid.counts();
    println!(
        "grid {}x{} @ {:.2} m, origin ({:.2}, {:.2}); unknown {unknown} free {free} wall {wall}",
        grid.rows, grid.cols, grid.cell_m, grid.x_min, grid.y_min
    );
    println!(
        "pose ({:.2}, {:.2}, {:.2}) tracking {} — cell under the duck: {:?}",
        frame.x,
        frame.y,
        frame.yaw,
        frame.tracking,
        grid.at(frame.x, frame.y)
    );
    // Raw frontier cells: free with an unknown neighbour.
    let mut raw = 0;
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            if grid.cell(row, col) != Some(Cell::Free) {
                continue;
            }
            let mut touches = false;
            for dr in -1isize..=1 {
                for dc in -1isize..=1 {
                    let (rr, cc) = (row as isize + dr, col as isize + dc);
                    if rr >= 0
                        && cc >= 0
                        && grid.cell(rr as usize, cc as usize) == Some(Cell::Unknown)
                    {
                        touches = true;
                    }
                }
            }
            raw += usize::from(touches);
        }
    }
    println!("raw frontier cells: {raw} (groups need at least {MIN_FRONTIER_CELLS})");
    if !walls.is_empty() {
        println!("local obstacles: {walls:?}");
    }
    let fs = frontiers(&grid, frame.x, frame.y, &[], &walls);
    println!("reachable frontier groups: {}", fs.len());
    for (i, f) in fs.iter().take(8).enumerate() {
        println!(
            "  {i}: {} cells, target ({:.2}, {:.2}), centroid ({:.2}, {:.2}), path {:.2} m ({} points)",
            f.cells,
            f.target.0,
            f.target.1,
            f.centroid.0,
            f.centroid.1,
            f.distance_m,
            f.path.len()
        );
    }
    Ok(())
}
