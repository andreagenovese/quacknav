//! The place tools, as a catalog fragment plus an executor.
//!
//! Agent-neutral on purpose: the catalog is JSON Schema, the result is
//! JSON, and `Err(text)` is the reason an LLM reads. A host splices
//! [`catalog`] into its own tool list and routes the names [`handles`]
//! claims to [`execute`]; quacksat does exactly that, and a standalone
//! daemon would serve the same four over its own MCP endpoint.

use std::time::Duration;
use duck_ipc_proto as proto;
use serde_json::{Value, json};

use std::time::Instant;

use crate::cliff::{CliffWatch, StreamState};
use crate::config::MapConfig;
use crate::map::{MapSupport, MapWatch};
use crate::places::{MAX_RADIUS_M, MIN_RADIUS_M, Registry};

/// Everything the place tools act on.
pub struct Places {
    /// The live map lane, when `[map] enabled`.
    pub map: Option<MapWatch>,
    pub registry: Registry,
    /// The cliff guard, when `[map] cliff_guard`.
    pub cliff: Option<CliffWatch>,
    /// The "map everything" job, idle or running.
    pub explore: crate::explore::ExploreHandle,
    /// Where robotd listens — the explore job opens its own lane there.
    pub robotd_socket: String,
    /// Where `robot.map` and the map library answer: robotd's socket, or
    /// quack-navd's own when it hosts the mapper (`[maploc]`).
    pub map_socket: String,
    /// The `[map]` section: the explore budget and the asking phrase.
    pub map_config: MapConfig,
    /// The `[gait]` section: yaw trim and per-side gains for every walk.
    pub gait: quack_duck::gait::GaitConfig,
}

/// Every lane the navigation acts on: robotd's request lane, and the
/// map, registry, cliff guard and explore job beside it.
pub struct Robot {
    /// The request lane; `None` while robotd is unreachable.
    pub control: Option<quack_duck::Control>,
    /// The map lane, the registry, the guard and the explore job.
    pub places: Places,
}

impl Robot {
    pub fn connect(
        config: &MapConfig,
        robotd_socket: &str,
        map_socket: &str,
        gait: quack_duck::gait::GaitConfig,
    ) -> Self {
        let control = match quack_duck::Control::connect(robotd_socket) {
            Ok(control) => Some(control),
            Err(e) => {
                tracing::warn!(error = %e, "robotd unreachable — the navigation runs without the robot");
                None
            }
        };
        let mut places = Places::connect(config, robotd_socket, map_socket);
        places.gait = gait;
        Self { control, places }
    }

    /// No robotd, no map, an in-memory registry (tests, dry runs).
    pub fn detached() -> Self {
        Self { control: None, places: Places::detached() }
    }
}

impl Places {
    /// Start the map lane (if enabled) and load the registry. Nothing here
    /// is fatal: a missing robotd or registry file degrades to "the tool
    /// says so" at call time.
    pub fn connect(config: &MapConfig, robotd_socket: &str, map_socket: &str) -> Self {
        let map = config
            .enabled
            .then(|| MapWatch::spawn(map_socket.to_owned()));
        let registry = match Registry::load(&config.places_path) {
            Ok(registry) => {
                tracing::info!(
                    path = %config.places_path,
                    places = registry.places().len(),
                    "places registry loaded"
                );
                registry
            }
            Err(e) => {
                tracing::warn!(error = %e, "places registry unusable — places will not persist");
                Registry::in_memory()
            }
        };
        let cliff = (config.enabled && config.cliff_guard)
            .then(|| CliffWatch::spawn(config.tof_socket.clone(), robotd_socket.to_owned()));
        Self {
            map,
            registry,
            cliff,
            explore: crate::explore::ExploreHandle::new().with_ground(&config.places_path),
            robotd_socket: robotd_socket.to_owned(),
            map_socket: map_socket.to_owned(),
            map_config: config.clone(),
            gait: quack_duck::gait::GaitConfig::default(),
        }
    }

    /// No map lane, no guard, an in-memory registry (tests, dry runs).
    /// `Err` while the explore job drives: one driver at a time.
    pub fn not_exploring(&self) -> Result<(), String> {
        if self.explore.running() {
            return Err(
                "the duck is exploring on its own (robot.map_explore); stop it first with \
                 robot.map_explore {\"stop\": true}"
                    .into(),
            );
        }
        Ok(())
    }

    pub fn detached() -> Self {
        Self {
            map: None,
            registry: Registry::in_memory(),
            cliff: None,
            explore: crate::explore::ExploreHandle::new(),
            robotd_socket: String::new(),
            map_socket: String::new(),
            map_config: MapConfig::default(),
            gait: quack_duck::gait::GaitConfig::default(),
        }
    }
}

/// The tool names this crate executes, in catalog order.
pub const TOOLS: [&str; 5] = [
    "robot.where_am_i",
    "robot.remember_place",
    "robot.forget_place",
    "robot.list_places",
    "robot.map_status",
];

fn handles_places(name: &str) -> bool {
    TOOLS.contains(&name)
}

/// The catalog fragment: JSON-Schema parameters, projectable to OpenAI
/// tools and MCP listings.
fn catalog_places() -> Vec<Value> {
    vec![
        json!({
            "name": "robot.where_am_i",
            "description": "Where the duck is in the house, by name. Returns the nearest \
        remembered place and the distance to it (at_place = inside that place), or known=false \
        when the duck cannot trust its position yet (seated, just carried, still looking around, \
        no map). Use when asked where you are or before deciding where to go. The duck only knows \
        places somebody taught it with robot.remember_place; it does not recognize rooms by sight.",
            "parameters": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "robot.remember_place",
            "description": "Teach the duck the name of the spot it is standing in right now \
        (the user says \"this is the kitchen\" → remember_place name=\"kitchen\"). Teaching the \
        same name again from another spot of the same room widens the place. Fails while the duck \
        is not sure of its position: then ask the user to let it stand still and look around, and \
        try again.",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "the place's name, as the user says it"},
                    "radius_m": {"type": "number", "description": "how far from this spot still counts as the place; default 1.5 m", "minimum": MIN_RADIUS_M, "maximum": MAX_RADIUS_M}
                },
                "required": ["name"]
            }
        }),
        json!({
            "name": "robot.forget_place",
            "description": "Forget a remembered place by name.",
            "parameters": {
                "type": "object",
                "properties": {"name": {"type": "string"}},
                "required": ["name"]
            }
        }),
        json!({
            "name": "robot.list_places",
            "description": "The places the duck knows by name, each with its distance from \
        where the duck is now when that is known. A stale place belongs to a map that was reset \
        since it was taught: it needs teaching again before it can be recognized.",
            "parameters": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "robot.map_status",
            "description": "How the duck's map of the house is doing: whether mapping is on, \
        whether the duck trusts its position, how much has been mapped (windows = stops that \
        reached the map, submaps = patches of about 4 m), the free distance ahead/left/right/behind \
        before a known wall (clearance; 'unknown' means unexplored), whether a drop — stairs or a hole, \
        invisible to the map — is in view (cliff), and a hint on what to do next. Use it \
        when asked about the map, at the start of a mapping tour, and after a few robot.map_step \
        calls to tell the user how it is going. house.percent_mapped is how much of the house \
        is mapped (an estimate, on the low side); house.done says the exploration is over — answer \
        \"how far along is the map\" with it.",
            "parameters": {"type": "object", "properties": {}}
        }),
    ]
}

/// Execute one of [`TOOLS`]. `Err(text)` is the LLM-readable reason.
fn execute_places(name: &str, args: &Value, places: &mut Places) -> Result<Value, String> {
    match name {
        "robot.where_am_i" => where_am_i(places),
        "robot.remember_place" => {
            let name = require_str(args, "name")?;
            let radius = args.get("radius_m").and_then(Value::as_f64);
            let fix = located(places)?;
            let place = places
                .registry
                .remember(name, fix.pose, radius)
                .map_err(|e| format!("cannot remember `{name}`: {e}"))?;
            Ok(json!({
                "remembered": place.name,
                "anchors": place.anchors.len(),
                "radius_m": place.radius_m,
                "pose": pose_json(fix.pose),
            }))
        }
        "robot.forget_place" => {
            let name = require_str(args, "name")?;
            let forgotten = places
                .registry
                .forget(name)
                .map_err(|e| format!("cannot forget `{name}`: {e}"))?;
            Ok(json!({"forgotten": forgotten, "name": name}))
        }
        "robot.list_places" => {
            let here = located(places).ok().map(|fix| fix.pose);
            let listed: Vec<Value> = places
                .registry
                .places()
                .iter()
                .map(|place| {
                    let stale = places.registry.is_stale(place);
                    json!({
                        "name": place.name,
                        "anchors": place.anchors.len(),
                        "radius_m": place.radius_m,
                        "stale": stale,
                        "distance_m": here
                            .filter(|_| !stale)
                            .map(|(x, y, _)| round2(place.distance_to(x, y))),
                    })
                })
                .collect();
            Ok(json!({"places": listed, "position_known": here.is_some()}))
        }
        "robot.map_status" => map_status(places),
        other => Err(format!("unknown tool `{other}`")),
    }
}

