//! The places registry: names for coordinates (ADR 0005 §2).
//!
//! The robot recognizes geometry, not rooms. `maploc` gives it a pose in
//! the map frame; a *place* is a name somebody attached to a pose — by
//! voice ("this is the kitchen") or through the agent. Recognizing a place
//! is a distance check: the nearest anchor within the place's radius.
//! Teaching the same name from another spot adds an anchor, so a room can
//! be covered from several points.
//!
//! Places are only as durable as the map frame they were taught in. The
//! wire carries no session id, so the registry keeps its own **generation**
//! and bumps it when the map was evidently reset: the map lane's epoch
//! changed in-process (see [`crate::map::MapStatus::epoch`]), or robotd
//! reports fewer submaps than this registry has ever seen — a wipe that
//! happened while quacksat was down. Places from an older generation are
//! *stale*: listed, never matched, and replaced when re-taught. A robotd
//! restart that restores its saved session keeps the submap count, so it
//! does not bump. (Blind spot: a wipe of a one-submap map looks like a
//! restore; the next teach fixes it.)
//!
//! Persisted as JSON, written atomically, under quacksat's own state
//! directory — never in robotd's.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// A place taught without a radius covers a typical room from its middle.
pub const DEFAULT_RADIUS_M: f64 = 1.5;
pub const MIN_RADIUS_M: f64 = 0.3;
pub const MAX_RADIUS_M: f64 = 6.0;
/// Teaching again within this distance of an existing anchor refreshes
/// that anchor instead of piling up duplicates.
const ANCHOR_MERGE_M: f64 = 0.25;

const FILE_VERSION: u32 = 1;

/// One taught pose, in the map frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub x: f64,
    pub y: f64,
    pub yaw: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    /// As the user said it (display form); matching is case-insensitive.
    pub name: String,
    pub anchors: Vec<Anchor>,
    pub radius_m: f64,
    /// The registry generation this place was taught in.
    pub generation: u64,
    /// Unix seconds of the last teach.
    pub taught_unix: u64,
}

impl Place {
    /// Distance from `(x, y)` to the nearest anchor.
    pub fn distance_to(&self, x: f64, y: f64) -> f64 {
        self.anchors
            .iter()
            .map(|a| ((a.x - x).powi(2) + (a.y - y).powi(2)).sqrt())
            .fold(f64::INFINITY, f64::min)
    }
}

/// The nearest current-generation place to a pose.
#[derive(Debug, Clone, PartialEq)]
pub struct Nearest<'a> {
    pub place: &'a Place,
    pub distance_m: f64,
    /// Inside the place's radius: "the duck is *at* this place".
    pub within: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct File {
    version: u32,
    generation: u64,
    /// Most submaps ever reported by robotd in this generation.
    max_submaps: u32,
    places: Vec<Place>,
}

pub struct Registry {
    path: Option<PathBuf>,
    file: File,
    /// The map lane's epoch last folded in, to notice in-process resets.
    watch_epoch: Option<u64>,
}

