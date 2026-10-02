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
//! three consecutive asks, a minute apart and with the map bigger each
//! time, must name the same map and put it in the same place, each at an
//! overlap of at least 0.5 and a margin of at most 0.5
//! (`HomecomingConfig::adopt_asks`, `recognize_every_s`,
//! `adopt_min_overlap`, `adopt_max_margin`; 27 replayed wakes, 2026-09-29:
//! 626 of 655 right answers pass, none of the other house). A wrong
//! candidate does not survive its own map growing into the rooms next
//! door; the right one gets better.
//!
//! ## Where it ends and the exploration begins
//!
//! Four steps, and only one of them is the explorer's (the user's rule,
//! 2026-09-28: a fix to one must not break another):
//!
//! 1. the boot search on the newest saved map (`search.rs`): its own
//!    stands, looks and steps — no explorer code;
//! 2. on a frozen map, or one explored session after session
//!    (`resume_explore`), nothing more: the search goes on a while, and
//!    found or not the homecoming ends there — on a frozen map the duck
//!    then navigates (journeys, `explore/navigate.rs`);
//! 3. otherwise, not found: a fresh map and the explorer as the search
//!    (`robot.map_explore`, the live map marked as searching so nothing
//!    of it is saved), asked map-to-map as it grows, and the saved map
//!    adopted — here, and only here, a change to the exploration changes
//!    the homecoming;
//! 4. home on a map still being mapped (`resume_explore`): the next
//!    exploring session is started, and the homecoming is done.
//!
//! What they share otherwise: maploc (the pose, the matches, the
//! sessions), the robot's tools, and the turn-in-place switch.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::config::HomecomingConfig;
use crate::tools::{self, Robot};

mod search;

/// The boot's walk-and-look search ([`search`]), for a pose the mapper no
/// longer trusts at all (see `crate::relocate`): stand, look, step where
/// the sensor sees room, until maploc confirms a pose, `seconds` pass or
/// `stop` says so. True when confirmed.
pub fn find_pose(robot: &Arc<Mutex<Robot>>, seconds: f64, stop: &dyn Fn() -> bool) -> bool {
    search::confirmed_within_or(robot, seconds, stop)
}

/// Why the duck moves on its own, as `explore.reason` says it while it does.
pub const WHY_BOOT: &str = "the duck woke up on a saved map and does not know where it is yet: walking and looking to find itself";
pub const WHY_FRESH: &str = "the duck did not recognise the house: exploring to find itself";
pub const WHY_ADOPTED: &str = "the duck found itself on the saved map: exploring on from there";
pub const WHY_RESUME: &str = "exploring on from where the last session stopped";

/// The walk-and-look search as the duck's own motion: `explore.state`
/// "searching" while it runs, stopped by the user's stop (and then nothing
/// more: see [`held`]).
fn search(robot: &Arc<Mutex<Robot>>, seconds: f64, why: &str) -> bool {
    let explore = robot.lock().expect("robot poisoned").places.explore.clone();
    explore.search_began(why);
    let found = search::confirmed_within_or(robot, seconds, &|| explore.held());
    explore.search_ended(found);
    found
}

/// The user stopped the duck: the homecoming moves it no more in this
/// power-on (until a job is asked for, which is the user's own).
fn held(robot: &Arc<Mutex<Robot>>) -> bool {
    let held = robot.lock().expect("robot poisoned").places.explore.held();
    if held {
        tracing::warn!("homecoming: {}", crate::explore::STOPPED_BY_USER);
    }
    held
}

/// How close two asks must agree, in metres, to count as the same answer.
const AGREE_M: f64 = 0.30;

