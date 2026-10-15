//! The band ([`maploc::band`]) on a recorded session: the recording
//! replayed through the mapper as the live worker feeds it, and every
//! standing frame with the pose tracked booked in the band at that pose —
//! what quack-navd's mapd does live. Writes, beside each other in
//! `out_dir`, the 2D map and the band on the same cells, for
//! `scripts/band/band_vs_truth.py`:
//!
//!     cargo run -p maploc --release --features kinematics --example band -- \
//!         <session.mdlg> <out_dir>
//!
//! `map.u8` and `band.u8` are one byte a cell, row 0 at `y_min` (the map:
//! 0 unknown, 1 free, 2 wall; the band: `Band::render`), `band.json` their
//! geometry.

use std::path::PathBuf;

use kinematics::tof::{Posture, Reprojector};
use maploc::band::{AnchoredBand, BandConfig};
use maploc::mapper::{Mapper, MapperConfig};
use maploc::pipeline::{Slam, SlamConfig};
use maploc::replay::Record;

const N_ZONES: usize = kinematics::tof::ROWS * kinematics::tof::COLS;

fn main() {
    let usage = "usage: band <session.mdlg> <out_dir>";
    let mut args = std::env::args().skip(1);
    let session: PathBuf = args.next().expect(usage).into();
    let out: PathBuf = args.next().expect(usage).into();
    std::fs::create_dir_all(&out).expect("out dir");

    let rp = Reprojector::alpha();
    let mut mapper = Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default()));
    // Bench knobs, to sweep the band's own: BAND_RANGE, BAND_ZLO, BAND_OCC,
    // BAND_HIT, BAND_PASS, BAND_MARGIN, BAND_WARY (`wall_occupied`), BAND_SNAP
    // (`wall_snap_m`).
    let envf = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let d = BandConfig::default();
    let cfg = BandConfig {
        max_range_m: envf("BAND_RANGE", d.max_range_m),
        z_lo_m: envf("BAND_ZLO", d.z_lo_m),
        occupied: envf("BAND_OCC", f32::from(d.occupied)) as i8,
        hit: envf("BAND_HIT", f32::from(d.hit)) as i8,
        pass: envf("BAND_PASS", f32::from(d.pass)) as i8,
        end_margin_m: envf("BAND_MARGIN", d.end_margin_m),
        wall_occupied: envf("BAND_WARY", f32::from(d.wall_occupied)) as i8,
        wall_snap_m: envf("BAND_SNAP", d.wall_snap_m),
        ..d
    };
    // A saved map (`<name>.session` beside its `<name>.band`) is drawn as
    // it was saved, with no replay.
    if session.extension().is_some_and(|e| e == "session") {
        let state = maploc::session::SessionState::load(&session).expect("session").expect("a saved map");
        let slam = Slam::from_session(SlamConfig::default(), state);
        let band = AnchoredBand::load(cfg, &session).expect("band");
        write(&out, &session, &slam, &band);
        return;
    }
    let mut band = AnchoredBand::new(cfg);
    let mut latest: Option<maploc::replay::OdomRecord> = None;
    // Experiment: books only frames whose head moved less than this (deg)
    // since the odometry sample before.
    let head_still = envf("BAND_HEADSTILL", f32::INFINITY).to_radians();
    let mut head_moved = false;
    let (mut frames, mut booked) = (0u64, 0u64);
    maploc::bench::replay(&session, &mut mapper, f32::INFINITY, |step| {
        match step.record {
            Record::Odom(o) => {
                head_moved = latest.is_some_and(|p| p.head.iter().zip(o.head.iter()).any(|(a, b)| (a - b).abs() > head_still));
                latest = Some(*o);
            }
            Record::Tof(frame) => {
                frames += 1;
                let Some(o) = latest else { return true };
                let m = step.mapper;
                if head_moved || o.moving || !m.still() || !m.tracking() || m.untrusted() {
                    return true;
                }
                let posture = Posture { gravity: o.gravity.map(f64::from), trunk_height_m: (o.trunk_z > 0.02).then_some(f64::from(o.trunk_z)) };
                let mut ranges = [None; N_ZONES];
                for (slot, (row, srow)) in ranges.chunks_mut(8).zip(frame.ranges_m.iter().zip(frame.status.iter())) {
                    for ((s, &r), &st) in slot.iter_mut().zip(row.iter()).zip(srow.iter()) {
                        if (st == 5 || st == 9) && r.is_finite() && r > 0.0 {
                            *s = Some(f64::from(r));
                        }
                    }
                }
                let flat = maploc::flat::flatten(&rp, &ranges, o.head.map(f64::from), &posture);
                if band.observe(m.slam(), &flat.rays) {
                    booked += 1;
                }
            }
            _ => {}
        }
        true
    })
    .expect("replay");
    println!("{}: {frames} depth frames, {booked} booked in the band", session.display());
    write(&out, &session, mapper.slam(), &band);
}

/// The map and the band, drawn on the same cells, into `out`.
fn write(out: &std::path::Path, session: &std::path::Path, slam: &Slam, band: &AnchoredBand) {
    let band = band.view(slam);
    let grid = slam.render().expect("a map");
    let (rows, cols) = (grid.height(), grid.width());
    let mut cells = Vec::with_capacity(rows * cols);
    let mut walls = Vec::with_capacity(rows * cols);
    for i in 0..rows {
        for j in 0..cols {
            let lo = grid.log_at(i, j);
            cells.push(if lo > 150 { 2u8 } else if lo < -50 { 1 } else { 0 });
            walls.push(lo > 150);
        }
    }
    let (x_min, y_min, cell) = (grid.cfg().x_range.0, grid.cfg().y_range.0, grid.cell());
    std::fs::write(out.join("map.u8"), &cells).expect("map");
    std::fs::write(out.join("band.u8"), band.render(x_min, y_min, cell, rows, cols, Some(&walls))).expect("band");
    std::fs::write(
        out.join("band.json"),
        format!(
            "{{\"x_min\": {x_min}, \"y_min\": {y_min}, \"cell_m\": {cell}, \"rows\": {rows}, \"cols\": {cols}, \"z_lo_m\": {}, \"slice_m\": {}}}\n",
            band.cfg().z_lo_m,
            band.cfg().slice_m
        ),
    )
    .expect("json");
    let blocked = band.blocked();
    let overhangs = blocked.iter().filter(|b| matches!(b.2, maploc::band::Column::Blocked { under_clear: true, .. })).count();
    println!(
        "{}: {} cells touched, {} blocked ({overhangs} with a beam under)",
        session.display(),
        band.len(),
        blocked.len()
    );
}