/// The map's state in numbers and in one line of advice — what a tour
/// narrates between steps.
fn map_status(places: &mut Places) -> Result<Value, String> {
    let Some(map) = &places.map else {
        return Err("this satellite has no map lane ([map] enabled = false)".into());
    };
    let status = map.snapshot();
    let (enabled, mode) = match &status.support {
        MapSupport::Unsupported => {
            return Err("this robot's software has no map (robotd predates the map API)".into());
        }
        MapSupport::Supported { enabled, mode } => (*enabled, mode.clone()),
        MapSupport::Unknown => (false, None),
    };
    if !enabled {
        return Ok(json!({
            "mapping": false,
            "hint": if status.latest.is_none() && matches!(status.support, MapSupport::Unknown) {
                "no answer from robotd yet: it may be down or still starting"
            } else {
                "mapping is disabled on this robot ([maploc] in robotd.toml); nothing can be mapped until it is enabled"
            },
        }));
    }
    let Some(frame) = &status.latest else {
        return Ok(json!({
            "mapping": true,
            "mode": mode,
            "hint": "mapping is on but no map frame has arrived yet; wait a moment",
        }));
    };
    if let Err(e) = places.registry.observe(status.epoch, frame.n_submaps) {
        tracing::warn!(error = %e, "places registry not saved");
    }
    let grid = frame.grid().ok();
    let (free, wall) = grid
        .as_ref()
        .map(|g| {
            let (_, f, w) = g.counts();
            (f, w)
        })
        .unwrap_or((0, 0));
    let clearance = grid
        .as_ref()
        .map(|g| clearance_json(g, frame.pose()))
        .unwrap_or(Value::Null);
    let drop_now = places
        .cliff
        .as_ref()
        .and_then(|c| c.snapshot().nearest(Instant::now()));
    let hint = if frame.seated {
        "the duck is seated or fallen: stand it up before mapping (nothing is mapped from the floor)"
    } else if !frame.tracking {
        "the duck is not sure of its position: keep it standing still and let it look around until it is"
    } else if drop_now.is_some() {
        "a drop — stairs or a hole — is in view: the map cannot show it; do not walk toward it (see cliff.bearing_deg)"
    } else if frame.windows == 0 {
        "nothing has reached the map yet: stand still for at least six seconds at a time (a stop is what maps)"
    } else if frame.n_loops == 0 {
        "mapping; each stop of six seconds adds to the map — walk a little, stop, repeat, and come back through places already mapped so the map can close its loops"
    } else {
        "mapping well: loops have closed, the map is consistent with itself"
    };
    Ok(json!({
        "mapping": true,
        "mode": mode,
        "tracking": frame.tracking,
        "still": frame.still,
        "seated": frame.seated,
        "windows": frame.windows,
        "submaps": frame.n_submaps,
        "loops": frame.n_loops,
        "cells": {"free": free, "wall": wall, "size_m": round2(frame.cell_m as f64)},
        "pose": pose_json(frame.pose()),
        "clearance": clearance,
        "cliff": cliff_json(places.cliff.as_ref(), Instant::now()),
        "places_known": places.registry.current().count(),
        "house": house_json(places, grid.as_ref()),
        "hint": hint,
    }))
}

/// How much of the house the duck has mapped, live from the map in hand,
/// and what the exploration's books say: sessions, and whether it is done
/// (found nothing left, or declared complete by the user).
fn house_json(places: &Places, grid: Option<&crate::map::Grid>) -> Value {
    let live = grid.map(|g| (crate::explore::explored_share(g).0 * 100.0).round());
    let progress = places.explore.status().progress;
    let field = |k: &str| progress.as_ref().and_then(|p| p.get(k)).cloned().unwrap_or(Value::Null);
    json!({
        "map": places.explore.map_name(),
        "percent_mapped": live,
        "sessions": field("sessions"),
        "done": field("done").as_bool().unwrap_or(false),
        "declared_by_user": field("declared_by_user").as_bool().unwrap_or(false),
    })
}

