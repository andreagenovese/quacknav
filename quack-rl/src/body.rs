//! The simulated duck as quack-navd's journey sees a body
//! (`quack_nav::explore::Body`): timed moves, stands, a map frame once a
//! second, the depth stream at 15 Hz, odometry — over a [`World`] whose
//! movers move while the duck walks and while it stands.
//!
//! The gait is the paper twin's, its numbers from [`Calib`]: forward speed
//! per vx, yaw per unit with a veer on "straight" legs, short pulses that
//! turn by what the gait happens to do, no turn in place below the dead
//! zone, backing only with a positive yaw from a standstill. A step into
//! something ends the move there (a bump); a step over a hole's edge is a
//! fall, and the run ends.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use quack_nav::cliff::{CliffStatus, StreamState};
use quack_nav::explore::{Body, ExploreHandle};
use quack_nav::map::MapFrame;
use serde_json::{Value, json};

use crate::calib::Calib;
use crate::scenarios::Scenario;
use crate::world::World;
use crate::{Rng, dist};

const DT: f64 = 0.1;
/// Frames older than this are dropped: the journey reads three seconds
/// (`cliff::MEMORY`), the pilot 1.4.
const KEEP_S: f64 = 3.5;
/// A stand this long is a stand for the mapper: the pose is corrected.
const STAND_CORRECTS_S: f64 = 1.5;
/// The head's sweep at a stand.
const SWEEP: f64 = 0.8;

/// What the body did since the brain last asked (the reward's terms).
#[derive(Debug, Clone, Default)]
pub struct Since {
    pub walked_forward_s: f64,
    pub walked_back_s: f64,
    pub bumps: u32,
    pub mover_bumps: u32,
    pub stood_s: f64,
}

pub struct Sim {
    pub world: World,
    pub calib: Calib,
    pub rng: Rng,
    t0: Instant,
    pub t: f64,
    // truth
    pub x: f64,
    pub y: f64,
    pub yaw: f64,
    pub fell: bool,
    pub bumps: u32,
    pub mover_bumps: u32,
    pub path_m: f64,
    pub min_hole_m: f64,
    pub min_static_m: f64,
    last_walk: Option<f64>,
    // the map's pose: truth + err + bias, published every map_period_s
    err: (f64, f64, f64),
    pub bias: (f64, f64),
    shown: (f64, f64, f64),
    next_map_t: f64,
    map_seq: u64,
    // odometry: its own frame, drifting
    odom: (f64, f64, f64),
    // the map
    cells_b64: String,
    grid_x0: f64,
    grid_y0: f64,
    rows: u32,
    cols: u32,
    // the sensor
    cliff: CliffStatus,
    next_frame_t: f64,
    seq: u64,
    head: f64,
    pub since: Since,
    /// Ends the journey at a fall or past the deadline.
    pub handle: Option<ExploreHandle>,
    pub deadline_s: f64,
}

pub fn b64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

impl Sim {
    pub fn new(s: &Scenario, calib: Calib, seed: u64) -> Self {
        let (codes, grid) = s.world.raster(s.bias);
        let mut b = Self {
            world: s.world.clone(),
            calib,
            rng: Rng::new(seed ^ 0xB0D7),
            t0: Instant::now(),
            t: 0.0,
            x: s.start.0,
            y: s.start.1,
            yaw: crate::wrap(s.start.2),
            fell: false,
            bumps: 0,
            mover_bumps: 0,
            path_m: 0.0,
            min_hole_m: f64::INFINITY,
            min_static_m: f64::INFINITY,
            last_walk: None,
            err: (0.0, 0.0, 0.0),
            bias: s.bias,
            shown: (0.0, 0.0, 0.0),
            next_map_t: 0.0,
            map_seq: 0,
            odom: (0.0, 0.0, 0.0),
            cells_b64: b64(&codes),
            grid_x0: grid.x_min,
            grid_y0: grid.y_min,
            rows: grid.rows as u32,
            cols: grid.cols as u32,
            cliff: CliffStatus { stream: StreamState::Serving, body_seen: true, ..CliffStatus::default() },
            next_frame_t: 0.0,
            seq: 0,
            head: 0.0,
            since: Since::default(),
            handle: None,
            deadline_s: f64::INFINITY,
        };
        b.publish_map();
        b
    }

    pub fn instant(&self) -> Instant {
        self.t0 + Duration::from_secs_f64(self.t)
    }

    pub fn truth(&self) -> (f64, f64, f64) {
        (self.x, self.y, self.yaw)
    }

