//! The replay a bench drives: a v2 `.mdlg` recording fed through a
//! [`Mapper`] the way the live worker feeds it — odometry ticks to
//! `observe`, depth frames reprojected through the head FK to `frame` —
//! with a callback after every record, so a bench can sample the pose, keep
//! the notes, or stop.
//!
//! Deterministic: the same recording through the same build gives the same
//! poses (the RNG is pinned, see [`crate::rng`]), which is what lets a
//! trajectory metric measured on the bench stand for a change to the mapper.
//! The frame decoding is `examples/evaluate.rs`'s, which stays as it is so
//! its numbers stay comparable with the ones already written down.

use std::io;
use std::path::Path;

use kinematics::tof::{Posture, Reprojector};

use crate::mapper::{Mapper, MapperSample, Note};
use crate::replay::{Record, SessionReplayer};
use crate::submap::Scan;

const N_ZONES: usize = kinematics::tof::ROWS * kinematics::tof::COLS;

/// What the callback is told after each record.
pub struct Step<'a> {
    /// Seconds since the recording began.
    pub t_s: f32,
    /// The same instant on the Unix clock (the recorder's epoch plus `t_s`),
    /// to join the replay with anything sampled live — the twin's truth.
    pub unix_s: f64,
    pub mapper: &'a Mapper,
    /// The notes this record produced.
    pub notes: &'a [Note],
}

/// How the replay went: records read, and why it stopped.
#[derive(Debug, Default, Clone, Copy)]
pub struct Replayed {
    pub odom: u64,
    pub frames: u64,
    pub t_end_s: f32,
}

