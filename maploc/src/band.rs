//! The band: a 2.5D layer of what stands at the duck's own height.
//!
//! The 2D map flattens every return to its spot on the floor plan and lets
//! every beam clear the cells it crossed, at whatever height it crossed
//! them. Under a coffee table that is wrong twice over: the beams that
//! pass beneath the top clear the cells the top's own returns inked, so
//! the map shows free floor where the duck's head meets the table (the
//! apartment's, top 0.14–0.22 m against a 0.27 m duck: the push and the
//! slide behind journey-books' falls, 2026-10-07).
//!
//! So here every cell (5 cm) keeps a short column of slices over the
//! duck's height only, [`BandConfig::z_lo_m`] up by [`BandConfig::slice_m`]:
//! a return inside the band marks its slice, a beam crossing a slice
//! clears that slice alone. A beam under the table clears the slices under
//! the table and leaves the top's alone; one over it, the slices above.
//! Above the band nothing is kept: a desk at 0.34 m is not in the duck's
//! way, whatever the 2D map makes of it.
//!
//! Rays come levelled, in the body's frame, with z above the floor
//! ([`crate::flat::flatten`] makes them from the same reprojection as the
//! 2D scan); the caller decides when the body and its pose are steady
//! enough to book them (standing, the pose tracked).

use std::collections::HashMap;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::pipeline::Slam;
use crate::pose_graph::{between, compose};
use crate::submap::Pose2;

/// Slices in a cell's column.
pub const SLICES: usize = 10;

#[derive(Debug, Clone, Copy)]
pub struct BandConfig {
    /// Cell pitch, metres — the 2D map's.
    pub cell_m: f32,
    /// The band's floor. Below it a return is the floor's own noise: the
    /// reprojection calls a return floor only above 85 % of the expected
    /// distance, so a far floor return a few degrees off reads 3–4 cm up.
    pub z_lo_m: f32,
    /// One slice's height; the band is `z_lo_m .. z_lo_m + SLICES * slice_m`
    /// (0.05–0.30 m: the duck stands 0.27 m). Not higher: the apartment's
    /// desk, its underside at 0.34 m, filled a slice up to 0.35 from its
    /// front edge and blocked a sixth of the floor under it.
    pub slice_m: f32,
    /// Returns and passes farther than this, horizontally, are not booked:
    /// a zone is ~6° wide and its height spread grows with range.
    pub max_range_m: f32,
    /// What a return adds to its slice, what a crossing beam takes off.
    pub hit: i8,
    pub pass: i8,
    /// Evidence is clamped to ±this, so a moved chair is forgotten in a
    /// few looks.
    pub cap: i8,
    /// A slice at or above this is occupied.
    pub occupied: i8,
    /// Sampling step along a beam.
    pub step_m: f32,
    /// A beam that hit something clears only up to this short of the
    /// return (the sensor's own spread).
    pub end_margin_m: f32,
}

impl Default for BandConfig {
    fn default() -> Self {
        Self {
            cell_m: 0.05,
            z_lo_m: 0.05,
            slice_m: 0.025,
            max_range_m: 1.5,
            hit: 3,
            pass: 1,
            cap: 12,
            occupied: 5,
            step_m: 0.02,
            end_margin_m: 0.05,
        }
    }
}

/// One beam, levelled, in the body frame: from the sensor to where it
/// stopped, z above the floor. `hit` is false for a beam that met the floor
/// (it clears all the way down) and true for one stopped by something.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub hit: bool,
}

/// What a cell's column says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    /// No beam crossed or stopped in the band here.
    Unseen,
    /// Crossed, nothing kept.
    Clear,
    /// Something in the band; `lowest` is the lowest occupied slice, and
    /// `under_clear` says a beam crossed under it — an overhang the 2D map
    /// is likely to call free.
    Blocked { lowest: usize, under_clear: bool },
}

#[derive(Debug, Clone)]
pub struct Band {
    cfg: BandConfig,
    cells: HashMap<(i32, i32), [i8; SLICES]>,
}

impl Band {
    pub fn new(cfg: BandConfig) -> Self {
        Self { cfg, cells: HashMap::new() }
    }

    pub fn cfg(&self) -> &BandConfig {
        &self.cfg
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }

    /// Cells any beam has touched.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    fn key(&self, x: f32, y: f32) -> (i32, i32) {
        ((x / self.cfg.cell_m).floor() as i32, (y / self.cfg.cell_m).floor() as i32)
    }

    fn slice(&self, z: f32) -> Option<usize> {
        let k = ((z - self.cfg.z_lo_m) / self.cfg.slice_m).floor();
        (k >= 0.0 && (k as usize) < SLICES).then_some(k as usize)
    }

    /// Books one frame's beams, taken with the body at `pose` on the map.
    pub fn observe(&mut self, pose: Pose2, rays: &[Ray]) {
        let (c, s) = (pose.2.cos(), pose.2.sin());
        let to_map = |p: [f32; 3]| [pose.0 + c * p[0] - s * p[1], pose.1 + s * p[0] + c * p[1], p[2]];
        let mut hits = Vec::new();
        for ray in rays {
            let (a, b) = (to_map(ray.from), to_map(ray.to));
            let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            let flat = (d[0] * d[0] + d[1] * d[1]).sqrt();
            if len <= 1e-4 {
                continue;
            }
            // How far along the beam to clear: short of a return by the
            // margin, and never past the range kept.
            let mut until = if ray.hit { len - self.cfg.end_margin_m } else { len };
            if flat > self.cfg.max_range_m {
                until = until.min(len * self.cfg.max_range_m / flat);
            }
            let mut last = None;
            let mut t = 0.0;
            while t < until {
                let p = [a[0] + d[0] * t / len, a[1] + d[1] * t / len, a[2] + d[2] * t / len];
                if let Some(k) = self.slice(p[2]) {
                    let key = (self.key(p[0], p[1]), k);
                    if last != Some(key) {
                        last = Some(key);
                        let col = self.cells.entry(key.0).or_insert([0; SLICES]);
                        col[k] = col[k].saturating_sub(self.cfg.pass).max(-self.cfg.cap);
                    }
                }
                t += self.cfg.step_m;
            }
            if ray.hit
                && flat <= self.cfg.max_range_m
                && let Some(k) = self.slice(b[2])
            {
                hits.push((self.key(b[0], b[1]), k));
            }
        }
        // Returns after every pass, so a frame's own beams grazing a
        // surface do not cancel what the frame saw on it.
        for (key, k) in hits {
            let col = self.cells.entry(key).or_insert([0; SLICES]);
            col[k] = col[k].saturating_add(self.cfg.hit).min(self.cfg.cap);
        }
    }

    fn judge(&self, col: &[i8; SLICES]) -> Column {
        match col.iter().position(|&v| v >= self.cfg.occupied) {
            Some(lowest) => Column::Blocked { lowest, under_clear: col[..lowest].iter().any(|&v| v < 0) },
            None if col.iter().any(|&v| v < 0) => Column::Clear,
            None => Column::Unseen,
        }
    }

    /// The column at a map point.
    pub fn at(&self, x: f32, y: f32) -> Column {
        self.cells.get(&self.key(x, y)).map_or(Column::Unseen, |c| self.judge(c))
    }

    /// Adds `other`'s evidence, its frame placed at `anchor` in this one's,
    /// cell by cell (the sums clamped as ever).
    pub fn merge(&mut self, other: &Band, anchor: Pose2) {
        let h = other.cfg.cell_m / 2.0;
        for (&(i, j), col) in &other.cells {
            let (x, y, _) = compose(anchor, (i as f32 * other.cfg.cell_m + h, j as f32 * other.cfg.cell_m + h, 0.0));
            let mine = self.cells.entry(self.key(x, y)).or_insert([0; SLICES]);
            for (m, &v) in mine.iter_mut().zip(col) {
                *m = m.saturating_add(v).clamp(-self.cfg.cap, self.cfg.cap);
            }
        }
    }

    /// Every blocked cell: its centre on the map and its column.
    pub fn blocked(&self) -> Vec<(f32, f32, Column)> {
        let h = self.cfg.cell_m / 2.0;
        let mut out: Vec<_> = self
            .cells
            .iter()
            .filter_map(|(&(i, j), col)| match self.judge(col) {
                b @ Column::Blocked { .. } => Some((i as f32 * self.cfg.cell_m + h, j as f32 * self.cfg.cell_m + h, b)),
                _ => None,
            })
            .collect();
        out.sort_by(|a, b| (a.0, a.1).partial_cmp(&(b.0, b.1)).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    /// The band drawn on a grid's geometry (row 0 at `y_min`, row-major):
    /// one byte a cell, 0 unseen, 1 clear, `2 + lowest` blocked from that
    /// slice up, `2 + SLICES + lowest` an overhang (blocked, with a beam
    /// seen crossing under it).
    pub fn render(&self, x_min: f32, y_min: f32, cell_m: f32, rows: usize, cols: usize) -> Vec<u8> {
        let mut out = vec![0u8; rows * cols];
        for (r, row) in out.chunks_mut(cols).enumerate() {
            let y = y_min + (r as f32 + 0.5) * cell_m;
            for (c, slot) in row.iter_mut().enumerate() {
                *slot = match self.at(x_min + (c as f32 + 0.5) * cell_m, y) {
                    Column::Unseen => 0,
                    Column::Clear => 1,
                    Column::Blocked { lowest, under_clear: false } => 2 + lowest as u8,
                    Column::Blocked { lowest, under_clear: true } => (2 + SLICES + lowest) as u8,
                };
            }
        }
        out
    }
}

/// The band kept the way the 2D map is: one [`Band`] a submap, in that
/// submap's own frame, so a loop closure that moves the submaps moves what
/// they saw with them. Booked at the pose of the moment it drifted with
/// the pose: on the apartment's first 30 minutes on the twin, 85 % of its
/// blocked cells within 10 cm of the furniture, against 97 % kept by
/// submap (the 2D map: 98 %).
#[derive(Debug, Clone)]
pub struct AnchoredBand {
    cfg: BandConfig,
    layers: Vec<Band>,
}

impl AnchoredBand {
    pub fn new(cfg: BandConfig) -> Self {
        Self { cfg, layers: Vec::new() }
    }

    pub fn cfg(&self) -> &BandConfig {
        &self.cfg
    }

    pub fn clear(&mut self) {
        self.layers.clear();
    }

    /// Books a frame into the submap the SLAM inks now, at the pose it
    /// tracks. False when there is no submap yet. Layers past the SLAM's
    /// submaps (a band saved after its map was) are dropped; another map
    /// altogether is the caller's to `clear`.
    pub fn observe(&mut self, slam: &Slam, rays: &[Ray]) -> bool {
        let n = slam.n_submaps();
        self.layers.truncate(n);
        let Some(anchor) = n.checked_sub(1).and_then(|i| slam.anchor(i)) else {
            return false;
        };
        while self.layers.len() < n {
            self.layers.push(Band::new(self.cfg));
        }
        self.layers[n - 1].observe(between(anchor, slam.tracked()), rays);
        true
    }

    /// Where the body stands is floor: the columns within `radius_m` of
    /// the tracked pose are cleared in the submap inked now (which
    /// outweighs what older submaps saw there).
    pub fn strike(&mut self, slam: &Slam, radius_m: f32) {
        let n = slam.n_submaps();
        let (Some(layer), Some(anchor)) = (n.checked_sub(1).and_then(|i| self.layers.get_mut(i)), n.checked_sub(1).and_then(|i| slam.anchor(i))) else {
            return;
        };
        let (x, y, _) = between(anchor, slam.tracked());
        let cell = layer.cfg.cell_m;
        let reach = (radius_m / cell).ceil() as i32 + 1;
        let (ci, cj) = layer.key(x, y);
        let cap = layer.cfg.cap;
        for i in ci - reach..=ci + reach {
            for j in cj - reach..=cj + reach {
                let (cx, cy) = ((i as f32 + 0.5) * cell, (j as f32 + 0.5) * cell);
                if (cx - x).hypot(cy - y) <= radius_m
                    && let Some(col) = layer.cells.get_mut(&(i, j))
                {
                    *col = [-cap; SLICES];
                }
            }
        }
    }

    /// The band on the map's frame, each submap's at its anchor now.
    pub fn view(&self, slam: &Slam) -> Band {
        let mut out = Band::new(self.cfg);
        for (i, layer) in self.layers.iter().enumerate() {
            if let Some(anchor) = slam.anchor(i) {
                out.merge(layer, anchor);
            }
        }
        out
    }

    /// Writes the band beside a session saved at `session` (see
    /// [`band_path`]), atomically, as the session itself is written.
    pub fn save(&self, session: &Path) -> io::Result<()> {
        let path = band_path(session);
        if let Some(p) = path.parent()
            && !p.as_os_str().is_empty()
        {
            std::fs::create_dir_all(p)?;
        }
        let layers: Layers = self.layers.iter().map(|l| l.cells.iter().map(|(k, v)| (*k, *v)).collect()).collect();
        let head = (SLICES as u32, self.cfg.cell_m, self.cfg.z_lo_m, self.cfg.slice_m);
        let tmp = path.with_extension("band.tmp");
        {
            let mut w = BufWriter::new(std::fs::File::create(&tmp)?);
            w.write_all(BAND_MAGIC)?;
            w.write_all(&BAND_VERSION.to_le_bytes())?;
            bincode::serialize_into(&mut w, &(head, layers)).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            w.flush()?;
            w.get_ref().sync_all()?;
        }
        std::fs::rename(&tmp, &path)
    }

    /// The band saved beside `session`; an empty one where there is none
    /// (a map saved before the band existed). `Err` for a file that is
    /// not a band, or one cut on other slices.
    pub fn load(cfg: BandConfig, session: &Path) -> io::Result<Self> {
        let path = band_path(session);
        let mut out = Self::new(cfg);
        if !path.exists() {
            return Ok(out);
        }
        let mut r = BufReader::new(std::fs::File::open(&path)?);
        let mut magic = [0u8; 8];
        r.read_exact(&mut magic)?;
        if &magic[..4] != BAND_MAGIC || u32::from_le_bytes([magic[4], magic[5], magic[6], magic[7]]) != BAND_VERSION {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not a band of this version"));
        }
        let (head, layers): ((u32, f32, f32, f32), Layers) = bincode::deserialize_from(r).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if head != (SLICES as u32, cfg.cell_m, cfg.z_lo_m, cfg.slice_m) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("a band cut otherwise ({head:?})")));
        }
        out.layers = layers
            .into_iter()
            .map(|cells| {
                let mut band = Band::new(cfg);
                band.cells = cells.into_iter().collect();
                band
            })
            .collect();
        Ok(out)
    }
}