    /// The pose the map would publish now.
    fn map_pose_now(&mut self) -> (f64, f64, f64) {
        let j = self.calib.pose_jitter_m;
        (
            self.x + self.err.0 + self.bias.0 + j * self.rng.gauss(),
            self.y + self.err.1 + self.bias.1 + j * self.rng.gauss(),
            crate::wrap(self.yaw + self.err.2 + 0.3 * j * self.rng.gauss()),
        )
    }

    fn publish_map(&mut self) {
        self.shown = self.map_pose_now();
        self.map_seq += 1;
        self.next_map_t = self.t + self.calib.map_period_s;
    }

    /// Time passes by `dt` with the body doing (v, w); returns false when
    /// the body met something (and did not move).
    fn tick(&mut self, dt: f64, v: f64, w: f64, walking: bool) -> bool {
        // Against something, the body still turns, and slides along it by
        // what of the step is along its face — at half the speed, the feet
        // scuffing (a counted bump all the same).
        let mut moved = true;
        let r = self.calib.body_r;
        let (dx, dy) = (v * dt * self.yaw.cos(), v * dt * self.yaw.sin());
        let mut to = (self.x + dx, self.y + dy);
        if v.abs() > 1e-9 {
            let blocked = |p: (f64, f64)| self.world.static_collides(p.0, p.1, r) || self.world.mover_collides(p.0, p.1, r).is_some();
            if blocked(to) {
                if self.world.mover_collides(to.0, to.1, r).is_some() {
                    self.mover_bumps += 1;
                    self.since.mover_bumps += 1;
                } else {
                    tracing::debug!(at = ?(self.x, self.y, self.yaw), v, w, clear = self.world.static_clearance(self.x, self.y) - r, "sim: bump");
                    self.bumps += 1;
                    self.since.bumps += 1;
                }
                moved = false;
                let slides = [(self.x + 0.5 * dx, self.y), (self.x, self.y + 0.5 * dy)];
                let best = slides.iter().filter(|p| !blocked(**p)).max_by(|a, b| dist(**a, (self.x, self.y)).total_cmp(&dist(**b, (self.x, self.y))));
                to = best.copied().unwrap_or((self.x, self.y));
            }
        }
        let ds = dist(to, (self.x, self.y));
        let dyaw = w * dt;
        self.x = to.0;
        self.y = to.1;
        self.yaw = crate::wrap(self.yaw + dyaw);
        if ds > 0.0 {
            self.path_m += ds;
            self.min_hole_m = self.min_hole_m.min(self.world.hole_clearance(self.x, self.y));
            self.min_static_m = self.min_static_m.min(self.world.static_clearance(self.x, self.y) - r);
        }
        // Odometry and the map's error drift with the motion.
        let c = &self.calib;
        let (exy, eyaw) = (c.odom_xy_per_m * ds, c.odom_yaw_per_rad * dyaw.abs());
        let (g1, g2, g3, g4, g5, g6) = (self.rng.gauss(), self.rng.gauss(), self.rng.gauss(), self.rng.gauss(), self.rng.gauss(), self.rng.gauss());
        let sign = if v >= 0.0 { 1.0 } else { -1.0 };
        self.odom.2 = crate::wrap(self.odom.2 + dyaw + eyaw * g1);
        self.odom.0 += sign * ds * self.odom.2.cos() + exy * g2;
        self.odom.1 += sign * ds * self.odom.2.sin() + exy * g3;
        self.err.0 += exy * g4;
        self.err.1 += exy * g5;
        self.err.2 += eyaw * g6;
        if !self.fell && self.world.in_hole(self.x, self.y) {
            self.fell = true;
            if let Some(h) = &self.handle {
                h.request_stop();
            }
        }
        // The rest of the house.
        let duck = (self.x, self.y);
        let mut rng = self.rng.clone();
        self.world.step_movers(dt, &mut rng, duck, r);
        self.rng = rng;
        self.t += dt;
        // The sensor's frames, the map's frame.
        while self.next_frame_t <= self.t {
            self.seq += 1;
            let at = self.t0 + Duration::from_secs_f64(self.next_frame_t);
            let mut rng = self.rng.clone();
            let f = self.world.sense((self.x, self.y, self.yaw), self.head, walking, &self.calib, &mut rng, self.seq, at);
            self.rng = rng;
            self.cliff.frames += 1;
            self.cliff.recent.push(f);
            self.next_frame_t += 1.0 / self.calib.tof_hz;
        }
        let keep = self.instant() - Duration::from_secs_f64(KEEP_S.min(self.t));
        self.cliff.recent.retain(|f| f.at >= keep);
        if self.t >= self.next_map_t {
            self.publish_map();
        }
        if self.t > self.deadline_s
            && let Some(h) = &self.handle
        {
            h.request_stop();
        }
        moved
    }

