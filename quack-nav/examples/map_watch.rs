//! Watch robotd's live map from the terminal — the smallest possible
//! `robot.map` client, for checking a robot (or the MuJoCo twin) before
//! any backend is involved.
//!
//!     cargo run -p quack-nav --example map_watch -- [socket] [frames]
//!
//! Defaults: `/run/robotd.sock`, 5 frames. Prints one line per frame and,
//! after the last one, the grid as text (`#` wall, `.` free, space unknown,
//! `D` the duck), decimated to fit 100 columns.

use std::sync::mpsc;

use quack_nav::map::{Cell, MapEvent, MapStreamEnd, run_map_stream};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let socket = args.next().unwrap_or_else(|| "/run/robotd.sock".to_owned());
    let wanted: u64 = args.next().map(|n| n.parse()).transpose()?.unwrap_or(5);

    let (tx, rx) = mpsc::channel();
    let lane = std::thread::spawn(move || run_map_stream(&socket, &tx));

    let mut got = 0;
    let mut last = None;
    for event in &rx {
        match event {
            MapEvent::Subscribed(ack) => {
                println!(
                    "subscribed: accepted={} enabled={} mode={}",
                    ack.accepted,
                    ack.enabled,
                    ack.mode.as_deref().unwrap_or("-")
                );
                if !ack.enabled {
                    println!("mapping is not enabled on this robot; no frames will come");
                    break;
                }
            }
            MapEvent::Frame(frame) => {
                let grid = frame.grid()?;
                let (unknown, free, wall) = grid.counts();
                println!(
                    "seq={} {}x{} @{:.2} m origin=({:+.2},{:+.2}) pose=({:+.2},{:+.2},{:+.2}) \
                     tracking={} seated={} still={} submaps={} loops={} windows={} \
                     free={free} wall={wall} unknown={unknown}",
                    frame.seq,
                    frame.rows,
                    frame.cols,
                    frame.cell_m,
                    frame.x_min,
                    frame.y_min,
                    frame.x,
                    frame.y,
                    frame.yaw,
                    frame.tracking,
                    frame.seated,
                    frame.still,
                    frame.n_submaps,
                    frame.n_loops,
                    frame.windows,
                );
                got += 1;
                last = Some(frame);
                if got >= wanted {
                    break;
                }
            }
        }
    }
    drop(rx);

    if let Some(frame) = last {
        let grid = frame.grid()?;
        let step = grid.cols.div_ceil(100).max(1);
        let duck_col = ((frame.x - grid.x_min) / grid.cell_m).floor() as isize;
        let duck_row = ((frame.y - grid.y_min) / grid.cell_m).floor() as isize;
        println!();
        // Row 0 sits at y_min: print top-down so +y is up, like a floor plan.
        for row in (0..grid.rows).step_by(step).rev() {
            let mut line = String::with_capacity(grid.cols / step + 1);
            for col in (0..grid.cols).step_by(step) {
                let is_duck = (duck_row - row as isize).abs() < step as isize
                    && (duck_col - col as isize).abs() < step as isize;
                // A block reads as its strongest cell, so a wall never
                // disappears in the decimation.
                let mut block = Cell::Unknown;
                for r in row..(row + step).min(grid.rows) {
                    for c in col..(col + step).min(grid.cols) {
                        match grid.cell(r, c) {
                            Some(Cell::Wall) => block = Cell::Wall,
                            Some(Cell::Free) if block == Cell::Unknown => block = Cell::Free,
                            _ => {}
                        }
                    }
                }
                line.push(match (is_duck, block) {
                    (true, _) => 'D',
                    (_, Cell::Wall) => '#',
                    (_, Cell::Free) => '.',
                    (_, Cell::Unknown) => ' ',
                });
            }
            println!("{}", line.trim_end());
        }
    }

    match lane.join().expect("lane thread panicked") {
        Ok(MapStreamEnd::Unsupported) => {
            println!("this robotd has no robot.map (predates API v17)")
        }
        Ok(MapStreamEnd::ReceiverGone) => {}
        Err(e) if got > 0 => eprintln!("(lane ended: {e})"),
        Err(e) => return Err(e),
    }
    Ok(())
}
