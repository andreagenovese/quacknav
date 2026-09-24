//! Waking up in a house the duck has already mapped.
//!
//! The obvious design does not work. A robot switched on at home cannot
//! recognise the place from what it sees standing still: an 8×8 depth
//! sensor at two metres, in a flat of repeated rectangles, matches the
//! true place and its mirror image equally well, and the search rightly
//! refuses to choose (measured 2026-09-09, `docs/study/upstream-asks.md`).
//! Waiting longer does not help, and neither does walking a metre and
//! looking again.
//!
//! What does work is to stop asking at boot. The duck explores, which is
//! the thing it does well, and after a few minutes it no longer holds a
//! scan — it holds a *map*. Asking whether that map fits inside a saved
//! one compares thousands of cells instead of a couple of hundred beams,
//! and it finds the right place (5 cm on a map of 1313 cells, 0.83 m on
//! one of 659). So this module boots, gives the cheap answer one minute to
//! happen, and then explores, asking the map-to-map question as it goes.
//!
//! Believing the answer is the part no threshold can settle: the flat's
//! own mirror image scores 0.7–0.8 of the winner even when the winner is
//! right. What separates them is the map growing. The rule here is that
//! two successive asks, minutes apart and with the map bigger the second
//! time, must name the same map and put it in the same place. A wrong
//! candidate does not survive its own map growing into the rooms next
//! door; the right one gets better.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::config::HomecomingConfig;
use crate::tools::{self, Robot};
use crate::cliff::CliffStatus;

/// How close two asks must agree, in metres, to count as the same answer.
const AGREE_M: f64 = 0.30;

/// Start the homecoming in the background. Returns at once; everything it
/// does, it logs.
pub fn spawn(robot: Arc<Mutex<Robot>>, cfg: HomecomingConfig) {
    if !cfg.enabled {
        return;
    }
    std::thread::Builder::new()
        .name("homecoming".into())
        .spawn(move || run(&robot, &cfg))
        .map(|_| ())
        .unwrap_or_else(|e| tracing::warn!(error = %e, "homecoming: cannot start the thread"));
}

