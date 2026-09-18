// SPDX-License-Identifier: MIT OR Apache-2.0

//! Marker/loop-region persistence: track-state file paths, JSON encode/decode
//! and the atomic background writer (006, data-model.md §4, contracts/
//! marker-service.md §3, research R10-R12). Mirrors `library::persist`'s
//! `.tmp` + `sync_all()` + `rename` pattern; the controller (research R11)
//! debounces on its own thread and hands finished bytes to [`spawn_writer`].

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use modplayer_audio_source::TrackId;

use super::model::{
    CueSlot, LoopRegion, MAX_MARKERS, Marker, MarkerId, MarkerKind, Owner, PaletteIndex, RegionId,
    RepeatCount, TrackMarkers,
};

/// Overrides the track-state directory (tests, portable use — mirrors
/// `library::persist::DATA_DIR_ENV` / `analysis::cache::ANALYSIS_DIR_ENV`).
pub const TRACK_STATE_DIR_ENV: &str = "MODPLAYER_TRACK_STATE_DIR";

const SCHEMA_VERSION: u32 = 1;

/// data-model.md §4: a state file larger than this is treated as
/// unreadable rather than parsed (also bounds `decode_track_id`'s hex
/// walk indirectly, since the file name is derived from the id, not the
/// other way around).
const MAX_FILE_BYTES: usize = 64 * 1024;

/// Resolved track-state directory (research R10).
#[derive(Debug, Clone)]
pub struct TrackStatePaths {
    dir: PathBuf,
}

impl TrackStatePaths {
    /// `MODPLAYER_TRACK_STATE_DIR` if set, else the platform data-local
    /// dir's `ModPlayer/track-state/` (mirrors `AnalysisPaths::resolve`).
    /// `None` only if neither is determinable.
    pub fn resolve() -> Option<Self> {
        if let Ok(dir) = std::env::var(TRACK_STATE_DIR_ENV) {
            return Some(Self::with_dir(dir));
        }
        ProjectDirs::from("", "ModPlayer", "ModPlayer")
            .map(|dirs| Self::with_dir(dirs.data_local_dir().join("track-state")))
    }

    pub fn with_dir(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The resolved directory itself (tests/diagnostics).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `<dir>/<hex(track_id)>.json`.
    pub fn file_for(&self, id: &TrackId) -> PathBuf {
        self.dir.join(format!("{}.json", encode_track_id(id)))
    }
}

/// Lowercase hex of `id`'s ASCII bytes (research R10): reversible,
/// case-free (unlike the base62 track id itself), filesystem-safe on
/// every platform without escaping.
pub fn encode_track_id(id: &TrackId) -> String {
    let mut out = String::with_capacity(id.as_str().len() * 2);
    for byte in id.as_str().as_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Inverse of [`encode_track_id`]: `None` on an odd length, a non-hex
/// character, or bytes that don't round-trip through `TrackId::new`.
/// Case-free (`to_digit(16)` accepts either case).
pub fn decode_track_id(name: &str) -> Option<TrackId> {
    if !name.len().is_multiple_of(2) {
        return None;
    }
    let chars: Vec<char> = name.chars().collect();
    let mut bytes = Vec::with_capacity(chars.len() / 2);
    for pair in chars.chunks_exact(2) {
        let hi = pair[0].to_digit(16)?;
        let lo = pair[1].to_digit(16)?;
        bytes.push(u8::try_from((hi << 4) | lo).ok()?);
    }
    let text = String::from_utf8(bytes).ok()?;
    TrackId::new(text).ok()
}

/// A warning raised while loading a track-state file (contracts/
/// marker-service.md §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadWarning {
    Unreadable,
    NewerSchema,
}

/// The result of [`load`] (data-model.md §4's read-rule table).
#[derive(Debug, Clone, PartialEq)]
pub struct LoadOutcome {
    pub state: TrackMarkers,
    pub warning: Option<LoadWarning>,
    /// Whether the loaded (necessarily empty, on any warning) state may be
    /// written back immediately — `false` only for `NewerSchema`, where
    /// the file is left untouched until the user's first mutation sets
    /// `state.is_dirty()` (the controller's normal dirty-gated flush
    /// already honours this without consulting the flag directly).
    pub rewrite_allowed: bool,
}

// -- JSON DTOs (data-model.md §4) -------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum MarkerKindDto {
    Point,
    RegionStart { region: u32 },
    RegionEnd { region: u32 },
    Cue { slot: u8 },
}

