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
use crate::places::{Look, MAX_RADIUS_M, MIN_RADIUS_M, PlaceState, Registry};

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

/// The maps the explore job saved since the registry last heard.
fn take_saves(explore: &crate::explore::ExploreHandle, registry: &mut Registry) {
    for name in explore.take_saved() {
        if let Err(e) = registry.saved_as(&name) {
            tracing::warn!(error = %format!("{e:#}"), "places registry not saved");
        }
    }
}

/// The live map started from nothing when the lane had seen `frames`.
fn started_afresh(explore: &crate::explore::ExploreHandle, registry: &mut Registry, frames: u64) {
    take_saves(explore, registry);
    if let Err(e) = registry.started_afresh(frames) {
        tracing::warn!(error = %format!("{e:#}"), "places registry not saved");
    }
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
                tracing::warn!(error = %format!("{e:#}"), "robotd unreachable — the navigation runs without the robot");
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
                tracing::warn!(
                    error = %format!("{e:#}"),
                    path = %config.places_path,
                    "places registry unusable — places will not persist; fix or move the file, or set [map] places_path"
                );
                Registry::in_memory()
            }
        };
        let cliff = (config.enabled && config.cliff_guard)
            .then(|| CliffWatch::spawn(config.tof_socket.clone(), robotd_socket.to_owned()));
        Self {
            map,
            registry,
            cliff,
            explore: crate::explore::ExploreHandle::new().with_ground(&config.places_path).with_oracle_book(),
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

    /// Bring the registry up to date before it is asked anything: the
    /// maps the explore job saved since, and what the map lane says now.
    pub fn fold(&mut self) {
        take_saves(&self.explore, &mut self.registry);
        if let Some(look) = self.map.as_ref().and_then(|m| Look::of(&m.snapshot()))
            && let Err(e) = self.registry.observe(look)
        {
            tracing::warn!(error = %format!("{e:#}"), "places registry not saved");
        }
    }

    /// The live map is now the saved map `name` (loaded or adopted). The
    /// lane is not folded in first: the reset this caused may already be
    /// in it, and it is this one's, not news.
    pub fn map_loaded(&mut self, name: &str) {
        take_saves(&self.explore, &mut self.registry);
        let frames = self.lane_frames();
        if let Err(e) = self.registry.loaded(name, frames) {
            tracing::warn!(error = %format!("{e:#}"), "places registry not saved");
        }
    }

    /// The live map starts from nothing (a wipe, a fresh exploration, the
    /// boot's search).
    pub fn map_started_afresh(&mut self) {
        let frames = self.lane_frames();
        started_afresh(&self.explore, &mut self.registry, frames);
    }

    /// The live map was saved to the library as `name`.
    pub fn map_saved(&mut self, name: &str) {
        take_saves(&self.explore, &mut self.registry);
        if let Err(e) = self.registry.saved_as(name) {
            tracing::warn!(error = %format!("{e:#}"), "places registry not saved");
        }
    }

    fn lane_frames(&self) -> u64 {
        self.map.as_ref().map_or(0, |m| m.snapshot().frames)
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
                    "radius_m": {"type": "number", "description": "how far from this spot still counts as the place; default 1.5 m", "minimum": MIN_RADIUS_M, "maximum": MAX_RADIUS_M},
                    "x": {"type": "number", "description": "only from a map view: teach a point on the map (map metres, with y) instead of where the duck stands; it must be mapped floor"},
                    "y": {"type": "number", "description": "only from a map view, with x"}
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
        where the duck is now when that is known, and the saved map it belongs to. `state`: \
        usable; pending (the duck has not found itself on that map yet, e.g. just switched on — \
        wait, do not teach it again); other_map (another map is loaded: it comes back with its own); \
        stale (its map was wiped or replaced by a new one: it needs teaching again).",
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
            if args.get("x").is_some() || args.get("y").is_some() {
                return remember_at_point(places, name, args, radius);
            }
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
            places.fold();
            let here = located(places).ok().map(|fix| fix.pose);
            let listed: Vec<Value> = places
                .registry
                .places()
                .iter()
                .map(|place| {
                    let state = places.registry.state(place);
                    json!({
                        "name": place.name,
                        "anchors": place.anchors.len(),
                        "radius_m": place.radius_m,
                        // `state` says it all: usable, pending (the duck has
                        // not found itself on the place's map yet), other_map
                        // (another saved map is live), stale (its map is
                        // gone). `stale` is the old flag: not on this map.
                        "state": state.as_str(),
                        "stale": state.is_stale(),
                        "map": places.registry.map_of(place),
                        // Where `robot.go_to` takes the duck: the first
                        // anchor, in map metres (a map view pins it there).
                        "at": place.anchors.first().map(|a| json!({"x": round2(a.x), "y": round2(a.y)})),
                        "distance_m": here
                            .filter(|_| state == PlaceState::Usable)
                            .map(|(x, y, _)| round2(place.distance_to(x, y))),
                    })
                })
                .collect();
            Ok(json!({"places": listed, "position_known": here.is_some(), "live_map": places.registry.live_map()}))
        }
        "robot.map_status" => map_status(places),
        other => Err(format!("unknown tool `{other}`")),
    }
}

/// Teach `name` at a point on the map (`x`, `y` in map metres) rather than
/// where the duck stands: what a map view offers by a tap. The point must
/// be floor the live map knows — not a wall, not unexplored, not off the
/// grid — and the map must be the registry's current one; the duck's own
/// position does not matter.
fn remember_at_point(places: &mut Places, name: &str, args: &Value, radius: Option<f64>) -> Result<Value, String> {
    let (Some(x), Some(y)) = (args.get("x").and_then(Value::as_f64), args.get("y").and_then(Value::as_f64)) else {
        return Err("x and y go together, in map metres".into());
    };
    places.fold();
    let Some(map) = &places.map else {
        return Err("this satellite has no map lane ([map] enabled = false)".into());
    };
    let status = map.snapshot();
    let Some(frame) = &status.latest else {
        return Err("no map yet: robotd is unreachable or has not sent a map frame".into());
    };
    let grid = frame.grid().map_err(|e| e.to_string())?;
    match grid.at(x, y) {
        Some(crate::map::Cell::Free) => {}
        Some(crate::map::Cell::Wall) => return Err(format!("({x:.2}, {y:.2}) is a wall on the map: pick a point on the floor")),
        Some(crate::map::Cell::Unknown) | None => {
            return Err(format!("({x:.2}, {y:.2}) is not mapped floor: pick a point the map knows"));
        }
    }
    let place = places
        .registry
        .remember(name, (x, y, 0.0), radius)
        .map_err(|e| format!("cannot remember `{name}`: {e}"))?;
    Ok(json!({
        "remembered": place.name,
        "anchors": place.anchors.len(),
        "radius_m": place.radius_m,
        "pose": pose_json((x, y, 0.0)),
        "at_point": true,
    }))
}

