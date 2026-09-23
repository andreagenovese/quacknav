//! The `[map]` configuration section, owned here so a host embeds it
//! rather than transcribing it.

use serde::Deserialize;

/// The live map and the places registry. Costs one idle socket on a robot
/// that does not map; a robotd without `robot.map` turns it off by itself.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MapConfig {
    pub enabled: bool,
    /// The places registry (names for map coordinates) — the host's own
    /// state, never inside robotd's directories.
    pub places_path: String,
    /// The cliff guard (see [`crate::cliff`]): read the depth stream and
    /// refuse mapping steps toward a drop the map cannot see.
    pub cliff_guard: bool,
    /// tofd's socket, for the cliff guard.
    pub tof_socket: String,
    /// "Map everything" gives up after this many seconds.
    pub explore_max_s: f64,
    /// What the duck asks out loud when it reaches a nameless area while
    /// mapping everything; empty disables the asking.
    pub ask_phrase: String,
    /// Which way the explorer turns when the way on is blocked: `"right"`
    /// or `"left"`. The same hand every time is the right-hand rule — it
    /// gets around an obstacle and along a wall to the next doorway,
    /// where "the wider side" changed its mind at every leg.
    pub explore_turn: String,
}

impl MapConfig {
    /// The turning hand as a yaw sign: -1 for right, +1 for left.
    pub fn turn_sign(&self) -> f64 {
        if self.explore_turn.eq_ignore_ascii_case("left") { 1.0 } else { -1.0 }
    }
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            places_path: "/var/lib/quacksat/places.json".to_string(),
            cliff_guard: true,
            tof_socket: "/run/tofd/tof.sock".to_string(),
            explore_max_s: 1800.0,
            ask_phrase: "Where are we?".to_string(),
            explore_turn: "right".to_string(),
        }
    }
}

/// Waking up in a house the duck has mapped before (see
/// [`crate::homecoming`]).
///
/// Off by default: it drives the robot on its own at startup, which is not
/// something a satellite should do unless it was asked to.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HomecomingConfig {
    pub enabled: bool,
    /// How long to wait after startup before doing anything. The robot has
    /// to be switched on and standing first, and that is somebody else's
    /// job — a satellite that stood the robot up by itself at boot would
    /// be a surprise.
    pub start_delay_s: f64,
    /// How long to stand still at boot hoping the mapper confirms the pose
    /// it resumed on. Worth a minute because when it works it is exact —
    /// a duck switched on where it was switched off, or on its dock — and
    /// not worth more, because on this sensor it either happens early or
    /// not at all. Zero skips the attempt entirely.
    pub boot_search_s: f64,
    /// How often, while exploring, to ask whether the map so far fits
    /// inside a saved one. Mapping pauses for the length of the search.
    pub recognize_every_s: f64,
    /// The budget for the exploring the homecoming starts.
    pub explore_max_s: f64,
    /// Ask, and write down the answer, but never adopt. For measuring what
    /// the question actually answers in a house — including a house the
    /// duck has never seen, which is the only way to learn what a wrong
    /// answer looks like.
    pub dry_run: bool,
    /// The bar an answer must clear on its own before it can be adopted,
    /// whatever the rest of the library says: a duck may be in a house no
    /// map of its describes, and "none of these" has to be an answer.
    /// The map-to-map score is a mean wall distance on the overlap, in
    /// metres. Measured on the twin (2026-09-09): every right answer
    /// scored 0.043–0.138, every wrong one 0.187 or worse.
    pub adopt_max_score: f64,
    /// The runner-up's score over the winner's: how close the second-best
    /// place came. A near tie is a fresh map that fits two places — a
    /// room barely present in the saved map fitted its identical
    /// neighbour at 0.97 and 0.84 (2026-09-15) where the right answers
    /// of the same night sat at 0.23–0.68. Above this, wait and ask again.
    pub adopt_max_margin: f64,
    /// How much the live map must have grown between two agreeing asks,
    /// as a ratio of wall cells. 1.0: it must not have shrunk, no more —
    /// a map past its first minutes grows 4–35 % between asks with every
    /// answer right (2026-09-15: five identical right answers refused at
    /// 1.5, then again at 1.2), and the two wrong adoptions were caught
    /// by the margin and the overlap, not by growth. Kept as a knob.
    pub adopt_min_growth: f64,
    /// How many consecutive asks must agree (same map, same place within
    /// 0.3 m) before adoption. Three: on the 2026-09-09 series three in a
    /// row happened in 5 of 13 and 9 of 14 right runs and never for the
    /// wrong map, and it costs one more ask.
    pub adopt_asks: u32,
    /// The share of the live map's wall cells that land where the saved
    /// map has an opinion. Below this, most of what the duck has mapped
    /// lies in territory the saved map never saw — a room it does not
    /// hold — and whatever fits is the neighbour that looks the same:
    /// the office fitted the bedroom at 0.52–0.69 (2026-09-15) where
    /// every right adoption of the night sat at 0.74–0.81.
    pub adopt_min_overlap: f64,
}