/// What the cliff guard sees: `guard` says whether it can see at all,
/// `edge_between_m` the nearest drop on its books — the floor ends somewhere
/// in that span — with its `bearing_deg` (body frame, positive to the
/// left), or null.
pub fn cliff_json(cliff: Option<&CliffWatch>, now: Instant) -> Value {
    let Some(cliff) = cliff else {
        return json!({"guard": "off"});
    };
    let s = cliff.snapshot();
    let guard = match (&s.stream, s.body_seen) {
        (StreamState::Unavailable(why), _) => format!("no depth sensor: {why}"),
        (StreamState::Unknown, _) => "no depth stream yet".to_owned(),
        (StreamState::Serving, false) => "no body pose yet".to_owned(),
        (StreamState::Serving, true) => "watching".to_owned(),
    };
    let drop = s.nearest(now);
    let ahead = s.obstacle_within(now, 0.0, std::f64::consts::PI);
    // What the guard holds right now, ray by ray, for an overlay to draw:
    // every recent frame's head yaw and wedge, its obstacles (bearing,
    // range) and its drops (bearing, edge between), all in the body frame.
    let frames: Vec<serde_json::Value> = s
        .recent
        .iter()
        .filter(|f| now.duration_since(f.at) <= crate::cliff::MEMORY && !f.moving)
        .map(|f| {
            json!({
                "head_yaw": round2(f.head_yaw),
                "obstacles": f.obstacles.iter().map(|o| [round2(o.bearing), round2(o.range_m)]).collect::<Vec<_>>(),
                "drops": f.drops.iter().map(|d| [round2(d.bearing), round2(d.edge_min_m), round2(d.range_m)]).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "guard": guard,
        "frames": s.frames,
        "rays": frames,
        "half_fov": round2(crate::cliff::HALF_FOV_RAD),
        "looked_ahead": s.looked_at(now, 0.0),
        "nearest_obstacle": ahead.map(|o| json!({
            "range_m": round2(o.range_m),
            "bearing_deg": o.bearing.to_degrees().round(),
        })),
        "edge_between_m": drop.map(|d| [round2(d.edge_min_m), round2(d.range_m)]),
        "bearing_deg": drop.map(|d| (d.bearing.to_degrees()).round()),
        "kind": drop.map(|d| match d.kind {
            crate::cliff::DropKind::Missing => "no floor return",
            crate::cliff::DropKind::Deep => "floor far below",
        }),
    })
}

/// How far the duck could go in each direction before a known wall — the
/// four rays a tour needs to choose its next leg. `by` says what ends the
/// ray: `wall`, `unknown` (unexplored: fine to walk into, carefully),
/// `edge` of the rendered map, or `open` beyond `LOOK_M`.
pub fn clearance_json(grid: &crate::map::Grid, (x, y, yaw): (f64, f64, f64)) -> Value {
    let dir = |name: &str, offset: f64| {
        let c = grid.clearance(x, y, yaw + offset, LOOK_M);
        (
            name.to_owned(),
            json!({"free_m": round2(c.free_m), "by": c.by.as_str()}),
        )
    };
    Value::Object(
        [
            dir("ahead", 0.0),
            dir("left", std::f64::consts::FRAC_PI_2),
            dir("right", -std::f64::consts::FRAC_PI_2),
            dir("behind", std::f64::consts::PI),
        ]
        .into_iter()
        .collect(),
    )
}

/// How far the clearance rays look.
pub const LOOK_M: f64 = 3.0;

/// A trusted position in the map frame, plus what the map looked like.
struct Fix {
    pose: (f64, f64, f64),
    n_submaps: u32,
    n_loops: u32,
    windows: u32,
}

/// Why there is no trusted position right now — the text the agent reads.
enum NoFix {
    /// The map lane is missing, unsupported, disabled or silent: the
    /// tool cannot work on this robot as configured.
    Unavailable(String),
    /// The lane works, the mapper just cannot vouch for the pose now.
    Untrusted(String),
}

/// Resolve the duck's position: the map lane's newest frame, folded into
/// the registry so a reset is noticed before any name is matched.
fn locate(places: &mut Places) -> Result<Fix, NoFix> {
    let Some(map) = &places.map else {
        return Err(NoFix::Unavailable(
            "this satellite has no map lane ([map] enabled = false)".into(),
        ));
    };
    let status = map.snapshot();
    match &status.support {
        MapSupport::Unsupported => {
            return Err(NoFix::Unavailable(
                "this robot's software has no map (robotd predates the map API)".into(),
            ));
        }
        MapSupport::Supported { enabled: false, .. } => {
            return Err(NoFix::Unavailable(
                "mapping is disabled on this robot ([maploc] in robotd.toml)".into(),
            ));
        }
        MapSupport::Supported { .. } | MapSupport::Unknown => {}
    }
    let Some(frame) = &status.latest else {
        return Err(NoFix::Unavailable(
            "no map yet: robotd is unreachable or has not sent a map frame".into(),
        ));
    };
    if let Err(e) = places.registry.observe(status.epoch, frame.n_submaps) {
        tracing::warn!(error = %e, "places registry not saved");
    }
    match status.trusted_pose() {
        Some(pose) => Ok(Fix {
            pose,
            n_submaps: frame.n_submaps,
            n_loops: frame.n_loops,
            windows: frame.windows,
        }),
        None if frame.seated => Err(NoFix::Untrusted(
            "the duck is seated or fallen; it cannot tell where it is until it stands".into(),
        )),
        None => Err(NoFix::Untrusted(
            "the duck is not sure of its position yet: it is still matching what it sees \
             against the map (let it stand still and look around)"
                .into(),
        )),
    }
}

/// Like [`locate`], for tools that need a position or nothing.
fn located(places: &mut Places) -> Result<Fix, String> {
    locate(places).map_err(|e| match e {
        NoFix::Unavailable(text) => text,
        NoFix::Untrusted(text) => format!("no trusted position: {text}"),
    })
}

fn where_am_i(places: &mut Places) -> Result<Value, String> {
    let fix = match locate(places) {
        Ok(fix) => fix,
        Err(NoFix::Unavailable(text)) => return Err(text),
        Err(NoFix::Untrusted(reason)) => {
            return Ok(json!({"known": false, "reason": reason}));
        }
    };
    let (x, y, _) = fix.pose;
    let nearest = places.registry.nearest(x, y);
    let stale = places
        .registry
        .places()
        .iter()
        .filter(|p| places.registry.is_stale(p))
        .count();
    Ok(json!({
        "known": true,
        "place": nearest.as_ref().map(|n| n.place.name.clone()),
        "distance_m": nearest.as_ref().map(|n| round2(n.distance_m)),
        "at_place": nearest.as_ref().is_some_and(|n| n.within),
        "pose": pose_json(fix.pose),
        "places_known": places.registry.current().count(),
        "stale_places": stale,
        "map": {"submaps": fix.n_submaps, "loops": fix.n_loops, "windows": fix.windows},
    }))
}

fn pose_json((x, y, yaw): (f64, f64, f64)) -> Value {
    json!({"x": round2(x), "y": round2(y), "yaw": round2(yaw)})
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is required"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{MapEvent, MapFrame, MapStreamResult};

    fn frame(seq: u64, x: f64, y: f64, tracking: bool, seated: bool) -> MapFrame {
        MapFrame {
            seq,
            x,
            y,
            yaw: 0.0,
            tracking,
            x_min: -5.0,
            y_min: -5.0,
            cell_m: 0.05,
            rows: 1,
            cols: 1,
            cells: "AA==".into(),
            n_submaps: 3,
            n_loops: 0,
            windows: 10,
            still: true,
            seated,
            frozen: false,
        }
    }

    /// A fed map lane: subscribed, mapping, one frame.
    fn mapped(frame_: MapFrame) -> Places {
        let watch = MapWatch::detached();
        watch.push(MapEvent::Subscribed(MapStreamResult {
            accepted: true,
            enabled: true,
            mode: Some("stop_and_scan".into()),
        }));
        watch.push(MapEvent::Frame(Box::new(frame_)));
        Places {
            map: Some(watch),
            registry: Registry::in_memory(),
            cliff: None,
            ..Places::detached()
        }
    }

    #[test]
    fn catalog_and_executor_agree() {
        let names: Vec<String> = catalog_places()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(names, TOOLS);
        let mut places = Places::detached();
        for name in TOOLS {
            assert!(handles_places(name));
            let err = execute_places(name, &json!({"name": "x"}), &mut places)
                .err()
                .unwrap_or_default();
            assert!(!err.starts_with("unknown tool"), "{name}: {err}");
        }
        assert!(!handles_places("robot.move"));
        assert_eq!(
            execute_places("robot.move", &json!({}), &mut places),
            Err("unknown tool `robot.move`".into())
        );
        assert_eq!(
            execute_places("robot.remember_place", &json!({}), &mut places),
            Err("name is required".into())
        );
    }

    #[test]
    fn map_status_narrates_the_map() {
        let mut places = Places::detached();
        let err = execute_places("robot.map_status", &json!({}), &mut places).unwrap_err();
        assert!(err.contains("no map lane"), "{err}");

        let watch = MapWatch::detached();
        watch.push(MapEvent::Subscribed(MapStreamResult {
            accepted: true,
            enabled: false,
            mode: None,
        }));
        places.map = Some(watch);
        let off = execute_places("robot.map_status", &json!({}), &mut places).unwrap();
        assert_eq!(off["mapping"], false);
        assert!(off["hint"].as_str().unwrap().contains("disabled"));

        let mut seated = frame(1, 0.0, 0.0, true, true);
        seated.windows = 0;
        let mut places = mapped(seated);
        let s = execute_places("robot.map_status", &json!({}), &mut places).unwrap();
        assert_eq!(s["mapping"], true);
        assert_eq!(s["mode"], "stop_and_scan");
        assert!(s["hint"].as_str().unwrap().contains("stand it up"));

        let mut nothing = frame(2, 0.0, 0.0, true, false);
        nothing.windows = 0;
        places
            .map
            .as_ref()
            .unwrap()
            .push(MapEvent::Frame(Box::new(nothing)));
        let s = execute_places("robot.map_status", &json!({}), &mut places).unwrap();
        assert!(s["hint"].as_str().unwrap().contains("six seconds"));

        places
            .map
            .as_ref()
            .unwrap()
            .push(MapEvent::Frame(Box::new(frame(3, 0.5, 0.5, true, false))));
        let s = execute_places("robot.map_status", &json!({}), &mut places).unwrap();
        assert_eq!(s["windows"], 10);
        assert_eq!(s["submaps"], 3);
        assert_eq!(s["cells"]["free"], 0);
        assert_eq!(s["pose"]["x"], 0.5);
        assert_eq!(s["clearance"]["ahead"]["by"], "edge");
        assert!(s["hint"].as_str().unwrap().contains("close its loops"));
    }

    #[test]
    fn without_a_map_lane_the_tools_say_so() {
        let mut places = Places::detached();
        let err = execute_places("robot.where_am_i", &json!({}), &mut places).unwrap_err();
        assert!(err.contains("no map lane"), "{err}");
        let err = execute_places(
            "robot.remember_place",
            &json!({"name": "cucina"}),
            &mut places,
        )
        .unwrap_err();
        assert!(err.contains("no map lane"), "{err}");
        // Listing works regardless: it is the registry, not the map.
        let listed = execute_places("robot.list_places", &json!({}), &mut places).unwrap();
        assert_eq!(listed["position_known"], false);
        assert_eq!(listed["places"].as_array().unwrap().len(), 0);

        let watch = MapWatch::detached();
        watch.push(MapEvent::Subscribed(MapStreamResult {
            accepted: true,
            enabled: false,
            mode: None,
        }));
        places.map = Some(watch);
        let err = execute_places("robot.where_am_i", &json!({}), &mut places).unwrap_err();
        assert!(err.contains("disabled"), "{err}");
    }

    #[test]
    fn an_untrusted_pose_is_an_honest_answer_not_an_error() {
        let mut places = mapped(frame(1, 0.0, 0.0, false, false));
        let answer = execute_places("robot.where_am_i", &json!({}), &mut places).unwrap();
        assert_eq!(answer["known"], false);
        assert!(answer["reason"].as_str().unwrap().contains("not sure"));
        // Teaching, on the other hand, must refuse: a place at a guessed
        // pose would be a lie the registry keeps.
        let err = execute_places(
            "robot.remember_place",
            &json!({"name": "cucina"}),
            &mut places,
        )
        .unwrap_err();
        assert!(err.starts_with("no trusted position"), "{err}");

        let mut places = mapped(frame(1, 0.0, 0.0, true, true));
        let answer = execute_places("robot.where_am_i", &json!({}), &mut places).unwrap();
        assert_eq!(answer["known"], false);
        assert!(answer["reason"].as_str().unwrap().contains("seated"));
    }

    #[test]
    fn teach_then_recognize_then_walk_away() {
        let mut places = mapped(frame(1, 1.0, 2.0, true, false));
        let taught = execute_places(
            "robot.remember_place",
            &json!({"name": "Cucina", "radius_m": 1.0}),
            &mut places,
        )
        .unwrap();
        assert_eq!(taught["remembered"], "Cucina");
        assert_eq!(taught["pose"]["x"], 1.0);

        let here = execute_places("robot.where_am_i", &json!({}), &mut places).unwrap();
        assert_eq!(here["known"], true);
        assert_eq!(here["place"], "Cucina");
        assert_eq!(here["at_place"], true);
        assert_eq!(here["distance_m"], 0.0);
        assert_eq!(here["places_known"], 1);

        // Three metres away: nearest is still the kitchen, but not at it.
        places
            .map
            .as_ref()
            .unwrap()
            .push(MapEvent::Frame(Box::new(frame(2, 4.0, 2.0, true, false))));
        let there = execute_places("robot.where_am_i", &json!({}), &mut places).unwrap();
        assert_eq!(there["place"], "Cucina");
        assert_eq!(there["at_place"], false);
        assert_eq!(there["distance_m"], 3.0);

        let listed = execute_places("robot.list_places", &json!({}), &mut places).unwrap();
        assert_eq!(listed["places"][0]["distance_m"], 3.0);
        assert_eq!(listed["places"][0]["stale"], false);

        let gone = execute_places(
            "robot.forget_place",
            &json!({"name": "cucina"}),
            &mut places,
        )
        .unwrap();
        assert_eq!(gone["forgotten"], true);
        let here = execute_places("robot.where_am_i", &json!({}), &mut places).unwrap();
        assert_eq!(here["known"], true);
        assert!(here["place"].is_null());
    }

    #[test]
    fn a_map_reset_turns_places_stale() {
        let mut places = mapped(frame(1, 0.0, 0.0, true, false));
        execute_places(
            "robot.remember_place",
            &json!({"name": "studio"}),
            &mut places,
        )
        .unwrap();
        // A wipe: the mapper starts over with a single submap.
        let mut wiped = frame(2, 0.0, 0.0, true, false);
        wiped.n_submaps = 1;
        wiped.windows = 0;
        places
            .map
            .as_ref()
            .unwrap()
            .push(MapEvent::Frame(Box::new(wiped)));
        let here = execute_places("robot.where_am_i", &json!({}), &mut places).unwrap();
        assert_eq!(here["known"], true);
        assert!(here["place"].is_null(), "a stale place never matches");
        assert_eq!(here["stale_places"], 1);
        assert_eq!(here["places_known"], 0);
        let listed = execute_places("robot.list_places", &json!({}), &mut places).unwrap();
        assert_eq!(listed["places"][0]["stale"], true);
        assert!(listed["places"][0]["distance_m"].is_null());
    }
}

// ---- the navigation's own tools (split from quacksat's tools.rs, 2026-09-22) ----

/// The navigation's whole tool surface: the place tools and the ones
/// that drive (`robot.map_step`, `robot.map_explore`, `robot.go_to`,
/// the map library). A host — the voice satellite over its socket, an
/// agent, anything — announces this beside its own.
pub fn catalog() -> Vec<Value> {
    let mut tools = catalog_places();
    tools.push(json!({
        "name": "robot.map_step",
        "description": "One step of a mapping tour: walk or turn for a bounded time, then \
    stand still so the stop reaches the map (mapping only happens while standing, with the \
    head sweeping on its own). Returns how many map windows the stop added and the free \
        distance ahead/left/right/behind, and refuses to walk into a mapped wall or toward a drop \
        (stairs, a hole) the depth sensor has seen. To map a room, \
    call it repeatedly — short walks (walk_s 3 with vx 0.3 ≈ 30 cm; to turn, keep vx 0.3 and \
        add vyaw 0.7 or -0.7 for about 2 s per 90°, the duck cannot turn in place), each followed \
        by the default 6 s stand — and tell the user how it goes; \
    come back through places already mapped so the map can close loops. Stop when the user \
    says so. Check robot.map_status first: the duck must be standing and mapping enabled.",
        "parameters": {
            "type": "object",
            "properties": {
                "vx": {"type": "number", "description": "m/s forward (+) / backward (-)"},
                "vy": {"type": "number", "description": "m/s sidestep left (+) / right (-)"},
                "vyaw": {"type": "number", "description": "rad/s turn, + is left"},
                "walk_s": {"type": "number", "minimum": 0.0, "maximum": quack_duck::body::MAX_MOVE_DURATION_S, "description": "seconds of walking before the stop (0 = just stand)"},
                "centre": {"type": "boolean", "description": "keep to the middle between mapped walls on the way (the explorer's own legs ask for it; off by default)"},
                "gap": {"type": "boolean", "description": "a doorway step: margins shrink to the body plus a little (the explorer's own legs ask for it; off by default)"},
                "passage": {"type": "boolean", "description": "a short straight leg along a passage beside a drop, with a wall seen by the sensor at the body's side: the cliff guard judges a narrower lane (the explorer's passage legs ask for it; off by default)"},
                "stop_s": {"type": "number", "minimum": 0.0, "maximum": quack_duck::body::MAX_STOP_S, "description": "seconds of standing still after the walk; default 6"}
            }
        }
    }));
    tools.push(json!({
        "name": "robot.map_explore",
        "description": "Map everything: the duck walks on its own to wherever the known floor \
    meets the unknown, stands to map it, and repeats until nothing reachable is left or the \
    time budget runs out — minutes. Starts in the background and returns at once: answer \
    the user now (\"I'm exploring\") and end your turn; do not wait for it or poll it in \
    the same reply. When the user asks how it is going, robot.map_status tells \
    (explore.state, frontiers_left, legs). While it runs, robot.move and robot.map_step are refused. Call with stop=true to stop it. \
    When the duck reaches a nameless area it asks the user where it is; answer by calling \
    robot.remember_place with the name the user gives. \
    Exploring is progressive: each call is one session (a charge's worth), it goes on from \
    where the last one stopped and saves the map at the end; robot.map_status explore.progress \
    says how much of the house is mapped (percent, sessions, done). When the house is already \
    mapped the call does not start and says so — tell the user. Only when the user explicitly \
    asks to map the house again from nothing, confirm with them first that the current map will \
    be replaced, then call with fresh=true: the first call only says what would be lost; ask the \
    user, and on their yes call again with fresh=true and confirmed=true. When the user says the exploration is complete \
    (\"esplorazione completata\", \"basta così, la casa è mappata\"), call with complete=true: \
    the map is saved, closed and declared complete as it is, and from then on the duck only \
    navigates on it.",
        "parameters": {
            "type": "object",
            "properties": {
                "stop": {"type": "boolean", "description": "stop a running exploration"},
                "fresh": {"type": "boolean", "description": "a new map from nothing, replacing the saved one when this session saves; without confirmed it only answers what would be lost"},
                "confirmed": {"type": "boolean", "description": "with fresh: the user has confirmed, after being told what is lost"},
                "complete": {"type": "boolean", "description": "the user declares the exploration complete: stop, save, close the map as it is"},
                "save_as": {"type": "string", "description": "the map's name; default the current map's, else \"casa\""},
                "watch": {"type": "boolean", "description": "do not walk: somebody else drives the duck, and it only books what it sees at each stop (the guided drive that writes the books)"},
                "max_s": {"type": "number", "description": "time budget in seconds; default from the config"}
            }
        }
    }));
    tools.push(json!({
        "name": "robot.go_to",
        "description": "Go where the user says: \"go to the kitchen\", \"take me to the \
    bedroom\", \"back to your dock\". Walks to a place the duck knows, or to a point on \
    its map. Give `place` (a name from robot.list_places) or `x` and `y` in map metres. It plans the \
    cheapest way on the map it has — it does not explore — and walks it with the same \
    guards as robot.map_step, so stairs and unmapped obstacles still stop it. The walk \
    takes a minute or more. Starts in the background and returns at once: answer the \
    user now (\"on my way\") and end your turn; do not wait for the arrival or poll it in \
    the same reply. When the user asks, robot.map_status tells (explore.state, \
    explore.target_distance_m, explore.reason once it is done). While it runs, \
    robot.move and robot.map_step are refused. Call with stop=true to stop it.",
        "parameters": {
            "type": "object",
            "properties": {
                "place": {"type": "string", "description": "the name of a known place"},
                "x": {"type": "number", "description": "map metres, when no place is given"},
                "y": {"type": "number", "description": "map metres, when no place is given"},
                "stop": {"type": "boolean", "description": "stop a running go_to"},
                "max_s": {"type": "number", "description": "time budget in seconds; default 300"}
            }
        }
    }));
    tools.push(json!({
        "name": "robot.map_save",
        "description": "Keep the map the duck has just built, under a name. A duck that \
    lives in one house maps it once: save the finished map (`home`, `ground_floor`) and it \
    can be given back after a reboot with robot.map_load. Names are letters, digits, '-' \
    and '_'. Saving again under the same name replaces it.",
        "parameters": {
            "type": "object",
            "properties": {"name": {"type": "string", "description": "what to call this map"}},
            "required": ["name"]
        }
    }));
    tools.push(json!({
        "name": "robot.map_list",
        "description": "What maps the duck has saved: name, size and when each was written.",
        "parameters": {"type": "object", "properties": {}}
    }));
    tools.push(json!({
        "name": "robot.map_match",
        "description": "Is the duck in one of the houses it has mapped before? Compares the \
    map it is holding against every saved map and answers candidates, best first — a name, \
    where the live map sits inside the saved one, and a score (lower is better). It is not a \
    verdict: on this sensor a flat and its mirror image score alike, so trust a name only if \
    the same one comes back a few minutes later with a bigger map. Early in a run there is \
    not enough map to ask (live_cells small) and the answer is empty.",
        "parameters": {
            "type": "object",
            "properties": {"name": {"type": "string", "description": "try only this saved map"}}
        }
    }));
    tools.push(json!({
        "name": "robot.map_load",
        "description": "Give the duck back a saved map. The map comes back; the position \
    does not — the duck does not know where in it it stands, so it searches. Have it stand \
    still and look around (robot.map_step with walk_s 0 turns on the spot), and watch \
    robot.map_status until tracking is true before asking it to go anywhere.",
        "parameters": {
            "type": "object",
            "properties": {"name": {"type": "string", "description": "which saved map"}},
            "required": ["name"]
        }
    }));
    tools
}

/// Whether this crate answers for `name`.
pub fn handles(name: &str) -> bool {
    handles_places(name)
        || matches!(
            name,
            "nav.take_question"|
            "robot.map_step"
                | "robot.map_explore"
                | "robot.go_to"
                | "robot.map_save"
                | "robot.map_list"
                | "robot.map_load"
                | "robot.map_match"
                | "robot.map_adopt"
                | "robot.map_wipe"
                | "robot.move"
        )
}

/// Execute one navigation tool.
pub fn execute(name: &str, args: &Value, robot: &mut Robot) -> Result<Value, String> {
    if handles_places(name) {
        let mut result = execute_places(name, args, &mut robot.places)?;
        // The map's numbers carry the explore job's state: what the
        // caller polls to know whether the duck is still walking.
        if name == "robot.map_status"
            && let Some(map) = result.as_object_mut()
        {
            map.insert("explore".into(), robot.places.explore.status().to_json());
        }
        return Ok(result);
    }
    match name {
        // The explorer's "where are we?", handed to whoever can speak:
        // the satellite polls this and puts the answer on the record
        // with `robot.remember_place` (the split of 2026-09-22 — the
        // question is raised here, the voice is over there).
        "nav.take_question" => Ok(match robot.places.explore.take_question() {
            Some(q) => json!({
                "asking": true,
                "phrase": robot.places.map_config.ask_phrase,
                "pose": {"x": round2(q.pose.0), "y": round2(q.pose.1), "yaw": round2(q.pose.2)},
            }),
            None => json!({"asking": false}),
        }),
        "robot.map_step" => map_step(robot, args),
        "robot.map_explore" => map_explore(robot, args),
        "robot.go_to" => go_to(robot, args),
        "robot.map_save" => {
            let name = map_name(args)?;
            let saved = map_library(&robot.places.map_socket, "robot.map_save", Some(name.clone()))?;
            if let Some(n) = name.get("name").and_then(Value::as_str) {
                robot.places.explore.name_live_map(n);
                robot.places.explore.keep_ground();
            }
            Ok(saved)
        }
        "robot.map_list" => map_library(&robot.places.map_socket, "robot.map_list", None),
        "robot.map_load" => {
            let name = map_name(args)?;
            let loaded = map_library(&robot.places.map_socket, "robot.map_load", Some(name.clone()))?;
            if let Some(n) = name.get("name").and_then(Value::as_str) {
                robot.places.explore.map_named(n);
            }
            Ok(loaded)
        }
        "robot.map_match" => map_library(
            &robot.places.map_socket,
            "robot.map_match",
            match args.get("name") {
                Some(_) => Some(map_name(args)?),
                None => Some(json!({})),
            },
        ),
        "robot.map_adopt" => {
            let name = map_name(args)?;
            let adopted = map_library(&robot.places.map_socket, "robot.map_adopt", Some(name.clone()))?;
            if let Some(n) = name.get("name").and_then(Value::as_str) {
                robot.places.explore.map_named(n);
            }
            Ok(adopted)
        }
        "robot.map_wipe" => map_library(&robot.places.map_socket, "robot.map_wipe", None),
        "robot.move" => {
            robot.places.not_exploring()?;
            let params = quack_duck::body::move_params(args);
            let duration = quack_duck::body::number(args, "duration_s").clamp(0.0, quack_duck::body::MAX_MOVE_DURATION_S);
            let params = quack_duck::body::trimmed(&robot.places.gait, params);
            let vyaw_cmd = params.vyaw;
            let hold_yaw = robot.places.cliff.as_ref().map(|c| {
                let c = c.clone();
                move || c.snapshot().odom_yaw
            });
            let hold: Option<(&dyn Fn() -> Option<f64>, f64, f64)> =
                match (quack_duck::body::hold_heading() && vyaw_cmd.abs() < quack_duck::body::HOLD_STRAIGHT_MAX, &hold_yaw) {
                    (true, Some(f)) => Some((f, 0.0, 0.0)),
                    _ => None,
                };
            quack_duck::body::timed_move_held(&mut robot.control, params, duration, hold)?;
            Ok(json!({"done": true, "walked_s": duration}))
        }
        other => Err(format!("this is not a navigation tool: `{other}`")),
    }
}


fn wall_margin_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("QK_WALL_MARGIN_M").ok().and_then(|v| v.parse().ok()).unwrap_or(0.18)
    })
}
/// A step shorter than this does not move the gait at all (measured).
const MIN_STEP_S: f64 = 1.0;
/// Allowance for the gait's start latency and odometry on top of the
/// margin: the leg planner's reserve is `WALL_MARGIN_M` + this.
const STEP_SLACK_M: f64 = 0.10;
/// Doorway margins (the explorer's `gap` legs): the body is 0.19 m wide,
/// a doorway leaves centimetres, so the frontal margin, the slack and the
/// lane shrink to the body plus a little.
const GAP_MARGIN_M: f64 = 0.15;
const GAP_SLACK_M: f64 = 0.05;
const GAP_LANE_HALF_M: f64 = 0.115;
/// A mapped wall closer than this on one side steers the step away from
/// it: a duck hugging a wall sees nothing but that wall.
const HUG_M: f64 = 0.20;
/// The margin a tight turning arc keeps from what is ahead.
const ARC_MARGIN_M: f64 = 0.10;
/// The yaw rate mixed in to peel away from a hugged wall.
const HUG_STEER_RAD_S: f64 = 0.35;
/// Mapped walls on both sides closer than this, added up, make a passage
/// the step keeps to the middle of; the correction is proportional to how
/// far off the middle the duck is, with a small dead band.
const CENTER_SPAN_M: f64 = 1.2;
/// rad/s at full offset (the duck against one wall), whatever the span.
const CENTER_GAIN: f64 = 0.5;
const CENTER_MIN_SPAN_M: f64 = 0.3;
const CENTER_DEADBAND_M: f64 = 0.05;
/// A drop seen within this half-angle of the heading blocks a forward
/// step — the whole front half: an arc swings the body toward what was
/// off to the side, and a hole is not a bump.
/// A drop counts against a forward step only within this much of the line
/// the duck would walk: the body's half-width plus a wide margin — a
/// stairwell beside the path is not in the way, and a cone (the front
/// half) made the passage between a wall and the stairwell impassable.
const CLIFF_LANE_HALF_M: f64 = 0.22;
/// The lane of a passage leg (see `passage` in `plan_step`), and how near
/// the sensed wall must be at the body's side for it to apply.
const CLIFF_LANE_PASSAGE_M: f64 = 0.17;
const CLIFF_WALL_NEAR_M: f64 = 0.35;
/// Stop this far short of a drop's edge — farther than a wall's margin,
/// because an edge is not a bump.
/// `QK_CLIFF_MARGIN_M`: 0.25 since 2026-09-16 (was 0.35) — the flank 15 cm from the
/// edge at the leg's end, and the same word as the planner's 0.22 round a
/// booked rim.
/// The least margin a leg may ask for from an edge (see `plan_step`).
const CLIFF_MARGIN_FLOOR_M: f64 = 0.12;
fn cliff_margin_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("QK_CLIFF_MARGIN_M").ok().and_then(|v| v.parse().ok()).unwrap_or(0.25)
    })
}
/// What an arc advances along its starting heading before the turn takes
/// (a second of walking), and the margin kept from a drop there — less
/// than a straight step's, because the body is turning off that line.
const ARC_FIRST_M: f64 = 0.12;
const ARC_FIRST_MARGIN_M: f64 = 0.10;
/// A drop this far off the heading (radians) may be turned away from
/// with a tight arc; nearer the beak only backing up is allowed.
const CLIFF_AWAY_RAD: f64 = 0.5;
/// Sensor obstacles within this much of the line the duck would walk are
/// in the way: half the body (0.095 m) plus a little.
const LANE_HALF_M: f64 = 0.16;