/// The map's state in numbers and in one line of advice — what a tour
/// narrates between steps.
fn map_status(places: &mut Places) -> Result<Value, String> {
    places.fold();
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
    let own = places.explore.self_started().map(|why| {
        format!("the duck is moving on its own: {why}. robot.go_to with stop=true stops it, and it does not start again on its own until a job is asked for")
    });
    let hint = if frame.seated {
        "the duck is seated or fallen: stand it up before mapping (nothing is mapped from the floor)"
    } else if let Some(own) = own.as_deref() {
        own
    } else if frame.untrusted {
        "the duck may have been moved while it rested: the next go_to (or exploration) first walks and looks until it finds where it is, then sets out"
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
        // A long idle stand: the pose carried by odometry, a window judged
        // against the map now and then (`rest_watch`, null before any).
        "resting": frame.resting,
        "rest_watch": frame.rest_watch,
        // Maybe moved while it rested: the next job finds the pose first.
        "untrusted": frame.untrusted,
        "seated": frame.seated,
        "windows": frame.windows,
        "submaps": frame.n_submaps,
        "loops": frame.n_loops,
        "cells": {"free": free, "wall": wall, "size_m": round2(frame.cell_m as f64)},
        "pose": pose_json(frame.pose()),
        // One standard deviation; along the major axis the pose is least
        // sure (null while lost, or from a mapper that keeps none).
        "pose_uncertainty": frame.pose_sigma.map(|s| json!({
            "xy_m": round2(s.xy_major_m),
            "xy_minor_m": round2(s.xy_minor_m),
            "along_deg": s.major_axis_deg.round(),
            "yaw_deg": (s.yaw_deg * 10.0).round() / 10.0,
        })),
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
/// the registry first so a change of map is noticed before any name is
/// matched.
fn locate(places: &mut Places) -> Result<Fix, NoFix> {
    places.fold();
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
    let count = |wanted: fn(PlaceState) -> bool| {
        places.registry.places().iter().filter(|p| wanted(places.registry.state(p))).count()
    };
    let stale = count(PlaceState::is_stale);
    let pending = count(|s| s == PlaceState::Pending);
    Ok(json!({
        "known": true,
        "place": nearest.as_ref().map(|n| n.place.name.clone()),
        "distance_m": nearest.as_ref().map(|n| round2(n.distance_m)),
        "at_place": nearest.as_ref().is_some_and(|n| n.within),
        "pose": pose_json(fix.pose),
        "places_known": places.registry.current().count(),
        "stale_places": stale,
        "pending_places": pending,
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
            pose_sigma: None,
            resting: false,
            untrusted: false,
            rest_watch: None,
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
        assert_eq!(s["resting"], false);
        assert!(s["rest_watch"].is_null());

        let mut resting = frame(4, 0.5, 0.5, true, false);
        resting.resting = true;
        resting.rest_watch = Some(crate::map::RestWatchSeen {
            verdict: "agrees".into(),
            ago_s: 12.0,
            residual_m: Some(0.012),
            observed: 412,
            beams: 600,
            offset_m: 0.004,
            offset_deg: 0.3,
        });
        places
            .map
            .as_ref()
            .unwrap()
            .push(MapEvent::Frame(Box::new(resting)));
        let s = execute_places("robot.map_status", &json!({}), &mut places).unwrap();
        assert_eq!(s["resting"], true);
        assert_eq!(s["rest_watch"]["verdict"], "agrees");
        assert_eq!(s["rest_watch"]["ago_s"], 12.0);
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
    fn a_place_is_taught_at_a_point_of_mapped_floor_and_listed_where_it_is() {
        // One row: floor, wall, unknown, from (-5, -5) at 5 cm.
        let mut f = frame(1, 0.0, 0.0, false, false);
        f.cols = 3;
        f.cells = crate::mapd::wire::b64_encode(&[1, 2, 0]);
        let mut places = mapped(f);
        let taught = execute_places(
            "robot.remember_place",
            &json!({"name": "divano", "x": -4.97, "y": -4.98}),
            &mut places,
        )
        .unwrap();
        assert_eq!(taught["at_point"], true, "an untrusted pose does not matter here");
        assert_eq!(taught["pose"]["x"], -4.97);
        for (x, why) in [(-4.92, "wall"), (-4.87, "not mapped"), (3.0, "not mapped")] {
            let e = execute_places("robot.remember_place", &json!({"name": "x", "x": x, "y": -4.98}), &mut places)
                .unwrap_err();
            assert!(e.contains(why), "{x}: {e}");
        }
        let e = execute_places("robot.remember_place", &json!({"name": "x", "x": 1.0}), &mut places).unwrap_err();
        assert!(e.contains("together"), "{e}");
        let listed = execute_places("robot.list_places", &json!({}), &mut places).unwrap();
        assert_eq!(listed["places"][0]["at"], json!({"x": -4.97, "y": -4.98}));
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
        // A wipe nobody asked quack-nav for: the mapper starts over with a
        // single submap, the lane's epoch moves, and the live map is a new
        // one. `studio` was taught on a map never saved, so it is gone.
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
        assert_eq!(listed["places"][0]["state"], "stale", "taught on a map never saved, and that map is gone");
        assert!(listed["places"][0]["distance_m"].is_null());
    }

    #[test]
    fn a_place_says_which_map_it_is_on_and_go_to_says_why_not() {
        let mut robot = Robot { control: None, places: mapped(frame(1, 0.0, 0.0, true, false)) };
        let places = &mut robot.places;
        execute_places("robot.remember_place", &json!({"name": "studio"}), places).unwrap();
        places.registry.saved_as("casa").unwrap();
        let listed = execute_places("robot.list_places", &json!({}), places).unwrap();
        assert_eq!(listed["places"][0]["state"], "usable");
        assert_eq!(listed["places"][0]["stale"], false);
        assert_eq!(listed["places"][0]["map"], "casa");
        assert_eq!(listed["live_map"], "casa");

        // Another map is live (a wipe): parked, not lost.
        places.map_started_afresh();
        let listed = execute_places("robot.list_places", &json!({}), places).unwrap();
        assert_eq!(listed["places"][0]["state"], "other_map");
        assert_eq!(listed["places"][0]["stale"], true);
        assert!(listed["live_map"].is_null());
        let e = go_to(&mut robot, &json!({"place": "studio"})).unwrap_err();
        assert!(e.contains("`casa`") && e.contains("map_load"), "{e}");

        // The homecoming is out: waiting, and go_to says so.
        robot.places.registry.await_homecoming();
        let listed = execute_places("robot.list_places", &json!({}), &mut robot.places).unwrap();
        assert_eq!(listed["places"][0]["state"], "pending");
        assert_eq!(listed["places"][0]["stale"], false);
        let e = go_to(&mut robot, &json!({"place": "studio"})).unwrap_err();
        assert!(e.starts_with("the duck is not sure of its position yet"), "{e}");
        let here = execute_places("robot.where_am_i", &json!({}), &mut robot.places).unwrap();
        assert_eq!(here["pending_places"], 1);
        assert_eq!(here["stale_places"], 0);
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
        (stairs, a hole) the depth sensor has seen. When the position is lost or uncertain the \
        map's walls are not judged (they would be at the wrong place): only the sensor's checks \
        apply, and the reply's `checks` says \"position uncertain: checks from the sensor only\". To map a room, \
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
                "gap": {"type": "boolean", "description": "a doorway step: margins shrink to the body plus a little (the explorer's legs no longer ask for it since 2026-09-30; off by default)"},
                "passage": {"type": "boolean", "description": "a short straight leg along a passage beside a drop, with a wall seen by the sensor at the body's side: the cliff guard judges a narrower lane (the explorer's legs no longer ask for it since 2026-09-30; off by default)"},
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
    robot.move and robot.map_step are refused. Call with stop=true to stop it. stop=true also \
    stops whatever the duck does on its own (the search for where it is after waking up, the \
    exploring it started itself, the relocalization before a job: robot.map_status says so, \
    explore.self_started), and then nothing starts moving it on its own again until a job is \
    asked for; robot.move and robot.map_step work right after. A robot.move or robot.map_step \
    while the duck moves on its own stops that motion and obeys (the reply's stopped_own).",
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
        "robot.map_step" => {
            let obeyed = drive_by_hand(robot)?;
            map_step(robot, args).map(|r| with_obeyed(r, obeyed))
        }
        "robot.map_explore" => map_explore(robot, args),
        "robot.go_to" => go_to(robot, args),
        "robot.map_save" => {
            let name = map_name(args)?;
            let saved = map_library(&robot.places.map_socket, "robot.map_save", Some(name.clone()))?;
            if let Some(n) = name.get("name").and_then(Value::as_str) {
                robot.places.explore.name_live_map(n);
                robot.places.explore.keep_ground();
                robot.places.map_saved(n);
            }
            Ok(saved)
        }
        "robot.map_list" => map_library(&robot.places.map_socket, "robot.map_list", None),
        "robot.map_load" => {
            let name = map_name(args)?;
            let mut loaded = map_library(&robot.places.map_socket, "robot.map_load", Some(name.clone()))?;
            if let Some(n) = name.get("name").and_then(Value::as_str) {
                robot.places.explore.map_named(n);
                robot.places.map_loaded(n);
                if let Some(frozen) = freeze_if_done(robot, n) {
                    loaded["frozen"] = json!(frozen);
                }
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
            // The place in the saved map goes with the name: the map
            // lane's `robot.map_adopt` reads all four, and with the name
            // alone it refused every adoption as a bad name — the boot's
            // map-to-map way home never adopted once on quack-navd's own
            // mapper (casa_arredata, 2026-09-26).
            let mut params = name.clone();
            for k in ["x", "y", "yaw"] {
                params[k] = json!(args.get(k).and_then(Value::as_f64).ok_or_else(|| format!("robot.map_adopt needs `{k}`"))?);
            }
            let mut adopted = map_library(&robot.places.map_socket, "robot.map_adopt", Some(params))?;
            if let Some(n) = name.get("name").and_then(Value::as_str) {
                robot.places.explore.map_named(n);
                robot.places.map_loaded(n);
                if let Some(frozen) = freeze_if_done(robot, n) {
                    adopted["frozen"] = json!(frozen);
                }
            }
            Ok(adopted)
        }
        "robot.map_wipe" => {
            let wiped = map_library(&robot.places.map_socket, "robot.map_wipe", None)?;
            robot.places.map_started_afresh();
            Ok(wiped)
        }
        // A caller's `robot.move` — nav.call from a client: the voice
        // front end, quack-control, any socket client — carries the cliff
        // guard. The duck's own moves (the explorer's legs, the
        // homecoming's pulses, the rim tour) take `internal_move`, as they
        // did before the guard: their own guards are tuned to approach a
        // rim, and the always-on guard stopped their legs at it in a tight
        // loop (x25, casa_arredata, 2026-10-02: 17 stops in a session, the
        // explorer sealed in). A self-driven call that comes this way is
        // routed there too.
        "robot.move" if own_motion(robot) => internal_move(robot, args),
        "robot.move" => {
            let obeyed = drive_by_hand(robot)?;
            let params = quack_duck::body::move_params(args);
            let duration = quack_duck::body::number(args, "duration_s").clamp(0.0, quack_duck::body::MAX_MOVE_DURATION_S);
            let params = quack_duck::body::trimmed(&robot.places.gait, params);
            guarded_move(robot, params, duration).map(|r| with_obeyed(r, obeyed))
        }
        other => Err(format!("this is not a navigation tool: `{other}`")),
    }
}


/// The duck's own timed move — the explorer's legs and turns
/// (`Body::blind_move`), the homecoming's pulses and steps, the rim tour:
/// blind to the map and to the sensor, as `robot.move` was before
/// 2026-10-02 (their callers judge the sensor themselves, with margins
/// tuned to approach a rim). It ends only at the user's stop.
pub fn internal_move(robot: &mut Robot, args: &Value) -> Result<Value, String> {
    if robot.places.explore.held() && own_motion(robot) {
        return Err(HELD.into());
    }
    let params = quack_duck::body::move_params(args);
    let duration = quack_duck::body::number(args, "duration_s").clamp(0.0, quack_duck::body::MAX_MOVE_DURATION_S);
    let params = quack_duck::body::trimmed(&robot.places.gait, params);
    let (own, explore) = (own_motion(robot), robot.places.explore.clone());
    let mut guard = || (own && explore.held()).then(|| HELD.to_string());
    match quack_duck::body::timed_move_guarded(&mut robot.control, params, duration, None, &mut guard)? {
        Some(_) => Err(HELD.into()),
        None => Ok(json!({"done": true, "walked_s": duration})),
    }
}

/// `robot.move`'s cliff guard (2026-10-02): a drop's edge the depth
/// sensor sees in the move's lane, nearer than this, stops the move — the
/// beak (0.15 m from the centre), what the gait coasts after the stop and
/// a frame's latency (about 0.08 m), and a margin. Judged as the blind
/// leg judges (`explore::guarded`): walking frames included, a true hole
/// (not a wall's foot), two frames agreeing when there are two.
pub(crate) const MOVE_DROP_REACH_M: f64 = 0.40;
/// The lane: the body's half-width (0.095 m) and the gait's sway.
pub(crate) const MOVE_DROP_LANE_M: f64 = 0.17;
/// The frames the guard reads: the last second's.
const MOVE_DROP_WITHIN: Duration = Duration::from_secs(1);

/// What the guard can judge of a move, before it starts.
#[derive(Debug, Clone, PartialEq)]
pub enum MoveCover {
    /// Forward, along this heading (body frame, radians).
    Ahead(f64),
    /// Not judged, and why: the sensor looks forward and down.
    NotCovered(&'static str),
    /// No guard to ask.
    Off(&'static str),
}

impl MoveCover {
    pub fn of(cliff: Option<&crate::cliff::CliffStatus>, params: &proto::MoveParams) -> Self {
        let Some(cliff) = cliff else {
            return MoveCover::Off("off: the cliff guard is not enabled ([map] cliff_guard)");
        };
        if matches!(cliff.stream, crate::cliff::StreamState::Unavailable(_)) {
            return MoveCover::Off("off: tofd has no depth sensor");
        }
        if params.vx > 0.0 {
            return MoveCover::Ahead(params.vy.atan2(params.vx));
        }
        if params.vx < 0.0 {
            MoveCover::NotCovered("not covered: backing up — the depth sensor looks forward")
        } else if params.vy != 0.0 {
            MoveCover::NotCovered("not covered: a sidestep — the depth sensor looks forward")
        } else {
            MoveCover::NotCovered("not judged: a turn in place does not advance")
        }
    }

    pub fn word(&self) -> &'static str {
        match self {
            MoveCover::Ahead(_) => "on",
            MoveCover::NotCovered(w) | MoveCover::Off(w) => w,
        }
    }
}

/// The drop that stops a move along `heading`, from the guard's newest
/// frames: the nearest true hole in the lane within [`MOVE_DROP_REACH_M`].
pub fn move_drop_ahead(cliff: &crate::cliff::CliffStatus, now: Instant, heading: f64) -> Option<crate::cliff::Drop> {
    let frames = cliff.recent.iter().filter(|f| now.duration_since(f.at) <= MOVE_DROP_WITHIN).count();
    if frames == 0 {
        return None;
    }
    cliff.hole_in_lane_walking(now, heading, MOVE_DROP_LANE_M, MOVE_DROP_REACH_M, MOVE_DROP_WITHIN, 2.min(frames))
}

/// What the reply says of a drop that stopped a move.
fn drop_words(d: &crate::cliff::Drop) -> String {
    format!(
        "a drop ahead (depth sensor): its edge {:.2}–{:.2} m away, {:.0}° {}",
        d.edge_min_m,
        d.range_m,
        d.bearing.to_degrees().abs(),
        if d.bearing >= 0.0 { "left" } else { "right" }
    )
}

/// `robot.move` as a caller asks it: the timed move, with the cliff guard always on. Every
/// tick the guard's newest frames are read; a true hole in the move's lane
/// within reach ends it (one explicit zero), whatever the pose says and
/// whether or not there is a map — the sensor sees the hole where the duck
/// is. Forward moves only: backing up and sidesteps are not covered.
fn guarded_move(robot: &mut Robot, params: proto::MoveParams, duration: f64) -> Result<Value, String> {
    let cliff = robot.places.cliff.clone();
    let cover = MoveCover::of(cliff.as_ref().map(|c| c.snapshot()).as_ref(), &params);
    let mut guard = || -> Option<String> {
        let MoveCover::Ahead(heading) = cover else { return None };
        let status = cliff.as_ref()?.snapshot();
        move_drop_ahead(&status, Instant::now(), heading).map(|d| drop_words(&d))
    };
    let stopped = quack_duck::body::timed_move_guarded(&mut robot.control, params, duration, None, &mut guard)?;
    Ok(match stopped {
        Some((why, walked_s)) => {
            tracing::warn!(why, walked_s = format!("{walked_s:.2}"), "robot.move: stopped early by the cliff guard");
            json!({"done": false, "stopped": why, "walked_s": round2(walked_s), "cliff_guard": cover.word()})
        }
        None => json!({"done": true, "walked_s": duration, "cliff_guard": cover.word()}),
    })
}

thread_local! {
    /// This thread moves the duck on its own: the homecoming's, the
    /// relocator's (see [`mark_self_driven`]).
    static SELF_DRIVEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The calling thread moves the duck on its own account — the homecoming,
/// the relocator — not on a caller's: its moves stop when the user stops
/// the duck, and it starts nothing while the user's stop holds.
pub fn mark_self_driven() {
    SELF_DRIVEN.with(|c| c.set(true));
}

/// Whether this call is the duck's own motion: a self-driven thread, or a
/// running job's own robot.
fn own_motion(robot: &Robot) -> bool {
    SELF_DRIVEN.with(|c| c.get()) || robot.places.explore.is_child()
}

/// What the duck's own motion is told once the user stopped it.
const HELD: &str = "stopped by the user: the duck does not move on its own until a job is asked for";

/// A stop asked for: the user's (everything stops, a leg in flight at
/// once, and nothing self-started starts again until asked), or the
/// duck's own (the homecoming trading maps: the job alone).
fn stop_motion(robot: &Robot) -> bool {
    if own_motion(robot) {
        let busy = robot.places.explore.busy();
        robot.places.explore.request_stop();
        busy
    } else {
        robot.places.explore.user_stop()
    }
}

/// A job asked for: the user's ends the hold of their last stop; the
/// duck's own is refused while that hold lasts.
fn asked(robot: &Robot) -> Result<(), String> {
    if own_motion(robot) {
        if robot.places.explore.held() {
            return Err(HELD.into());
        }
    } else {
        robot.places.explore.user_asks();
    }
    Ok(())
}

/// Before a move or a step. The duck's own: refused once the user stopped
/// it. A caller's: while the duck moves on its own (the homecoming's search
/// or exploration, a relocalization) that motion is stopped and the caller
/// obeyed — the user may always take the duck by hand; the answer says what
/// was stopped. While a job the user asked for runs, refused, as ever.
fn drive_by_hand(robot: &mut Robot) -> Result<Option<String>, String> {
    if own_motion(robot) {
        return if robot.places.explore.held() { Err(HELD.into()) } else { Ok(None) };
    }
    if let Some(why) = robot.places.explore.take_halted().or_else(|| robot.places.explore.self_started()) {
        robot.places.explore.user_stop();
        tracing::warn!(why, "a move by hand while the duck moved on its own: that motion stopped, the move obeyed");
        return Ok(Some(why));
    }
    robot.places.not_exploring()?;
    Ok(None)
}

/// The user's word before the robot is theirs: a caller's stop, or a
/// caller's move while the duck moves on its own, holds the duck's own
/// motion at once — the leg it walks ends within a tick, its stand is cut
/// short — instead of after the step that motion holds the robot for
/// (a STOP on the twin waited 7 s, the leg walked out, before this). The
/// daemon calls it for every caller's call, before the lock.
pub fn before_the_lock(explore: &crate::explore::ExploreHandle, name: &str, args: &Value) {
    let stop = args.get("stop").and_then(Value::as_bool).unwrap_or(false);
    match name {
        "robot.go_to" | "robot.map_explore" if stop => {
            explore.halt();
        }
        "robot.move" | "robot.map_step" => {
            explore.halt_own();
        }
        _ => {}
    }
}

/// The reply of a move that stopped the duck's own motion says so.
fn with_obeyed(mut reply: Value, obeyed: Option<String>) -> Value {
    if let (Some(why), Some(map)) = (obeyed, reply.as_object_mut()) {
        map.insert(
            "stopped_own".into(),
            json!(format!("the duck was moving on its own ({why}): stopped to obey, and it does not start again on its own until a job is asked for")),
        );
    }
    reply
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
/// The least margin a leg may ask for from an edge (see `plan_step`).
const CLIFF_MARGIN_FLOOR_M: f64 = 0.12;
/// Stop this far short of a drop's edge — farther than a wall's margin,
/// because an edge is not a bump.
/// `QK_CLIFF_MARGIN_M`: 0.25 since 2026-09-16 (was 0.35) — the flank 15 cm from the
/// edge at the leg's end, and the same word as the planner's 0.22 round a
/// booked rim.
pub(crate) fn cliff_margin_m() -> f64 {
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
        let was_running = stop_motion(robot);
        return Ok(json!({"stopped": was_running, "explore": robot.places.explore.status().to_json()}));
    }
    asked(robot)?;
    if robot.places.explore.busy() {
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
    // Moved while it rested, maybe anywhere: found first (see `relocate`).
    // A new map from nothing needs no pose on the old one.
    if frame.untrusted && !frame.seated && !args.get("fresh").and_then(Value::as_bool).unwrap_or(false) {
        quack_duck::body::with_robot(&mut robot.control)?;
        robot.places.explore.relocalize_then(crate::relocate::Request { tool: "robot.map_explore".into(), args: args.clone(), goal: None })?;
        return Ok(json!({"started": true, "relocalizing": true, "reason": crate::relocate::REASON}));
    }
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
        started_afresh(&robot.places.explore, &mut robot.places.registry, frames0);
        tracing::info!(map = name, "map explore: a new map from nothing; the saved one is replaced when this session saves");
    }
    // What the answer reports is the map as it is now — after `fresh`, the
    // new one, not what was read before the wipe.
    let progress = robot.places.explore.status().progress;
    let frame = map.snapshot().latest.clone().unwrap_or(frame);
    // `session: false`: an exploration that is not a session of the
    // house's map — the boot's search on a fresh map, which must never be
    // saved over the map it is looking for (casa_arredata, 2026-09-26: the
    // search's six minutes replaced four sessions of the house when the
    // adoption stopped it).
    let session = (args.get("session").and_then(Value::as_bool) != Some(false)).then(|| crate::explore::Session {
        save_as: name.clone(),
        battery_min_pct: args.get("battery_min_pct").and_then(Value::as_f64).unwrap_or(25.0),
    });
    // A session inks: a map frozen at run time (a house declared done,
    // loaded or adopted) is thawed for it. Only another name gets here on
    // a done house — its own is refused above — and `fresh` started a new
    // map, which is never frozen.
    if session.is_some() && frame.frozen && !localize_mode(robot) {
        map_library(&robot.places.map_socket, crate::mapd::wire::METHOD_QUACK_MAP_FREEZE, Some(json!({"on": false})))?;
        tracing::info!(map = name, "map explore: the frozen map is live again for this session");
    }
    robot.places.explore.start(
        &robot.places.robotd_socket,
        &robot.places,
        known,
        max_s,
        !robot.places.map_config.ask_phrase.is_empty(),
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
    // The live map is the boot's search, not the house: closing it would
    // save six minutes of search over the saved map and freeze them there
    // (casa_arredata, 2026-09-26).
    if robot.places.explore.searching() {
        return Err("the duck has not found itself on the saved map yet — the map in hand is a search, not the house: \
                    nothing to declare complete; wait for the homecoming (robot.map_status) and ask again"
            .into());
    }
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
    robot.places.map_saved(&name);
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

/// A house declared done is navigated, not inked: loaded or adopted, its
/// map is frozen as `localize` freezes every map. The mapper keeps
/// searching for the pose as when mapping (frozen only bites once the
/// pose is confirmed) and inks nothing meanwhile, the confirming window
/// included. Before this a power-on in `stop_and_scan` froze it only with
/// `resume_explore` on, and the duck inked the house it navigated
/// (casa_grande 627 → 630 submaps, 2026-10-02). `None` when the house of
/// `name` is not done, else whether the freeze was accepted.
pub(crate) fn freeze_if_done(robot: &Robot, name: &str) -> Option<bool> {
    let done = robot.places.explore.status().progress.as_ref().and_then(|p| p.get("done")).and_then(Value::as_bool).unwrap_or(false);
    if !done {
        return None;
    }
    Some(match map_library(&robot.places.map_socket, crate::mapd::wire::METHOD_QUACK_MAP_FREEZE, Some(json!({"on": true}))) {
        Ok(_) => {
            tracing::info!(map = name, "map: the house is done; its map frozen — navigated on, inked no more");
            true
        }
        Err(e) => {
            tracing::warn!(map = name, error = %e, "map: the house is done, but its map could not be frozen");
            false
        }
    })
}

/// maploc runs in `localize`: every map is frozen by the mode itself.
fn localize_mode(robot: &Robot) -> bool {
    robot.places.map.as_ref().is_some_and(|m| {
        matches!(&m.snapshot().support, crate::map::MapSupport::Supported { mode: Some(mode), .. } if mode == "localize")
    })
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
        let was_running = stop_motion(robot);
        return Ok(json!({"stopped": was_running, "explore": robot.places.explore.status().to_json()}));
    }
    asked(robot)?;
    if let Some(why) = robot.places.explore.self_started() {
        return Err(format!("the duck is moving on its own ({why}): stop it first with robot.go_to {{\"stop\": true}}"));
    }
    if robot.places.explore.busy() {
        return Err("the duck is already on its way (or exploring, or finding where it is): stop that first".into());
    }
    robot.places.fold();
    let Some(map) = &robot.places.map else {
        return Err("this satellite has no map lane ([map] enabled = false)".into());
    };
    let snapshot = map.snapshot();
    let Some(frame) = snapshot.latest.clone() else {
        return Err("no map yet: robotd is unreachable or has not sent a map frame".into());
    };
    // The oracle (the twin only): its map and, while fresh, its pose.
    let oracle = crate::oracle::oracle();
    let vouched = oracle.is_some_and(|o| o.pose_fresh()) && !frame.seated;
    let frame = match oracle {
        Some(o) => o.apply(frame),
        None => frame,
    };
    // Moved while it rested, maybe anywhere: found first (see `relocate`).
    let untrusted = frame.untrusted && !frame.seated && !vouched;
    if snapshot.trusted_pose().is_none() && !untrusted && !vouched {
        return Err(if frame.seated {
            "the duck is seated or fallen: stand it up first (sit_toggle)".into()
        } else {
            "the duck is not sure of its position yet: let it stand still and look around first".into()
        });
    }
    // A place by name, or a point.
    let (goal, what) = match args.get("place").and_then(Value::as_str) {
        Some(name) => {
            let registry = &robot.places.registry;
            let place = registry.named(name).ok_or_else(|| {
                let known: Vec<&str> = registry.current().map(|p| p.name.as_str()).collect();
                if known.is_empty() {
                    format!("the duck knows no place called `{name}`, and no places at all yet")
                } else {
                    format!("the duck knows no place called `{name}`; it knows: {}", known.join(", "))
                }
            })?;
            let on = registry.map_of(place).map_or_else(|| "a map never saved".to_owned(), |m| format!("the map `{m}`"));
            match registry.state(place) {
                PlaceState::Usable => {}
                PlaceState::Pending => {
                    return Err(format!(
                        "the duck is not sure of its position yet: `{}` is on {on}, and the duck has not found itself \
                         on it yet — let it stand still and look around first",
                        place.name
                    ));
                }
                PlaceState::OtherMap => {
                    return Err(format!(
                        "`{}` is on {on}, and the duck is on another map now ({}): load that map first (robot.map_load)",
                        place.name,
                        registry.live_map().map_or_else(|| "one not saved yet".to_owned(), |m| format!("`{m}`"))
                    ));
                }
                PlaceState::Stale => {
                    return Err(format!(
                        "`{}` was taught on a map that is gone (wiped, or replaced by a new one under its name): \
                         teach it again on this map with robot.remember_place",
                        place.name
                    ));
                }
            }
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
    if untrusted {
        quack_duck::body::with_robot(&mut robot.control)?;
        robot.places.explore.relocalize_then(crate::relocate::Request { tool: "robot.go_to".into(), args: args.clone(), goal: Some(goal) })?;
        return Ok(json!({
            "started": true, "relocalizing": true, "reason": crate::relocate::REASON,
            "to": what, "goal": {"x": goal.0, "y": goal.1},
        }));
    }
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
    /// The pose was lost or untrusted: the map's walls were left out —
    /// judged at the believed pose they would be judged at the wrong
    /// place — and only the depth sensor's checks applied.
    pub sensor_only: bool,
}

/// What a step's reply says when the pose could not be used.
pub const SENSOR_ONLY: &str = "position uncertain: checks from the sensor only";

/// Whether the map's word can be taken at this frame's pose: tracking,
/// and not marked untrusted (moved while it rested). Otherwise every
/// map-based check of a step — the walls ahead, the passage's sides, the
/// wall-hug and the centring — would be judged at a pose the duck may not
/// be at.
pub fn pose_known(frame: &crate::map::MapFrame) -> bool {
    frame.tracking && !frame.untrusted
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
    if pose_known(first)
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
        let grid = if pose_known(first) { first.grid().ok() } else { None };
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
                    .ok()
                    .filter(|_| pose_known(first))
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
        && pose_known(first)
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
        sensor_only: !pose_known(first),
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
    let StepPlan { params, walk_s, stop_s, shortened, steered, sensor_only } = plan;
    if walk_s > 0.0 {
        // The heading held on a walking leg that is not an arc: the
        // steering asked for (before the trim) is what the hold means to
        // integrate.
        // Straight legs only: on a steered leg the hold fought the
        // steering (house4tour, 2026-09-18: 634 s for a 472–499 s tour).
        // A leg that asks for it (`hold`, with `hold_bias` radians to
        // aim off the starting heading — the passage law rejoining its
        // line).
        let asked = args.get("hold").and_then(Value::as_bool).unwrap_or(false);
        let bias = quack_duck::body::number(args, "hold_bias");
        // The yaw the hold closes on is the cliff guard's odometry
        // heading, handed over as a closure so the body's lane knows
        // nothing of the guard (the split of 2026-09-22).
        let yaw_now = robot.places.cliff.clone().map(|c| move || c.snapshot().odom_yaw);
        let hold: Option<(&dyn Fn() -> Option<f64>, f64, f64)> =
            match (asked, &yaw_now) {
                (true, Some(f)) => Some((f, 0.0, bias)),
                _ => None,
            };
        // The duck's own leg ends the moment the user stops it.
        let (own, explore) = (own_motion(robot), robot.places.explore.clone());
        let mut guard = || (own && explore.held()).then(|| HELD.to_string());
        if quack_duck::body::timed_move_guarded(&mut robot.control, params, walk_s, hold, &mut guard)?.is_some() {
            return Err(HELD.into());
        }
    }
    // The stand; the duck's own is cut short by the user's stop, so the
    // robot is theirs at once.
    let (own, explore) = (own_motion(robot), robot.places.explore.clone());
    let stand_end = Instant::now() + Duration::from_secs_f64(stop_s);
    while Instant::now() < stand_end {
        if own && explore.held() {
            return Err(HELD.into());
        }
        std::thread::sleep(Duration::from_millis(100).min(stand_end.saturating_duration_since(Instant::now())));
    }
    let after = map.snapshot();
    let last = after.latest.clone().unwrap_or(first.clone());
    let new_windows = last.windows.saturating_sub(first.windows);
    // The map's clearance at a pose it does not vouch for would describe
    // somewhere else: none.
    let clearance = last
        .grid()
        .ok()
        .filter(|_| pose_known(&last))
        .map(|g| crate::tools::clearance_json(&g, last.pose()))
        .unwrap_or(Value::Null);
    let hug_hint = last.grid().ok().filter(|_| pose_known(&last)).and_then(|g| {
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
    } else if sensor_only && last.untrusted {
        "position uncertain (the duck may have been moved): this step was judged by the depth sensor alone, not by the map; robot.go_to finds the position first when asked"
    } else if sensor_only {
        "position uncertain: this step was judged by the depth sensor alone, not by the map; let it stand until robot.map_status says it is tracking again"
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
        // Which checks judged the step: the map's and the sensor's, or —
        // the pose lost or untrusted — the sensor's alone.
        "checks": if sensor_only { SENSOR_ONLY } else { "map and sensor" },
        "hint": hint,
    }))
}

#[cfg(test)]
mod move_guard_tests {
    use super::*;
    use crate::cliff::{CliffFrame, CliffStatus, Drop, DropKind};
    use std::io::BufRead;
    use std::sync::{Arc, Mutex};

    fn frame(drops: Vec<Drop>, moving: bool) -> CliffFrame {
        CliffFrame {
            seq: 1,
            at: Instant::now(),
            head_yaw: 0.0,
            moving,
            drops,
            floors: Vec::new(),
            obstacles: Vec::new(),
            floor_beams: 0,
            judged: 16,
        }
    }

    fn hole(bearing: f64, edge_min_m: f64) -> Drop {
        Drop { bearing, range_m: edge_min_m + 0.1, edge_min_m, floor_beyond_m: 0.0, kind: DropKind::Missing }
    }

    fn mv(vx: f64, vy: f64, vyaw: f64) -> proto::MoveParams {
        proto::MoveParams { vx, vy, vyaw }
    }

    /// The guard judges what a forward move walks into, from walking
    /// frames too, and nothing it does not walk toward.
    #[test]
    fn the_move_guard_stops_on_a_hole_in_the_lane_and_only_there() {
        let now = Instant::now();
        let mut s = CliffStatus::default();
        s.absorb(frame(vec![hole(0.05, 0.30)], true));
        s.absorb(frame(vec![hole(0.05, 0.28)], true));
        let d = move_drop_ahead(&s, now, 0.0).expect("a hole 0.3 m ahead stops a forward move");
        assert!((d.edge_min_m - 0.28).abs() < 1e-9);
        assert!(drop_words(&d).starts_with("a drop ahead (depth sensor)"));
        // Beside the lane, or past the reach: not this move's.
        let mut s = CliffStatus::default();
        s.absorb(frame(vec![hole(1.2, 0.30)], false));
        s.absorb(frame(vec![hole(0.0, 0.70)], false));
        assert!(move_drop_ahead(&s, now, 0.0).is_none());
        // One frame alone counts when it is the only one there is; two
        // frames disagreeing do not.
        let mut s = CliffStatus::default();
        s.absorb(frame(vec![hole(0.0, 0.20)], true));
        assert!(move_drop_ahead(&s, now, 0.0).is_some());
        s.absorb(frame(Vec::new(), true));
        assert!(move_drop_ahead(&s, now, 0.0).is_none());

        // What is covered: forward (a sidestep's lane turned with it), not
        // backing up, not sidesteps alone, not turns in place.
        assert_eq!(MoveCover::of(Some(&s), &mv(0.3, 0.0, 0.5)), MoveCover::Ahead(0.0));
        assert!(matches!(MoveCover::of(Some(&s), &mv(0.3, 0.3, 0.0)), MoveCover::Ahead(h) if (h - std::f64::consts::FRAC_PI_4).abs() < 1e-9));
        assert!(MoveCover::of(Some(&s), &mv(-0.3, 0.0, 0.0)).word().starts_with("not covered: backing up"));
        assert!(MoveCover::of(Some(&s), &mv(0.0, 0.2, 0.0)).word().starts_with("not covered: a sidestep"));
        assert!(MoveCover::of(Some(&s), &mv(0.0, 0.0, 1.5)).word().starts_with("not judged"));
        assert!(MoveCover::of(None, &mv(0.3, 0.0, 0.0)).word().starts_with("off"));
        let mut blind = CliffStatus::default();
        blind.stream = StreamState::Unavailable("no sensor".into());
        assert!(MoveCover::of(Some(&blind), &mv(0.3, 0.0, 0.0)).word().starts_with("off"));
    }

    /// A mapped room at the believed pose (0, 0) facing +x, all floor but
    /// a wall 0.25 m ahead; tracking and untrusted as asked.
    fn walled(tracking: bool, untrusted: bool) -> crate::map::MapFrame {
        let (rows, cols) = (40usize, 40usize);
        let mut cells = vec![1u8; rows * cols];
        for r in 0..rows {
            for c in 25..27 {
                cells[r * cols + c] = 2;
            }
        }
        crate::map::MapFrame {
            seq: 1, x: 0.0, y: 0.0, yaw: 0.0, tracking,
            x_min: -1.0, y_min: -1.0, cell_m: 0.05, rows: rows as u32, cols: cols as u32,
            cells: crate::mapd::wire::b64_encode(&cells),
            n_submaps: 1, n_loops: 0, windows: 5, still: true, seated: false, frozen: false,
            pose_sigma: None, resting: false, untrusted, rest_watch: None,
        }
    }

    /// With the pose lost or untrusted the map's wall is not judged — at
    /// the believed pose it may be anywhere — and the step says so; the
    /// sensor's own checks still hold.
    #[test]
    fn an_uncertain_pose_leaves_the_map_out_of_a_step() {
        let now = Instant::now();
        let mut looked = CliffStatus::default();
        looked.absorb(frame(Vec::new(), false));
        let gait = quack_duck::gait::GaitConfig::default();
        let step = json!({"vx": 0.3, "walk_s": 3.0, "stop_s": 0.0});
        // Trusted: the mapped wall refuses the step.
        let e = plan_step(&step, &walled(true, false), Some(&looked), &gait, now).err().expect("refused");
        assert!(e.starts_with("a wall is 0.2"), "{e}");
        // Untrusted, or not tracking: the wall is the map's, not the
        // sensor's; the sensor looked and saw nothing.
        for f in [walled(false, true), walled(false, false), walled(true, true)] {
            let plan = plan_step(&step, &f, Some(&looked), &gait, now).expect("judged by the sensor alone");
            assert!(plan.sensor_only && plan.steered.is_none());
            assert_eq!(plan.walk_s, 3.0);
        }
        // The sensor still speaks: a drop ahead refuses the step.
        let mut hole = CliffStatus::default();
        hole.absorb(frame(vec![hole_at(0.0, 0.3)], false));
        let e = plan_step(&step, &walled(false, true), Some(&hole), &gait, now).err().expect("a drop refuses");
        assert!(e.starts_with("a drop — stairs or a hole —"), "{e}");
        assert!(e.contains("clearance left null"), "no clearance from a map at the wrong place: {e}");
        // And one it has not looked at: stand first.
        let e = plan_step(&step, &walled(false, true), Some(&CliffStatus::default()), &gait, now).err().expect("unlooked");
        assert!(e.contains("has not looked ahead yet"), "{e}");
    }

    fn hole_at(bearing: f64, edge_min_m: f64) -> Drop {
        hole(bearing, edge_min_m)
    }

    /// A robotd that keeps every line it is sent.
    fn fake_robotd() -> (tempfile::TempDir, String, Arc<Mutex<Vec<Value>>>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("robotd.sock").to_str().unwrap().to_string();
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let kept = lines.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let kept = kept.clone();
                std::thread::spawn(move || {
                    for line in std::io::BufReader::new(stream).lines() {
                        let Ok(line) = line else { return };
                        if let Ok(v) = serde_json::from_str::<Value>(&line) {
                            kept.lock().unwrap().push(v);
                        }
                    }
                });
            }
        });
        (dir, path, lines)
    }

    fn sent_vx(lines: &Arc<Mutex<Vec<Value>>>) -> Vec<f64> {
        lines.lock().unwrap().iter().filter(|v| v["method"] == "robot.move").map(|v| v["params"]["vx"].as_f64().unwrap()).collect()
    }

    /// The duck moving on its own (the homecoming's search) is said in the
    /// status, and the user's STOP — `robot.go_to` `stop` — stops it and
    /// holds: the duck's own motion is refused from then on (its moves,
    /// its jobs), until the user asks for a job. A move by hand works
    /// right after the STOP, and a move by hand *during* the duck's own
    /// motion stops that motion and obeys, saying so.
    #[test]
    fn the_users_stop_holds_the_ducks_own_motion_and_frees_the_hand() {
        let (_dir, path, lines) = fake_robotd();
        let mut robot = Robot::detached();
        robot.control = Some(quack_duck::Control::connect(&path).unwrap());
        let explore = robot.places.explore.clone();

        // The boot's search: searching, self-started, the reason said.
        explore.search_began(crate::homecoming::WHY_BOOT);
        let st = explore.status().to_json();
        assert_eq!(st["state"], "searching");
        assert_eq!(st["self_started"], true);
        assert_eq!(st["reason"], crate::homecoming::WHY_BOOT);
        assert!(explore.busy());
        // A go_to meanwhile is told what moves and how to stop it.
        let e = execute("robot.go_to", &json!({"x": 1.0, "y": 0.0}), &mut robot).unwrap_err();
        assert!(e.starts_with("the duck is moving on its own") && e.contains("\"stop\": true"), "{e}");

        // STOP, as quack-control's button sends it.
        let r = execute("robot.go_to", &json!({"stop": true}), &mut robot).unwrap();
        assert_eq!(r["stopped"], true);
        assert_eq!(r["explore"]["state"], "stopped");
        assert_eq!(r["explore"]["reason"], crate::explore::STOPPED_BY_USER);
        assert_eq!(r["explore"]["stopped_by_user"], true);
        assert!(!explore.busy() && explore.held());

        // The duck's own motion, from a self-driven thread (the homecoming,
        // the relocator), is refused while the stop holds: no move, no job.
        let own = std::thread::spawn({
            let path = path.clone();
            let explore = explore.clone();
            move || {
                mark_self_driven();
                let mut own = Robot::detached();
                own.control = Some(quack_duck::Control::connect(&path).unwrap());
                own.places.explore = explore;
                (
                    execute("robot.move", &json!({"vx": 0.3, "duration_s": 0.5}), &mut own),
                    execute("robot.map_explore", &json!({}), &mut own),
                )
            }
        });
        let (moved, explored) = own.join().unwrap();
        assert_eq!(moved.unwrap_err(), HELD);
        assert_eq!(explored.unwrap_err(), HELD);
        std::thread::sleep(Duration::from_millis(100));
        assert!(sent_vx(&lines).is_empty(), "nothing sent by the duck's own motion");

        // By hand, right after the STOP: obeyed, the pose irrelevant.
        let r = execute("robot.move", &json!({"vx": 0.3, "duration_s": 0.2}), &mut robot).unwrap();
        assert_eq!(r["done"], true, "{r}");
        assert!(r.get("stopped_own").is_none());

        // The duck moving on its own again (a job asked for ends the hold;
        // the relocalization is the duck's own walk): a move by hand stops
        // it and obeys, and the reply says what it stopped.
        explore.user_asks();
        assert!(!explore.held());
        explore.search_began(crate::homecoming::WHY_BOOT);
        let r = execute("robot.move", &json!({"vx": 0.3, "duration_s": 0.2}), &mut robot).unwrap();
        assert_eq!(r["done"], true, "{r}");
        assert!(r["stopped_own"].as_str().unwrap().contains(crate::homecoming::WHY_BOOT), "{r}");
        assert_eq!(explore.status().state, crate::explore::State::Stopped);
        assert!(explore.held());
    }

    /// A map socket that accepts every call and keeps what it was asked.
    fn fake_map_socket() -> (tempfile::TempDir, String, Arc<Mutex<Vec<Value>>>) {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("map.sock").to_str().unwrap().to_string();
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let asked = Arc::new(Mutex::new(Vec::new()));
        let kept = asked.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let mut line = String::new();
                if std::io::BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    kept.lock().unwrap().push(v);
                }
                let _ = (&stream).write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"accepted\":true}}\n");
            }
        });
        (dir, path, asked)
    }

    /// A house declared done is frozen as it is loaded or adopted — what
    /// `localize` does to every map — and one still being explored is not
    /// (the stop_and_scan life test, 2026-10-02: casa_grande, done, went
    /// on being inked after a power-on, 627 → 630 submaps).
    #[test]
    fn a_done_house_is_frozen_when_loaded_or_adopted() {
        let (dir, path, asked) = fake_map_socket();
        let places_path = dir.path().join("places.json");
        std::fs::write(
            dir.path().join("ground.json"),
            json!({
                "done_house.progress": {"done": true, "declared_by_user": true, "percent": 96},
                "open_house.progress": {"done": false, "percent": 60},
            })
            .to_string(),
        )
        .unwrap();
        let mut robot = Robot::detached();
        robot.places.map_socket = path;
        robot.places.explore = crate::explore::ExploreHandle::new().with_ground(places_path.to_str().unwrap());
        let freezes = |asked: &Arc<Mutex<Vec<Value>>>| -> Vec<Value> {
            asked.lock().unwrap().iter().filter(|v| v["method"] == crate::mapd::wire::METHOD_QUACK_MAP_FREEZE).map(|v| v["params"]["on"].clone()).collect()
        };

        let r = execute("robot.map_load", &json!({"name": "open_house"}), &mut robot).unwrap();
        assert!(r.get("frozen").is_none(), "{r}");
        assert!(freezes(&asked).is_empty());

        let r = execute("robot.map_load", &json!({"name": "done_house"}), &mut robot).unwrap();
        assert_eq!(r["frozen"], true, "{r}");
        assert_eq!(freezes(&asked), vec![json!(true)]);

        let r = execute("robot.map_adopt", &json!({"name": "open_house", "x": 0.5, "y": 0.0, "yaw": 0.1}), &mut robot).unwrap();
        assert!(r.get("frozen").is_none(), "{r}");
        let r = execute("robot.map_adopt", &json!({"name": "done_house", "x": 0.5, "y": 0.0, "yaw": 0.1}), &mut robot).unwrap();
        assert_eq!(r["frozen"], true, "{r}");
        assert_eq!(freezes(&asked), vec![json!(true), json!(true)]);
    }

    /// A job's leg in flight ends the moment the user stops the duck: the
    /// job's own robot shares the stop.
    #[test]
    fn a_leg_in_flight_ends_at_the_users_stop() {
        let (_dir, path, lines) = fake_robotd();
        let parent = crate::explore::ExploreHandle::new();
        let mut job = Robot::detached();
        job.control = Some(quack_duck::Control::connect(&path).unwrap());
        job.places.explore = parent.child();
        let stopper = std::thread::spawn({
            let parent = parent.clone();
            move || {
                std::thread::sleep(Duration::from_millis(200));
                parent.user_stop();
            }
        });
        let began = Instant::now();
        let r = execute("robot.move", &json!({"vx": 0.3, "duration_s": 2.5}), &mut job);
        stopper.join().unwrap();
        assert_eq!(r.unwrap_err(), HELD);
        assert!(began.elapsed() < Duration::from_millis(600), "{:?}", began.elapsed());
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(sent_vx(&lines).last(), Some(&0.0), "stopped with a zero");
    }

    /// The STOP does not wait for the step the duck's own motion holds the
    /// robot for: the daemon's caller lane sets the hold before the lock,
    /// the leg ends within a tick, and the lock is the caller's at once.
    #[test]
    fn a_stop_reaches_the_ducks_own_step_before_the_lock() {
        let (_dir, path, _lines) = fake_robotd();
        let mut robot = Robot::detached();
        robot.control = Some(quack_duck::Control::connect(&path).unwrap());
        let explore = robot.places.explore.clone();
        explore.search_began(crate::homecoming::WHY_BOOT);
        let robot = Arc::new(Mutex::new(robot));
        let search = std::thread::spawn({
            let robot = robot.clone();
            move || {
                mark_self_driven();
                let mut robot = robot.lock().unwrap();
                execute("robot.move", &json!({"vx": 0.3, "duration_s": 3.0}), &mut robot)
            }
        });
        std::thread::sleep(Duration::from_millis(200));
        let asked = Instant::now();
        before_the_lock(&explore, "robot.go_to", &json!({"stop": true}));
        let r = {
            let mut robot = robot.lock().unwrap();
            execute("robot.go_to", &json!({"stop": true}), &mut robot).unwrap()
        };
        assert!(asked.elapsed() < Duration::from_millis(500), "the STOP waited {:?}", asked.elapsed());
        assert_eq!(search.join().unwrap().unwrap_err(), HELD);
        assert_eq!(r["stopped"], true, "{r}");
        assert_eq!(r["explore"]["state"], "stopped");
    }

    /// The always-on guard is the callers' alone: the same hole the sensor
    /// sees 0.3 m ahead stops a caller's `robot.move`, and does not stop
    /// the duck's own legs — a job's `Body::blind_move`, or a self-driven
    /// thread's move (the homecoming, the rim tour) — whose own guards are
    /// tuned to approach a rim (x25, 2026-10-02).
    #[test]
    fn the_ducks_own_legs_are_not_stopped_by_the_callers_guard() {
        use crate::explore::Body;
        let (_dir, path, _lines) = fake_robotd();
        let cliff = crate::cliff::CliffWatch::detached();
        cliff.push(frame(vec![hole(0.0, 0.30)], true));
        cliff.push(frame(vec![hole(0.0, 0.29)], true));
        let robot_with = |explore: crate::explore::ExploreHandle| {
            let mut r = Robot::detached();
            r.control = Some(quack_duck::Control::connect(&path).unwrap());
            r.places.cliff = Some(cliff.clone());
            r.places.explore = explore;
            r
        };
        // A caller's move: stopped at once.
        let parent = crate::explore::ExploreHandle::new();
        let mut caller = robot_with(parent.clone());
        let r = execute("robot.move", &json!({"vx": 0.3, "duration_s": 0.3}), &mut caller).unwrap();
        assert_eq!(r["done"], false, "{r}");
        // A job's leg toward the same hole: walked its time.
        let mut job = robot_with(parent.child());
        let r = job.blind_move(&json!({"vx": 0.3, "duration_s": 0.3})).unwrap();
        assert_eq!(r["done"], true, "{r}");
        assert!(r.get("cliff_guard").is_none());
        // ... and through the tool's name from the job's robot.
        let r = execute("robot.move", &json!({"vx": 0.3, "duration_s": 0.3}), &mut job).unwrap();
        assert_eq!(r["done"], true, "{r}");
        // A self-driven thread's move (the homecoming's pulses): walked.
        let r = std::thread::spawn({
            let mut own = robot_with(parent.clone());
            move || {
                mark_self_driven();
                execute("robot.move", &json!({"vx": 0.3, "duration_s": 0.3}), &mut own)
            }
        })
        .join()
        .unwrap()
        .unwrap();
        assert_eq!(r["done"], true, "{r}");
    }

    /// `robot.move` toward a hole: refused before the first step when the
    /// hole is already within reach, stopped mid-walk when the sensor sees
    /// it on the way — an explicit zero, and the reply says why and when.
    /// No map, no pose: the guard does not need them.
    #[test]
    fn robot_move_stops_at_a_drop_the_sensor_sees() {
        let (_dir, path, lines) = fake_robotd();
        let cliff = crate::cliff::CliffWatch::detached();
        let mut robot = Robot::detached();
        robot.control = Some(quack_duck::Control::connect(&path).unwrap());
        robot.places.cliff = Some(cliff.clone());

        // Open floor: the move runs its time.
        let r = execute("robot.move", &json!({"vx": 0.3, "duration_s": 0.3}), &mut robot).unwrap();
        assert_eq!(r["done"], true, "{r}");
        assert_eq!(r["cliff_guard"], "on");

        // The sensor sees the hole on the way, 0.2 s in.
        let feeder = cliff.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            feeder.push(frame(vec![hole(0.0, 0.32)], true));
            feeder.push(frame(vec![hole(0.0, 0.31)], true));
        });
        let r = execute("robot.move", &json!({"vx": 0.3, "duration_s": 2.0}), &mut robot).unwrap();
        t.join().unwrap();
        assert_eq!(r["done"], false, "{r}");
        assert!(r["stopped"].as_str().unwrap().starts_with("a drop ahead (depth sensor)"), "{r}");
        let walked = r["walked_s"].as_f64().unwrap();
        assert!((0.15..0.6).contains(&walked), "{r}");
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(sent_vx(&lines).last(), Some(&0.0), "the stop is an explicit zero");

        // Already at the edge: not one step forward.
        let before = sent_vx(&lines).len();
        let r = execute("robot.move", &json!({"vx": 0.3, "duration_s": 2.0}), &mut robot).unwrap();
        assert_eq!(r["done"], false);
        assert_eq!(r["walked_s"], 0.0);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(sent_vx(&lines)[before..], [0.0]);

        // Backing away from it is the caller's: not covered, and said so.
        let r = execute("robot.move", &json!({"vx": -0.3, "duration_s": 0.2}), &mut robot).unwrap();
        assert_eq!(r["done"], true);
        assert!(r["cliff_guard"].as_str().unwrap().starts_with("not covered: backing up"), "{r}");
    }
}