impl Registry {
    /// Load from `path`; a missing file is an empty registry, an
    /// unreadable one is an error (silently starting empty would forget
    /// every place on a typo).
    pub fn load(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let file = match std::fs::read(&path) {
            Ok(bytes) => {
                let file: File = serde_json::from_slice(&bytes)
                    .with_context(|| format!("parsing places registry {}", path.display()))?;
                anyhow::ensure!(
                    file.version == FILE_VERSION,
                    "places registry {} is version {}, this build reads {FILE_VERSION}",
                    path.display(),
                    file.version
                );
                file
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => File {
                version: FILE_VERSION,
                ..File::default()
            },
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("reading places registry {}", path.display()));
            }
        };
        Ok(Self {
            path: Some(path),
            file,
            watch_epoch: None,
        })
    }

    /// A registry that lives only in memory (tests, no state directory).
    pub fn in_memory() -> Self {
        Self {
            path: None,
            file: File {
                version: FILE_VERSION,
                ..File::default()
            },
            watch_epoch: None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn generation(&self) -> u64 {
        self.file.generation
    }

    pub fn places(&self) -> &[Place] {
        &self.file.places
    }

    pub fn is_stale(&self, place: &Place) -> bool {
        place.generation != self.file.generation
    }

    /// Places taught in the current generation.
    pub fn current(&self) -> impl Iterator<Item = &Place> {
        self.file.places.iter().filter(|p| !self.is_stale(p))
    }

    /// Fold in what the map lane says. Returns true when the generation
    /// bumped — every place taught so far just went stale.
    pub fn observe(&mut self, watch_epoch: u64, n_submaps: u32) -> anyhow::Result<bool> {
        let new_mapper = self.watch_epoch.is_some_and(|seen| seen != watch_epoch);
        let fewer_submaps = n_submaps < self.file.max_submaps;
        self.watch_epoch = Some(watch_epoch);
        let mut changed = false;
        if new_mapper || fewer_submaps {
            self.file.generation += 1;
            self.file.max_submaps = n_submaps;
            tracing::info!(
                generation = self.file.generation,
                new_mapper,
                fewer_submaps,
                "places: the map was reset; remembered places are stale"
            );
            changed = true;
        } else if n_submaps > self.file.max_submaps {
            self.file.max_submaps = n_submaps;
            changed = true;
        }
        if changed {
            self.save()?;
        }
        Ok(new_mapper || fewer_submaps)
    }

    /// Teach `name` at `pose`. An existing current place gains (or
    /// refreshes) an anchor; a stale one is re-taught from scratch.
    pub fn remember(
        &mut self,
        name: &str,
        pose: (f64, f64, f64),
        radius_m: Option<f64>,
    ) -> anyhow::Result<&Place> {
        let name = name.trim();
        anyhow::ensure!(!name.is_empty(), "a place needs a name");
        let anchor = Anchor {
            x: pose.0,
            y: pose.1,
            yaw: pose.2,
        };
        let radius = radius_m.map(|r| r.clamp(MIN_RADIUS_M, MAX_RADIUS_M));
        let generation = self.file.generation;
        let now = unix_now();
        let idx = match self.find(name) {
            Some(i) if self.file.places[i].generation == generation => {
                let place = &mut self.file.places[i];
                match place.anchors.iter_mut().find(|a| {
                    ((a.x - anchor.x).powi(2) + (a.y - anchor.y).powi(2)).sqrt() < ANCHOR_MERGE_M
                }) {
                    Some(existing) => *existing = anchor,
                    None => place.anchors.push(anchor),
                }
                if let Some(r) = radius {
                    place.radius_m = r;
                }
                place.taught_unix = now;
                i
            }
            Some(i) => {
                let place = &mut self.file.places[i];
                place.anchors = vec![anchor];
                place.radius_m = radius.unwrap_or(place.radius_m);
                place.generation = generation;
                place.taught_unix = now;
                i
            }
            None => {
                self.file.places.push(Place {
                    name: name.to_owned(),
                    anchors: vec![anchor],
                    radius_m: radius.unwrap_or(DEFAULT_RADIUS_M),
                    generation,
                    taught_unix: now,
                });
                self.file.places.len() - 1
            }
        };
        self.save()?;
        Ok(&self.file.places[idx])
    }

    /// Forget by name; false when there was nothing to forget.
    pub fn forget(&mut self, name: &str) -> anyhow::Result<bool> {
        let Some(i) = self.find(name) else {
            return Ok(false);
        };
        self.file.places.remove(i);
        self.save()?;
        Ok(true)
    }

    /// The nearest current-generation place to `(x, y)`.
    pub fn nearest(&self, x: f64, y: f64) -> Option<Nearest<'_>> {
        self.current()
            .map(|place| (place, place.distance_to(x, y)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(place, distance_m)| Nearest {
                place,
                distance_m,
                within: distance_m <= place.radius_m,
            })
    }

    fn find(&self, name: &str) -> Option<usize> {
        let wanted = key(name);
        self.file.places.iter().position(|p| key(&p.name) == wanted)
    }

    fn save(&self) -> anyhow::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&self.file)?;
        std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    }
}

fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_place_is_by_distance_and_within_by_radius() {
        let mut reg = Registry::in_memory();
        reg.remember("Cucina", (0.0, 0.0, 0.0), None).unwrap();
        reg.remember("studio", (4.0, 0.0, 1.0), Some(1.0)).unwrap();

        let n = reg.nearest(0.5, 0.2).unwrap();
        assert_eq!(n.place.name, "Cucina");
        assert!(n.within);

        let n = reg.nearest(2.5, 0.0).unwrap();
        assert_eq!(n.place.name, "studio");
        assert!((n.distance_m - 1.5).abs() < 1e-9);
        assert!(!n.within, "1.5 m is outside studio's 1 m radius");

        assert!(Registry::in_memory().nearest(0.0, 0.0).is_none());
    }

    #[test]
    fn teaching_again_adds_an_anchor_and_matching_is_case_insensitive() {
        let mut reg = Registry::in_memory();
        reg.remember("cucina", (0.0, 0.0, 0.0), None).unwrap();
        reg.remember("CUCINA ", (2.0, 0.0, 0.0), None).unwrap();
        assert_eq!(reg.places().len(), 1);
        assert_eq!(reg.places()[0].anchors.len(), 2);
        assert_eq!(reg.places()[0].name, "cucina", "the first spelling is kept");
        // Near an existing anchor: refreshed, not duplicated.
        reg.remember("cucina", (2.1, 0.1, 0.5), Some(2.0)).unwrap();
        assert_eq!(reg.places()[0].anchors.len(), 2);
        assert_eq!(reg.places()[0].radius_m, 2.0);
        assert!((reg.places()[0].anchors[1].yaw - 0.5).abs() < 1e-9);
        // The room is now covered from both ends.
        assert!(reg.nearest(2.8, 0.0).unwrap().within);

        assert!(reg.forget("Cucina").unwrap());
        assert!(!reg.forget("cucina").unwrap());
        assert!(reg.remember("   ", (0.0, 0.0, 0.0), None).is_err());
    }

    #[test]
    fn radius_is_clamped() {
        let mut reg = Registry::in_memory();
        reg.remember("a", (0.0, 0.0, 0.0), Some(50.0)).unwrap();
        reg.remember("b", (0.0, 0.0, 0.0), Some(0.0)).unwrap();
        assert_eq!(reg.places()[0].radius_m, MAX_RADIUS_M);
        assert_eq!(reg.places()[1].radius_m, MIN_RADIUS_M);
    }

    #[test]
    fn a_map_reset_makes_places_stale_until_retaught() {
        let mut reg = Registry::in_memory();
        assert!(!reg.observe(0, 3).unwrap());
        reg.remember("cucina", (0.0, 0.0, 0.0), None).unwrap();
        // The map grows: nothing happens.
        assert!(!reg.observe(0, 7).unwrap());
        assert!(reg.nearest(0.0, 0.0).is_some());
        // A robotd restart restoring its session keeps the submaps: same
        // frame, and the in-process watcher epoch is still 0.
        assert!(!reg.observe(0, 7).unwrap());
        // A wipe: fewer submaps than ever seen.
        assert!(reg.observe(0, 1).unwrap());
        assert_eq!(reg.generation(), 1);
        assert!(reg.is_stale(&reg.places()[0]));
        assert!(reg.nearest(0.0, 0.0).is_none(), "stale places never match");
        assert_eq!(reg.places().len(), 1, "but they stay listed");
        // Re-teaching replaces the stale anchors.
        reg.remember("cucina", (5.0, 5.0, 0.0), None).unwrap();
        let place = &reg.places()[0];
        assert_eq!(place.generation, 1);
        assert_eq!(place.anchors.len(), 1);
        assert!(!reg.is_stale(place));
        // An in-process epoch change (the watcher saw a reset) also bumps.
        assert!(reg.observe(1, 1).unwrap());
        assert_eq!(reg.generation(), 2);
    }

    #[test]
    fn registry_round_trips_through_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state").join("places.json");
        {
            let mut reg = Registry::load(&path).unwrap();
            assert!(reg.places().is_empty());
            reg.observe(0, 4).unwrap();
            reg.remember("bagno", (1.0, 2.0, 3.0), Some(1.0)).unwrap();
        }
        let mut reg = Registry::load(&path).unwrap();
        assert_eq!(reg.places().len(), 1);
        assert_eq!(reg.places()[0].anchors[0].y, 2.0);
        // The wipe that happened while quacksat was down shows as fewer
        // submaps than the file remembers.
        assert!(reg.observe(0, 1).unwrap());
        assert!(reg.is_stale(&reg.places()[0]));

        std::fs::write(&path, b"{not json").unwrap();
        assert!(Registry::load(&path).is_err());
    }
}
