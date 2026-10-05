//! The oracle (branch `oracle`, the user's plan of 2026-09-28): the
//! navigation given the house as it is — the true walls and holes, the
//! true pose — so what fails is the navigation's own, and nothing of the
//! map's or the localization's.
//!
//! Three knobs, each an oracle on its own:
//!
//! - `QK_ORACLE_WALLS=<walls.toml>`: the map the navigation plans on is
//!   drawn from the truth's wall segments (centimetres, the MuJoCo world
//!   frame, as `scripts/twin/houses/*.toml` hold them): every cell inside
//!   the house free, every segment a line of wall cells.
//! - `QK_ORACLE_HOLES=<truth.json>`: its `holes` (`[x0, x1, y0, y1]`,
//!   metres) drawn into that map as wall — the planner keeps off them.
//!   The drop book is the protocol's to give (a book of the true rims).
//! - `QK_ORACLE_POSE=<host:port>`: the pose the navigation reads is the
//!   simulator's trunk, read as `poseerr.py` reads it, at 20 Hz.
//! - `QK_ORACLE_AS_MAPPED=1`: the truth drawn as a mapper draws a house —
//!   the holes unknown (no floor ever seen there) instead of wall, and the
//!   inside of each box of the truth's `boxes` unknown past a 5 cm band
//!   (nothing sees inside furniture) — what the pilot (`crate::rlnav`)
//!   reads on a saved map.
//!
//! maploc runs as ever beneath: the homecoming, the tracking flag and the
//! tools' own guards are the real ones. Only what the explorer's `Body`
//! reads as the frame is replaced.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::map::MapFrame;

/// The cell pitch of the drawn map.
const CELL_M: f64 = 0.05;
/// Around the walls' extent, this much more of the drawn map.
const PAD_M: f64 = 0.3;
/// The true pose older than this is not used.
const POSE_STALE: Duration = Duration::from_millis(500);

/// The drawn map: its origin, size and cells (0 unknown, 1 free, 2 wall).
#[derive(Debug, Clone)]
pub struct DrawnMap {
    pub x_min: f64,
    pub y_min: f64,
    pub rows: usize,
    pub cols: usize,
    pub cells: Vec<u8>,
}

/// The wall segments of a truth toml, in metres.
pub fn read_walls(path: &str) -> anyhow::Result<Vec<(f64, f64, f64, f64)>> {
    let v: toml::Value = toml::from_str(&std::fs::read_to_string(path)?)?;
    let walls = v.get("walls").and_then(|w| w.as_array()).ok_or_else(|| anyhow::anyhow!("{path}: no walls"))?;
    let f = |x: &toml::Value| x.as_float().or_else(|| x.as_integer().map(|i| i as f64)).unwrap_or(f64::NAN) / 100.0;
    Ok(walls
        .iter()
        .filter_map(|s| s.as_array())
        .filter(|s| s.len() >= 4)
        .map(|s| (f(&s[0]), f(&s[1]), f(&s[2]), f(&s[3])))
        .collect())
}

/// The holes of a truth json, `[x0, x1, y0, y1]` in metres.
pub fn read_holes(path: &str) -> anyhow::Result<Vec<(f64, f64, f64, f64)>> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    Ok(v.get("holes")
        .and_then(|h| h.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|h| {
                    let h = h.as_array()?;
                    Some((h.first()?.as_f64()?, h.get(1)?.as_f64()?, h.get(2)?.as_f64()?, h.get(3)?.as_f64()?))
                })
                .collect()
        })
        .unwrap_or_default())
}