fn run(robot: &Arc<Mutex<Robot>>, cfg: &HomecomingConfig) {
    // The map lane needs a moment to receive its first frame, and the
    // robot has to be switched on and standing before any of this means
    // anything; nothing here can be asked before then.
    std::thread::sleep(Duration::from_secs_f64(cfg.start_delay_s.max(1.0)));

    let library = match call(robot, "robot.map_list", &json!({})) {
        Ok(answer) => answer
            .get("maps")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        Err(e) => {
            tracing::info!(reason = %e, "homecoming: no map library on this robot — nothing to come home to");
            return;
        }
    };
    if library.is_empty() {
        tracing::info!("homecoming: the library is empty; whatever is mapped now can be saved as the first map");
        return;
    }

    // The cheap answer first, because when it works it is instant and
    // exact: a duck switched on where it was switched off, or on its dock,
    // confirms its pose from one still window.
    let newest = library
        .iter()
        .max_by_key(|m| m.get("saved_at").and_then(Value::as_u64).unwrap_or(0))
        .and_then(|m| m.get("name").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();
    if !newest.is_empty() && cfg.boot_search_s > 0.0 {
        match call(robot, "robot.map_load", &json!({"name": newest})) {
            Ok(_) => {
                tracing::info!(map = newest, "homecoming: loaded the newest map; standing still to see if the duck knows where it is");
                if confirmed_within(robot, cfg.boot_search_s) {
                    tracing::info!(map = newest, "homecoming: home — the pose is confirmed on the saved map");
                    resume_exploring(robot, cfg, &newest);
                    return;
                }
                // A frozen map (maploc in `localize`) cannot be inked: a
                // fresh map to explore is meaningless there, and the
                // search is the only way home — keep at it, three more
                // budgets, then stand down (the user's ask, 2026-09-16).
                let frozen = {
                    let robot = robot.lock().expect("robot poisoned");
                    robot.places.map.as_ref().is_some_and(|m| {
                        matches!(&m.snapshot().support, crate::map::MapSupport::Supported { mode: Some(mode), .. } if mode == "localize")
                    })
                };
                // Exploring a map session after session, a lost boot must
                // not start a fresh map: saved at the end of the session
                // it would replace the map it could not find itself on.
                if frozen || cfg.resume_explore {
                    tracing::info!(map = newest, waited_s = cfg.boot_search_s, "homecoming: no confirmation yet; the map is frozen, so the search goes on");
                    if confirmed_within(robot, cfg.boot_search_s * 3.0) {
                        tracing::info!(map = newest, "homecoming: home — the pose is confirmed on the saved map");
                        resume_exploring(robot, cfg, &newest);
                    } else {
                        tracing::warn!(map = newest, "homecoming: no confirmation on the frozen map; standing down — the duck does not know where it is");
                    }
                    return;
                }
                tracing::info!(
                    map = newest,
                    waited_s = cfg.boot_search_s,
                    "homecoming: no confirmation; starting a fresh map and exploring instead"
                );
            }
            Err(e) => tracing::warn!(error = %e, "homecoming: cannot load the newest map"),
        }
        // The loaded map with an unconfirmed pose is the one thing worse
        // than no map: every wall it inks would land in the wrong room.
        if let Err(e) = wipe(robot) {
            tracing::warn!(error = %e, "homecoming: cannot start a fresh map; giving up");
            return;
        }
        // The map lane learns of the wipe from the next map frame, a
        // second later. Asking to explore before then is refused for a
        // pose that no longer exists — the frame in hand is the one from
        // the map just thrown away.
        if !confirmed_within(robot, 20.0) {
            tracing::warn!("homecoming: the fresh map has not settled; exploring anyway");
        }
    }

    // Explore, and ask the map-to-map question as the map grows.
    if let Err(e) = start_exploring(robot, cfg.explore_max_s) {
        tracing::warn!(error = %e, "homecoming: cannot start exploring");
        return;
    }
    // The run of agreeing asks so far: the answer, and how many in a row.
    let mut previous: Option<(String, f64, f64, u64)> = None;
    let mut agreed: u32 = 0;
    let mut asked_after_exploring = false;
    loop {
        std::thread::sleep(Duration::from_secs_f64(cfg.recognize_every_s));
        let exploring = robot.lock().expect("robot poisoned").places.explore.running();
        if !exploring && asked_after_exploring {
            tracing::info!("homecoming: the duck stopped exploring; not asking any more");
            return;
        }
        if !exploring {
            // The map is at its biggest the moment exploring stops, which
            // makes this the strongest ask of the run — worth one more.
            asked_after_exploring = true;
        }
        let answer = match call(robot, "robot.map_match", &json!({})) {
            Ok(answer) => answer,
            Err(e) => {
                tracing::warn!(error = %e, "homecoming: the recognition question failed");
                continue;
            }
        };
        let cells = answer.get("live_cells").and_then(Value::as_u64).unwrap_or(0);
        let Some(best) = answer
            .get("matches")
            .and_then(Value::as_array)
            .and_then(|m| m.first())
            .cloned()
        else {
            tracing::info!(cells, "homecoming: nothing fits yet");
            continue;
        };
        let (name, x, y) = (
            best.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
            best.get("x").and_then(Value::as_f64).unwrap_or(0.0),
            best.get("y").and_then(Value::as_f64).unwrap_or(0.0),
        );
        let score = best.get("score").and_then(Value::as_f64).unwrap_or(0.0);
        let margin = best.get("margin").and_then(Value::as_f64).unwrap_or(1.0);
        let overlap = best.get("overlap").and_then(Value::as_f64).unwrap_or(0.0);
        tracing::info!(
            map = name,
            cells,
            x = format!("{x:.2}"),
            y = format!("{y:.2}"),
            score = format!("{score:.3}"),
            margin = format!("{margin:.2}"),
            overlap = format!("{overlap:.2}"),
            "homecoming: the map so far fits here"
        );
        // The bar first, absolute: an answer that does not fit on its own
        // is not a candidate, however it ranks against the other maps —
        // the duck may be in none of them. A wrong map once got adopted
        // as "the best of the library" at 0.116–0.133 with the old
        // instrument; on the overlap instrument wrong answers score 0.187
        // and up, right ones 0.138 and down (2026-09-09).
        if score > cfg.adopt_max_score {
            tracing::info!(
                map = name,
                score = format!("{score:.3}"),
                bar = cfg.adopt_max_score,
                "homecoming: the best fit is above the bar — not this house, or not enough of it yet"
            );
            previous = None;
            agreed = 0;
            continue;
        }
        // A near tie is two places, not one: wait for the map to grow
        // past the ambiguity. (The office fitted the bedroom at 0.97.)
        // A refused ask seeds nothing: the next one has to be the first
        // of a new pair, or a refused answer confirms itself two minutes
        // later (the office did exactly that, 2026-09-15).
        if margin > cfg.adopt_max_margin {
            tracing::info!(
                map = name,
                margin = format!("{margin:.2}"),
                bar = cfg.adopt_max_margin,
                "homecoming: the runner-up is too close — two places fit, asking again later"
            );
            previous = None;
            agreed = 0;
            continue;
        }
        if overlap < cfg.adopt_min_overlap {
            tracing::info!(
                map = name,
                overlap = format!("{overlap:.2}"),
                floor = cfg.adopt_min_overlap,
                "homecoming: most of the live map lies where the saved one has never been — a room it does not hold; exploring on"
            );
            previous = None;
            agreed = 0;
            continue;
        }
        // Then the rule: the same map, in the same place, with enough more
        // of the house behind the answer than last time that it is a
        // second look and not the first one twice.
        let agrees = previous.as_ref().is_some_and(|(pn, px, py, pc)| {
            *pn == name
                && (px - x).hypot(py - y) <= AGREE_M
                && cells as f64 >= *pc as f64 * cfg.adopt_min_growth
        });
        agreed = if agrees { agreed + 1 } else { 0 };
        previous = Some((name.clone(), x, y, cells));
        if agreed + 1 < cfg.adopt_asks {
            tracing::info!(agreed = agreed + 1, needed = cfg.adopt_asks, "homecoming: agreeing so far");
            continue;
        }
        let yaw = best.get("yaw").and_then(Value::as_f64).unwrap_or(0.0);
        if cfg.dry_run {
            // Measuring, not deciding: say what would have happened and
            // carry on asking, so one run yields the whole series instead
            // of stopping at its first mistake.
            tracing::warn!(
                map = name,
                cells,
                score = format!("{score:.3}"),
                "homecoming: two asks agree — WOULD adopt (dry run)"
            );
            previous = Some((name, x, y, cells));
            continue;
        }
        tracing::info!(map = name, "homecoming: two asks agree — adopting the saved map");
        adopt(robot, &name, x, y, yaw, cfg.explore_max_s);
        return;
    }
}

/// Trade the fresh map for the saved one and pick exploring back up.
///
/// The job is stopped first and its ground forgotten: the drops on its
/// books and its walked trail are coordinates on the map that is being
/// traded away, and after the swap the duck stands somewhere else entirely
/// as far as those numbers are concerned.
fn adopt(robot: &Arc<Mutex<Robot>>, name: &str, x: f64, y: f64, yaw: f64, max_s: f64) {
    {
        let robot = robot.lock().expect("robot poisoned");
        robot.places.explore.request_stop();
    }
    // Long enough to outlast a panorama: the job checks for the stop
    // between legs and stands, and a full circle of stands is a minute.
    // Adopting while it still ran left `robot.map_explore` answering
    // "already running" to the restart, and nothing explored after
    // (run home2, 2026-09-09).
    for _ in 0..180 {
        if !robot.lock().expect("robot poisoned").places.explore.running() {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if robot.lock().expect("robot poisoned").places.explore.running() {
        tracing::warn!("homecoming: the exploring job will not stop; not adopting");
        return;
    }
    let params = json!({"name": name, "x": x, "y": y, "yaw": yaw});
    match call(robot, "robot.map_adopt", &params) {
        Ok(_) => {
            robot.lock().expect("robot poisoned").places.explore.map_named(name);
            tracing::info!(map = name, "homecoming: home — the saved map is the live one again");
        }
        Err(e) => {
            tracing::warn!(error = %e, "homecoming: the robot refused to adopt the map");
            return;
        }
    }
    // Adopting sets the place from a map-to-map comparison, which the
    // mapper treats as suspect until a still window agrees with it — and
    // the explorer will not start without a pose it trusts. So stand and
    // let it confirm, exactly as at boot.
    if !confirmed_within(robot, 60.0) {
        tracing::warn!("homecoming: the adopted place is still unconfirmed; exploring anyway");
    }
    if let Err(e) = start_exploring(robot, max_s) {
        tracing::warn!(error = %e, "homecoming: cannot pick exploring back up after adopting");
    }
}

/// Start the exploring job, giving the map lane a few seconds to catch up
/// if the first attempt is refused for want of a trustworthy pose.
/// Progressive exploration (`resume_explore`): home on a map still being
/// explored — the mapper mapping, not frozen — the next session starts
/// from here: the frontiers left are where the last one stopped. A map
/// its last session found finished is left alone.
fn resume_exploring(robot: &Arc<Mutex<Robot>>, cfg: &HomecomingConfig, name: &str) {
    if !cfg.resume_explore {
        return;
    }
    let (frozen, done) = {
        let robot = robot.lock().expect("robot poisoned");
        let frozen = robot.places.map.as_ref().is_some_and(|m| {
            matches!(&m.snapshot().support, crate::map::MapSupport::Supported { mode: Some(mode), .. } if mode == "localize")
        });
        let done = robot.places.explore.status().progress.as_ref().and_then(|p| p.get("done")).and_then(Value::as_bool).unwrap_or(false);
        (frozen, done)
    };
    if frozen {
        tracing::info!(map = name, "homecoming: the map is frozen; navigating on it");
        return;
    }
    if done {
        // The house is mapped: from here on the duck navigates — the map
        // frozen, journeys blind on the floor it knows and guarded where it
        // does not — and explores no more unless asked for a new map.
        let socket = robot.lock().expect("robot poisoned").places.map_socket.clone();
        match tools::map_library(&socket, crate::mapd::wire::METHOD_QUACK_MAP_FREEZE, Some(json!({"on": true}))) {
            Ok(_) => tracing::info!(map = name, "homecoming: the house is mapped; the map frozen, navigating on it"),
            Err(e) => tracing::warn!(map = name, error = %e, "homecoming: the house is mapped, but the map could not be frozen"),
        }
        return;
    }
    let mut last = String::new();
    for _ in 0..10 {
        match call(robot, "robot.map_explore", &json!({"max_s": cfg.explore_max_s, "save_as": name, "battery_min_pct": cfg.resume_battery_min_pct})) {
            Ok(answer) if answer.get("started").is_some() => {
                tracing::info!(map = name, "homecoming: exploring on from where the last session stopped");
                return;
            }
            Ok(answer) => last = format!("the duck did not start: {answer}"),
            Err(e) => last = e,
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    tracing::warn!(map = name, error = last, "homecoming: could not explore on");
}

fn start_exploring(robot: &Arc<Mutex<Robot>>, max_s: f64) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..10 {
        match call(robot, "robot.map_explore", &json!({"max_s": max_s})) {
            // `map_explore` answers `running: true` when a job is already
            // going, which is not the same as having started one.
            Ok(answer) if answer.get("started").is_some() => return Ok(()),
            Ok(answer) => {
                last = format!("the duck did not start: {answer}");
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(e) => {
                last = e;
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }
    Err(last)
}

/// Stand still and watch for the mapper to confirm the pose it resumed on.
///
/// The search walks: a candidate is believed only after a metre of chord
/// and confirmed after another half (maploc's gates, which are what keep
/// the mirror image out), and each step ends in a six-second stand, the
/// mapper's own floor for a still window. Eight steps is the floor —
/// 65 s. What made boots three times longer was every step that did not
/// make chord: a kick into the wall the duck woke facing, a leg refused
/// at the stairwell, a turn back over its own trail (2026-09-16: 8–23
/// refusals in the 170–240 s boots, 1–2 in the 65 s ones). So the search
/// looks before it walks — the user's three asks of 2026-09-16:
/// - the leg goes where the guard saw the most room, within the sweep;
/// - never a leg into less than [`LEG_MIN_FREE_M`]: boxed, it turns by
///   what the gait can do (a walking kick with room ahead, backing with
///   the yaw without) toward the freer side, stands, and looks again;
/// - a dead-reckoned trail of its stands, and a bearing that leads back
///   over it counts as that short.
fn confirmed_within(robot: &Arc<Mutex<Robot>>, seconds: f64) -> bool {
    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    // The frame in hand is the one from BEFORE the load: the map lane
    // learns of a load from the next frame, a second later, and that stale
    // frame still says `tracking` for the map just thrown away. Three
    // wake-ups came "home" in six seconds on it, at the pose the session
    // had ended at, in another room (2026-09-14). Only a frame newer than
    // the one we start with can vouch for anything.
    let frames0 = {
        let robot = robot.lock().expect("robot poisoned");
        robot.places.map.as_ref().map_or(0, |map| map.snapshot().frames)
    };
    let mut refusals = 0u32;
    let mut steps = 0u32;
    // Where the body has stood, by the odometry-carried pose (the map's
    // frame carries it while nothing is confirmed): the search does not
    // walk back over it.
    let mut trail: Vec<(f64, f64)> = Vec::new();
    // Which way a boxed-in turn goes: away from what the guard named.
    let mut turn = 0.7_f64;
    // Boxed-in looks in a row, and where the body stood at the last one.
    let mut boxed = 0u32;
    let mut boxed_at: Option<(f64, f64)> = None;
    // Turns on the spot in a row that moved nothing (the same look).
    let mut turns_stuck = 0u32;
    // The first step is a look: a stand alone, so the head sweeps and the
    // guard has frames along the whole cone before anything walks.
    let mut params = json!({"vx": 0.0, "vyaw": 0.0, "walk_s": 0.0, "stop_s": STAND_S});
    let mut what = "look";
    let mut scanned = false;
    while Instant::now() < deadline {
        steps += 1;
        let began = Instant::now();
        let step = call(robot, "robot.map_step", &params);
        tracing::info!(
            took_s = format!("{:.1}", began.elapsed().as_secs_f64()),
            what,
            reply = %match &step {
                Ok(v) => format!("{}", v.get("stop").cloned().unwrap_or(serde_json::Value::Null)),
                Err(e) => format!("refused: {e}"),
            },
            "homecoming: step-and-stand"
        );
        let pose = {
            let robot = robot.lock().expect("robot poisoned");
            robot.places.map.as_ref().and_then(|m| m.snapshot().latest.as_ref().map(|f| f.pose()))
        };
        if let Some((x, y, _)) = pose
            && trail.last().is_none_or(|l| (l.0 - x).hypot(l.1 - y) > 0.10)
        {
            trail.push((x, y));
        }
        BEHIND_IS_TRAIL.with(|b| b.set(behind_is_trail(pose, &trail)));
        if let Err(why) = &step {
            refusals += 1;
            // The guard says where the obstacle is ("… 12° right"): turn
            // the other way. A refusal that names no side keeps the turn.
            if why.contains("° right") {
                turn = 0.7;
            } else if why.contains("° left") {
                turn = -0.7;
            }
            let range = why
                .split("sees something ")
                .nth(1)
                .and_then(|r| r.split(" m").next())
                .and_then(|r| r.trim().parse::<f64>().ok());
            let back = turn_in_place(robot, turn, 0.8, range.unwrap_or(f64::INFINITY), range.is_some_and(|r| r < 0.15));
            tracing::info!(refusals, backed = back.is_ok(), "homecoming: refused; turning toward the free side");
            if back.is_err() {
                std::thread::sleep(Duration::from_secs(3));
            }
            params = json!({"vx": 0.0, "vyaw": 0.0, "walk_s": 0.0, "stop_s": STAND_S});
            what = "look";
            continue;
        }
        let confirmed = {
            let robot = robot.lock().expect("robot poisoned");
            robot.places.map.as_ref().is_some_and(|map| {
                let snap = map.snapshot();
                snap.frames > frames0 + 1 && snap.trusted_pose().is_some()
            })
        };
        if confirmed {
            tracing::info!(steps, refusals, "homecoming: confirmed");
            return true;
        }
        // Look: the freest bearing within the sweep, the trail counted as
        // an obstacle.
        let (cliff, now) = {
            let robot = robot.lock().expect("robot poisoned");
            (robot.places.cliff.as_ref().map(|c| c.snapshot()), Instant::now())
        };
        let mut seen = cliff.as_ref().map(|c| look_around(c, now, pose, &trail));
        // The first look, and no metre of floor ahead: a turn of the whole
        // horizon first (the user's ask, 2026-09-17) — four quarter
        // turns, a stand and a look at each — then the freest way of all
        // of them, not of one cone. Born facing the bed with its back to
        // the wall (spawn-bed) the one-cone look was "boxed in" and the
        // backing turn pinned it there for nine minutes.
        if !scanned
            && let Some(look) = seen.as_ref()
            && look.ahead < STRAIGHT_ENOUGH_M
        {
            scanned = true;
            seen = Some(scan_around(robot, look.clone(), &trail));
        }
        match seen {
            None => {
                // No guard: the old ladder — a kick of a turn, then legs.
                let leg = steps % 3 != 0;
                params = if leg {
                    json!({"vx": 0.3, "vyaw": 0.0, "walk_s": LEG_S, "stop_s": STAND_S})
                } else {
                    json!({"vx": 0.3, "vyaw": turn, "walk_s": 1.0, "stop_s": STAND_S})
                };
                what = if leg { "leg" } else { "turn" };
            }
            Some(Look { bearing, free, left, right, ahead, .. }) if free >= LEG_MIN_FREE_M => {
                boxed = 0;
                // A leg along the freest bearing, sized to the room there
                // (0.12 m/s, a quarter metre kept): straight, or a walking
                // turn first (25°/s at 0.7, advancing 0.11 m/s — measured
                // 2026-09-16) for the bearings off the nose; a bearing to
                // the right beyond the walking turn's reach, or without
                // room for its arc, is a turn on the spot first.
                let leg_s = ((free - 0.25) / 0.12).clamp(1.0, LEG_S);
                if bearing.abs() < 0.25 {
                    turns_stuck = 0;
                    params = json!({"vx": 0.3, "vyaw": 0.0, "walk_s": leg_s, "stop_s": STAND_S});
                    what = "leg";
                } else if ahead >= 0.45 {
                    turns_stuck = 0;
                    let secs = (bearing.abs() / WALK_TURN_RAD_S).clamp(0.5, 2.5);
                    params = json!({"vx": 0.3, "vyaw": 0.7 * bearing.signum(), "walk_s": secs, "stop_s": STAND_S});
                    what = "leg toward";
                } else {
                    // No room for the arc of a walking turn (look7: a
                    // walking turn toward −52° with 0.20 m ahead, refused
                    // ten times over): turn on the spot, then look again.
                    // With a drop AHEAD of the beak the kick of that turn
                    // is refused, silently, look after look (dense1,
                    // 2026-09-21: nine minutes 45 cm north of the
                    // stairwell, "−63°, 0.34 m ahead" every six seconds).
                    // Three such looks running: a step back, blind — away
                    // from a drop that is ahead is away from it (the rule
                    // against blind backing was for a drop BESIDE, look9).
                    // Any edge ahead, however near: backing from a drop
                    // under the beak is away from it (north2, 2026-09-22:
                    // thirteen minutes with the rim 0.10 m ahead, the step
                    // back withheld for an edge nearer than 0.25).
                    let drop_ahead = cliff.as_ref().and_then(|c| c.nearest(now)).is_some_and(|d| d.bearing.abs() <= 1.2);
                    // Measured, not trusted: on the pulse path `turn_to`
                    // returns the bearing asked for whatever the pulses did.
                    let yaw_before = odom_yaw(robot);
                    let _ = turn_to(robot, bearing, ahead);
                    let turned = match (yaw_before, odom_yaw(robot)) {
                        (Some(a), Some(b)) => ((b - a).sin().atan2((b - a).cos())).abs(),
                        _ => 1.0,
                    };
                    turns_stuck = if turned < 0.1 { turns_stuck + 1 } else { 0 };
                    if turns_stuck >= 3 && drop_ahead {
                        tracing::info!(turns_stuck, "homecoming: the turn beside a drop ahead moved nothing three times; a step back from it, blind");
                        let _ = call(robot, "robot.move", &json!({"vx": -0.3, "vyaw": 0.5, "duration_s": 1.5}));
                        turns_stuck = 0;
                    }
                    params = json!({"vx": 0.0, "vyaw": 0.0, "walk_s": 0.0, "stop_s": STAND_S});
                    what = "look";
                }
                tracing::info!(
                    bearing_deg = format!("{:.0}", bearing.to_degrees()),
                    free_m = format!("{free:.2}"),
                    left_m = format!("{left:.2}"),
                    right_m = format!("{right:.2}"),
                    ahead_m = format!("{ahead:.2}"),
                    "homecoming: the freest way"
                );
            }
            Some(Look { looked: false, left, right, ahead, .. }) => {
                // A sector not looked at yet is unknown, not closed: stand
                // and let the head sweep before anything is decided.
                tracing::info!(left_m = format!("{left:.2}"), right_m = format!("{right:.2}"), ahead_m = format!("{ahead:.2}"), "homecoming: not every way looked at yet; standing to look");
                params = json!({"vx": 0.0, "vyaw": 0.0, "walk_s": 0.0, "stop_s": STAND_S});
                what = "look";
            }
            Some(Look { left, right, ahead, .. }) => {
                // Boxed in: a turn on the spot toward the freer side —
                // about a right angle, so the next look sees new floor —
                // then stand and look again. Boxed in twice running,
                // the turn did not free it (wedged on a door post,
                // look7): a step back, blind, first — onto the trail only.
                boxed += 1;
                turn = if left > right { 0.7 } else { -0.7 };
                tracing::info!(
                    boxed,
                    left_m = format!("{left:.2}"),
                    right_m = format!("{right:.2}"),
                    ahead_m = format!("{ahead:.2}"),
                    to = if turn > 0.0 { "left" } else { "right" },
                    "homecoming: boxed in; turning on the spot"
                );
                let drop_in_view = cliff.as_ref().is_some_and(|c| c.nearest(now).is_some());
                // Wedged: boxed in three times running without the body
                // moving between them — the step back and the turn moved
                // nothing (spawn-off, 2026-09-20: eight minutes on the
                // office's door post, 0.10 m all round, the pose still to
                // the centimetre). Backing again is the same nothing; a
                // walking kick forward with the yaw, blind — nothing that
                // could be fallen into is in view — is what is left.
                // The blind kick goes only where the head has looked and
                // seen floor: the side with the room, and at least
                // WEDGE_KICK_FREE_M of it — never toward a side unseen.
                let side_free = if turn > 0.0 { left } else { right };
                let drop_seen_now = cliff.as_ref().and_then(|c| c.nearest(now)).is_some();
                let wedged = boxed >= 3
                    && !drop_in_view
                    && !drop_seen_now
                    && side_free >= WEDGE_KICK_FREE_M
                    && matches!((boxed_at, pose), (Some(a), Some((x, y, _))) if (a.0 - x).hypot(a.1 - y) < 0.03);
                boxed_at = pose.map(|(x, y, _)| (x, y));
                let behind_is_trail = BEHIND_IS_TRAIL.with(|b| b.get()) && !drop_seen_now;
                if wedged {
                    tracing::info!(boxed, side_free = format!("{side_free:.2}"), "homecoming: wedged — the backing moved nothing; a walking kick with the yaw toward the looked, free side");
                    let _ = call(robot, "robot.move", &json!({"vx": 0.3, "vyaw": turn, "duration_s": 1.0}));
                } else if boxed >= 2 && !drop_in_view && behind_is_trail {
                    let _ = call(robot, "robot.move", &json!({"vx": -0.3, "vyaw": 0.6, "duration_s": 2.5}));
                }
                let _ = turn_to(robot, turn * 2.0, ahead);
                params = json!({"vx": 0.0, "vyaw": 0.0, "walk_s": 0.0, "stop_s": STAND_S});
                what = "look";
            }
        }
    }
    false
}

/// The stand that closes a still window: the mapper's own floor (four
/// seconds was not enough — the next step began just before stillness,
/// and in 240 s not one window closed, 2026-09-14).
const STAND_S: f64 = 6.0;
/// A leg: three seconds straight, about 0.35 m of chord.
const LEG_S: f64 = 3.0;
/// A leg goes only where the guard saw at least this much floor.
const LEG_MIN_FREE_M: f64 = 0.45;
/// The room a looked-at side must show before the wedged escape kicks
/// toward it, blind.
const WEDGE_KICK_FREE_M: f64 = 0.35;
/// As far as a look counts: the guard sees little past two metres.
const LOOK_FAR_M: f64 = 2.0;
/// The lane a look judges: the body's half-width and a little.
const LOOK_LANE_M: f64 = 0.18;
/// The walking turn's rate at vyaw 0.7 (measured 2026-09-16).
const WALK_TURN_RAD_S: f64 = 0.44;
/// The cone the look sweeps, either side of the nose.
const LOOK_CONE_RAD: f64 = 1.4;
/// A trail point this close ahead, in the lane, ends the free range there.
const TRAIL_AHEAD_M: f64 = 1.2;
/// Room ahead beyond which a look does not turn the duck at all.
const STRAIGHT_ENOUGH_M: f64 = 1.0;
/// A way this clear ends the scan of the horizon early.
const SCAN_ENOUGH_M: f64 = 1.5;

#[derive(Clone)]
struct Look {
    bearing: f64,
    free: f64,
    left: f64,
    right: f64,
    ahead: f64,
    /// Whether every sector (left, ahead, right) had a frame looking
    /// along it: a sector the head has not swept yet reads 0.00, which is
    /// not "the floor ends here" — the boot from north of the stairwell
    /// read "left 0.00, ahead 0.00" facing east, called itself boxed in,
    /// and its escape kicked blind to the right into the hole 8 cm away
    /// that the head had never looked at (spawn-north, 2026-09-22).
    looked: bool,
}

/// The free range along `bearing` (body frame) as the guard saw it from
/// this stand: the nearest obstacle or drop edge in a body-wide lane, or
/// [`LOOK_FAR_M`]; `None` when no recent frame looked that way.
fn free_along(cliff: &CliffStatus, now: Instant, bearing: f64) -> Option<f64> {
    if !cliff.looked_at(now, bearing) {
        return None;
    }
    let mut free = LOOK_FAR_M;
    if let Some(o) = cliff.obstacle_in_lane(now, bearing, LOOK_LANE_M) {
        free = free.min(o.range_m);
    }
    if let Some(d) = cliff.drop_in_lane(now, bearing, LOOK_LANE_M) {
        free = free.min(d.edge_min_m.max(0.10));
    }
    Some(free)
}

/// Every bearing in the cone the guard looked along, scored by its free
/// range with the trail counted as an obstacle; the best, and the room to
/// the left, the right and straight ahead.
fn look_around(cliff: &CliffStatus, now: Instant, pose: Option<(f64, f64, f64)>, trail: &[(f64, f64)]) -> Look {
    let mut best: Option<(f64, f64, f64)> = None;
    let (mut left, mut right, mut ahead) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (mut saw_left, mut saw_right, mut saw_ahead) = (false, false, false);
    let mut b = -LOOK_CONE_RAD;
    while b <= LOOK_CONE_RAD + 1e-9 {
        if let Some(mut free) = free_along(cliff, now, b) {
            if b > 0.3 {
                saw_left = true;
            } else if b < -0.3 {
                saw_right = true;
            } else {
                saw_ahead = true;
            }
            // The trail: a stand of this search within the lane ahead ends
            // the free range there (all but the one just made).
            if let Some((x, y, yaw)) = pose {
                let dir = yaw + b;
                for (tx, ty) in trail.iter().take(trail.len().saturating_sub(1)) {
                    let (dx, dy) = (tx - x, ty - y);
                    let along = dx * dir.cos() + dy * dir.sin();
                    let across = (dx * dir.sin() - dy * dir.cos()).abs();
                    if along > 0.15 && along < TRAIL_AHEAD_M && across < LOOK_LANE_M + 0.10 {
                        free = free.min(along);
                    }
                }
            }
            if b > 0.3 {
                left = left.max(free);
            } else if b < -0.3 {
                right = right.max(free);
            } else {
                ahead = ahead.max(free);
            }
            // Straight while there is a metre ahead — every turn is chord
            // not made (look6: ten legs weaving ±40° in a room where
            // everything was "2 m free"); the farthest free point only
            // when the way ahead is shorter than that.
            let score = free.min(STRAIGHT_ENOUGH_M) - 0.2 * b.abs();
            if best.is_none_or(|(_, s, _)| score > s) {
                best = Some((b, score, free));
            }
        }
        b += 0.1;
    }
    let (bearing, _, free) = best.unwrap_or((0.0, 0.0, 0.0));
    Look { bearing, free, left, right, ahead, looked: saw_left && saw_right && saw_ahead }
}

/// The odometry heading, the fastest there is.
fn odom_yaw(robot: &Arc<Mutex<Robot>>) -> Option<f64> {
    let robot = robot.lock().expect("robot poisoned");
    robot.places.cliff.as_ref().and_then(|c| c.snapshot().odom_yaw)
}

/// Turn on the spot to `bearing` (body frame, positive left) the way the
/// explorer's panorama does: a one-second walking kick through the guard
/// (the gait does not turn from a standstill; once stepping, yaw alone
/// spins about 30°/s in place), then yaw only in short chunks closed on
/// the odometry heading. On the twin the spin goes left whatever the
/// sign, so a bearing to the right is the long way round — unless it is
/// small enough for the backing pulses and no drop is in view. Then a
/// stand. Returns what was turned, radians, signed.
fn turn_to(robot: &Arc<Mutex<Robot>>, bearing: f64, ahead: f64) -> f64 {
    // A pure turn in place the short way first: past the gait's dead zone
    // it turns 30–60°/s with the body within a few centimetres, so nothing
    // at the beak refuses it and nothing blind is walked. What follows —
    // pulses, the kick — is for a gait that does not turn so.
    if let Some(turned) = pure_turn(robot, bearing.signum(), bearing.abs())
        && turned >= 0.7 * bearing.abs()
    {
        let _ = call(robot, "robot.map_step", &json!({"vx": 0.0, "vyaw": 0.0, "walk_s": 0.0, "stop_s": 3.0}));
        tracing::info!(want_deg = format!("{:.0}", bearing.to_degrees()), turned_deg = format!("{:.0}", (turned * bearing.signum()).to_degrees()), "homecoming: turned in place");
        return turned * bearing.signum();
    }
    let drop_seen = {
        let robot = robot.lock().expect("robot poisoned");
        robot.places.cliff.as_ref().is_some_and(|c| c.snapshot().nearest(Instant::now()).is_some())
    };
    // With a drop in view nothing blind — unless the floor behind is the
    // body's own trail (it walked in from there): a step back onto it is
    // the user's trusted floor, and the one way out of a stand at a rim
    // where the kick is refused (rim16, 2026-09-18: seven minutes of the
    // same look 34 cm from the stairwell's north rim).
    let drop_in_view = drop_seen && !BEHIND_IS_TRAIL.with(|b| b.get());
    if bearing < -0.2 && bearing > -1.2 && !drop_in_view && ahead < 0.45 {
        let before = odom_yaw(robot);
        let _ = turn_in_place(robot, -1.0, bearing.abs(), ahead, ahead < 0.15);
        // The pulses turn 35–55° once the gait is stepping, 5° from a
        // standstill (spawn-kit, 2026-09-20: four rounds of eight pulses,
        // 5° each, 100 s for a 34° want). Little turned: the long way
        // round by the kick and the yaw, not the pulses again.
        let turned = match (before, odom_yaw(robot)) {
            (Some(a), Some(b)) => ((b - a).sin().atan2((b - a).cos())).abs(),
            _ => bearing.abs(),
        };
        if turned >= bearing.abs() * 0.4 {
            return bearing;
        }
        tracing::info!(turned_deg = format!("{:.0}", turned.to_degrees()), want_deg = format!("{:.0}", bearing.to_degrees()), "homecoming: the pulses turned little; the long way round");
    }
    let want = if bearing >= 0.0 { bearing } else { std::f64::consts::TAU + bearing };
    let Some(yaw0) = odom_yaw(robot) else {
        let _ = turn_in_place(robot, bearing.signum(), bearing.abs(), ahead, ahead < 0.15);
        return bearing;
    };
    // The kick: guarded, so nothing at the beak is walked into; refused,
    // the spin is tried from standstill anyway (it turns little).
    let mut kicked = call(robot, "robot.map_step", &json!({"vx": 0.3, "vyaw": 0.7, "walk_s": BOOT_KICK_S, "stop_s": 0.0})).is_ok();
    // A drop the sensor sees at all — however near, whatever the trail
    // says — forbids the blind step back: with the rim 0.10 m ahead and
    // the trail behind, the exemption let a 1.5 s backing pulse carry
    // the body into the stairwell (north6, 2026-09-22; the trail's own
    // point was where the body had stood ON the rim's edge).
    if !kicked && !drop_seen {
        // No room for the kick (bed2: 0.22 m ahead, refused, and yaw
        // alone from standstill turned 0°): a step back, blind — nothing
        // is in view that could be fallen into — then the kick again.
        let _ = call(robot, "robot.move", &json!({"vx": -0.3, "vyaw": 0.5, "duration_s": 1.5}));
        std::thread::sleep(Duration::from_millis(800));
        kicked = call(robot, "robot.map_step", &json!({"vx": 0.3, "vyaw": 0.7, "walk_s": BOOT_KICK_S, "stop_s": 0.0})).is_ok();
    }
    if !kicked {
        // Still no room: the backing pulses, what there is — and with a
        // drop in view `turn_in_place` keeps to guarded kicks.
        let _ = turn_in_place(robot, bearing.signum(), bearing.abs(), ahead, ahead < 0.15 && !drop_seen);
        return odom_yaw(robot).map_or(0.0, |y| (y - yaw0).sin().atan2((y - yaw0).cos()));
    }
    let started = Instant::now();
    let budget = Duration::from_secs_f64((want / 0.4).clamp(2.0, 20.0));
    let mut turned = 0.0_f64;
    while started.elapsed() < budget {
        let _ = call(robot, "robot.move", &json!({"vx": 0.0, "vyaw": 0.7, "duration_s": 0.25}));
        if let Some(y) = odom_yaw(robot) {
            // Unwrapped: a turn past π keeps counting.
            let d = (y - yaw0).sin().atan2((y - yaw0).cos());
            turned = if d < turned - std::f64::consts::PI { d + std::f64::consts::TAU } else { d };
            if turned >= want - 0.35 {
                break;
            }
        }
    }
    let _ = call(robot, "robot.map_step", &json!({"vx": 0.0, "vyaw": 0.0, "walk_s": 0.0, "stop_s": 3.0}));
    let after = odom_yaw(robot).map(|y| (y - yaw0).sin().atan2((y - yaw0).cos()));
    tracing::info!(kicked, want_deg = format!("{:.0}", want.to_degrees()), turned_deg = format!("{:.0}", after.unwrap_or(turned).to_degrees()), "homecoming: turned on the spot (kick, then yaw)");
    after.unwrap_or(turned)
}

/// The whole horizon: from the look in hand, three more quarter turns on
/// the spot, a stand and a look at each; then a turn to the freest way
/// of the four, and that look — re-taken there — is the answer.
fn scan_around(robot: &Arc<Mutex<Robot>>, first: Look, trail: &[(f64, f64)]) -> Look {
    let mut looks: Vec<(f64, Look)> = vec![(0.0, first)];
    let mut heading = 0.0_f64;
    for _ in 0..3 {
        let turned = turn_to(robot, std::f64::consts::FRAC_PI_2, looks.last().map_or(0.0, |l| l.1.ahead));
        heading += turned;
        if turned.abs() < 0.3 {
            tracing::info!("homecoming: scan cut short; the body does not turn here");
            break;
        }
        let _ = call(robot, "robot.map_step", &json!({"vx": 0.0, "vyaw": 0.0, "walk_s": 0.0, "stop_s": STAND_S}));
        let (cliff, pose) = {
            let robot = robot.lock().expect("robot poisoned");
            (
                robot.places.cliff.as_ref().map(|c| c.snapshot()),
                robot.places.map.as_ref().and_then(|m| m.snapshot().latest.as_ref().map(|f| f.pose())),
            )
        };
        if let Some(c) = cliff {
            let look = look_around(&c, Instant::now(), pose, trail);
            tracing::info!(
                heading_deg = format!("{:.0}", heading.to_degrees()),
                bearing_deg = format!("{:.0}", look.bearing.to_degrees()),
                free_m = format!("{:.2}", look.free),
                ahead_m = format!("{:.2}", look.ahead),
                "homecoming: scan"
            );
            let enough = look.free >= SCAN_ENOUGH_M;
            looks.push((heading, look));
            // A way this clear is worth more than the rest of the horizon
            // (the corridor: the first quarter turn shows it, the other
            // two and the turn back cost forty seconds — scan13).
            if enough {
                break;
            }
        }
    }
    let (best_heading, best) = looks
        .iter()
        .max_by(|a, b| a.1.free.total_cmp(&b.1.free))
        .cloned()
        .unwrap_or((heading, looks[0].1.clone()));
    // Where the freest way lies from the heading of now, then a look there.
    let to = (best_heading + best.bearing - heading).sin().atan2((best_heading + best.bearing - heading).cos());
    tracing::info!(to_deg = format!("{:.0}", to.to_degrees()), free_m = format!("{:.2}", best.free), "homecoming: scan done; the freest way of the horizon");
    if to.abs() > 0.25 {
        let _ = turn_to(robot, to, best.ahead.min(looks.last().map_or(0.0, |l| l.1.ahead)));
    }
    let _ = call(robot, "robot.map_step", &json!({"vx": 0.0, "vyaw": 0.0, "walk_s": 0.0, "stop_s": STAND_S}));
    let (cliff, pose) = {
        let robot = robot.lock().expect("robot poisoned");
        (
            robot.places.cliff.as_ref().map(|c| c.snapshot()),
            robot.places.map.as_ref().and_then(|m| m.snapshot().latest.as_ref().map(|f| f.pose())),
        )
    };
    let fresh = cliff.map(|c| look_around(&c, Instant::now(), pose, trail)).unwrap_or(best.clone());
    // The scan's verdict stands: a fresh look from the new heading reads
    // the stools beside the body as "boxed in" (the kitchen boot, five
    // boxed-in stands and 375 s, spawn-kit 2026-09-17) where the scan
    // saw 0.8–1.0 m along this very bearing. The leg goes where the scan
    // said, straight, sized to what it saw.
    if fresh.free < LEG_MIN_FREE_M && best.free >= LEG_MIN_FREE_M {
        tracing::info!(fresh_m = format!("{:.2}", fresh.free), scan_m = format!("{:.2}", best.free), "homecoming: the fresh look is boxed in where the scan saw room; the scan's leg it is");
        return Look { bearing: 0.0, free: best.free, left: fresh.left, right: fresh.right, ahead: best.free, looked: fresh.looked };
    }
    fresh
}

/// Turn in place toward `sign` by about `want` radians, by what the gait
/// can do: with `ahead` room a walking kick (25°/s, advancing 0.11 m/s);
/// without, backing with the yaw — and that turn is a transient of the
/// first half-second (−11.7°/s over 0.8 s, −7.5°/s over 1.5 s, ~1°/s
/// over 4 s: look7, 2026-09-16, six 4 s commands for 7°), so it is
/// PULSES of [`PULSE_S`] with a pause between, each measured on the
/// odometry heading, up to [`PULSES_MAX`]. A step back first with
/// something at the beak. Then a stand.
/// Yaw alone past the dead zone, closed on the odometry in short chunks,
/// stopped by an edge the sensor sees near the beak. How far it turned
/// toward `sign`; `None` when switched off (`QK_TURN_IN_PLACE=0`) or with
/// no odometry to close on. See `explore::Job::turn_in_place`.
fn pure_turn(robot: &Arc<Mutex<Robot>>, sign: f64, want: f64) -> Option<f64> {
    if !crate::explore::turn_in_place_on() {
        return None;
    }
    let yaw0 = odom_yaw(robot)?;
    // Nowhere near a drop, on any side: the legs swing as the body turns
    // (see `explore::Job::turn_in_place`).
    let edge_near = || {
        let robot = robot.lock().expect("robot poisoned");
        robot.places.cliff.as_ref().is_some_and(|c| c.snapshot().nearest_hole_m(Instant::now()).is_some_and(|m| m < crate::passage::BODY_HALF_M + crate::passage::LEG_DRIFT_M))
    };
    if edge_near() {
        tracing::info!("homecoming: a drop this near; no turn in place here");
        return Some(0.0);
    }
    let goal = (want - 0.10).max(0.05);
    let started = Instant::now();
    let budget = Duration::from_secs_f64(2.0 * want / 0.5 + 1.0);
    let mut turned = 0.0_f64;
    while started.elapsed() < budget {
        let _ = call(robot, "robot.move", &json!({"vx": 0.0, "vyaw": quack_duck::body::TURN_IN_PLACE_RAD_S * sign.signum(), "duration_s": 0.15}));
        if let Some(y) = odom_yaw(robot) {
            turned = (y - yaw0).sin().atan2((y - yaw0).cos()) * sign.signum();
            if turned >= goal {
                break;
            }
        }
        if edge_near() {
            tracing::info!("homecoming: an edge came near while turning in place; stopping the turn");
            break;
        }
    }
    Some(turned)
}

fn turn_in_place(robot: &Arc<Mutex<Robot>>, sign: f64, want: f64, ahead: f64, at_beak: bool) -> Result<Value, String> {
    // A drop in view: nothing blind, nothing backwards — the guard does
    // not look behind, and a backing pulse beside the stairwell put
    // look9 in it (2026-09-16). Guarded kicks only, short.
    let drop_in_view = {
        let robot = robot.lock().expect("robot poisoned");
        robot.places.cliff.as_ref().is_some_and(|c| c.snapshot().nearest(Instant::now()).is_some())
    };
    if drop_in_view {
        return call(robot, "robot.map_step", &json!({"vx": 0.3, "vyaw": 0.7 * sign.signum(), "walk_s": 0.6, "stop_s": 3.0}));
    }
    if at_beak {
        // Blind: the guard judges nothing walked backwards (look7: a
        // map_step with vx −0.3 did nothing, wedged on a door post for
        // three minutes). The gait backs only with +yaw, 6–10 cm in 1.5 s.
        return call(robot, "robot.move", &json!({"vx": -0.3, "vyaw": 0.5, "duration_s": 1.5}))
            .and_then(|_| call(robot, "robot.map_step", &json!({"vx": 0.0, "vyaw": 0.0, "walk_s": 0.0, "stop_s": 3.0})));
    }
    if ahead >= 0.45 {
        let secs = (want / WALK_TURN_RAD_S).clamp(0.8, 2.0);
        return call(robot, "robot.map_step", &json!({"vx": 0.3, "vyaw": 0.7 * sign.signum(), "walk_s": secs, "stop_s": 3.0}));
    }
    let yaw_now = || {
        let robot = robot.lock().expect("robot poisoned");
        robot.places.cliff.as_ref().and_then(|c| c.snapshot().odom_yaw)
    };
    let start = yaw_now();
    let mut turned = 0.0_f64;
    let mut pulses = 0u32;
    // Once the gait is stepping a pulse turns 35–55°, not the 9° of one
    // from standstill (look9): so one at a time, measured after the
    // pause, and the aim a little short — the look that follows settles
    // the rest.
    while pulses < PULSES_MAX && turned < want - 0.25 {
        pulses += 1;
        call(robot, "robot.move", &json!({"vx": -0.3, "vyaw": 0.7 * sign.signum(), "duration_s": PULSE_S}))?;
        std::thread::sleep(Duration::from_secs_f64(PULSE_PAUSE_S));
        if let (Some(a), Some(b)) = (start, yaw_now()) {
            turned = ((b - a).sin().atan2((b - a).cos()) * sign.signum()).max(0.0);
        } else {
            // No odometry heading to measure by: pulses by the book.
            turned += PULSE_RAD;
        }
    }
    tracing::info!(pulses, turned_deg = format!("{:.0}", turned.to_degrees() * sign.signum()), want_deg = format!("{:.0}", want.to_degrees() * sign.signum()), "homecoming: turned on the spot by backing pulses");
    call(robot, "robot.map_step", &json!({"vx": 0.0, "vyaw": 0.0, "walk_s": 0.0, "stop_s": 3.0}))
}
/// The walking kick that gets the gait stepping before a spin: the
/// explorer's is a second (12 cm); the user asked for a shorter one
/// (2026-09-17) — measured below.
const BOOT_KICK_S: f64 = 0.5;
thread_local! {
    /// Set by the search before a turn: the last stand lies behind the
    /// body, within [`BEHIND_TRAIL_M`], so a short step back is onto
    /// floor it has walked.
    static BEHIND_IS_TRAIL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
const BEHIND_TRAIL_M: f64 = 0.45;
/// Whether the stand before this one lies behind the body, close.
fn behind_is_trail(pose: Option<(f64, f64, f64)>, trail: &[(f64, f64)]) -> bool {
    let (Some((x, y, yaw)), Some(prev)) = (pose, trail.iter().rev().nth(1)) else { return false };
    let (dx, dy) = (prev.0 - x, prev.1 - y);
    let d = dx.hypot(dy);
    d > 0.05 && d <= BEHIND_TRAIL_M && (dx * yaw.cos() + dy * yaw.sin()) / d < -0.6
}
/// One backing pulse, and the pause that lets the gait settle before the
/// next: the yaw is in the start of the step.
const PULSE_S: f64 = 0.8;
const PULSE_PAUSE_S: f64 = 1.5;
/// What a pulse turns, by the book (≈ 9°), when nothing measures it.
const PULSE_RAD: f64 = 0.16;
const PULSES_MAX: u32 = 8;

fn wipe(robot: &Arc<Mutex<Robot>>) -> Result<(), String> {
    let mut robot = robot.lock().expect("robot poisoned");
    let control = robot
        .control
        .as_mut()
        .ok_or_else(|| "robot unreachable".to_string())?;
    // By method name like the rest of the map library: `robot.map_wipe`
    // is upstream's, but the released proto crate this satellite builds
    // against predates it.
    let response = control
        .request_method("robot.map_wipe", None)
        .map_err(|e| format!("robotd: {e}"))?;
    if let Some(error) = &response.error {
        return Err(format!("robotd refused robot.map_wipe: {error}"));
    }
    let result = response.result.unwrap_or(Value::Null);
    match result.get("accepted").and_then(Value::as_bool) {
        Some(true) => Ok(()),
        _ => Err(result
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("the robot refused")
            .to_string()),
    }
}

/// One tool call on the shared robot. The lock is held for the call and
/// nothing more: whoever calls in over the daemon's socket wants it too.
fn call(robot: &Arc<Mutex<Robot>>, name: &str, args: &Value) -> Result<Value, String> {
    let mut robot = robot.lock().expect("robot poisoned");
    tools::execute(name, args, &mut robot)
}