impl Default for HomecomingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            start_delay_s: 15.0,
            boot_search_s: 60.0,
            recognize_every_s: 180.0,
            explore_max_s: 1800.0,
            dry_run: false,
            adopt_max_score: 0.16,
            adopt_max_margin: 0.80,
            adopt_min_growth: 1.0,
            adopt_asks: 3,
            adopt_min_overlap: 0.70,
        }
    }
}

/// When to paint scans into the map (`[maploc] mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaplocMode {
    /// Integrate only while the robot stands still: the frames of a stop
    /// are voted against each other before any of them inks the map.
    StopAndScan,
    /// Also integrate while walking: more coverage, blurrier walls.
    Continuous,
    /// The map is what it is: nothing is inked, the pose is corrected
    /// against the map as saved. For a house mapped once and driven many
    /// times.
    Localize,
}

impl MaplocMode {
    /// The spelling `robot.map` reports, as robotd's own `[maploc]` did.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StopAndScan => "stop_and_scan",
            Self::Continuous => "continuous",
            Self::Localize => "localize",
        }
    }
}

/// The mapper hosted in this daemon (`[maploc]`, see [`crate::mapd`]).
///
/// Off by default, and then the map comes from robotd's own `robot.map`,
/// as it did from the robotd fork. On, `quack-navd` runs `maploc` itself
/// against the released robotd — `robot.state` and tofd's stream in, the
/// same `robot.map*` dialect out on [`MaplocConfig::socket`] — and the
/// rest of the navigation reads its map there. The fields are robotd's
/// `[maploc]`, moved.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MaplocConfig {
    pub enabled: bool,
    pub mode: MaplocMode,
    /// Where the working session persists (autosaved each minute and on
    /// shutdown); the library of named maps is `maps/` beside it.
    pub map_path: String,
    /// Start from a clean slate instead of the saved session.
    pub wipe_on_boot: bool,
    /// Pan the head at every stop and while the pose is suspect: a stop
    /// then sees ~150° instead of one 45° wedge.
    pub search_sweep: bool,
    /// When set, every tick and frame the mapper consumed is appended to a
    /// `.mdlg` here, for `maploc`'s `evaluate` bench.
    pub record_dir: Option<String>,
    /// Where the map is served, in robotd's `robot.map*` dialect.
    pub socket: String,
}

impl Default for MaplocConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: MaplocMode::StopAndScan,
            map_path: "/var/lib/quack-nav/maploc.session".into(),
            wipe_on_boot: false,
            search_sweep: true,
            record_dir: None,
            socket: "/run/quack-nav/map.sock".into(),
        }
    }
}

/// The navigation daemon's own config file (`/etc/robot/quack-nav.toml`).
/// The satellite's `[map]`, `[gait]` and `[homecoming]` sections moved
/// here when the navigation left quacksat (2026-09-22).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NavdConfig {
    /// Where this daemon listens for callers.
    pub socket: String,
    /// Where robotd listens.
    pub robotd_socket: String,
    pub map: MapConfig,
    pub gait: quack_duck::gait::GaitConfig,
    pub homecoming: HomecomingConfig,
    pub maploc: MaplocConfig,
}

impl NavdConfig {
    /// Where `robot.map` and the map library answer: this daemon's own map
    /// socket when it hosts the mapper, robotd's otherwise.
    pub fn map_socket(&self) -> &str {
        if self.maploc.enabled { &self.maploc.socket } else { &self.robotd_socket }
    }
}

impl Default for NavdConfig {
    fn default() -> Self {
        Self {
            socket: "/run/quack-nav/nav.sock".into(),
            robotd_socket: "/run/robotd.sock".into(),
            map: MapConfig::default(),
            gait: quack_duck::gait::GaitConfig::default(),
            homecoming: HomecomingConfig::default(),
            maploc: MaplocConfig::default(),
        }
    }
}

impl NavdConfig {
    /// Read the file, or take the defaults when there is none: a daemon
    /// that cannot be started because a file is missing is a daemon
    /// somebody has to know about before the duck can walk.
    pub fn load(path: &str) -> anyhow::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(path, "no config file; the defaults it is");
                Ok(Self::default())
            }
            Err(e) => Err(e.into()),
        }
    }
}