/// The boxes of a truth json (`[name, x0, x1, y0, y1, h]`), in metres.
pub fn read_boxes(path: &str) -> anyhow::Result<Vec<(f64, f64, f64, f64)>> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    Ok(v.get("boxes")
        .and_then(|h| h.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|b| {
                    let b = b.as_array()?;
                    Some((b.get(1)?.as_f64()?, b.get(2)?.as_f64()?, b.get(3)?.as_f64()?, b.get(4)?.as_f64()?))
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Draw the house: free inside the walls' extent, each segment a line of
/// wall cells (sampled at a quarter cell), each hole's cells wall.
pub fn draw(walls: &[(f64, f64, f64, f64)], holes: &[(f64, f64, f64, f64)]) -> DrawnMap {
    draw_as(walls, holes, &[], false)
}

/// [`draw`], or as a mapper draws it (`as_mapped`): the holes unknown, and
/// `boxes`' insides unknown past a 5 cm band.
pub fn draw_as(walls: &[(f64, f64, f64, f64)], holes: &[(f64, f64, f64, f64)], boxes: &[(f64, f64, f64, f64)], as_mapped: bool) -> DrawnMap {
    let xs = walls.iter().flat_map(|w| [w.0, w.2]);
    let ys = walls.iter().flat_map(|w| [w.1, w.3]);
    let (x_lo, x_hi) = xs.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
    let (y_lo, y_hi) = ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
    let (x_min, y_min) = (x_lo - PAD_M, y_lo - PAD_M);
    let cols = ((x_hi + PAD_M - x_min) / CELL_M).ceil() as usize;
    let rows = ((y_hi + PAD_M - y_min) / CELL_M).ceil() as usize;
    let mut cells = vec![0u8; rows * cols];
    // Free inside the walls' extent; unknown in the pad around it.
    for r in 0..rows {
        for c in 0..cols {
            let (x, y) = (x_min + (c as f64 + 0.5) * CELL_M, y_min + (r as f64 + 0.5) * CELL_M);
            if (x_lo..=x_hi).contains(&x) && (y_lo..=y_hi).contains(&y) {
                cells[r * cols + c] = 1;
            }
        }
    }
    let mut mark = |x: f64, y: f64| {
        let (c, r) = (((x - x_min) / CELL_M).floor(), ((y - y_min) / CELL_M).floor());
        if c >= 0.0 && r >= 0.0 && (c as usize) < cols && (r as usize) < rows {
            cells[r as usize * cols + c as usize] = 2;
        }
    };
    for &(x1, y1, x2, y2) in walls {
        let n = (((x2 - x1).hypot(y2 - y1)) / (CELL_M / 4.0)).ceil().max(1.0) as usize;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            mark(x1 + t * (x2 - x1), y1 + t * (y2 - y1));
        }
    }
    if !as_mapped {
        for &(x0, x1, y0, y1) in holes {
            let mut x = x0.min(x1);
            while x <= x0.max(x1) {
                let mut y = y0.min(y1);
                while y <= y0.max(y1) {
                    mark(x, y);
                    y += CELL_M / 2.0;
                }
                x += CELL_M / 2.0;
            }
        }
        return DrawnMap { x_min, y_min, rows, cols, cells };
    }
    const BAND_M: f64 = 0.05;
    for r in 0..rows {
        for c in 0..cols {
            let (x, y) = (x_min + (c as f64 + 0.5) * CELL_M, y_min + (r as f64 + 0.5) * CELL_M);
            let in_hole = holes.iter().any(|&(x0, x1, y0, y1)| x >= x0.min(x1) && x <= x0.max(x1) && y >= y0.min(y1) && y <= y0.max(y1));
            let inside = boxes.iter().any(|&(x0, x1, y0, y1)| x > x0 + BAND_M && x < x1 - BAND_M && y > y0 + BAND_M && y < y1 - BAND_M);
            if in_hole || (inside && cells[r * cols + c] != 2) {
                cells[r * cols + c] = 0;
            }
        }
    }
    DrawnMap { x_min, y_min, rows, cols, cells }
}

/// Standard base64, padded (as `map.rs` decodes it).
fn b64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut acc = 0u32;
        for (i, &b) in chunk.iter().enumerate() {
            acc |= u32::from(b) << (16 - 8 * i);
        }
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[((acc >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The oracle as configured, built once.
pub struct Oracle {
    map: Option<(DrawnMap, String)>,
    pose: Option<Arc<Mutex<Option<((f64, f64, f64), Instant)>>>>,
}

/// The process's oracle, from the knobs (none set: `None`).
pub fn oracle() -> Option<&'static Oracle> {
    static O: OnceLock<Option<Oracle>> = OnceLock::new();
    O.get_or_init(|| {
        let walls = std::env::var("QK_ORACLE_WALLS").ok();
        let holes = std::env::var("QK_ORACLE_HOLES").ok();
        let pose_at = std::env::var("QK_ORACLE_POSE").ok();
        if walls.is_none() && pose_at.is_none() {
            return None;
        }
        let map = walls.and_then(|w| {
            let segs = read_walls(&w).map_err(|e| tracing::warn!(error = %e, "oracle: no walls")).ok()?;
            let hs = holes.as_deref().map(|h| read_holes(h).unwrap_or_default()).unwrap_or_default();
            let as_mapped = std::env::var("QK_ORACLE_AS_MAPPED").is_ok_and(|v| v == "1");
            let boxes = if as_mapped { holes.as_deref().map(|h| read_boxes(h).unwrap_or_default()).unwrap_or_default() } else { Vec::new() };
            let m = draw_as(&segs, &hs, &boxes, as_mapped);
            tracing::info!(segments = segs.len(), holes = hs.len(), rows = m.rows, cols = m.cols, "oracle: the map drawn from the truth");
            let enc = b64(&m.cells);
            Some((m, enc))
        });
        let pose = pose_at.map(|addr| {
            let slot = Arc::new(Mutex::new(None));
            let s = slot.clone();
            std::thread::Builder::new()
                .name("oracle-pose".into())
                .spawn(move || pose_loop(&addr, &s))
                .expect("spawn the oracle's pose reader");
            tracing::info!("oracle: the pose read from the simulator");
            slot
        });
        Some(Oracle { map, pose })
    })
    .as_ref()
}

/// Read the simulator's trunk at 20 Hz, reconnecting when it goes.
fn pose_loop(addr: &str, slot: &Mutex<Option<((f64, f64, f64), Instant)>>) {
    loop {
        let Ok(mut s) = TcpStream::connect(addr) else {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        };
        let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
        let Ok(r) = s.try_clone() else { continue };
        let mut r = BufReader::new(r);
        let mut line = String::new();
        if writeln!(s, "{}", serde_json::json!({"op": "hello", "protocol": 1, "joints": 15})).is_err() || r.read_line(&mut line).is_err() {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        loop {
            line.clear();
            if writeln!(s, "{}", serde_json::json!({"op": "read"})).is_err() || r.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            let t = v.get("trunk").and_then(|t| t.as_array());
            let q = v.get("imu").and_then(|i| i.get("quat")).and_then(|q| q.as_array());
            if let (Some(t), Some(q)) = (t, q) {
                let g = |a: &Vec<serde_json::Value>, i: usize| a.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0);
                let (w, x, y, z) = (g(q, 0), g(q, 1), g(q, 2), g(q, 3));
                let yaw = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z));
                *slot.lock().expect("oracle pose poisoned") = Some(((g(t, 0), g(t, 1), yaw), Instant::now()));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

impl Oracle {
    /// The frame as the navigation should see it: the drawn map, the true
    /// pose — whichever of them the oracle holds.
    pub fn apply(&self, mut frame: MapFrame) -> MapFrame {
        if let Some((m, enc)) = &self.map {
            frame.x_min = m.x_min as f32;
            frame.y_min = m.y_min as f32;
            frame.cell_m = CELL_M as f32;
            frame.rows = m.rows as u32;
            frame.cols = m.cols as u32;
            frame.cells = enc.clone();
        }
        if let Some(slot) = &self.pose
            && let Some(((x, y, yaw), at)) = *slot.lock().expect("oracle pose poisoned")
            && at.elapsed() < POSE_STALE
        {
            frame.x = x;
            frame.y = y;
            frame.yaw = yaw;
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square room with a hole: walls on its outline, the hole walled,
    /// the rest free, and the base64 decodes back through `MapFrame`.
    #[test]
    fn the_drawn_map_is_the_house() {
        let walls = vec![(-1.0, -1.0, 1.0, -1.0), (1.0, -1.0, 1.0, 1.0), (1.0, 1.0, -1.0, 1.0), (-1.0, 1.0, -1.0, -1.0)];
        let holes = vec![(0.2, 0.4, 0.2, 0.4)];
        let m = draw(&walls, &holes);
        let at = |x: f64, y: f64| m.cells[((y - m.y_min) / CELL_M) as usize * m.cols + ((x - m.x_min) / CELL_M) as usize];
        assert_eq!(at(0.0, 0.0), 1);
        assert_eq!(at(1.0, 0.0), 2);
        assert_eq!(at(0.3, 0.3), 2);
        assert_eq!(at(-1.2, 0.0), 0);
        assert_eq!(crate::map::b64_decode(&b64(&m.cells)).as_deref(), Some(&m.cells[..]));
    }
}