    /// A timed move, as the gait answers (vx, vyaw) (the paper twin's
    /// model, its numbers from the calibration).
    pub fn walk(&mut self, vx: f64, vyaw: f64, secs: f64) {
        let c = self.calib.clone();
        let spinning = vx.abs() < 0.05;
        let can_spin = self.last_walk.is_some_and(|w| self.t - w < 1.5);
        let pulse_gain = if vx > 0.05 && secs <= 0.8 && vyaw.abs() > 0.3 { (c.pulse_gain_mean + c.pulse_gain_sd * self.rng.gauss()).clamp(-0.6, 2.2) } else { 1.0 };
        let speed_k = (1.0 + c.speed_sd * self.rng.gauss()).clamp(0.5, 1.5);
        let turn_k = (1.0 + c.turn_sd * self.rng.gauss()).clamp(0.5, 1.5);
        self.head = 0.0;
        let n = (secs / DT).ceil().max(1.0);
        let dt = secs / n;
        let (b0, m0, sb0, sm0) = (self.bumps, self.mover_bumps, self.since.bumps, self.since.mover_bumps);
        for _ in 0..n as usize {
            let (v, w) = if vx > 0.05 {
                (c.speed_at_03 * vx / 0.3 * speed_k, c.yaw_per_unit * vyaw * pulse_gain + if vyaw.abs() < 0.1 { c.straight_veer } else { 0.0 })
            } else if vx < -0.05 {
                if vyaw > 0.3 || can_spin { (-c.back_speed * speed_k, 0.6 * vyaw) } else { (0.0, 0.0) }
            } else if let Some(w) = spinning.then(|| c.in_place_rate(vyaw)).flatten() {
                (0.0, w * turn_k)
            } else if spinning && can_spin && vyaw.abs() > 0.3 {
                (0.0, c.spin_rate * vyaw.signum() * turn_k)
            } else {
                (0.0, 0.0)
            };
            let walking = v.abs() > 0.0 || w.abs() > 0.0;
            self.tick(dt, v, w, walking);
            if v > 0.0 {
                self.since.walked_forward_s += dt;
            } else if v < 0.0 {
                self.since.walked_back_s += dt;
            }
            if self.fell {
                break;
            }
        }
        if vx.abs() > 0.05 {
            self.last_walk = Some(self.t);
        }
        // One bump per move, however many ticks it pushed.
        self.bumps = self.bumps.min(b0 + 1);
        self.mover_bumps = self.mover_bumps.min(m0 + 1);
        self.since.bumps = self.since.bumps.min(sb0 + 1);
        self.since.mover_bumps = self.since.mover_bumps.min(sm0 + 1);
    }

    /// Standing still for `secs`: the head sweeps, the movers move, and a
    /// stand long enough is the mapper's — the pose's error shrinks.
    pub fn stand(&mut self, secs: f64) {
        let n = (secs / DT).ceil();
        let dt = if n > 0.0 { secs / n } else { 0.0 };
        for k in 0..n as usize {
            // The head: centre, left, right, centre over the stand.
            let phase = if secs >= STAND_CORRECTS_S { (k as f64 * dt / secs * std::f64::consts::TAU).sin() } else { 0.0 };
            self.head = SWEEP * phase;
            self.tick(dt, 0.0, 0.0, false);
            self.since.stood_s += dt;
        }
        self.head = 0.0;
        if secs >= STAND_CORRECTS_S {
            let k = self.calib.stand_keep;
            self.err = (self.err.0 * k, self.err.1 * k, self.err.2 * k);
            self.publish_map();
        }
    }

    pub fn take_since(&mut self) -> Since {
        std::mem::take(&mut self.since)
    }
}

/// The [`Sim`] behind a lock, as a `Body`: the journey drives it, and the
/// trainer's brain, called from inside the journey, reads the truth.
#[derive(Clone)]
pub struct SimBody(pub Arc<Mutex<Sim>>);