/// The robot as the tools see it.
fn map_explore(robot: &mut Robot, args: &Value) -> Result<Value, String> {
    if args.get("complete").and_then(Value::as_bool).unwrap_or(false) {
        return map_explore_complete(robot, args);
    }
    if args.get("stop").and_then(Value::as_bool).unwrap_or(false) {
        let was_running = robot.places.explore.running();
        robot.places.explore.request_stop();
        return Ok(json!({"stopped": was_running, "explore": robot.places.explore.status().to_json()}));
    }
    if robot.places.explore.running() {
        return Ok(json!({"running": true, "explore": robot.places.explore.status().to_json()}));
    }
    // The same preconditions as a mapping step: a map, a standing duck.
    let Some(map) = &robot.places.map else {
        return Err("this satellite has no map lane ([map] enabled = false)".into());
    };
    let snapshot = map.snapshot();
    let Some(frame) = snapshot.latest.clone() else {
        return Err("no map yet: robotd is unreachable or has not sent a map frame".into());
    };
    if snapshot.trusted_pose().is_none() {
        return Err(if frame.seated {
            "the duck is seated or fallen: stand it up first (sit_toggle)".into()
        } else {
            "the duck is not sure of its position yet: let it stand still and look around first"
                .into()
        });
    }
    quack_duck::body::with_robot(&mut robot.control)?;
    let max_s = args
        .get("max_s")
        .and_then(Value::as_f64)
        .unwrap_or(robot.places.map_config.explore_max_s)
        .clamp(30.0, 3600.0);
    if args.get("watch").and_then(Value::as_bool).unwrap_or(false) {
        robot.places.explore.watch(&robot.places.robotd_socket, &robot.places, max_s, robot.places.gait.clone())?;
        return Ok(json!({"started": true, "watch": true, "max_s": max_s}));
    }
    let known: Vec<(f64, f64)> = robot
        .places
        .registry
        .current()
        .flat_map(|p| p.anchors.iter().map(|a| (a.x, a.y)))
        .collect();
    // Every exploration is a session of a progressive one: saved at the
    // end under the name asked, else the live map's own, else "casa";
    // ended early at the battery level asked.
    let name = match args.get("save_as").and_then(Value::as_str) {
        Some(n) => map_name(&json!({"name": n}))?.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
        None => robot.places.explore.map_name().unwrap_or_else(|| "casa".to_string()),
    };
    let fresh = args.get("fresh").and_then(Value::as_bool).unwrap_or(false);
    let progress = robot.places.explore.status().progress;
    let done = progress.as_ref().and_then(|p| p.get("done")).and_then(Value::as_bool).unwrap_or(false);
    if done && !fresh && robot.places.explore.map_name().as_deref() == Some(name.as_str()) {
        return Ok(json!({
            "started": false,
            "done": true,
            "progress": progress,
            "reason": "the house is already mapped: nothing is left to explore. A new map from nothing replaces it only if the user asks for one (fresh: true)",
        }));
    }
    // A new map replaces the one the user has: never on one sentence. The
    // first call answers what it would lose and waits for `confirmed` — on
    // the twin (2026-09-24) the assistant took "redo the map from scratch"
    // for the confirmation the description asked it to seek, and started.
    if fresh && !args.get("confirmed").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(json!({
            "started": false,
            "needs_confirmation": true,
            "map_name": name,
            "progress": progress,
            "reason": "a new map from nothing replaces the saved one (its named places are lost) when its first session saves: ask the user to confirm, then call again with fresh=true and confirmed=true",
        }));
    }
    if fresh {
        // The live map goes; the saved one stays in the library until the
        // new session saves over it — a new exploration that goes wrong
        // loses nothing.
        map_library(&robot.places.map_socket, crate::map::METHOD_ROBOT_MAP_WIPE, None)?;
        robot.places.explore.fresh_map(&name);
        let frames0 = map.snapshot().frames;
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        while map.snapshot().frames <= frames0 + 1 && Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        tracing::info!(map = name, "map explore: a new map from nothing; the saved one is replaced when this session saves");
    }
    // What the answer reports is the map as it is now — after `fresh`, the
    // new one, not what was read before the wipe.
    let progress = robot.places.explore.status().progress;
    let frame = map.snapshot().latest.clone().unwrap_or(frame);
    let session = Some(crate::explore::Session {
        save_as: name.clone(),
        battery_min_pct: args.get("battery_min_pct").and_then(Value::as_f64).unwrap_or(25.0),
    });
    robot.places.explore.start(
        &robot.places.robotd_socket,
        &robot.places,
        known,
        max_s,
        !robot.places.map_config.ask_phrase.is_empty(),
        robot.places.map_config.turn_sign(),
        robot.places.gait.clone(),
        session,
    )?;
    Ok(json!({
        "started": true,
        "map_name": name,
        "fresh": fresh,
        "progress": progress,
        "max_s": max_s,
        "from": {"x": (frame.x * 100.0).round() / 100.0, "y": (frame.y * 100.0).round() / 100.0},
        "map": {"submaps": frame.n_submaps, "windows": frame.windows},
    }))
}

