//! quack-nav: where the Microduck is, and how it gets somewhere else.
//!
//! Eight pieces (and their [`config`]), none of them about voice:
//!
//! - [`map`] — the `robot.map` client: robotd's on-board `maploc` publishes
//!   a pose and an occupancy grid once a second; this keeps the newest one
//!   and notices when the map frame was reset.
//! - [`places`] — the registry: a *place* is a name somebody attached to a
//!   pose in that frame, recognized by distance. The robot knows geometry;
//!   the names come from people.
//! - [`cliff`] — the cliff guard: the depth sensor's downward beams judged
//!   against the floor, because a map of walls cannot see a staircase.
//! - [`frontier`] — where the known floor meets the unknown, and the path
//!   there: what "map everything" loops over.
//! - [`passage`] — threading a narrow passage: two side boundaries and
//!   the steering that keeps the body between them.
//! - [`tools`] — the twelve `robot.*` tools (the places, the map, the
//!   explorer, `go_to`, saved maps and `map_match`) as an agent-neutral
//!   catalog (JSON Schema) plus an executor, ready to be projected onto
//!   OpenAI tools, MCP, or anything else by whoever hosts them.
//! - [`explore`] — the jobs that drive: map a house, walk to a goal on a
//!   map already made, and the rules that keep a leg off the stairs.
//! - [`homecoming`] — waking up in a house the duck has mapped before.
//!
//! The crate depends on the duck's IPC types, its head geometry crate
//! (`kinematics`, pure Rust), the robot lane (`quack-duck`) and serde,
//! nothing else: it is hosted by `quack-navd`, a daemon of its own
//! (2026-09-22), and the voice satellite reaches it over a socket.

pub mod cliff;
pub mod config;
pub mod explore;
pub mod frontier;
pub mod homecoming;
pub mod map;
pub mod passage;
pub mod places;
pub mod tools;

pub use cliff::CliffWatch;
pub use config::MapConfig;
pub use map::{MapFrame, MapStatus, MapWatch};
pub use places::Registry;
pub use tools::Places;