fn default_owner() -> String {
    "host".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MarkerDto {
    id: u32,
    #[serde(flatten)]
    kind: MarkerKindDto,
    position: u64,
    #[serde(default)]
    name: String,
    /// Wider than `PaletteIndex`'s `u8` so an out-of-range saved value is
    /// a clamp (data-model.md §4), not a parse failure.
    #[serde(default)]
    color: u32,
    #[serde(default = "default_owner")]
    #[allow(dead_code)] // round-tripped for forward compatibility only (FR-024)
    owner: String,
    #[serde(default)]
    transient: bool,
    #[serde(default = "default_true")]
    visible: bool,
}

fn default_crossfade_ms() -> u32 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegionDto {
    id: u32,
    #[serde(default)]
    a: Option<u32>,
    #[serde(default)]
    b: Option<u32>,
    /// Wider than the domain's `u8` so an out-of-range saved value is a
    /// clamp (data-model.md §4), not a parse failure.
    #[serde(default = "default_crossfade_ms")]
    crossfade_ms: u32,
    /// A wider type than `RepeatCount::Times`'s `u16` so an out-of-range
    /// saved value is a clamp (data-model.md §4), not a parse failure.
    #[serde(default)]
    repeat: Option<u32>,
}

fn default_next_id() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrackStateFile {
    schema_version: u32,
    track_id: String,
    sample_rate: u32,
    len_frames: u64,
    #[serde(default = "default_next_id")]
    next_marker_id: u32,
    #[serde(default = "default_next_id")]
    next_region_id: u32,
    #[serde(default)]
    current_region: Option<u32>,
    #[serde(default)]
    markers: Vec<MarkerDto>,
    #[serde(default)]
    regions: Vec<RegionDto>,
}

fn marker_to_dto(marker: &Marker) -> MarkerDto {
    let kind = match marker.kind {
        MarkerKind::Point => MarkerKindDto::Point,
        MarkerKind::RegionStart { region } => MarkerKindDto::RegionStart {
            region: region.raw(),
        },
        MarkerKind::RegionEnd { region } => MarkerKindDto::RegionEnd {
            region: region.raw(),
        },
        MarkerKind::Cue { slot } => MarkerKindDto::Cue { slot: slot.get() },
    };
    MarkerDto {
        id: marker.id.raw(),
        kind,
        position: marker.position,
        name: marker.name.clone(),
        color: u32::from(marker.color.get()),
        owner: default_owner(),
        transient: marker.transient,
        visible: marker.visible,
    }
}

fn region_to_dto(region: &LoopRegion) -> RegionDto {
    RegionDto {
        id: region.id.raw(),
        a: region.a.map(MarkerId::raw),
        b: region.b.map(MarkerId::raw),
        crossfade_ms: u32::from(region.crossfade_ms),
        repeat: match region.repeat {
            RepeatCount::Infinite => None,
            RepeatCount::Times(n) => Some(u32::from(n)),
        },
    }
}

fn to_file(state: &TrackMarkers) -> TrackStateFile {
    TrackStateFile {
        schema_version: SCHEMA_VERSION,
        track_id: state.track().to_string(),
        sample_rate: state.sample_rate(),
        len_frames: state.len_frames(),
        next_marker_id: state.next_marker_id_raw(),
        next_region_id: state.next_region_id_raw(),
        current_region: state.current_region().map(RegionId::raw),
        markers: state.markers().iter().map(marker_to_dto).collect(),
        regions: state.regions().iter().map(region_to_dto).collect(),
    }
}

/// Serialize `state` to its on-disk JSON form (data-model.md §4). Never
/// fails: every field is already valid (the mutation API enforces it), so
/// a `serde_json` encode error here would be a bug, not a runtime
/// condition — falls back to an empty JSON object rather than panicking.
pub fn encode(state: &TrackMarkers) -> Vec<u8> {
    serde_json::to_vec(&to_file(state)).unwrap_or_else(|_| b"{}".to_vec())
}

/// `rate` differing from `file_rate` rescales (research R12): `pos *
/// rate / file_rate`, rounded. `file_rate == 0` (a malformed file) is
/// treated as "no rescale" rather than dividing by zero.
fn rescale(pos: u64, file_rate: u32, rate: u32) -> u64 {
    if file_rate == 0 || file_rate == rate {
        return pos;
    }
    let scaled = u128::from(pos) * u128::from(rate) / u128::from(file_rate);
    u64::try_from(scaled).unwrap_or(u64::MAX)
}

/// Apply data-model.md §4's field-repair rules and rebuild a
/// [`TrackMarkers`] from a parsed file. `id`/`rate`/`len_frames` are the
/// *current* track identity/rate/length — the file's own `track_id`/
/// `sample_rate`/`len_frames` are informational only (§4's last rule).
fn from_file(file: TrackStateFile, id: &TrackId, rate: u32, len_frames: u64) -> TrackMarkers {
    let file_rate = file.sample_rate;
    let region_ids: HashSet<u32> = file.regions.iter().map(|r| r.id).collect();

    let mut markers: Vec<Marker> = Vec::new();
    let mut seen_cue_slots: HashSet<u8> = HashSet::new();
    for dto in &file.markers {
        if markers.len() >= MAX_MARKERS {
            break;
        }
        let kind = match dto.kind {
            MarkerKindDto::Point => MarkerKind::Point,
            MarkerKindDto::RegionStart { region } if region_ids.contains(&region) => {
                MarkerKind::RegionStart {
                    region: RegionId::from_raw(region),
                }
            }
            // The referenced region doesn't exist in this file at all: an
            // orphan marker becomes a plain point (data-model.md §4).
            MarkerKindDto::RegionStart { .. } => MarkerKind::Point,
            MarkerKindDto::RegionEnd { region } if region_ids.contains(&region) => {
                MarkerKind::RegionEnd {
                    region: RegionId::from_raw(region),
                }
            }
            MarkerKindDto::RegionEnd { .. } => MarkerKind::Point,
            MarkerKindDto::Cue { slot } => match CueSlot::new(slot) {
                // Out-of-range or a slot already claimed by an earlier
                // marker in the file: dropped entirely (§4).
                Some(slot) if seen_cue_slots.insert(slot.get()) => MarkerKind::Cue { slot },
                _ => continue,
            },
        };
        let rescaled = rescale(dto.position, file_rate, rate);
        let position = rescaled.min(len_frames);
        markers.push(Marker {
            id: MarkerId::from_raw(dto.id),
            kind,
            position,
            name: dto.name.clone(),
            color: PaletteIndex::new(u8::try_from(dto.color).unwrap_or(u8::MAX)),
            owner: Owner::Host,
            transient: dto.transient,
            visible: dto.visible,
            clamped: rescaled > len_frames,
        });
    }

    // For each retained marker, the exact (id, kind) it ended up with —
    // used below to validate each region's `a`/`b` against the matching
    // kind, repairing a dangling or kind-mismatched side to `None`.
    let marker_kinds: HashMap<u32, MarkerKind> =
        markers.iter().map(|m| (m.id.raw(), m.kind)).collect();

    let mut regions: Vec<LoopRegion> = Vec::with_capacity(file.regions.len());
    for dto in &file.regions {
        let valid_a = dto.a.filter(|marker_id| {
            matches!(
                marker_kinds.get(marker_id),
                Some(MarkerKind::RegionStart { region }) if region.raw() == dto.id
            )
        });
        let valid_b = dto.b.filter(|marker_id| {
            matches!(
                marker_kinds.get(marker_id),
                Some(MarkerKind::RegionEnd { region }) if region.raw() == dto.id
            )
        });
        regions.push(LoopRegion {
            id: RegionId::from_raw(dto.id),
            a: valid_a.map(MarkerId::from_raw),
            b: valid_b.map(MarkerId::from_raw),
            crossfade_ms: u8::try_from(dto.crossfade_ms.min(50)).unwrap_or(50),
            repeat: match dto.repeat {
                None => RepeatCount::Infinite,
                Some(n) => RepeatCount::times_clamped(u16::try_from(n).unwrap_or(u16::MAX)),
            },
            armed: false,
            wraps: 0,
        });
    }

    // I8: `current_region` is `Some` iff `regions` is non-empty. A missing
    // or dangling file value falls back to the highest surviving id
    // (`delete`'s own fallback rule, data-model.md §1.5 I9).
    let current_region = file
        .current_region
        .filter(|id| regions.iter().any(|r| r.id.raw() == *id))
        .map(RegionId::from_raw)
        .or_else(|| regions.iter().map(|r| r.id).max());

    let max_marker_id = markers.iter().map(|m| m.id.raw()).max().unwrap_or(0);
    let max_region_id = regions.iter().map(|r| r.id.raw()).max().unwrap_or(0);
    let next_marker_id = file.next_marker_id.max(max_marker_id + 1);
    let next_region_id = file.next_region_id.max(max_region_id + 1);

    TrackMarkers::from_parts(
        id.clone(),
        rate,
        len_frames,
        markers,
        regions,
        current_region,
        next_marker_id,
        next_region_id,
    )
}

/// Load `id`'s track-state file per data-model.md §4's read rules:
/// missing → empty, no warning; > 64 KiB, I/O error or invalid JSON →
/// empty + `Unreadable`; `schema_version > 1` → empty + `NewerSchema`;
/// otherwise every field-level repair in [`from_file`]. `rate`/
/// `len_frames` are the *current* track's, used both to clamp/flag
/// positions (FR-018) and to seed the empty fallback.
pub fn load(paths: &TrackStatePaths, id: &TrackId, rate: u32, len_frames: u64) -> LoadOutcome {
    let empty = |warning: Option<LoadWarning>, rewrite_allowed: bool| LoadOutcome {
        state: TrackMarkers::new(id.clone(), rate, len_frames),
        warning,
        rewrite_allowed,
    };
    let path = paths.file_for(id);
    if !path.exists() {
        return empty(None, true);
    }
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => return empty(Some(LoadWarning::Unreadable), true),
    };
    if bytes.len() > MAX_FILE_BYTES {
        return empty(Some(LoadWarning::Unreadable), true);
    }
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return empty(Some(LoadWarning::Unreadable), true),
    };
    let schema = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(u64::from(SCHEMA_VERSION));
    if schema > u64::from(SCHEMA_VERSION) {
        return empty(Some(LoadWarning::NewerSchema), false);
    }
    let file: TrackStateFile = match serde_json::from_value(value) {
        Ok(file) => file,
        Err(_) => return empty(Some(LoadWarning::Unreadable), true),
    };
    LoadOutcome {
        state: from_file(file, id, rate, len_frames),
        warning: None,
        rewrite_allowed: true,
    }
}