/// "Esplorazione completata": the running session stopped (and saved, as
/// every session is), the map saved again under its name, declared done
/// with the share it has, and frozen — from here on the duck navigates.
/// Refused while the duck does not know where it is: the mapper will not
/// save a map on a guessed pose.
fn map_explore_complete(robot: &mut Robot, args: &Value) -> Result<Value, String> {
    if robot.places.explore.running() {
        robot.places.explore.request_stop();
        // The session saves itself as it ends (a stand, a leg, the save:
        // up to a minute); declared before it ends, the declaration was
        // written over (house2 on the twin, 2026-09-24).
        let deadline = Instant::now() + std::time::Duration::from_secs(120);
        while robot.places.explore.running() && Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
    let name = match args.get("save_as").and_then(Value::as_str) {
        Some(n) => map_name(&json!({"name": n}))?.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
        None => robot.places.explore.map_name().unwrap_or_else(|| "casa".to_string()),
    };
    map_library(&robot.places.map_socket, crate::mapd::wire::METHOD_ROBOT_MAP_SAVE, Some(json!({"name": name})))?;
    let percent = robot
        .places
        .map
        .as_ref()
        .and_then(|m| m.snapshot().latest.and_then(|f| f.grid().ok()))
        .map_or(0.0, |g| crate::explore::explored_share(&g).0 * 100.0);
    let progress = robot.places.explore.declare_done(&name, percent);
    robot.places.explore.keep_ground();
    let frozen = map_library(&robot.places.map_socket, crate::mapd::wire::METHOD_QUACK_MAP_FREEZE, Some(json!({"on": true}))).is_ok();
    Ok(json!({
        "complete": true,
        "map_name": name,
        "percent_mapped": percent.round(),
        "frozen": frozen,
        "progress": progress,
        "hint": "the map is closed as it is: the duck explores it no more and navigates on it — blind where it knows the floor, guarded where it does not; a new map from nothing only if the user asks for one (robot.map_explore fresh=true)",
    }))
}

/// The name a map is to be saved or loaded under, checked here so an
/// impossible one is refused with an explanation instead of by the daemon.
fn map_name(args: &Value) -> Result<Value, String> {
    let name = quack_duck::body::require_str(args, "name")?.trim().to_string();
    if name.is_empty()
        || name.len() > 64
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("a map name is 1 to 64 letters, digits, '-' or '_'".into());
    }
    Ok(json!({"name": name}))
}

/// One call to the map library on robotd: save, list or load.
///
/// Spelled by method name rather than through a typed `proto::Call`: the
/// three are prototyped on a local robotd branch and asked of upstream in
/// docs/study/upstream-asks.md, and a robotd without them says so.
pub(crate) fn map_library(map_socket: &str, method: &str, params: Option<Value>) -> Result<Value, String> {
    let response = library_request(map_socket, method, params).map_err(|e| format!("the map: {e}"))?;
    if let Some(error) = &response.error {
        return Err(if error.code == proto::code::METHOD_NOT_FOUND {
            "this robot's software has no map library yet (robotd is older than the \
             map_save/map_list/map_load calls, and quack-navd is not hosting the mapper)"
                .to_string()
        } else {
            format!("the map refused {method}: {error}")
        });
    }
    let result = response.result.unwrap_or(Value::Null);
    // `list` answers with the library; `save` and `load` with an intent
    // result, whose refusal is the daemon's own sentence and belongs in
    // front of the caller unchanged.
    if matches!(method, "robot.map_list" | "robot.map_match") {
        return Ok(result);
    }
    match result.get("accepted").and_then(Value::as_bool) {
        Some(true) => Ok(json!({"done": true})),
        _ => Err(result
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("the robot refused")
            .to_string()),
    }
}