/// Start the homecoming in the background. Returns at once; everything it
/// does, it logs.
pub fn spawn(robot: Arc<Mutex<Robot>>, cfg: HomecomingConfig) {
    if !cfg.enabled {
        return;
    }
    // Until it has settled, which map is live is the homecoming's call:
    // the places wait instead of being judged on the boot's fresh map.
    robot.lock().expect("robot poisoned").places.registry.await_homecoming();
    let ours = robot.clone();
    std::thread::Builder::new()
        .name("homecoming".into())
        .spawn(move || {
            // Everything this thread moves is the duck's own motion: said,
            // and stopped by the user's stop (see `tools::mark_self_driven`).
            tools::mark_self_driven();
            run(&ours, &cfg);
            ours.lock().expect("robot poisoned").places.registry.homecoming_settled();
        })
        .map(|_| ())
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "homecoming: cannot start the thread");
            robot.lock().expect("robot poisoned").places.registry.homecoming_settled();
        });
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
                if search(robot, cfg.boot_search_s, WHY_BOOT) {
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
                    if held(robot) {
                        return;
                    }
                    tracing::info!(map = newest, waited_s = cfg.boot_search_s, "homecoming: no confirmation yet; the map is frozen, so the search goes on");
                    if search(robot, cfg.boot_search_s * 3.0, WHY_BOOT) {
                        tracing::info!(map = newest, "homecoming: home — the pose is confirmed on the saved map");
                        resume_exploring(robot, cfg, &newest);
                    } else if !held(robot) {
                        tracing::warn!(map = newest, "homecoming: no confirmation on the frozen map; standing down — the duck does not know where it is");
                    }
                    return;
                }
                if held(robot) {
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
        // The fresh map is a search, not the house: nothing of it is saved
        // or declared under the house's name until the saved map is adopted.
        {
            let mut robot = robot.lock().expect("robot poisoned");
            robot.places.explore.map_searching();
            robot.places.map_started_afresh();
        }
        // The map lane learns of the wipe from the next map frame, a
        // second later. Asking to explore before then is refused for a
        // pose that no longer exists — the frame in hand is the one from
        // the map just thrown away.
        if !search(robot, 20.0, WHY_FRESH) {
            if held(robot) {
                return;
            }
            tracing::warn!("homecoming: the fresh map has not settled; exploring anyway");
        }
    }
    if held(robot) {
        return;
    }

    // Explore, and ask the map-to-map question as the map grows.
    // A search, not a session: nothing of this fresh map is saved under the
    // house's name — adopting the saved map stops it, and a session would
    // save the six minutes over the house.
    if let Err(e) = start_exploring(robot, cfg.explore_max_s, false, WHY_FRESH) {
        tracing::warn!(error = %e, "homecoming: cannot start exploring");
        return;
    }
    // The run of agreeing asks so far: the answer, and how many in a row.
    let mut previous: Option<(String, f64, f64, u64)> = None;
    let mut agreed: u32 = 0;
    let mut asked_after_exploring = false;
    loop {
        std::thread::sleep(Duration::from_secs_f64(cfg.recognize_every_s));
        if held(robot) {
            return;
        }
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
    if held(robot) {
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
    if !search(robot, 60.0, WHY_ADOPTED) {
        if held(robot) {
            return;
        }
        tracing::warn!("homecoming: the adopted place is still unconfirmed; exploring anyway");
    }
    // A done house is navigated, not explored: the adoption froze it.
    let done = robot.lock().expect("robot poisoned").places.explore.status().progress.as_ref().and_then(|p| p.get("done")).and_then(Value::as_bool).unwrap_or(false);
    if done {
        tracing::info!(map = name, "homecoming: the house is mapped; the map frozen, navigating on it");
        return;
    }
    if let Err(e) = start_exploring(robot, max_s, true, WHY_ADOPTED) {
        tracing::warn!(error = %e, "homecoming: cannot pick exploring back up after adopting");
    }
}

/// Start the exploring job, giving the map lane a few seconds to catch up
/// if the first attempt is refused for want of a trustworthy pose.
/// Progressive exploration (`resume_explore`): home on a map still being
/// explored — the mapper mapping, not frozen — the next session starts
/// from here: the frontiers left are where the last one stopped. A map
/// whose house is done is frozen and navigated on, `resume_explore` or not.
fn resume_exploring(robot: &Arc<Mutex<Robot>>, cfg: &HomecomingConfig, name: &str) {
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
        // does not — and explores no more unless asked for a new map. The
        // load froze it already (`tools::freeze_if_done`); asked again, it
        // costs nothing. With or without `resume_explore`: without, a
        // done house went on being inked (casa_grande, 2026-10-02).
        let frozen = tools::freeze_if_done(&robot.lock().expect("robot poisoned"), name);
        if frozen == Some(true) {
            tracing::info!(map = name, "homecoming: the house is mapped; the map frozen, navigating on it");
        }
        return;
    }
    if !cfg.resume_explore {
        return;
    }
    let mut last = String::new();
    for _ in 0..10 {
        if held(robot) {
            return;
        }
        match call(robot, "robot.map_explore", &json!({"max_s": cfg.explore_max_s, "save_as": name, "battery_min_pct": cfg.resume_battery_min_pct})) {
            Ok(answer) if answer.get("started").is_some() => {
                robot.lock().expect("robot poisoned").places.explore.self_start(WHY_RESUME);
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

fn start_exploring(robot: &Arc<Mutex<Robot>>, max_s: f64, session: bool, why: &str) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..10 {
        if held(robot) {
            return Err(crate::explore::STOPPED_BY_USER.into());
        }
        match call(robot, "robot.map_explore", &json!({"max_s": max_s, "session": session})) {
            // `map_explore` answers `running: true` when a job is already
            // going, which is not the same as having started one.
            Ok(answer) if answer.get("started").is_some() => {
                robot.lock().expect("robot poisoned").places.explore.self_start(why);
                return Ok(());
            }
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

fn wipe(robot: &Arc<Mutex<Robot>>) -> Result<(), String> {
    // On the map socket, as every map-library call: robotd's when it hosts
    // the mapper, quack-navd's own when `[maploc]` does. Asked of robotd's
    // lane, the wipe was "unknown method" against the released robotd
    // (casa_arredata on the twin, 2026-09-25) — the fresh map never came.
    let socket = robot.lock().expect("robot poisoned").places.map_socket.clone();
    tools::map_library(&socket, crate::map::METHOD_ROBOT_MAP_WIPE, None).map(|_| ())
}

/// The search's own pulses and turns: the duck's own move, without the
/// callers' cliff guard (see `tools::internal_move`).
fn blind_move(robot: &Arc<Mutex<Robot>>, args: &Value) -> Result<Value, String> {
    let mut robot = robot.lock().expect("robot poisoned");
    tools::internal_move(&mut robot, args)
}

/// One tool call on the shared robot. The lock is held for the call and
/// nothing more: whoever calls in over the daemon's socket wants it too.
fn call(robot: &Arc<Mutex<Robot>>, name: &str, args: &Value) -> Result<Value, String> {
    let mut robot = robot.lock().expect("robot poisoned");
    tools::execute(name, args, &mut robot)
}
