//! The map library's wire types, as the robotd fork's `duck-ipc-proto`
//! spelled them (`maploc-quacknav`, docs/study/upstream-asks.md §5): no
//! released proto crate carries them, and the map socket must answer the
//! callers that already speak them — this crate's own tools, the homecoming,
//! the twin's viewer — byte for byte.

use serde::{Deserialize, Serialize};

pub const METHOD_ROBOT_MAP_SAVE: &str = "robot.map_save";
pub const METHOD_ROBOT_MAP_LIST: &str = "robot.map_list";
pub const METHOD_ROBOT_MAP_LOAD: &str = "robot.map_load";
pub const METHOD_ROBOT_MAP_MATCH: &str = "robot.map_match";
pub const METHOD_ROBOT_MAP_ADOPT: &str = "robot.map_adopt";
/// quack-navd's own: freeze the live map once the house is mapped.
pub const METHOD_QUACK_MAP_FREEZE: &str = "quack.map_freeze";

/// Which saved map. The name becomes a file name, so it is checked before
/// it does: at most 64 letters, digits, `-` and `_`. A name with a slash or
/// a dot in it is refused rather than sanitised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapNameParams {
    pub name: String,
}

impl MapNameParams {
    pub fn is_valid(&self) -> bool {
        valid_name(&self.name)
    }
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// One saved map in the answer to `robot.map_list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedMap {
    pub name: String,
    /// Bytes on disk.
    pub bytes: u64,
    /// Seconds since the Unix epoch when it was last written.
    pub saved_at: u64,
}

/// The answer to `robot.map_list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedMaps {
    pub maps: Vec<SavedMap>,
}

/// Which saved maps to try: one by name, or the whole library.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapMatchParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// One place a saved map might sit under the live one: the transform that
/// carries a point on the live map to its twin on the saved one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapMatch {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub yaw: f32,
    /// Mean distance from a live wall cell to the nearest saved wall.
    pub wall_residual_m: f32,
    /// Share of the live walls that land where the saved map has an opinion.
    pub overlap: f32,
    /// Share of the live floor this transform lays on a saved wall.
    pub floor_on_wall: f32,
    /// Walls and floor as one number, lower better, within one ask.
    pub score: f32,
    /// This candidate's score over the next best in the same saved map.
    #[serde(default = "one")]
    pub margin: f32,
}

fn one() -> f32 {
    1.0
}

/// The answer to `robot.map_match`: candidates, best first — not a verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapMatches {
    /// How many wall cells the live map offered. Small means "too early".
    pub live_cells: u32,
    pub matches: Vec<MapMatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl MapMatches {
    pub fn none(reason: impl Into<String>) -> Self {
        Self { live_cells: 0, matches: Vec::new(), reason: Some(reason.into()) }
    }
}

/// Adopt this saved map at this transform (one `robot.map_match` reported).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapAdoptParams {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub yaw: f32,
}

/// Standard base64 with padding, the encoding `map.frame` cells travel in
/// ([`crate::map::b64_decode`] is its reader).
pub fn b64_encode(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_through_the_map_reader() {
        for s in [&b""[..], b"f", b"fo", b"foo", b"foobar"] {
            assert_eq!(crate::map::b64_decode(&b64_encode(s)).as_deref(), Some(s));
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(crate::map::b64_decode(&b64_encode(&all)), Some(all));
        assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_name_that_could_leave_the_directory_is_refused() {
        assert!(valid_name("house-2_a"));
        for bad in ["", "../x", "a.b", "a/b", &"x".repeat(65)] {
            assert!(!valid_name(bad), "{bad}");
        }
    }
}