/// One request on its own connection to the map socket. The library is
/// asked rarely, and a lane of its own means neither a robotd that came up
/// late nor a map socket that restarted leaves it dead; the timeout is
/// `map_match`'s, which searches every saved map (the host allows 120 s).
fn library_request(path: &str, method: &str, params: Option<Value>) -> anyhow::Result<proto::Response> {
    use std::io::{BufRead, Write};
    let stream = std::os::unix::net::UnixStream::connect(path)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(130)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(3)))?;
    let mut request = json!({"jsonrpc": "2.0", "id": 1, "method": method});
    if let Some(params) = params {
        request["params"] = params;
    }
    let mut line = serde_json::to_vec(&request)?;
    line.push(b'\n');
    (&stream).write_all(&line)?;
    let mut reader = std::io::BufReader::new(&stream);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            anyhow::bail!("the map socket closed the connection");
        }
        // Notifications (a `map.frame` on a shared socket) are skipped.
        if serde_json::from_str::<proto::Request>(&line).is_ok() {
            continue;
        }
        let response: proto::Response = serde_json::from_str(&line)?;
        if response.id == Some(proto::Id::Number(1)) {
            return Ok(response);
        }
    }
}

/// Walk to a known place, or to a point on the map: `robot.go_to`.
fn go_to(robot: &mut Robot, args: &Value) -> Result<Value, String> {
    if args.get("stop").and_then(Value::as_bool).unwrap_or(false) {
        let was_running = robot.places.explore.running();
        robot.places.explore.request_stop();
        return Ok(json!({"stopped": was_running, "explore": robot.places.explore.status().to_json()}));
    }
    if robot.places.explore.running() {
        return Err("the duck is already on its way (or exploring): stop that first".into());
    }
    let Some(map) = &robot.places.map else {
        return Err("this satellite has no map lane ([map] enabled = false)".into());
    };
    let snapshot = map.snapshot();
    let Some(frame) = snapshot.latest.clone() else {
        return Err("no map yet: robotd is unreachable or has not sent a map frame".into());
    };
    if snapshot.trusted_pose().is_none() {
        return Err(if frame.seated {
            "the duck is seated or fallen: stand it up first (sit_toggle)".into()
        } else {
            "the duck is not sure of its position yet: let it stand still and look around first".into()
        });
    }
    // A place by name, or a point.
    let (goal, what) = match args.get("place").and_then(Value::as_str) {
        Some(name) => {
            let place = robot
                .places
                .registry
                .current()
                .find(|p| p.name.eq_ignore_ascii_case(name.trim()))
                .ok_or_else(|| {
                    let known: Vec<&str> = robot.places.registry.current().map(|p| p.name.as_str()).collect();
                    if known.is_empty() {
                        format!("the duck knows no place called `{name}`, and no places at all yet")
                    } else {
                        format!("the duck knows no place called `{name}`; it knows: {}", known.join(", "))
                    }
                })?;
            let anchor = place
                .anchors
                .first()
                .ok_or_else(|| format!("`{name}` has no anchor on this map yet"))?;
            ((anchor.x, anchor.y), format!("`{}`", place.name))
        }
        None => {
            let (x, y) = (quack_duck::body::number(args, "x"), quack_duck::body::number(args, "y"));
            if args.get("x").is_none() || args.get("y").is_none() {
                return Err("give a `place` name, or `x` and `y` in map metres".into());
            }
            ((x, y), format!("({x:.2}, {y:.2})"))
        }
    };
    // Is there a way there at all? Answer now, not after a minute of walking.
    let grid = frame.grid().map_err(|e| e.to_string())?;
    let pose = frame.pose();
    if crate::frontier::path_to(&grid, pose.0, pose.1, goal, &[], crate::frontier::INFLATE_M, &[]).is_none() {
        return Err(format!(
            "no way to {what} on the map the duck has: it may be in a part it has not mapped yet — explore first"
        ));
    }
    quack_duck::body::with_robot(&mut robot.control)?;
    let max_s = args.get("max_s").and_then(Value::as_f64).unwrap_or(300.0).clamp(10.0, 1800.0);
    robot.places.explore.start_goto(
        &robot.places.robotd_socket,
        &robot.places,
        goal,
        max_s,
        robot.places.map_config.turn_sign(),
        robot.places.gait.clone(),
    )?;
    let away = ((goal.0 - pose.0).powi(2) + (goal.1 - pose.1).powi(2)).sqrt();
    Ok(json!({
        "started": true, "to": what, "goal": {"x": goal.0, "y": goal.1},
        "straight_line_m": (away * 100.0).round() / 100.0, "max_s": max_s,
    }))
}

/// How far a timed move actually carries the body forward: about 0.4 of
/// the commanded distance straight on, a quarter of that in a full arc
/// (vyaw ≥ 0.7), measured on the MuJoCo twin.
pub struct StepPlan {
    pub params: proto::MoveParams,
    pub walk_s: f64,
    pub stop_s: f64,
    pub shortened: Option<String>,
    pub steered: Option<&'static str>,
}

/// How long a thing seen beside the body counts as a passage boundary:
/// one full head sweep (±0.9 rad over 6 s) and a little.
const SIDE_MEMORY: Duration = Duration::from_secs(8);

/// A passage leg is short and re-measured: the boundaries are sampled
/// 0.3 m ahead, and a leg longer than that walks past its own evidence.
const PASSAGE_LEG_MAX_S: f64 = 1.5;

/// The passage the next leg would walk, if there is one: boundaries on
/// both sides within `PASSAGE_MAX_M`, from the map's walls and from what
/// the sensor saw beside the leg (drops and obstacles), at the body and
/// `AHEAD_M` on.
fn passage_here(
    first: &crate::map::MapFrame,
    cliff: Option<&crate::cliff::CliffStatus>,
    now: Instant,
) -> Option<crate::passage::Passage> {
    use crate::map::Blocked;
    use crate::passage::{AHEAD_M, PASSAGE_MAX_M, Sides, passage, side_of};
    let half_pi = std::f64::consts::FRAC_PI_2;
    // Mapped walls, at the body and ahead — only with a pose the map vouches for.
    let mut near = Sides::open();
    let mut far = Sides::open();
    if first.tracking
        && let Ok(grid) = first.grid()
    {
        // A mapped wall bounds the side; open floor leaves it open; the
        // unknown says nothing — and ahead, where the map thins out, a
        // wall beside the body is taken to go on (a young map has inked
        // the wall at the body and not yet the metre past it).
        let side = |x: f64, y: f64, heading: f64| {
            let c = grid.clearance(x, y, heading, PASSAGE_MAX_M);
            match c.by {
                Blocked::Wall => (Some(c.free_m), false),
                Blocked::Open => (None, false),
                Blocked::Unknown | Blocked::Edge => (None, true),
            }
        };
        let (nl, _) = side(first.x, first.y, first.yaw + half_pi);
        let (nr, _) = side(first.x, first.y, first.yaw - half_pi);
        near = Sides { left: nl, right: nr };
        let (ax, ay) = (first.x + AHEAD_M * first.yaw.cos(), first.y + AHEAD_M * first.yaw.sin());
        let (fl, ul) = side(ax, ay, first.yaw + half_pi);
        let (fr, ur) = side(ax, ay, first.yaw - half_pi);
        far = Sides {
            left: fl.or(if ul { nl } else { None }),
            right: fr.or(if ur { nr } else { None }),
        };
    }
    // What the sensor saw beside the leg: a drop is a boundary as much as
    // a wall is, and the map cannot show it.
    if let Some(s) = cliff {
        // Two buckets by forward distance, so the skew of the heading
        // against the boundaries can be read from the sensor alone.
        let mut seen_near = Sides::open();
        let mut seen_far = Sides::open();
        // A side boundary is remembered for a whole head sweep, not the
        // guard's three seconds: the sweep takes six to look both ways,
        // and a wall beside the body is as long as the leg is short.
        for f in s.recent.iter().filter(|f| now.duration_since(f.at) <= SIDE_MEMORY && !f.moving) {
            let things = f
                .drops
                .iter()
                .map(|d| (d.range_m, d.bearing))
                .chain(f.obstacles.iter().map(|o| (o.range_m, o.bearing)));
            for (range, bearing) in things {
                if let Some((left, lateral)) = side_of(range, bearing, AHEAD_M) {
                    let bucket = if range * bearing.cos() < AHEAD_M / 2.0 { &mut seen_near } else { &mut seen_far };
                    let slot = if left { &mut bucket.left } else { &mut bucket.right };
                    *slot = Some(slot.map_or(lateral, |v: f64| v.min(lateral)));
                }
            }
        }
        // a side seen in one bucket only is taken to go on into the other
        let fill = |a: Sides, b: Sides| Sides { left: a.left.or(b.left), right: a.right.or(b.right) };
        near = near.nearest(fill(seen_near, seen_far));
        far = far.nearest(fill(seen_far, seen_near));
    }
    let p = passage(near, far, AHEAD_M);
    tracing::info!(
        near = ?near,
        far = ?far,
        passage = ?p.map(|p| (p.width_m, p.offset_m, p.skew_rad, p.vyaw, p.spare_m)),
        "map step: sides"
    );
    p
}