/// Replays `path` into `mapper` up to `max_t_s` seconds (or the end), calling
/// `each` after every record; `each` returning false stops the replay.
pub fn replay(
    path: &Path,
    mapper: &mut Mapper,
    max_t_s: f32,
    each: impl FnMut(Step<'_>) -> bool,
) -> io::Result<Replayed> {
    replay_loading(path, mapper, max_t_s, None, each)
}

/// [`replay`], with the map swapped at `load.0` seconds for the one
/// `load.1` builds from the mapper of the moment — as
/// the daemon does it live: it boots on a fresh map and the homecoming
/// loads the saved one some seconds later, so the first still windows go
/// to the fresh map. A replay into the saved map from the first frame
/// had those windows for its search and came home in 45 s where the live
/// run took 150 (casa_arredata on the twin, 2026-09-28).
pub fn replay_loading(
    path: &Path,
    mapper: &mut Mapper,
    max_t_s: f32,
    mut load: Option<(f32, Box<dyn FnOnce(&Mapper) -> Mapper>)>,
    mut each: impl FnMut(Step<'_>) -> bool,
) -> io::Result<Replayed> {
    let replayer = SessionReplayer::open(path)?;
    let epoch_s = replayer.epoch_unix_ms() as f64 / 1000.0;
    let rp = Reprojector::alpha();
    let mut latest = None;
    let mut notes: Vec<Note> = Vec::new();
    let mut out = Replayed::default();
    // `REPLAY_HEAD_DT_MS=<ms>`: each depth frame takes the head's pose from
    // the robot-state sample nearest its own time plus this, instead of the
    // last sample before it — to measure what the pairing of the head with
    // the frames costs the map (a head sweeping at a stand, paired a few
    // tens of milliseconds off, points every beam a few degrees wrong).
    let head_dt_us: Option<i64> = std::env::var("REPLAY_HEAD_DT_MS").ok().and_then(|v| v.parse::<f64>().ok()).map(|ms| (ms * 1000.0) as i64);
    // A recording still being written ends mid-record: the records before
    // the cut are replayed, as the streaming replay always did.
    let mut records: Vec<Record> = Vec::new();
    for r in replayer {
        match r {
            Ok(r) => records.push(r),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
    }
    let heads: Vec<(i64, [f32; 4])> = records
        .iter()
        .filter_map(|r| if let Record::Odom(o) = r { Some((o.ts_us as i64, o.head)) } else { None })
        .collect();
    let head_at = |t_us: i64| -> Option<[f32; 4]> {
        let i = heads.partition_point(|(t, _)| *t < t_us);
        let a = i.checked_sub(1).map(|k| heads[k]);
        let b = heads.get(i).copied();
        match (a, b) {
            (Some(a), Some(b)) => Some(if (t_us - a.0) <= (b.0 - t_us) { a.1 } else { b.1 }),
            (Some(a), None) => Some(a.1),
            (None, Some(b)) => Some(b.1),
            (None, None) => None,
        }
    };
    // A recording with robotd's and tofd's CLOCK_MONOTONIC stamps (from
    // 2026-09-29) replays the live worker's pairing exactly: a frame waits
    // until a sample at or after its stamp plus the lead has come, then takes
    // the head, gravity and height interpolated at that instant (see
    // quack-nav's `mapd::head_at`); older recordings keep the last sample
    // before the frame, or `REPLAY_HEAD_DT_MS`.
    let lead_ns: u64 = std::env::var("MAPLOC_HEAD_LEAD_MS").ok().and_then(|v| v.parse::<f64>().ok()).map(|ms| (ms.max(0.0) * 1e6) as u64).unwrap_or(5_000_000);
    let stamped = records.iter().any(|r| matches!(r, Record::Odom(o) if o.t_ns > 0));
    let mut recent: std::collections::VecDeque<crate::replay::OdomRecord> = std::collections::VecDeque::with_capacity(128);
    let mut pending: std::collections::VecDeque<crate::replay::TofRecord> = std::collections::VecDeque::new();
    let ingest = |mapper: &mut Mapper, t: f32, frame: &crate::replay::TofRecord, o: &crate::replay::OdomRecord| {
        let posture = Posture { gravity: o.gravity.map(f64::from), trunk_height_m: (o.trunk_z > 0.02).then_some(f64::from(o.trunk_z)) };
        let mut ranges = [None; N_ZONES];
        for (slot, (row, srow)) in ranges.chunks_mut(8).zip(frame.ranges_m.iter().zip(frame.status.iter())) {
            for ((s, &r), &st) in slot.iter_mut().zip(row.iter()).zip(srow.iter()) {
                if (st == 5 || st == 9) && r.is_finite() && r > 0.0 {
                    *s = Some(f64::from(r));
                }
            }
        }
        let flat = crate::flat::flatten(&rp, &ranges, o.head.map(f64::from), &posture);
        if !flat.angles_body.is_empty() {
            let scan = Scan::from_polar(&flat.angles_body, &flat.ranges, flat.sensor_xy, 1e-3);
            mapper.frame(t, scan);
        }
    };
    for record in records {
        let t = record.ts_us() as f32 / 1e6;
        if t > max_t_s {
            break;
        }
        out.t_end_s = t;
        if load.as_ref().is_some_and(|(at, _)| t >= *at)
            && let Some((_, build)) = load.take()
        {
            *mapper = build(mapper);
        }
        match record {
            Record::Twin(_) => {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "a v1 prototype capture: use the `replay` example"));
            }
            Record::Odom(o) => {
                out.odom += 1;
                mapper.observe(
                    t,
                    MapperSample { odom: (o.odom_x, o.odom_y, o.odom_yaw), moving: o.moving, sitting: o.sitting, fallen: o.fallen },
                    &mut notes,
                );
                latest = Some(o);
                if stamped {
                    recent.push_back(o);
                    while recent.len() > 128 {
                        recent.pop_front();
                    }
                    while let Some(frame) = pending.front() {
                        let bracketed = frame.t_ns.saturating_add(lead_ns) <= o.t_ns;
                        let stale = o.t_ns.saturating_sub(frame.t_ns) > 200_000_000;
                        if !bracketed && !stale {
                            break;
                        }
                        let frame = pending.pop_front().expect("front seen");
                        let paired = paired_at(&recent, frame.t_ns.saturating_add(lead_ns)).unwrap_or(o);
                        ingest(mapper, t, &frame, &paired);
                    }
                }
            }
            Record::Tof(frame) => {
                out.frames += 1;
                let Some(o) = latest.as_ref() else { continue };
                if stamped && frame.t_ns > 0 {
                    if recent.back().is_some_and(|s| s.t_ns < frame.t_ns.saturating_add(lead_ns)) {
                        pending.push_back(frame);
                        while pending.len() > 8 {
                            pending.pop_front();
                        }
                    } else {
                        let paired = paired_at(&recent, frame.t_ns.saturating_add(lead_ns)).unwrap_or(*o);
                        ingest(mapper, t, &frame, &paired);
                    }
                } else {
                    let mut o = *o;
                    if let Some(h) = head_dt_us.and_then(|dt| head_at(frame.ts_us as i64 + dt)) {
                        o.head = h;
                    }
                    ingest(mapper, t, &frame, &o);
                }
            }
        }
        let go_on = each(Step { t_s: t, unix_s: epoch_s + f64::from(t), mapper, notes: &notes });
        notes.clear();
        if !go_on {
            break;
        }
    }
    Ok(out)
}

/// The robot state as it was at `t_ns`, as the live worker pairs it: the
/// two samples around that instant, head, gravity and height interpolated,
/// the rest from the later one; `None` when the ring does not bracket it.
fn paired_at(recent: &std::collections::VecDeque<crate::replay::OdomRecord>, t_ns: u64) -> Option<crate::replay::OdomRecord> {
    if t_ns == 0 || recent.len() < 2 || recent.back()?.t_ns < t_ns {
        return None;
    }
    let after = recent.iter().position(|s| s.t_ns >= t_ns)?;
    if after == 0 {
        return None;
    }
    let (a, b) = (&recent[after - 1], &recent[after]);
    let span = (b.t_ns - a.t_ns) as f64;
    let f = if span > 0.0 { (t_ns - a.t_ns) as f64 / span } else { 1.0 };
    // In f64 as live mixes them, then back to the recording's f32.
    let mix = |x: f32, y: f32| (f64::from(x) + (f64::from(y) - f64::from(x)) * f) as f32;
    let mut paired = *b;
    for k in 0..4 {
        paired.head[k] = mix(a.head[k], b.head[k]);
    }
    for k in 0..3 {
        paired.gravity[k] = mix(a.gravity[k], b.gravity[k]);
    }
    paired.trunk_z = mix(a.trunk_z, b.trunk_z);
    Some(paired)
}
