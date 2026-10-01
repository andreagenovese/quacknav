//! The places registry: names for coordinates (ADR 0005 §2).
//!
//! The robot recognizes geometry, not rooms. `maploc` gives it a pose in
//! the map frame; a *place* is a name somebody attached to a pose — by
//! voice ("this is the kitchen") or through the agent. Recognizing a place
//! is a distance check: the nearest anchor within the place's radius.
//! Teaching the same name from another spot adds an anchor, so a room can
//! be covered from several points.
//!
//! ## A place belongs to a map
//!
//! Coordinates mean something only on the map they were taught on, so
//! every place carries that map's **lineage**: an id minted here whenever
//! a map starts from nothing (a wipe, a fresh exploration, a reset the map
//! lane saw), kept with the map's name when it is saved to the library and
//! taken back when it is loaded or adopted. The wire carries no session
//! id, and the library's files none either; quack-nav keeps its own books,
//! as it does for the drops (`ground.json`). A place is
//!
//! - **usable** when its lineage is the live map's and the duck has had a
//!   trusted pose on it since it became the live one;
//! - **pending** while that is not known yet: the live map unconfirmed, or
//!   which map is live not known at all — at boot, until the homecoming
//!   has loaded and confirmed a saved map. Never matched, never stale;
//! - **other_map** when it belongs to a saved map that is not the live one
//!   (another house, or the live map was wiped): back as soon as that map
//!   is loaded or adopted again;
//! - **stale** when no map of its lineage is left: the map it was taught on
//!   was saved over by one started from nothing (a fresh exploration under
//!   the same name), or wiped and never saved. Listed, never matched, and
//!   replaced when re-taught.
//!
//! Until 2026-10-01 the registry kept a single generation and bumped it
//! whenever robotd reported fewer submaps than ever seen. Every boot with
//! the homecoming starts on a fresh map and only then loads the saved one,
//! so any tool call in between made every place stale: one "the map was
//! reset" per session, and "go to the kitchen" refused after each power-on.
//!
//! Persisted as JSON, written atomically, under quack-nav's own state
//! directory (`/var/lib/quack-nav`) — never in robotd's.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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

const FILE_VERSION: u32 = 2;
/// A map reset the lane sees within this many frames of one asked for
/// here (a load, a wipe) is that one: the new map's first frame comes a
/// second or two after the ask.
const GRACE_FRAMES: u64 = 5;
/// The lineage of a version-1 place that was already stale: nothing is.
const RETIRED: &str = "retired";

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
    /// The lineage of the map this place was taught on.
    pub map: String,
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

/// Whether a place can be used now (see the module's notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceState {
    Usable,
    Pending,
    OtherMap,
    Stale,
}

impl PlaceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Usable => "usable",
            Self::Pending => "pending",
            Self::OtherMap => "other_map",
            Self::Stale => "stale",
        }
    }

    /// What the `stale` flag of `robot.list_places` has always meant: not
    /// on the map in hand. A pending place is not stale — it is waiting.
    pub fn is_stale(self) -> bool {
        matches!(self, Self::OtherMap | Self::Stale)
    }
}

/// The nearest usable place to a pose.
#[derive(Debug, Clone, PartialEq)]
pub struct Nearest<'a> {
    pub place: &'a Place,
    pub distance_m: f64,
    /// Inside the place's radius: "the duck is *at* this place".
    pub within: bool,
}

/// What the map lane says, folded in by [`Registry::observe`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Look {
    /// [`crate::map::MapStatus::epoch`] and `epoch_frames`.
    pub epoch: u64,
    pub epoch_frames: u64,
    /// Frames the lane has received.
    pub frames: u64,
    pub n_submaps: u32,
    /// The lane trusts the pose (tracking, standing).
    pub trusted: bool,
}