/// The guards of `robot.map_step`, on the newest map frame and the cliff
/// guard's view, without touching the robot.
pub fn plan_step(
    args: &Value,
    first: &crate::map::MapFrame,
    cliff: Option<&crate::cliff::CliffStatus>,
    gait: &quack_duck::gait::GaitConfig,
    now: Instant,
) -> Result<StepPlan, String> {
    let mut walk_s = quack_duck::body::number(args, "walk_s").clamp(0.0, quack_duck::body::MAX_MOVE_DURATION_S);
    // A step cut down to the floor in front of it, and what limited it.
    let mut shortened: Option<String> = None;
    let stop_s = args
        .get("stop_s")
        .and_then(Value::as_f64)
        .unwrap_or(quack_duck::body::DEFAULT_STOP_S)
        .clamp(0.0, quack_duck::body::MAX_STOP_S);
    if first.seated {
        return Err("the duck is seated or fallen: stand it up first (sit_toggle)".into());
    }
    // A passage — two boundaries close on either side of the leg, mapped
    // walls or drops and things the sensor saw — changes what a forward
    // step is judged by and how it is steered. See `crate::passage`.
    let vx0 = quack_duck::body::clamp(quack_duck::body::number(args, "vx"), quack_duck::body::MAX_SPEED_M_S);
    let passage_plan = if walk_s > 0.0 && vx0 > 0.0 && quack_duck::body::number(args, "vyaw").abs() < 0.6 {
        passage_here(first, cliff, now)
    } else {
        None
    };
    if let Some(p) = passage_plan.as_ref()
        && !p.fits()
    {
        let (turn, why) = if p.width_m < 2.0 * (crate::passage::BODY_HALF_M + crate::passage::LEG_DRIFT_M) {
            ("", "too narrow for the body and one leg's drift")
        } else if p.skew_rad.abs() > 0.1 {
            (if p.skew_rad > 0.0 { "right" } else { "left" }, "the heading is skewed into a boundary")
        } else {
            (if p.offset_m > 0.0 { "left" } else { "right" }, "the body is off the middle")
        };
        return Err(format!(
            "a passage {:.2} m wide: {why} — the steered leg does not fit{}{}",
            p.width_m,
            if p.offset_m.abs() > 0.03 {
                format!(" (the middle is {:.2} m to the {})", p.offset_m.abs(), if p.offset_m > 0.0 { "left" } else { "right" })
            } else {
                String::new()
            },
            if turn.is_empty() { "; back away".to_string() } else { format!("; turn in place {turn} first (a kick, then vyaw only), then try again") }
        ));
    }
    if passage_plan.is_some() && walk_s > PASSAGE_LEG_MAX_S {
        walk_s = PASSAGE_LEG_MAX_S;
        shortened = Some("a passage: short legs, re-measured each time".into());
    }
    // Never walk into a mapped wall: the grid is right here. Unknown
    // territory is allowed (that is what exploring is), walls are not.
    // The map's word needs a pose the map vouches for; the depth sensor's
    // does not, and it is the one guard a wake-up has: on a saved map
    // the pose is unconfirmed until the duck has walked, and a wake-up
    // that walked unguarded pinned itself against a bed (2026-09-14).
    let vx = quack_duck::body::clamp(quack_duck::body::number(args, "vx"), quack_duck::body::MAX_SPEED_M_S);
    if walk_s > 0.0 && vx.abs() > 0.0 {
        let heading = first.yaw + if vx < 0.0 { std::f64::consts::PI } else { 0.0 };
        let grid = if first.tracking { first.grid().ok() } else { None };
        let ahead = grid
            .as_ref()
            .map(|g| g.clearance(first.x, first.y, heading, crate::tools::LOOK_M));
        // The gait covers roughly a third of the commanded distance — far
        // less in a tight arc, which is how this duck turns (11 cm forward
        // in a 4 s arc at vyaw 0.7, measured) — the duck is about 15 cm
        // from its centre to its beak, and a step must not end inside the
        // wall margin.
        let advance = quack_duck::body::step_advance_m(vx, quack_duck::body::number(args, "vyaw"), walk_s);
        // A tight arc is how the duck turns: it advances a few centimetres,
        // and gets a smaller margin so it can turn away from what is close.
        // In a doorway (`gap`, the explorer's word) the margins shrink to
        // what a doorway allows: the body itself plus a little.
        let gap = args.get("gap").and_then(Value::as_bool).unwrap_or(false) || passage_plan.is_some();
        let margin = if quack_duck::body::number(args, "vyaw").abs() >= 0.6 {
            ARC_MARGIN_M
        } else if gap {
            GAP_MARGIN_M
        } else {
            wall_margin_m()
        };
        let slack = if gap { GAP_SLACK_M } else { STEP_SLACK_M };
        let lane = if gap { GAP_LANE_HALF_M } else { LANE_HALF_M };
        let needed = advance + slack + margin;
        let side = if vx < 0.0 { "behind" } else { "ahead" };
        // What the depth sensor itself says about this direction.
        let heading_rel = if vx < 0.0 { std::f64::consts::PI } else { 0.0 };
        let (looked_ahead, obstacle) = match cliff {
            Some(s) => (
                s.looked_at(now, heading_rel),
                s.obstacle_in_lane(now, heading_rel, lane),
            ),
            None => (false, None),
        };
        let sides = match grid.as_ref() {
            Some(g) => {
                let c = crate::tools::clearance_json(g, first.pose());
                format!(
                    "clearance left {} m, right {} m, behind {} m",
                    c["left"]["free_m"], c["right"]["free_m"], c["behind"]["free_m"]
                )
            }
            None => "the pose is unconfirmed, so the map cannot say what is beside".to_string(),
        };
        use crate::map::Blocked;
        // The nearer of what the sensor sees and the mapped wall, when the
        // step as asked would end inside the margin.
        let mut limit: Option<(f64, String)> = None;
        if let Some(o) = obstacle
            && o.range_m < needed
        {
            limit = Some((
                o.range_m,
                format!(
                    "the depth sensor sees something {:.2} m {side}, {:.0}° {}",
                    o.range_m,
                    o.bearing.to_degrees().abs(),
                    if o.bearing >= 0.0 { "left" } else { "right" },
                ),
            ));
        }
        if let Some(ahead) = ahead.as_ref()
            && ahead.by == Blocked::Wall
            && ahead.free_m < needed
            && limit.as_ref().is_none_or(|(f, _)| ahead.free_m < *f)
        {
            limit = Some((ahead.free_m, format!("a wall is {:.2} m {side}", ahead.free_m)));
        }
        if let Some((free, what)) = limit {
            // Rather than refuse, walk as far as the floor allows: the
            // useful distance, proportionally. Below a second of walking
            // the gait does not move at all, and then the answer is "turn".
            let per_s = quack_duck::body::step_advance_m(vx, quack_duck::body::number(args, "vyaw"), 1.0);
            let fit_s = (free - slack - margin) / per_s;
            if fit_s >= MIN_STEP_S {
                walk_s = fit_s.min(walk_s);
                shortened = Some(what);
            } else {
                return Err(format!(
                    "{what}: this step would end closer than {margin} m to it, turn first ({sides})"
                ));
            }
        }
        if let Some(ahead) = ahead.as_ref() {
            match ahead.by {
                // Unknown right at the beak: the map cannot say. The depth
                // sensor can, if it has looked this way lately — a wall it
                // reports is refused below; open floor it reports is walkable.
                // Without a recent look, stand first: the twin toppled pushing
                // into a wall the map had not inked.
                Blocked::Unknown | Blocked::Edge if ahead.free_m < 0.15 && !looked_ahead => {
                    return Err(format!(
                        "only {:.2} m of known floor {side} and then unmapped space, and the depth \
                         sensor has not looked that way yet: stand still first (walk_s 0) so the \
                         head sweep sees it, or turn ({sides})",
                        ahead.free_m
                    ));
                }
                _ => {}
            }
        }
        // Unconfirmed pose and nothing seen this way yet: the map cannot
        // say and the sensor has not looked. Stand first — the sweep looks.
        if grid.is_none() && !looked_ahead && vx > 0.0 {
            return Err(format!(
                "the pose is unconfirmed and the depth sensor has not looked {side} yet: stand \
                 still first (walk_s 0) so the head sweep sees it, or turn"
            ));
        }
    }
    // A drop the map cannot see: the cliff guard's word is final for a
    // forward step (it looks where the head looks; backing up is blind and
    // stays the agent's risk, said so in the result).
    if walk_s > 0.0
        && vx > 0.0
        && let Some(s) = cliff
    {
        let vyaw = quack_duck::body::number(args, "vyaw");
        let arc = vyaw.abs() >= 0.5;
        // An arc is judged twice: along the heading it starts on for its
        // first second — the body goes that way before the turn takes,
        // and a twin with a hole 0.2 m off its beak turned "away" into
        // it — and along the heading it ends on, so turning toward a drop
        // beside the path is refused while walking past it is not. A
        // straight step is judged along the heading for its whole advance.
        // (heading, distance needed, whether an arc turning away from the
        // drop is let through: on the starting heading only once the edge
        // is a body's length past the first stretch — the fall case had it
        // under the beak; on the ending heading when the drop is off to
        // the side).
        // The margin from an edge: the guard's own, or the one the leg
        // asks for — a passage leg (the explorer aligned on the axis,
        // short, re-measured at every stand) asks for less, because the
        // guard's 0.25 plus the flank left 7 cm of admissible floor in
        // the 0.6 m passage beside the stairwell and the route ran
        // outside it (rim5/rim6, 2026-09-17). Never under the floor.
        let margin = args
            .get("cliff_margin_m")
            .and_then(Value::as_f64)
            .map_or(cliff_margin_m(), |m| m.max(CLIFF_MARGIN_FLOOR_M));
        let mut checks: Vec<(f64, f64, bool)> = Vec::new();
        if arc {
            checks.push((0.0, ARC_FIRST_M + 0.15 + ARC_FIRST_MARGIN_M, false));
            checks.push((
                quack_duck::body::YAW_RATE_PER_UNIT * vyaw * walk_s,
                quack_duck::body::step_advance_m(vx, vyaw, walk_s) + 0.15 + margin,
                true,
            ));
        } else {
            checks.push((0.0, quack_duck::body::step_advance_m(vx, vyaw, walk_s) + 0.15 + margin, false));
        }
        // A passage leg — short, straight, the explorer aligned on the
        // axis, and a wall the sensor sees within CLIFF_WALL_NEAR_M of the
        // body's side — is judged with a narrower lane: the body is held
        // against the wall, and the 0.22 m lane made a 0.54 m passage a
        // 5 cm affair. The wall must be seen; the flag alone changes
        // nothing.
        let self_passage = passage_plan.is_some() && !arc;
        let passage = args.get("passage").and_then(Value::as_bool).unwrap_or(false)
            && !arc
            && walk_s <= 2.0
            && s.recent.iter().any(|f| {
                !f.moving && now.duration_since(f.at) <= crate::cliff::MEMORY
                    && f.obstacles.iter().any(|o| {
                        o.bearing.abs() > 0.5 && o.range_m * o.bearing.sin().abs() <= CLIFF_WALL_NEAR_M && o.range_m * o.bearing.cos() <= 0.6
                    })
            });
        if let Some(p) = passage_plan.as_ref()
            && self_passage
        {
            // A passage leg is judged where it will actually go: the leg
            // as the passage law steers it, rolled out with the measured
            // gait, against every drop the sensor holds. The straight lane
            // along the current heading refused the stairwell lane the
            // moment the heading was a few degrees off toward the hole —
            // which is the very error the law corrects (2026-09-15).
            let keep = crate::passage::BODY_HALF_M + crate::passage::LEG_DRIFT_M - 0.01;
            let mut t = 0.0;
            let (mut px, mut py, mut h) = (0.0_f64, 0.0_f64, 0.0_f64);
            let mut path: Vec<(f64, f64)> = vec![(0.0, 0.0)];
            while t < walk_s {
                h += p.vyaw * quack_duck::body::YAW_RATE_PER_UNIT * 0.1;
                px += 0.12 * 0.1 * h.cos();
                py += 0.12 * 0.1 * h.sin();
                path.push((px, py));
                t += 0.1;
            }
            // and the stretch the body coasts past the end of the leg
            for k in 1..=3 {
                path.push((px + 0.05 * k as f64 * h.cos(), py + 0.05 * k as f64 * h.sin()));
            }
            let nearest = s
                .recent
                .iter()
                .filter(|f| now.duration_since(f.at) <= crate::cliff::MEMORY && !f.moving)
                .flat_map(|f| f.drops.iter())
                .map(|d| {
                    let (dx, dy) = (d.range_m * d.bearing.cos(), d.range_m * d.bearing.sin());
                    path.iter().map(|(qx, qy)| (dx - qx).hypot(dy - qy)).fold(f64::INFINITY, f64::min)
                })
                .fold(f64::INFINITY, f64::min);
            if nearest < keep {
                return Err(format!(
                    "a passage {:.2} m wide, but a drop lies {:.2} m off the steered leg — closer than \
                     the body and a leg's drift ({:.2} m): stand and look, or back away",
                    p.width_m, nearest, keep
                ));
            }
        } else {
        let lane = if passage { CLIFF_LANE_PASSAGE_M } else { CLIFF_LANE_HALF_M };
        for (heading, needed, end_heading) in checks {
            let Some(drop) = s.drop_in_lane(now, heading, lane) else {
                continue;
            };
            // An arc turning away from a drop is how the duck leaves it:
            // let that one through, under the conditions above.
            let away = arc && vyaw.signum() != drop.bearing.signum();
            let turning_away = away
                && if end_heading {
                    drop.bearing.abs() > CLIFF_AWAY_RAD
                } else {
                    drop.edge_min_m >= ARC_FIRST_M + 0.15
                };
            // The edge is somewhere in [edge_min_m, range_m]: plan on the near end.
            if drop.edge_min_m < needed && !turning_away {
                let c = first
                    .grid()
                    .map(|g| crate::tools::clearance_json(&g, first.pose()))
                    .unwrap_or(Value::Null);
                return Err(format!(
                    "a drop — stairs or a hole — begins {:.2}–{:.2} m ahead, {:.0}° {}: the map cannot \
                     show it; do not walk this way. Back away first with robot.move (vx -0.3, and \
                     vyaw 0.5 if the duck does not move straight back), then turn (clearance left \
                     {} m, right {} m, behind {} m)",
                    drop.edge_min_m,
                    drop.range_m,
                    drop.bearing.to_degrees().abs(),
                    if drop.bearing >= 0.0 { "left" } else { "right" },
                    c["left"]["free_m"],
                    c["right"]["free_m"],
                    c["behind"]["free_m"],
                ));
            }
        }
        }
    }
    // A wall hugging one side (only mapped walls count) peels the step
    // away from it; the agent is told which way it was steered.
    let mut params = quack_duck::body::move_params(args);
    let mut steered: Option<&str> = None;
    // `steer: false`: the caller steers by its own account of the sides —
    // the explorer's passage beside a drop keeps to the middle between a
    // mapped wall and a hole the map cannot show, and the wall-hug below,
    // seeing only the wall, peeled the step straight into the hole.
    let hands_off = args.get("steer").and_then(Value::as_bool) == Some(false);
    if let Some(p) = passage_plan.as_ref()
        && !hands_off
    {
        // In a passage the passage steers: back to the middle and along
        // the axis, whatever the caller's aim — the aim is beyond the
        // passage, and the passage is the only way there.
        params.vyaw = p.vyaw;
        steered = Some("passage");
    } else if walk_s > 0.0
        && vx > 0.0
        && !hands_off
        && first.tracking
        && let Ok(grid) = first.grid()
    {
        use crate::map::Blocked;
        let look = crate::tools::LOOK_M;
        let left = grid.clearance(
            first.x,
            first.y,
            first.yaw + std::f64::consts::FRAC_PI_2,
            look,
        );
        let right = grid.clearance(
            first.x,
            first.y,
            first.yaw - std::f64::consts::FRAC_PI_2,
            look,
        );
        let left_hug = left.by == Blocked::Wall && left.free_m < HUG_M;
        let right_hug = right.by == Blocked::Wall && right.free_m < HUG_M;
        let walled_in = left.by == Blocked::Wall
            && right.by == Blocked::Wall
            && left.free_m + right.free_m < CENTER_SPAN_M;
        // Centring is the explorer's: its legs ask for it (`centre`). A
        // driver steering from outside — an agent, a person — gets only
        // the wall-hug below; the centring on top of their own steering
        // read as a stranger's hand on the wheel and broke the guided
        // tour's return leg twice.
        let centre = args.get("centre").and_then(Value::as_bool).unwrap_or(false);
        if params.vyaw.abs() >= 0.6 {
            // A tight arc is a turn: leave it alone.
        } else if walled_in && centre {
            // Walls on both sides within reach: keep to the middle. More
            // room on the left means the right wall is the near one.
            // The correction is relative to the passage: against one wall
            // of a 0.4 m corridor is as far off the middle as it gets.
            let span = (left.free_m + right.free_m).max(CENTER_MIN_SPAN_M);
            let off = left.free_m - right.free_m;
            if off.abs() > CENTER_DEADBAND_M {
                let add = (CENTER_GAIN * off / span).clamp(-HUG_STEER_RAD_S, HUG_STEER_RAD_S);
                params.vyaw = (params.vyaw + add).clamp(-quack_duck::body::MAX_YAW_RAD_S, quack_duck::body::MAX_YAW_RAD_S);
                steered = Some(if add > 0.0 { "left" } else { "right" });
            }
        } else if left_hug && !(right_hug && right.free_m <= left.free_m) {
            params.vyaw = (params.vyaw - HUG_STEER_RAD_S).clamp(-quack_duck::body::MAX_YAW_RAD_S, quack_duck::body::MAX_YAW_RAD_S);
            steered = Some("right");
        } else if right_hug {
            params.vyaw = (params.vyaw + HUG_STEER_RAD_S).clamp(-quack_duck::body::MAX_YAW_RAD_S, quack_duck::body::MAX_YAW_RAD_S);
            steered = Some("left");
        }
    }
    Ok(StepPlan {
        params: quack_duck::body::trimmed(gait, params),
        walk_s,
        stop_s,
        shortened,
        steered,
    })
}

