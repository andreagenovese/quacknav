//! Does a map the robot has just built fit inside one it saved before?
//!
//! A robot switched on in a house it has mapped before cannot name its
//! place from one still window of an 8×8 depth sensor: a flat of repeated
//! rectangles offers several perfect matches and the search rightly
//! refuses to choose (measured 2026-09-09). But it does not have to answer
//! at boot. After a few minutes of ordinary exploring it no longer holds a
//! scan — it holds a map, and asking whether *that* fits inside a saved one
//! compares thousands of cells instead of a couple of hundred beams.
//!
//! The fresh map's occupied cells become a synthetic scan and the same
//! coarse-to-fine search that relocalizes a robot searches the saved map
//! for it. Two things differ from scan-to-map and both are in
//! [`AlignConfig`]: the residual that means "the same place" is larger,
//! because two maps disagree by their own noise rather than by sensor
//! noise; and the wall residual alone cannot see a candidate that lays the
//! fresh map's *floor* on top of the saved map's walls, which the mirror
//! image of a flat always does. Scoring that share alongside the residual
//! puts the truth first.
//!
//! Whether a leading candidate is *believed* is deliberately not decided
//! here. On this sensor no threshold calibrates honestly (see
//! docs — the rival sits at 0.7–0.8 of the winner even when the winner is
//! right). What separates them is time: ask again a few minutes later,
//! with a bigger fresh map, and require the same answer. That rule belongs
//! to the client that watches the map grow, not to one comparison.

use crate::grid::OccupancyGrid;
use crate::relocalize::{RelocalizeConfig, relocalize_against_grid};
use crate::submap::Scan;

/// How the two maps are compared.
#[derive(Debug, Clone, Copy)]
pub struct AlignConfig {
    /// Cells with a log-odds this certain are the map's opinion; the rest
    /// is territory it never saw.
    pub certain_log: i16,
    /// How many wall cells the synthetic scan carries. All of them is
    /// slower and no better: the search cost is linear in beams.
    pub beams: usize,
    /// The residual a candidate must reach to be a candidate at all.
    pub max_mean_residual_m: f32,
    /// What a floor cell laid on a wall costs, against the wall residual.
    pub wall_penalty: f32,
}

impl Default for AlignConfig {
    fn default() -> Self {
        Self {
            certain_log: 200,
            beams: 600,
            max_mean_residual_m: 0.12,
            wall_penalty: 0.5,
        }
    }
}

/// One place in the saved map where the fresh map might sit.
#[derive(Debug, Clone, Copy)]
pub struct Candidate {
    /// Where the fresh map's origin lands in the saved map's frame, and
    /// how it is turned: the transform that carries a fresh-map point to
    /// its saved-map twin.
    pub pose: (f32, f32, f32),
    /// Mean distance from a fresh wall cell to the nearest saved wall,
    /// over the cells where the comparison is possible at all — see
    /// [`Candidate::overlap`].
    pub wall_residual_m: f32,
    /// Share of the fresh map's wall cells that land where the saved map
    /// has an opinion. The rest fall in territory the saved map never
    /// visited, where "how far is the nearest wall" answers a question
    /// nobody asked: a saved map covering half a flat scores worse and
    /// worse as the live map grows past it, which is a property of the
    /// measurement and not of the house. Judge the residual on the
    /// overlap, and read the overlap as its own number.
    pub overlap: f32,
    /// Share of the fresh map's floor cells that land on a saved wall,
    /// counting only cells the saved map has an opinion about.
    pub floor_on_wall: f32,
    /// `wall_residual_m + wall_penalty × floor_on_wall`: lower is better.
    pub score: f32,
}

/// How many cells the map is sure are wall — how much there is to ask
/// with. A map a few minutes old carries a few hundred; below about a
/// hundred the question cannot be asked at all.
pub fn wall_cells(grid: &OccupancyGrid, certain_log: i16) -> usize {
    grid.log_raw().iter().filter(|lo| **lo >= certain_log).count()
}

/// Search `saved` for `fresh`, best first.
///
/// Empty when either map is too thin to ask with — a robot that has just
/// switched on has nothing to compare, and saying so beats answering.
pub fn match_maps(fresh: &OccupancyGrid, saved: &mut OccupancyGrid, cfg: &AlignConfig) -> Vec<Candidate> {
    let (beams, free_cells) = walls_and_floor(fresh, cfg.certain_log);
    if beams.len() < 100 {
        return Vec::new();
    }
    let walls: Vec<(f32, f32)> = beams.iter().map(|(_, end)| *end).collect();
    let scan = Scan { beams }.decimated(cfg.beams);

    let mut reloc = RelocalizeConfig::default();
    reloc.max_mean_residual_m = cfg.max_mean_residual_m;
    reloc.min_beams_used = 100;
    let Some(result) = relocalize_against_grid(saved, &scan, &reloc) else {
        return Vec::new();
    };

    // Re-score every basin the search found, on the overlap only. The
    // search's own residual ranks candidates within one ask, where every
    // candidate meets the same saved map and the bias cancels; it cannot
    // be compared between asks as the live map grows, and comparing across
    // asks is the whole of the recognition rule.
    let field = saved.distance_field_shared(reloc.wall_threshold_fp);
    let mut out: Vec<Candidate> = result
        .basins
        .iter()
        .map(|(pose, _)| {
            let (wall_residual_m, overlap) = residual_on_overlap(saved, &field, &walls, *pose, cfg.certain_log);
            let floor_on_wall = floor_on_wall(saved, &free_cells, *pose, cfg.certain_log);
            Candidate {
                pose: *pose,
                wall_residual_m,
                overlap,
                floor_on_wall,
                score: wall_residual_m + cfg.wall_penalty * floor_on_wall,
            }
        })
        .collect();
    out.sort_by(|a, b| a.score.total_cmp(&b.score));
    out
}