impl Look {
    pub fn of(status: &crate::map::MapStatus) -> Option<Self> {
        let frame = status.latest.as_ref()?;
        Some(Self {
            epoch: status.epoch,
            epoch_frames: status.epoch_frames,
            frames: status.frames,
            n_submaps: frame.n_submaps,
            trusted: status.trusted_pose().is_some(),
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct File {
    version: u32,
    /// The saved maps this registry has seen saved, loaded or adopted:
    /// name → lineage.
    #[serde(default)]
    maps: BTreeMap<String, String>,
    /// The live map when last known: its lineage, its saved name if it has
    /// one, its submaps — so a restart of quack-navd alone, or a boot that
    /// resumes the working session, finds the map it left.
    #[serde(default)]
    live: Option<String>,
    #[serde(default)]
    live_name: Option<String>,
    #[serde(default)]
    live_submaps: u32,
    /// The lineage of the places carried over from a version-1 file, which
    /// knew no map names: the first saved map the duck confirms itself on
    /// that this registry had never seen takes them.
    #[serde(default)]
    unclaimed: Option<String>,
    places: Vec<Place>,
}

/// The version-1 file: one generation for everything.
#[derive(Deserialize)]
struct FileV1 {
    generation: u64,
    max_submaps: u32,
    places: Vec<PlaceV1>,
}

#[derive(Deserialize)]
struct PlaceV1 {
    name: String,
    anchors: Vec<Anchor>,
    radius_m: f64,
    generation: u64,
    taught_unix: u64,
}

impl FileV1 {
    /// The places of the current generation keep together under one new
    /// lineage, unclaimed, and the live map is assumed to be theirs (as
    /// version 1 assumed while the submaps held). Stale ones stay stale: a
    /// version-1 bump cannot tell a real reset from the boot's false one,
    /// and a wrong place walks the duck into the wrong room — re-teaching
    /// is the cheap side.
    fn migrate(self) -> File {
        let lineage = mint();
        let current = self.places.iter().any(|p| p.generation == self.generation);
        let places = self
            .places
            .into_iter()
            .map(|p| Place {
                map: if p.generation == self.generation { lineage.clone() } else { RETIRED.to_owned() },
                name: p.name,
                anchors: p.anchors,
                radius_m: p.radius_m,
                taught_unix: p.taught_unix,
            })
            .collect();
        File {
            version: FILE_VERSION,
            maps: BTreeMap::new(),
            live: Some(lineage.clone()),
            live_name: None,
            live_submaps: self.max_submaps,
            unclaimed: current.then_some(lineage),
            places,
        }
    }
}

pub struct Registry {
    path: Option<PathBuf>,
    file: File,
    /// The live map's lineage in this run; `None` until it is known.
    live: Option<String>,
    /// A trusted pose was seen on the live map since it became the live one.
    confirmed: bool,
    /// The homecoming has not settled yet: which map is live is its call.
    awaiting: bool,
    /// The lane's frame count when this registry last changed the live map
    /// itself; `None` when it never did this run.
    asked_at: Option<u64>,
    /// The map lane's epoch last folded in, to notice in-process resets.
    watch_epoch: Option<u64>,
    /// A saved map loaded for the first time while version-1 places wait:
    /// confirmed on, it takes them.
    claim: Option<String>,
}

impl Registry {
    /// Load from `path`; a missing file is an empty registry, an
    /// unreadable one is an error (silently starting empty would forget
    /// every place on a typo). A version-1 file is migrated in memory and
    /// written as version 2 at the next change.
    pub fn load(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let file = match std::fs::read(&path) {
            Ok(bytes) => {
                let value: serde_json::Value = serde_json::from_slice(&bytes)
                    .with_context(|| format!("parsing places registry {}", path.display()))?;
                match value.get("version").and_then(serde_json::Value::as_u64) {
                    Some(1) => {
                        let v1: FileV1 = serde_json::from_value(value)
                            .with_context(|| format!("parsing places registry {}", path.display()))?;
                        let file = v1.migrate();
                        tracing::info!(
                            path = %path.display(),
                            places = file.places.len(),
                            "places: a version-1 registry; its current places wait for the first saved map the duck confirms itself on"
                        );
                        file
                    }
                    Some(v) if v == FILE_VERSION as u64 => serde_json::from_value(value)
                        .with_context(|| format!("parsing places registry {}", path.display()))?,
                    v => anyhow::bail!(
                        "places registry {} is version {}, this build reads 1 and {FILE_VERSION}",
                        path.display(),
                        v.map_or("?".to_owned(), |v| v.to_string())
                    ),
                }
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
        Ok(Self::with(Some(path), file))
    }

    /// A registry that lives only in memory (tests, no state directory).
    pub fn in_memory() -> Self {
        Self::with(
            None,
            File {
                version: FILE_VERSION,
                ..File::default()
            },
        )
    }

    fn with(path: Option<PathBuf>, file: File) -> Self {
        Self {
            path,
            file,
            live: None,
            confirmed: false,
            awaiting: false,
            asked_at: None,
            watch_epoch: None,
            claim: None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn places(&self) -> &[Place] {
        &self.file.places
    }

    /// The saved name of the live map, when it is known and has one.
    pub fn live_map(&self) -> Option<&str> {
        self.live.as_ref().and(self.file.live_name.as_deref())
    }

    /// The saved map a place belongs to, by name; `None` for a map never
    /// saved (or since saved over).
    pub fn map_of(&self, place: &Place) -> Option<&str> {
        self.file
            .maps
            .iter()
            .find(|(_, lineage)| **lineage == place.map)
            .map(|(name, _)| name.as_str())
    }

    pub fn state(&self, place: &Place) -> PlaceState {
        if self.live.as_ref() == Some(&place.map) {
            return if self.confirmed { PlaceState::Usable } else { PlaceState::Pending };
        }
        let saved = self.file.maps.values().any(|l| *l == place.map);
        let unclaimed = self.file.unclaimed.as_ref() == Some(&place.map);
        let last_live = self.file.live.as_ref() == Some(&place.map);
        if (self.live.is_none() || self.awaiting) && (saved || unclaimed || last_live) {
            return PlaceState::Pending;
        }
        if unclaimed {
            // Waiting for its map's first load: no saved map it could be
            // on has been confirmed yet.
            return PlaceState::Pending;
        }
        if saved { PlaceState::OtherMap } else { PlaceState::Stale }
    }

    /// Not usable on the map in hand (see [`PlaceState::is_stale`]).
    pub fn is_stale(&self, place: &Place) -> bool {
        self.state(place).is_stale()
    }

    /// The usable places.
    pub fn current(&self) -> impl Iterator<Item = &Place> {
        self.file
            .places
            .iter()
            .filter(|p| self.state(p) == PlaceState::Usable)
    }

    /// A place by name, whatever its state.
    pub fn named(&self, name: &str) -> Option<&Place> {
        self.find(name).map(|i| &self.file.places[i])
    }

    /// The homecoming is about to decide which map is live: until it has
    /// settled, places wait instead of being judged on the boot's fresh map.
    pub fn await_homecoming(&mut self) {
        self.awaiting = true;
    }

    /// The homecoming is over, home or not.
    pub fn homecoming_settled(&mut self) {
        self.awaiting = false;
    }

    /// The saved map `name` is the live one now (`robot.map_load`,
    /// `robot.map_adopt`): its places come back once the pose is confirmed
    /// on it. `frames` is the map lane's count when it was asked.
    pub fn loaded(&mut self, name: &str, frames: u64) -> anyhow::Result<()> {
        let lineage = match self.file.maps.get(name) {
            Some(lineage) => lineage.clone(),
            None => {
                let lineage = mint();
                self.file.maps.insert(name.to_owned(), lineage.clone());
                lineage
            }
        };
        self.claim = (self.file.unclaimed.is_some() && !self.claimed_before(name)).then(|| name.to_owned());
        self.go_live(lineage, Some(name.to_owned()), frames);
        tracing::info!(map = name, "places: the saved map is the live one; its places come back once the pose is confirmed");
        self.save()
    }

    /// The live map starts from nothing (`robot.map_wipe`, a fresh
    /// exploration, the boot's search): a new lineage. Places stay with
    /// the maps they were taught on.
    pub fn started_afresh(&mut self, frames: u64) -> anyhow::Result<()> {
        self.claim = None;
        self.go_live(mint(), None, frames);
        tracing::info!("places: the live map starts from nothing; known places stay with their saved maps");
        self.save()
    }

    /// The live map was saved to the library as `name`: the name now means
    /// this map. Places of the map it replaced under that name are stale
    /// from here on — unless that was the same map, saved again.
    pub fn saved_as(&mut self, name: &str) -> anyhow::Result<()> {
        let lineage = match &self.live {
            Some(lineage) => lineage.clone(),
            None => {
                let frames = self.asked_at.unwrap_or(0);
                self.go_live(mint(), None, frames);
                self.live.clone().expect("just set")
            }
        };
        if let Some(before) = self.file.maps.insert(name.to_owned(), lineage.clone())
            && before != lineage
        {
            tracing::info!(map = name, "places: a new map saved over the old one under its name; the old one's places are stale");
        }
        if self.file.unclaimed.as_ref() == Some(&lineage) {
            self.file.unclaimed = None;
        }
        self.file.live_name = Some(name.to_owned());
        self.save()
    }

    /// Fold in what the map lane says: a reset nobody asked for here, the
    /// first look of this run, a confirmation.
    pub fn observe(&mut self, look: Look) -> anyhow::Result<()> {
        let mut changed = false;
        // A reset the lane saw that nothing here asked for — robotd
        // restarted on a fresh map, or a client wiped it directly: the live
        // map is a new one. Nothing is stale for it; places stay with
        // their maps.
        let asked = self.asked_at.is_some_and(|at| look.epoch_frames <= at + GRACE_FRAMES);
        if self.watch_epoch.is_some_and(|seen| seen != look.epoch) && !asked && self.live.is_some() {
            tracing::info!("places: the map lane saw the map start over; known places stay with their saved maps");
            self.claim = None;
            self.go_live(mint(), None, look.frames);
            changed = true;
        }
        self.watch_epoch = Some(look.epoch);
        // The first look of this run, with no homecoming to decide: the
        // map quack-navd left, if the submaps are all still there (robotd
        // kept or resumed it), else one started afresh while it was down.
        if self.live.is_none() && !self.awaiting {
            match self.file.live.clone() {
                Some(lineage) if look.n_submaps >= self.file.live_submaps => {
                    tracing::info!(map = self.file.live_name.as_deref().unwrap_or("-"), "places: the live map is the one left at the last run");
                    self.live = Some(lineage);
                }
                _ => {
                    tracing::info!("places: the live map is not the one left at the last run; known places stay with their saved maps");
                    self.file.live = Some(mint());
                    self.file.live_name = None;
                    self.live = self.file.live.clone();
                }
            }
            self.confirmed = false;
            changed = true;
        }
        if self.live.is_some() {
            // Only a frame drawn after the change can vouch for the map it
            // brought: the one in hand when a load returns is the old map's.
            let after = self.asked_at.is_none_or(|at| look.frames > at + 1);
            if look.trusted && after && !self.confirmed {
                self.confirmed = true;
                changed |= self.settle_claim();
            }
            let settled = self.asked_at.is_none_or(|at| look.frames > at + GRACE_FRAMES);
            if settled && look.n_submaps != self.file.live_submaps {
                self.file.live_submaps = look.n_submaps;
                changed = true;
            }
        }
        if changed {
            self.save()?;
        }
        Ok(())
    }

    /// Whether a place can be taught now: the live map must be known.
    pub fn can_teach(&self) -> Result<(), String> {
        if self.live.is_none() {
            return Err("the duck does not know yet which map it is on (it is still finding itself on its saved map): \
                        let it stand still and look around, then teach the place"
                .into());
        }
        // The homecoming's search: a fresh map thrown away when the saved
        // one is adopted, and every place taught on it with it.
        if self.awaiting && self.file.live_name.is_none() {
            return Err("the duck is still looking for itself on its saved map, and the map in hand is that search's, \
                        thrown away when it finds itself: teach the place once it has (robot.map_status)"
                .into());
        }
        Ok(())
    }

    /// Teach `name` at `pose` on the live map. A place of this map gains
    /// (or refreshes) an anchor; one of another map, or stale, is re-taught
    /// here from scratch.
    pub fn remember(
        &mut self,
        name: &str,
        pose: (f64, f64, f64),
        radius_m: Option<f64>,
    ) -> anyhow::Result<&Place> {
        let name = name.trim();
        anyhow::ensure!(!name.is_empty(), "a place needs a name");
        self.can_teach().map_err(anyhow::Error::msg)?;
        let map = self.live.clone().expect("checked by can_teach");
        let anchor = Anchor {
            x: pose.0,
            y: pose.1,
            yaw: pose.2,
        };
        let radius = radius_m.map(|r| r.clamp(MIN_RADIUS_M, MAX_RADIUS_M));
        let now = unix_now();
        let idx = match self.find(name) {
            Some(i) if self.file.places[i].map == map => {
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
                place.map = map;
                place.taught_unix = now;
                i
            }
            None => {
                self.file.places.push(Place {
                    name: name.to_owned(),
                    anchors: vec![anchor],
                    radius_m: radius.unwrap_or(DEFAULT_RADIUS_M),
                    map,
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

    /// The nearest usable place to `(x, y)`.
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

    fn go_live(&mut self, lineage: String, name: Option<String>, frames: u64) {
        if self.claim.as_deref() != name.as_deref() {
            self.claim = None;
        }
        self.live = Some(lineage.clone());
        self.file.live = Some(lineage);
        self.file.live_name = name;
        self.file.live_submaps = 0;
        self.confirmed = false;
        self.asked_at = Some(frames);
    }

    /// A name this registry knew before the version-1 places came in has
    /// its own places already; only a map first seen since may claim them.
    fn claimed_before(&self, name: &str) -> bool {
        let Some(lineage) = self.file.maps.get(name) else { return false };
        self.file.places.iter().any(|p| p.map == *lineage)
    }

    /// The pose is confirmed on a saved map loaded for the first time:
    /// the version-1 places are its.
    /// Or the live map is the one they were left on (no homecoming, the
    /// working session resumed): they are its, and no other map claims them.
    fn settle_claim(&mut self) -> bool {
        let (Some(unclaimed), Some(live)) = (self.file.unclaimed.clone(), self.live.clone()) else {
            return false;
        };
        if live == unclaimed {
            self.file.unclaimed = None;
            self.claim = None;
            return true;
        }
        let Some(name) = self.claim.take() else {
            return false;
        };
        if self.file.maps.get(&name) != Some(&live) {
            return false;
        }
        let mut n = 0;
        for place in self.file.places.iter_mut().filter(|p| p.map == unclaimed) {
            place.map = live.clone();
            n += 1;
        }
        self.file.unclaimed = None;
        tracing::info!(map = name, places = n, "places: the places of the version-1 registry are this map's");
        true
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

/// A new lineage: the time it was minted, and a counter for two in the
/// same nanosecond.
fn mint() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("m{nanos:x}-{}", N.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A look at the map lane: no reset since frame `epoch_frames`.
    fn look(epoch: u64, frames: u64, n_submaps: u32, trusted: bool) -> Look {
        Look { epoch, epoch_frames: 0, frames, n_submaps, trusted }
    }

    /// An in-memory registry whose live map is known and confirmed.
    fn live() -> Registry {
        let mut reg = Registry::in_memory();
        reg.observe(look(0, 1, 3, true)).unwrap();
        reg
    }

    fn state_of(reg: &Registry, name: &str) -> PlaceState {
        reg.state(reg.named(name).unwrap())
    }

    #[test]
    fn nearest_place_is_by_distance_and_within_by_radius() {
        let mut reg = live();
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
        let mut reg = live();
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
        let mut reg = live();
        reg.remember("a", (0.0, 0.0, 0.0), Some(50.0)).unwrap();
        reg.remember("b", (0.0, 0.0, 0.0), Some(0.0)).unwrap();
        assert_eq!(reg.places()[0].radius_m, MAX_RADIUS_M);
        assert_eq!(reg.places()[1].radius_m, MIN_RADIUS_M);
    }

    /// Run 1 teaches on the saved map `casa`; the file is what the next
    /// run starts from.
    fn taught_on_casa(path: &Path) {
        let mut reg = Registry::load(path).unwrap();
        reg.observe(look(0, 1, 2, true)).unwrap();
        reg.loaded("casa", 10).unwrap();
        // The frame in hand when the load returns is the old map's: it
        // vouches for nothing.
        reg.observe(look(0, 11, 9, true)).unwrap();
        reg.remember("cucina", (1.0, 1.0, 0.0), None).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Pending, "taught at once, confirmed on the next frame");
        reg.observe(look(0, 12, 9, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Usable);
        reg.observe(look(0, 30, 12, true)).unwrap();
        assert_eq!(reg.map_of(reg.named("cucina").unwrap()), Some("casa"));
    }

    #[test]
    fn a_reboot_on_a_fresh_map_then_the_saved_one_keeps_every_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.json");
        taught_on_casa(&path);

        // The boot: robotd (wipe_on_boot) starts on a fresh map, one
        // submap, tracking at once; the homecoming has not loaded yet.
        let mut reg = Registry::load(&path).unwrap();
        reg.await_homecoming();
        for frames in 1..8 {
            reg.observe(look(0, frames, 1, true)).unwrap();
            assert_eq!(state_of(&reg, "cucina"), PlaceState::Pending, "pending, never stale");
            assert!(!reg.is_stale(reg.named("cucina").unwrap()));
            assert!(reg.nearest(1.0, 1.0).is_none());
        }
        assert!(reg.can_teach().is_err(), "nothing is taught on a map nobody knows");
        // "homecoming: loaded the newest map": unconfirmed, still pending.
        reg.loaded("casa", 8).unwrap();
        reg.observe(look(0, 9, 12, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Pending);
        // "confirmed … on the saved map": usable, as taught.
        reg.observe(look(0, 10, 12, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Usable);
        assert_eq!(reg.nearest(1.0, 1.0).unwrap().place.name, "cucina");
        reg.homecoming_settled();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Usable);

        // The other way home: not confirmed, a fresh map to search from,
        // the saved one adopted minutes later.
        let mut reg = Registry::load(&path).unwrap();
        reg.await_homecoming();
        reg.observe(look(0, 1, 1, true)).unwrap();
        reg.loaded("casa", 2).unwrap();
        reg.observe(look(0, 30, 12, false)).unwrap();
        reg.started_afresh(60).unwrap();
        // The wipe's reset reaches the lane a frame later: asked for here.
        reg.observe(Look { epoch: 1, epoch_frames: 61, frames: 62, n_submaps: 1, trusted: true }).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Pending, "the search is not a verdict");
        assert!(reg.can_teach().is_err(), "the search's map is thrown away");
        reg.observe(look(1, 300, 6, true)).unwrap();
        reg.loaded("casa", 400).unwrap();
        reg.observe(look(1, 402, 12, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Usable);
    }

    #[test]
    fn a_wipe_parks_places_and_a_new_map_saved_over_theirs_makes_them_stale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.json");
        taught_on_casa(&path);
        let mut reg = Registry::load(&path).unwrap();
        reg.observe(look(0, 1, 12, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Usable, "quack-navd restarted alone: the map it left");

        // robot.map_wipe: the live map starts over; `casa` is still in the
        // library, so its places wait for it rather than die.
        reg.started_afresh(5).unwrap();
        reg.observe(Look { epoch: 1, epoch_frames: 6, frames: 8, n_submaps: 1, trusted: true }).unwrap();
        let state = state_of(&reg, "cucina");
        assert_eq!(state, PlaceState::OtherMap);
        assert!(state.is_stale(), "the old flag: not on this map");
        assert!(reg.nearest(1.0, 1.0).is_none());
        // A fresh exploration saves over `casa`: that map is gone.
        reg.observe(look(1, 100, 7, true)).unwrap();
        reg.saved_as("casa").unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Stale);
        reg.loaded("casa", 120).unwrap();
        reg.observe(look(1, 122, 7, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Stale, "the new `casa` is another map");
        // Re-teaching puts it on the map in hand.
        reg.remember("cucina", (5.0, 5.0, 0.0), None).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Usable);
        assert_eq!(reg.named("cucina").unwrap().anchors.len(), 1);

        // A reset nobody asked for (robotd restarted on a fresh map):
        // a new live map, never saved; a place taught on it dies with it.
        reg.remember("bagno", (0.0, 0.0, 0.0), None).unwrap();
        reg.started_afresh(130).unwrap();
        reg.observe(look(2, 140, 1, true)).unwrap();
        reg.remember("ripostiglio", (0.0, 0.0, 0.0), None).unwrap();
        assert_eq!(state_of(&reg, "ripostiglio"), PlaceState::Usable);
        reg.observe(Look { epoch: 3, epoch_frames: 300, frames: 301, n_submaps: 1, trusted: true }).unwrap();
        assert_eq!(state_of(&reg, "ripostiglio"), PlaceState::Stale);
        assert_eq!(state_of(&reg, "cucina"), PlaceState::OtherMap);
    }

    #[test]
    fn another_house_parks_places_and_coming_back_restores_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.json");
        taught_on_casa(&path);
        let mut reg = Registry::load(&path).unwrap();
        reg.observe(look(0, 1, 12, true)).unwrap();
        reg.loaded("mare", 10).unwrap();
        reg.observe(look(0, 12, 5, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::OtherMap);
        assert_eq!(reg.live_map(), Some("mare"));
        reg.remember("terrazza", (2.0, 0.0, 0.0), None).unwrap();
        assert_eq!(reg.nearest(1.0, 1.0).unwrap().place.name, "terrazza", "casa's kitchen is not here");
        reg.saved_as("mare").unwrap();

        reg.loaded("casa", 20).unwrap();
        reg.observe(look(0, 22, 12, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Usable);
        assert_eq!(state_of(&reg, "terrazza"), PlaceState::OtherMap);
        assert_eq!(reg.map_of(reg.named("terrazza").unwrap()), Some("mare"));
    }

    #[test]
    fn a_version_1_file_loads_and_its_places_go_to_the_first_map_confirmed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.json");
        // The twin's file of 2026-10-01: generation 7, the kitchen taught
        // in it, the living room two false bumps ago.
        let v1 = r#"{"version": 1, "generation": 7, "max_submaps": 5, "places": [
            {"name": "cucina", "anchors": [{"x": 1.0, "y": 2.0, "yaw": 0.0}], "radius_m": 1.5, "generation": 7, "taught_unix": 1},
            {"name": "soggiorno", "anchors": [{"x": 4.0, "y": 2.0, "yaw": 0.0}], "radius_m": 1.5, "generation": 5, "taught_unix": 1}]}"#;
        std::fs::write(&path, v1).unwrap();

        let mut reg = Registry::load(&path).unwrap();
        assert_eq!(reg.places().len(), 2);
        assert_eq!(reg.places()[0].anchors[0].y, 2.0);
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Pending);
        assert_eq!(state_of(&reg, "soggiorno"), PlaceState::Stale, "stale in version 1, stale still");
        reg.await_homecoming();
        reg.observe(look(0, 1, 1, true)).unwrap();
        // The newest map does not confirm; another one is adopted, and
        // confirmed: that one takes the kitchen.
        reg.loaded("casa_arredata", 2).unwrap();
        reg.observe(look(0, 40, 9, false)).unwrap();
        reg.started_afresh(41).unwrap();
        reg.observe(look(0, 43, 1, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Pending);
        reg.loaded("casa_grande", 300).unwrap();
        reg.observe(look(0, 302, 14, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Usable);
        assert_eq!(reg.map_of(reg.named("cucina").unwrap()), Some("casa_grande"));
        reg.homecoming_settled();

        // Written back as version 2; the next boot finds it on its map.
        let written: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(written["version"], 2);
        let mut reg = Registry::load(&path).unwrap();
        reg.await_homecoming();
        reg.loaded("casa_arredata", 2).unwrap();
        reg.observe(look(0, 4, 9, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Pending, "casa_grande's, waiting while the homecoming is out");
        assert!(reg.nearest(1.0, 2.0).is_none(), "claimed once, by casa_grande: not casa_arredata's");
        reg.loaded("casa_grande", 10).unwrap();
        reg.observe(look(0, 12, 14, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Usable);

        // No homecoming: the live map is the one version 1 knew while all
        // its submaps are there, as version 1 had it.
        std::fs::write(&path, v1).unwrap();
        let mut reg = Registry::load(&path).unwrap();
        reg.observe(look(0, 1, 5, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Usable);
        std::fs::write(&path, v1).unwrap();
        let mut reg = Registry::load(&path).unwrap();
        reg.observe(look(0, 1, 1, true)).unwrap();
        assert_eq!(state_of(&reg, "cucina"), PlaceState::Pending, "a fresh map: waits for a saved one");
    }

    #[test]
    fn registry_round_trips_through_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state").join("places.json");
        {
            let mut reg = Registry::load(&path).unwrap();
            assert!(reg.places().is_empty());
            reg.observe(look(0, 1, 4, true)).unwrap();
            reg.remember("bagno", (1.0, 2.0, 3.0), Some(1.0)).unwrap();
        }
        let mut reg = Registry::load(&path).unwrap();
        assert_eq!(reg.places().len(), 1);
        assert_eq!(reg.places()[0].anchors[0].y, 2.0);
        // The live map was never saved: a boot on a fresh one (fewer
        // submaps than it had) is another map, and `bagno` went with it.
        reg.observe(look(0, 1, 1, true)).unwrap();
        assert_eq!(state_of(&reg, "bagno"), PlaceState::Stale);

        std::fs::write(&path, b"{not json").unwrap();
        assert!(Registry::load(&path).is_err());
        std::fs::write(&path, br#"{"version": 3, "places": []}"#).unwrap();
        assert!(Registry::load(&path).is_err());
    }
}