// -- Atomic background writer (contracts/marker-service.md §3) --------------

/// A dirty snapshot (or deletion) to persist, sent to the background
/// writer thread (design note 2: a frame must never call `File` I/O
/// itself).
pub enum PersistJob {
    Save { path: PathBuf, bytes: Vec<u8> },
    Delete { path: PathBuf },
}

/// The background writer's failure replies, drained by `tick()`. A
/// successful save has nothing to report.
pub enum StoreEvent {
    SaveFailed { path: PathBuf },
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp_path, path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Spawn the long-lived background persistence thread (contracts/
/// marker-service.md §3 rule 1): `<name>.json.tmp` → `write_all` →
/// `sync_all` → `rename`; on any error the previous file, if any, is left
/// untouched and a `StoreEvent::SaveFailed` is sent back (no retry). The
/// returned `JoinHandle` lets the controller's `shutdown` wait for the
/// last queued write (the loop ends once every `Sender` is dropped).
pub fn spawn_writer(
    notify: Sender<StoreEvent>,
) -> (Sender<PersistJob>, std::thread::JoinHandle<()>) {
    let (tx, rx) = std::sync::mpsc::channel::<PersistJob>();
    let handle = std::thread::spawn(move || {
        for job in rx {
            match job {
                PersistJob::Save { path, bytes } => {
                    if write_atomic(&path, &bytes).is_err() {
                        let _ = notify.send(StoreEvent::SaveFailed { path });
                    }
                }
                PersistJob::Delete { path } => {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    });
    (tx, handle)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn track_id_hex_round_trips() {
        let id = TrackId::new("spotify:track:4uLU6hMCjMI75M1A2tKUQC").unwrap();
        let hex = encode_track_id(&id);
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_eq!(decode_track_id(&hex), Some(id));
    }

    #[test]
    fn decode_rejects_odd_length_and_non_hex() {
        assert_eq!(decode_track_id("abc"), None);
        assert_eq!(decode_track_id("zz"), None);
    }
}