impl SimBody {
    pub fn new(sim: Sim) -> Self {
        Self(Arc::new(Mutex::new(sim)))
    }
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Sim> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Body for SimBody {
    fn step(&mut self, args: &Value) -> Result<Value, String> {
        let mut s = self.lock();
        if s.fell {
            s.t += 1.0;
            return Err("the duck is seated or fallen: stand it up first (sit_toggle)".into());
        }
        let walk_s = args.get("walk_s").and_then(Value::as_f64).unwrap_or(0.0);
        let stop_s = args.get("stop_s").and_then(Value::as_f64).unwrap_or(0.0);
        if walk_s > 0.0 {
            let vx = args.get("vx").and_then(Value::as_f64).unwrap_or(0.3);
            let vyaw = args.get("vyaw").and_then(Value::as_f64).unwrap_or(0.0);
            s.walk(vx, vyaw, walk_s);
        }
        s.stand(stop_s);
        let f = s.frame_now();
        Ok(json!({"walked_s": walk_s, "stood_s": stop_s, "tracking": f.tracking, "pose": {"x": f.x, "y": f.y, "yaw": f.yaw}}))
    }

    fn blind_move(&mut self, args: &Value) -> Result<Value, String> {
        let vx = args.get("vx").and_then(Value::as_f64).unwrap_or(0.0);
        let vyaw = args.get("vyaw").and_then(Value::as_f64).unwrap_or(0.0);
        let secs = args.get("duration_s").and_then(Value::as_f64).unwrap_or(0.0);
        let mut s = self.lock();
        if s.fell {
            s.t += secs.max(0.1);
            return Err("fallen".into());
        }
        s.walk(vx, vyaw, secs);
        Ok(json!({"done": true, "walked_s": secs}))
    }

    fn frame(&self) -> Option<MapFrame> {
        Some(self.lock().frame_now())
    }

    fn pose_trusted(&self) -> bool {
        !self.lock().fell
    }

    fn frozen_map(&self) -> bool {
        true
    }

    fn cliff(&self) -> Option<CliffStatus> {
        let s = self.lock();
        let mut c = s.cliff.clone();
        c.odom_xy = Some((s.odom.0, s.odom.1));
        c.odom_yaw = Some(s.odom.2);
        Some(c)
    }

    fn now(&self) -> Instant {
        self.lock().instant()
    }

    fn sleep(&mut self, d: Duration) {
        self.lock().stand(d.as_secs_f64().max(DT));
    }
}

impl Sim {
    pub fn frame_now(&self) -> MapFrame {
        MapFrame {
            seq: self.map_seq,
            x: self.shown.0,
            y: self.shown.1,
            yaw: self.shown.2,
            tracking: !self.fell,
            x_min: self.grid_x0 as f32,
            y_min: self.grid_y0 as f32,
            cell_m: crate::world::CELL as f32,
            rows: self.rows,
            cols: self.cols,
            cells: self.cells_b64.clone(),
            n_submaps: 1,
            n_loops: 0,
            windows: 0,
            still: true,
            seated: self.fell,
            frozen: true,
            pose_sigma: None,
            resting: false,
            untrusted: false,
            rest_watch: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenarios::generate;

    #[test]
    fn a_step_walks_at_the_gait_speed() {
        let s = generate(3, 0, Some("clutter"));
        let mut c = Calib { speed_sd: 0.0, odom_xy_per_m: 0.0, odom_yaw_per_rad: 0.0, pose_jitter_m: 0.0, ..Calib::default() };
        c.straight_veer = 0.0;
        let mut b = Sim::new(&s, c, 1);
        b.world.rects.clear();
        b.world.posts.clear();
        b.world.movers.clear();
        b.world.holes.clear();
        let (x0, y0) = (b.x, b.y);
        b.walk(0.3, 0.0, 6.0);
        let d = dist((x0, y0), (b.x, b.y));
        assert!((d - 0.684).abs() < 0.02, "{d}");
        // The map's pose comes once a second: within a second of the truth.
        let f = b.frame_now();
        assert!(dist((f.x, f.y), (b.x + b.bias.0, b.y + b.bias.1)) < 0.12);
        // Frames at 15 Hz, three and a half seconds kept.
        assert!((50..=54).contains(&b.cliff.recent.len()), "{}", b.cliff.recent.len());
    }

    #[test]
    fn no_turn_in_place_below_the_dead_zone() {
        let s = generate(3, 0, Some("clutter"));
        let mut b = Sim::new(&s, Calib::default(), 1);
        let y0 = b.yaw;
        b.walk(0.0, 0.8, 1.0);
        assert!(crate::wrap(b.yaw - y0).abs() < 1e-9);
        b.walk(0.0, 1.5, 1.0);
        assert!(crate::wrap(b.yaw - y0) > 0.5);
    }
}