/// The map's walls as beams from its own origin, and its floor as points.
/// What matters to the search is where the endpoints land, so the origin
/// of every beam is the map's own.
fn walls_and_floor(
    grid: &OccupancyGrid,
    certain: i16,
) -> (Vec<((f32, f32), (f32, f32))>, Vec<(f32, f32)>) {
    let cfg = *grid.cfg();
    let (w, h) = (grid.width(), grid.height());
    let log = grid.log_raw();
    let (mut walls, mut floor) = (Vec::new(), Vec::new());
    for i in 0..h {
        for j in 0..w {
            let x = cfg.x_range.0 + (j as f32 + 0.5) * cfg.cell;
            let y = cfg.y_range.0 + (i as f32 + 0.5) * cfg.cell;
            let lo = log[i * w + j];
            if lo >= certain {
                walls.push(((0.0, 0.0), (x, y)));
            } else if lo <= -certain {
                floor.push((x, y));
            }
        }
    }
    (walls, floor)
}

/// How far the fresh map's walls are from the saved map's walls, counting
/// only the ones that land where the saved map has an opinion — and how
/// large a share of them that was.
///
/// A fresh wall on saved floor is a real disagreement and counts, with its
/// distance to the nearest saved wall. A fresh wall in unmapped territory
/// counts for nothing: the distance field there measures how far away the
/// saved map's nearest wall happens to be, which says nothing about
/// whether this is the same house.
fn residual_on_overlap(
    saved: &OccupancyGrid,
    field: &[f32],
    walls: &[(f32, f32)],
    pose: (f32, f32, f32),
    certain: i16,
) -> (f32, f32) {
    let log = saved.log_raw();
    let (w, h) = (saved.width(), saved.height());
    let cfg = *saved.cfg();
    let (sy, cy) = pose.2.sin_cos();
    let (mut sum, mut judged) = (0.0f32, 0u32);
    for (fx, fy) in walls {
        let ex = pose.0 + cy * fx - sy * fy;
        let ey = pose.1 + sy * fx + cy * fy;
        let j = ((ex - cfg.x_range.0) / cfg.cell).floor() as i32;
        let i = ((ey - cfg.y_range.0) / cfg.cell).floor() as i32;
        if i < 0 || j < 0 || i as usize >= h || j as usize >= w {
            continue;
        }
        let idx = i as usize * w + j as usize;
        if log[idx].abs() < certain {
            continue; // the saved map never saw this corner
        }
        judged += 1;
        sum += field[idx];
    }
    if judged == 0 {
        return (f32::MAX, 0.0);
    }
    (sum / judged as f32, judged as f32 / walls.len().max(1) as f32)
}

/// Negative evidence the wall score cannot see: floor the fresh map is
/// sure about, laid on a wall the saved map is sure about. The right
/// overlay puts floor on floor; a mirror image of a flat puts the
/// kitchen's floor inside its walls.
fn floor_on_wall(
    saved: &OccupancyGrid,
    free_cells: &[(f32, f32)],
    pose: (f32, f32, f32),
    certain: i16,
) -> f32 {
    let log = saved.log_raw();
    let (w, h) = (saved.width(), saved.height());
    let cfg = *saved.cfg();
    let (sy, cy) = pose.2.sin_cos();
    let (mut hit, mut judged) = (0u32, 0u32);
    for (fx, fy) in free_cells {
        let ex = pose.0 + cy * fx - sy * fy;
        let ey = pose.1 + sy * fx + cy * fy;
        let j = ((ex - cfg.x_range.0) / cfg.cell).floor() as i32;
        let i = ((ey - cfg.y_range.0) / cfg.cell).floor() as i32;
        if i < 0 || j < 0 || i as usize >= h || j as usize >= w {
            continue;
        }
        let lo = log[i as usize * w + j as usize];
        if lo.abs() < certain {
            continue; // the saved map has no opinion here
        }
        judged += 1;
        if lo >= certain {
            hit += 1;
        }
    }
    if judged > 0 { hit as f32 / judged as f32 } else { 1.0 }
}