const BAND_MAGIC: &[u8; 4] = b"MDLB";
/// A band's layers as written: each one's columns by cell.
type Layers = Vec<Vec<((i32, i32), [i8; SLICES])>>;
const BAND_VERSION: u32 = 1;

/// The band's file beside a session's: `house.session` → `house.band`.
pub fn band_path(session: &Path) -> PathBuf {
    session.with_extension("band")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fan of beams from a sensor 0.24 m up, straight ahead along x,
    /// elevations from -20° to +20°, stopped by a table top spanning
    /// x in `top` at heights `z_top` (or by a far wall at x = 2).
    fn fan(top: (f32, f32), z_top: (f32, f32)) -> Vec<Ray> {
        let from = [0.0, 0.0, 0.24];
        (0..16)
            .map(|i| {
                let el = (-20.0 + 40.0 * i as f32 / 15.0f32).to_radians();
                let (dx, dz) = (el.cos(), el.sin());
                // March to the first thing: the floor, the top, the wall.
                let mut t = 0.0;
                loop {
                    t += 0.005;
                    let p = [dx * t, 0.0, from[2] + dz * t];
                    if p[2] <= 0.0 {
                        return Ray { from, to: p, hit: false };
                    }
                    let in_top = p[0] >= top.0 && p[0] <= top.1 && p[2] >= z_top.0 && p[2] <= z_top.1;
                    if in_top || p[0] >= 2.0 {
                        return Ray { from, to: p, hit: true };
                    }
                }
            })
            .collect()
    }

    #[test]
    fn a_low_table_top_stays_blocked_with_the_floor_clear_under_it() {
        let mut band = Band::new(BandConfig::default());
        // The apartment's coffee table: top 0.14–0.22 m, 0.6 m deep, seen
        // from 0.5 m, from 0.4, from 0.3 as the duck closes in.
        for x0 in [0.5f32, 0.4, 0.3] {
            for _ in 0..5 {
                band.observe((0.0, 0.0, 0.0), &fan((x0, x0 + 0.6), (0.14, 0.22)));
            }
        }
        match band.at(0.32, 0.0) {
            Column::Blocked { lowest, under_clear } => {
                assert!((3..=6).contains(&lowest), "the top's slices (0.125–0.225 m): {lowest}");
                assert!(under_clear, "the floor under it was crossed");
            }
            other => panic!("the table's edge is in the duck's way: {other:?}"),
        }
    }

    #[test]
    fn a_desk_above_the_band_is_not_in_the_way() {
        let mut band = Band::new(BandConfig::default());
        for _ in 0..10 {
            band.observe((0.0, 0.0, 0.0), &fan((0.5, 1.1), (0.34, 0.38)));
        }
        for x in [0.55f32, 0.7, 0.9] {
            assert_ne!(
                std::mem::discriminant(&band.at(x, 0.0)),
                std::mem::discriminant(&Column::Blocked { lowest: 0, under_clear: false }),
                "under the desk at {x}: {:?}",
                band.at(x, 0.0)
            );
        }
    }

    #[test]
    fn the_pose_places_the_beams_and_a_render_draws_them() {
        let mut band = Band::new(BandConfig::default());
        let wall = |_: ()| Ray { from: [0.0, 0.0, 0.24], to: [1.01, 0.0, 0.10], hit: true };
        for _ in 0..3 {
            band.observe((1.025, 2.0, std::f32::consts::FRAC_PI_2), &[wall(())]);
        }
        // Facing +y from (1.025, 2): the return is at (1.025, 3.01), 0.10 m
        // up — slice 2.
        assert_eq!(band.at(1.025, 3.01), Column::Blocked { lowest: 2, under_clear: false });
        assert_eq!(band.at(1.025, 2.6), Column::Clear);
        let grid = band.render(0.5, 2.5, 0.05, 12, 20);
        // (1.025, 3.01) sits in row 10, column 10.
        assert_eq!(grid[10 * 20 + 10], 4);
        assert!(grid.contains(&1) && grid.contains(&0));
    }

    #[test]
    fn a_layer_merged_at_its_anchor_lands_where_the_anchor_puts_it() {
        let mut layer = Band::new(BandConfig::default());
        let ray = Ray { from: [0.0, 0.0, 0.24], to: [0.51, 0.0, 0.10], hit: true };
        for _ in 0..3 {
            layer.observe((0.0, 0.0, 0.0), &[ray]);
        }
        assert!(matches!(layer.at(0.52, 0.02), Column::Blocked { .. }));
        // The submap's frame at (1, 1) turned a quarter left: its +x is +y.
        let mut map = Band::new(BandConfig::default());
        map.merge(&layer, (1.0, 1.0, std::f32::consts::FRAC_PI_2));
        assert!(matches!(map.at(0.98, 1.52), Column::Blocked { .. }), "{:?}", map.at(0.98, 1.52));
        assert_eq!(map.at(1.52, 0.98), Column::Unseen);
        // Merged twice, the evidence adds up to the cap and no further.
        map.merge(&layer, (1.0, 1.0, std::f32::consts::FRAC_PI_2));
        assert_eq!(map.blocked().len(), 1);
    }

    #[test]
    fn an_anchored_band_waits_for_a_submap() {
        let mut band = AnchoredBand::new(BandConfig::default());
        let slam = Slam::new(crate::pipeline::SlamConfig::default());
        assert!(!band.observe(&slam, &[Ray { from: [0.0, 0.0, 0.24], to: [0.5, 0.0, 0.1], hit: true }]));
        assert!(band.view(&slam).is_empty());
    }

    #[test]
    fn a_band_saved_beside_its_session_comes_back() {
        let dir = std::env::temp_dir().join(format!("band-test-{}", std::process::id()));
        let session = dir.join("house.session");
        let mut band = AnchoredBand::new(BandConfig::default());
        let mut layer = Band::new(BandConfig::default());
        for _ in 0..3 {
            layer.observe((0.0, 0.0, 0.0), &[Ray { from: [0.0, 0.0, 0.24], to: [0.51, 0.0, 0.10], hit: true }]);
        }
        band.layers = vec![Band::new(BandConfig::default()), layer];
        band.save(&session).unwrap();
        assert!(dir.join("house.band").exists());
        let back = AnchoredBand::load(BandConfig::default(), &session).unwrap();
        assert_eq!(back.layers.len(), 2);
        assert_eq!(back.layers[1].blocked(), band.layers[1].blocked());
        // None saved: empty; cut otherwise: refused.
        assert!(AnchoredBand::load(BandConfig::default(), &dir.join("other.session")).unwrap().layers.is_empty());
        assert!(AnchoredBand::load(BandConfig { slice_m: 0.05, ..BandConfig::default() }, &session).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn where_the_body_stands_is_struck_off() {
        let mut slam = Slam::new(crate::pipeline::SlamConfig::default());
        slam.observe_odom((0.0, 0.0, 0.0));
        slam.tick(0.0);
        let mut band = AnchoredBand::new(BandConfig::default());
        // Something booked right under the body (a phantom), and a wall ahead.
        let under = Ray { from: [0.08, 0.0, 0.24], to: [0.12, 0.01, 0.10], hit: true };
        let ahead = Ray { from: [0.08, 0.0, 0.24], to: [0.81, 0.0, 0.10], hit: true };
        for _ in 0..3 {
            assert!(band.observe(&slam, &[under, ahead]));
        }
        assert!(matches!(band.view(&slam).at(0.12, 0.01), Column::Blocked { .. }));
        // The body steps forward 0.1 m and stands there.
        slam.set_tracked((0.1, 0.0, 0.0));
        band.strike(&slam, 0.06);
        let view = band.view(&slam);
        assert!(!matches!(view.at(0.12, 0.01), Column::Blocked { .. }), "{:?}", view.at(0.12, 0.01));
        assert!(matches!(view.at(0.81, 0.0), Column::Blocked { .. }));
    }
}