fn map_step(robot: &mut Robot, args: &Value) -> Result<Value, String> {
    let Some(map) = robot.places.map.clone() else {
        return Err("this satellite has no map lane ([map] enabled = false)".into());
    };
    let before = map.snapshot();
    match &before.support {
        crate::map::MapSupport::Supported { enabled: true, .. } => {}
        crate::map::MapSupport::Supported { .. } => {
            return Err("mapping is disabled on this robot ([maploc] in robotd.toml)".into());
        }
        crate::map::MapSupport::Unsupported => {
            return Err("this robot's software has no map (robotd predates the map API)".into());
        }
        crate::map::MapSupport::Unknown => {
            return Err("no map yet: robotd is unreachable or has not answered".into());
        }
    }
    let Some(first) = before.latest.clone() else {
        return Err("no map frame yet; wait a moment and check robot.map_status".into());
    };
    quack_duck::body::with_robot(&mut robot.control)?;
    let cliff_now = robot.places.cliff.as_ref().map(|c| c.snapshot());
    let plan = plan_step(args, &first, cliff_now.as_ref(), &robot.places.gait, Instant::now())?;
    let StepPlan { params, walk_s, stop_s, shortened, steered } = plan;
    if walk_s > 0.0 {
        // The heading held on a walking leg that is not an arc: the
        // steering asked for (before the trim) is what the hold means to
        // integrate.
        // Straight legs only: on a steered leg the hold fought the
        // steering (house4tour, 2026-09-18: 634 s for a 472–499 s tour).
        // A leg that asks for it (`hold`, with `hold_bias` radians to
        // aim off the starting heading — the passage law rejoining its
        // line), or every straight leg when QK_HOLD_HEADING=1.
        let vyaw_cmd = quack_duck::body::number(args, "vyaw");
        let asked = args.get("hold").and_then(Value::as_bool).unwrap_or(false);
        let bias = quack_duck::body::number(args, "hold_bias");
        // The yaw the hold closes on is the cliff guard's odometry
        // heading, handed over as a closure so the body's lane knows
        // nothing of the guard (the split of 2026-09-22).
        let yaw_now = robot.places.cliff.clone().map(|c| move || c.snapshot().odom_yaw);
        let hold: Option<(&dyn Fn() -> Option<f64>, f64, f64)> =
            match (asked || (quack_duck::body::hold_heading() && vyaw_cmd.abs() < quack_duck::body::HOLD_STRAIGHT_MAX), &yaw_now) {
                (true, Some(f)) => Some((f, 0.0, bias)),
                _ => None,
            };
        quack_duck::body::timed_move_held(&mut robot.control, params, walk_s, hold)?;
    }
    std::thread::sleep(Duration::from_secs_f64(stop_s));
    let after = map.snapshot();
    let last = after.latest.clone().unwrap_or(first.clone());
    let new_windows = last.windows.saturating_sub(first.windows);
    let clearance = last
        .grid()
        .map(|g| crate::tools::clearance_json(&g, last.pose()))
        .unwrap_or(Value::Null);
    let hug_hint = last.grid().ok().and_then(|g| {
        use crate::map::Blocked;
        let look = crate::tools::LOOK_M;
        let l = g.clearance(last.x, last.y, last.yaw + std::f64::consts::FRAC_PI_2, look);
        let r = g.clearance(last.x, last.y, last.yaw - std::f64::consts::FRAC_PI_2, look);
        match (
            l.by == Blocked::Wall && l.free_m < HUG_M,
            r.by == Blocked::Wall && r.free_m < HUG_M,
        ) {
            (true, false) => Some("a wall is close on the left: steer right next (vyaw negative)"),
            (false, true) => Some("a wall is close on the right: steer left next (vyaw positive)"),
            (true, true) => {
                Some("walls close on both sides: a narrow passage, go straight and slowly")
            }
            _ => None,
        }
    });
    let hint = if last.seated {
        "the duck fell (or sat) during this step: robotd stands it back up on its own; wait, then check robot.map_status"
    } else if !last.tracking {
        "the duck lost its position during this step: let it stand until robot.map_status says it is tracking again"
    } else if let Some(hug) = hug_hint {
        hug
    } else if stop_s < 3.0 {
        "the stop was too short to reach the map; stand at least six seconds"
    } else if new_windows == 0 {
        "this stop added nothing: the duck may still have been moving when the stand began, or the mapper was busy — stand a little longer next time"
    } else {
        "this stop reached the map"
    };
    Ok(json!({
        "walked_s": walk_s,
        "stood_s": stop_s,
        "new_windows": new_windows,
        "windows": last.windows,
        "submaps": last.n_submaps,
        "loops": last.n_loops,
        "tracking": last.tracking,
        "pose": {
            "x": (last.x * 100.0).round() / 100.0,
            "y": (last.y * 100.0).round() / 100.0,
            "yaw": (last.yaw * 100.0).round() / 100.0
        },
        "clearance": clearance,
        "cliff": crate::tools::cliff_json(robot.places.cliff.as_ref(), Instant::now()),
        "steered": steered,
        "shortened": shortened,
        "hint": hint,
    }))
}
