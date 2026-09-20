// SPDX-License-Identifier: MIT OR Apache-2.0

//! `library::persist`: JSON DTOs and atomic save/load for `index.json` and
//! `play_log.json` (data-model.md §3.4/§3.5, contracts/library-and-
//! search-core.md §5), mirroring `settings::store`'s pattern (`.tmp` +
//! `sync_all()` + `rename`, degrade-to-empty-plus-warning on any read
//! failure). The trait crate stays serde-free (research R13), so every DTO
//! here is this module's own mirror, never a re-derive on
//! `modplayer-audio-source`'s types. Loads and writes both run off the UI
//! thread (design note 2): [`spawn_background_load`] loads once at
//! startup; [`spawn_writer`] is a long-lived background thread the
//! controller sends dirty snapshots to.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use modplayer_audio_source::{
    AlbumId, AlbumRef, ArtistId, ArtistRef, Availability, LibrarySet, PlaylistId, PlaylistRef,
    ReleaseDate, TrackId, TrackListSource, TrackRef,
};

use super::index::{LibraryIndex, SavedEntry, SyncMeta, SyncOutcome};
use super::play_log::PlayLog;

/// Environment variable that overrides the library data directory (tests
/// and portable use — mirrors `settings::store::CONFIG_DIR_ENV`).
pub const DATA_DIR_ENV: &str = "MODPLAYER_LIBRARY_DIR";

const INDEX_FILE_NAME: &str = "index.json";
const INDEX_TMP_NAME: &str = "index.json.tmp";
const PLAY_LOG_FILE_NAME: &str = "play_log.json";
const PLAY_LOG_TMP_NAME: &str = "play_log.json.tmp";
const SCHEMA_VERSION: u32 = 1;

/// Resolved file paths for both persisted files (contracts/library-and-
/// search-core.md §5).
#[derive(Debug, Clone)]
pub struct LibraryPaths {
    pub index_path: PathBuf,
    pub play_log_path: PathBuf,
}

impl LibraryPaths {
    /// Resolve from `MODPLAYER_LIBRARY_DIR` (if set) or the platform's
    /// default data-local directory's `library/` subdirectory (research
    /// R6). `None` only if neither is determinable.
    pub fn resolve() -> Option<Self> {
        Some(Self::with_dir(data_dir()?))
    }

    /// Point both files directly at files under `dir` (tests).
    pub fn with_dir(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        Self {
            index_path: dir.join(INDEX_FILE_NAME),
            play_log_path: dir.join(PLAY_LOG_FILE_NAME),
        }
    }
}

fn data_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var(DATA_DIR_ENV) {
        return Some(PathBuf::from(dir));
    }
    ProjectDirs::from("", "ModPlayer", "ModPlayer")
        .map(|dirs| dirs.data_local_dir().join("library"))
}

/// A warning raised while loading either file — mapped to a Fluent key by
/// the caller, matching `settings::SettingsWarning`'s shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadWarning {
    Unreadable,
    NewerSchema,
}

// -- Index DTOs -----------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct SavedEntryDto {
    id: String,
    added_at: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct ReleaseDateDto {
    year: u16,
    month: Option<u8>,
    day: Option<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
enum AvailabilityDto {
    #[default]
    Available,
    UnavailableRegion,
    Removed,
}

impl From<Availability> for AvailabilityDto {
    fn from(value: Availability) -> Self {
        match value {
            Availability::Available => AvailabilityDto::Available,
            Availability::UnavailableRegion => AvailabilityDto::UnavailableRegion,
            Availability::Removed => AvailabilityDto::Removed,
        }
    }
}

impl From<AvailabilityDto> for Availability {
    fn from(value: AvailabilityDto) -> Self {
        match value {
            AvailabilityDto::Available => Availability::Available,
            AvailabilityDto::UnavailableRegion => Availability::UnavailableRegion,
            AvailabilityDto::Removed => Availability::Removed,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct TrackDto {
    id: String,
    title: String,
    artists: Vec<String>,
    artist_ids: Vec<String>,
    album: Option<String>,
    album_id: Option<String>,
    artwork_url: Option<String>,
    duration_ms: u32,
    explicit: bool,
    release_date: Option<ReleaseDateDto>,
    availability: AvailabilityDto,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct AlbumDto {
    id: String,
    name: String,
    artists: Vec<String>,
    artwork_url: Option<String>,
    release_date: Option<ReleaseDateDto>,
    track_count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct ArtistDto {
    id: String,
    name: String,
    artwork_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct PlaylistDto {
    id: String,
    name: String,
    owner_name: String,
    editable: bool,
    artwork_url: Option<String>,
    track_count: u32,
    revision: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct TrackListDto {
    /// `"album" | "playlist" | "artist_top"`.
    kind: String,
    id: String,
    tracks: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
enum SyncOutcomeDto {
    #[default]
    Never,
    Ok,
    Partial,
    RateLimited,
    Failed,
}

impl From<SyncOutcome> for SyncOutcomeDto {
    fn from(value: SyncOutcome) -> Self {
        match value {
            SyncOutcome::Never => SyncOutcomeDto::Never,
            SyncOutcome::Ok => SyncOutcomeDto::Ok,
            SyncOutcome::Partial => SyncOutcomeDto::Partial,
            SyncOutcome::RateLimited => SyncOutcomeDto::RateLimited,
            SyncOutcome::Failed => SyncOutcomeDto::Failed,
        }
    }
}

impl From<SyncOutcomeDto> for SyncOutcome {
    fn from(value: SyncOutcomeDto) -> Self {
        match value {
            SyncOutcomeDto::Never => SyncOutcome::Never,
            SyncOutcomeDto::Ok => SyncOutcome::Ok,
            SyncOutcomeDto::Partial => SyncOutcome::Partial,
            SyncOutcomeDto::RateLimited => SyncOutcome::RateLimited,
            SyncOutcomeDto::Failed => SyncOutcome::Failed,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct SyncTokenDto {
    set: String,
    token: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct SyncMetaDto {
    last_synced_at: Option<u64>,
    last_outcome: SyncOutcomeDto,
    sync_tokens: Vec<SyncTokenDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct IndexFile {
    schema: u32,
    saved_tracks: Vec<SavedEntryDto>,
    saved_albums: Vec<SavedEntryDto>,
    followed_artists: Vec<String>,
    playlists: Vec<String>,
    tracks: Vec<TrackDto>,
    albums: Vec<AlbumDto>,
    artists: Vec<ArtistDto>,
    playlist_refs: Vec<PlaylistDto>,
    track_lists: Vec<TrackListDto>,
    meta: SyncMetaDto,
}

fn set_key(set: LibrarySet) -> &'static str {
    match set {
        LibrarySet::SavedTracks => "saved_tracks",
        LibrarySet::SavedAlbums => "saved_albums",
        LibrarySet::FollowedArtists => "followed_artists",
        LibrarySet::Playlists => "playlists",
    }
}

fn set_from_key(key: &str) -> Option<LibrarySet> {
    match key {
        "saved_tracks" => Some(LibrarySet::SavedTracks),
        "saved_albums" => Some(LibrarySet::SavedAlbums),
        "followed_artists" => Some(LibrarySet::FollowedArtists),
        "playlists" => Some(LibrarySet::Playlists),
        _ => None,
    }
}

fn track_list_kind(source: &TrackListSource) -> &'static str {
    match source {
        TrackListSource::Album(_) => "album",
        TrackListSource::Playlist(_) => "playlist",
        TrackListSource::ArtistTop(_) => "artist_top",
    }
}

fn track_list_id(source: &TrackListSource) -> String {
    match source {
        TrackListSource::Album(id) => id.to_string(),
        TrackListSource::Playlist(id) => id.to_string(),
        TrackListSource::ArtistTop(id) => id.to_string(),
    }
}

fn track_list_source_from_dto(kind: &str, id: &str) -> Option<TrackListSource> {
    match kind {
        "album" => Some(TrackListSource::Album(AlbumId::new(id).ok()?)),
        "playlist" => Some(TrackListSource::Playlist(PlaylistId::new(id).ok()?)),
        "artist_top" => Some(TrackListSource::ArtistTop(ArtistId::new(id).ok()?)),
        _ => None,
    }
}

fn release_date_to_dto(date: &ReleaseDate) -> ReleaseDateDto {
    ReleaseDateDto {
        year: date.year,
        month: date.month,
        day: date.day,
    }
}

fn release_date_from_dto(dto: &ReleaseDateDto) -> ReleaseDate {
    ReleaseDate {
        year: dto.year,
        month: dto.month,
        day: dto.day,
    }
}

fn track_to_dto(track: &TrackRef) -> TrackDto {
    TrackDto {
        id: track.id.to_string(),
        title: track.title.clone(),
        artists: track.artists.clone(),
        artist_ids: track.artist_ids.iter().map(ToString::to_string).collect(),
        album: track.album.clone(),
        album_id: track.album_id.as_ref().map(ToString::to_string),
        artwork_url: track.artwork_url.clone(),
        duration_ms: track.duration_ms,
        explicit: track.explicit,
        release_date: track.release_date.as_ref().map(release_date_to_dto),
        availability: track.availability.into(),
    }
}

fn track_from_dto(dto: &TrackDto) -> Option<TrackRef> {
    let id = TrackId::new(dto.id.clone()).ok()?;
    let mut track = TrackRef::new(
        id,
        dto.title.clone(),
        dto.artists.clone(),
        dto.album.clone(),
        dto.artwork_url.clone(),
        dto.duration_ms,
        dto.availability.into(),
    );
    track = track.with_extras(modplayer_audio_source::TrackRefExtras {
        artist_ids: dto
            .artist_ids
            .iter()
            .filter_map(|s| ArtistId::new(s.clone()).ok())
            .collect(),
        album_id: dto
            .album_id
            .as_ref()
            .and_then(|s| AlbumId::new(s.clone()).ok()),
        explicit: dto.explicit,
        release_date: dto.release_date.as_ref().map(release_date_from_dto),
    });
    Some(track)
}

fn album_to_dto(album: &AlbumRef) -> AlbumDto {
    AlbumDto {
        id: album.id.to_string(),
        name: album.name.clone(),
        artists: album.artists.clone(),
        artwork_url: album.artwork_url.clone(),
        release_date: album.release_date.as_ref().map(release_date_to_dto),
        track_count: album.track_count,
    }
}

fn album_from_dto(dto: &AlbumDto) -> Option<AlbumRef> {
    Some(AlbumRef {
        id: AlbumId::new(dto.id.clone()).ok()?,
        name: dto.name.clone(),
        artists: dto.artists.clone(),
        artwork_url: dto.artwork_url.clone(),
        release_date: dto.release_date.as_ref().map(release_date_from_dto),
        track_count: dto.track_count,
    })
}

fn artist_to_dto(artist: &ArtistRef) -> ArtistDto {
    ArtistDto {
        id: artist.id.to_string(),
        name: artist.name.clone(),
        artwork_url: artist.artwork_url.clone(),
    }
}

fn artist_from_dto(dto: &ArtistDto) -> Option<ArtistRef> {
    Some(ArtistRef {
        id: ArtistId::new(dto.id.clone()).ok()?,
        name: dto.name.clone(),
        artwork_url: dto.artwork_url.clone(),
    })
}

fn playlist_to_dto(playlist: &PlaylistRef) -> PlaylistDto {
    PlaylistDto {
        id: playlist.id.to_string(),
        name: playlist.name.clone(),
        owner_name: playlist.owner_name.clone(),
        editable: playlist.editable,
        artwork_url: playlist.artwork_url.clone(),
        track_count: playlist.track_count,
        revision: playlist.revision.clone(),
    }
}

fn playlist_from_dto(dto: &PlaylistDto) -> Option<PlaylistRef> {
    Some(PlaylistRef {
        id: PlaylistId::new(dto.id.clone()).ok()?,
        name: dto.name.clone(),
        owner_name: dto.owner_name.clone(),
        editable: dto.editable,
        artwork_url: dto.artwork_url.clone(),
        track_count: dto.track_count,
        revision: dto.revision.clone(),
    })
}

fn index_to_file(index: &LibraryIndex) -> IndexFile {
    IndexFile {
        schema: SCHEMA_VERSION,
        saved_tracks: index
            .saved_tracks
            .iter()
            .map(|e| SavedEntryDto {
                id: e.id.to_string(),
                added_at: e.added_at,
            })
            .collect(),
        saved_albums: index
            .saved_albums
            .iter()
            .map(|e| SavedEntryDto {
                id: e.id.to_string(),
                added_at: e.added_at,
            })
            .collect(),
        followed_artists: index
            .followed_artists
            .iter()
            .map(ToString::to_string)
            .collect(),
        playlists: index.playlists.iter().map(ToString::to_string).collect(),
        tracks: index.tracks.values().map(track_to_dto).collect(),
        albums: index.albums.values().map(album_to_dto).collect(),
        artists: index.artists.values().map(artist_to_dto).collect(),
        playlist_refs: index.playlist_refs.values().map(playlist_to_dto).collect(),
        track_lists: index
            .track_lists
            .iter()
            .map(|(source, ids)| TrackListDto {
                kind: track_list_kind(source).to_string(),
                id: track_list_id(source),
                tracks: ids.iter().map(ToString::to_string).collect(),
            })
            .collect(),
        meta: SyncMetaDto {
            last_synced_at: index.meta.last_synced_at,
            last_outcome: index.meta.last_outcome.into(),
            sync_tokens: index
                .meta
                .sync_tokens
                .iter()
                .map(|(set, token)| SyncTokenDto {
                    set: set_key(*set).to_string(),
                    token: token.clone(),
                })
                .collect(),
        },
    }
}

fn index_from_file(file: IndexFile) -> LibraryIndex {
    let saved_tracks: Vec<SavedEntry<TrackId>> = file
        .saved_tracks
        .into_iter()
        .filter_map(|e| {
            Some(SavedEntry {
                id: TrackId::new(e.id).ok()?,
                added_at: e.added_at,
            })
        })
        .collect();
    let saved_albums: Vec<SavedEntry<AlbumId>> = file
        .saved_albums
        .into_iter()
        .filter_map(|e| {
            Some(SavedEntry {
                id: AlbumId::new(e.id).ok()?,
                added_at: e.added_at,
            })
        })
        .collect();
    let followed_artists: Vec<ArtistId> = file
        .followed_artists
        .into_iter()
        .filter_map(|s| ArtistId::new(s).ok())
        .collect();
    let playlists: Vec<PlaylistId> = file
        .playlists
        .into_iter()
        .filter_map(|s| PlaylistId::new(s).ok())
        .collect();
    let tracks: HashMap<TrackId, TrackRef> = file
        .tracks
        .iter()
        .filter_map(|dto| track_from_dto(dto).map(|t| (t.id.clone(), t)))
        .collect();
    let albums: HashMap<AlbumId, AlbumRef> = file
        .albums
        .iter()
        .filter_map(|dto| album_from_dto(dto).map(|a| (a.id.clone(), a)))
        .collect();
    let artists: HashMap<ArtistId, ArtistRef> = file
        .artists
        .iter()
        .filter_map(|dto| artist_from_dto(dto).map(|a| (a.id.clone(), a)))
        .collect();
    let playlist_refs: HashMap<PlaylistId, PlaylistRef> = file
        .playlist_refs
        .iter()
        .filter_map(|dto| playlist_from_dto(dto).map(|p| (p.id.clone(), p)))
        .collect();
    let track_lists: HashMap<TrackListSource, Vec<TrackId>> = file
        .track_lists
        .into_iter()
        .filter_map(|dto| {
            let source = track_list_source_from_dto(&dto.kind, &dto.id)?;
            let ids: Vec<TrackId> = dto
                .tracks
                .into_iter()
                .filter_map(|s| TrackId::new(s).ok())
                .collect();
            Some((source, ids))
        })
        .collect();
    let meta = SyncMeta {
        last_synced_at: file.meta.last_synced_at,
        last_outcome: file.meta.last_outcome.into(),
        sync_tokens: file
            .meta
            .sync_tokens
            .into_iter()
            .filter_map(|dto| Some((set_from_key(&dto.set)?, dto.token)))
            .collect(),
    };
    LibraryIndex::from_parts(
        saved_tracks,
        saved_albums,
        followed_artists,
        playlists,
        tracks,
        albums,
        artists,
        playlist_refs,
        track_lists,
        meta,
    )
}

// -- Play log DTOs ----------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct PlayLogEntryDto {
    id: String,
    last_played: u64,
    play_count: u32,
    first_played: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct PlayLogFile {
    schema: u32,
    entries: Vec<PlayLogEntryDto>,
    refs: Vec<TrackDto>,
}

fn play_log_to_file(log: &PlayLog) -> PlayLogFile {
    let entries = log
        .last_played_map()
        .iter()
        .map(|(id, last_played)| PlayLogEntryDto {
            id: id.to_string(),
            last_played: *last_played,
            play_count: log.play_count(id),
            first_played: log.first_played_at(id).unwrap_or(*last_played),
        })
        .collect();
    let refs = log.refs_map().values().map(track_to_dto).collect();
    PlayLogFile {
        schema: SCHEMA_VERSION,
        entries,
        refs,
    }
}

fn play_log_from_file(file: PlayLogFile) -> PlayLog {
    let mut last_played = HashMap::new();
    let mut play_count = HashMap::new();
    let mut first_played = HashMap::new();
    for entry in file.entries {
        let Ok(id) = TrackId::new(entry.id) else {
            continue;
        };
        last_played.insert(id.clone(), entry.last_played);
        play_count.insert(id.clone(), entry.play_count);
        first_played.insert(id, entry.first_played);
    }
    let refs: HashMap<TrackId, TrackRef> = file
        .refs
        .iter()
        .filter_map(|dto| track_from_dto(dto).map(|t| (t.id.clone(), t)))
        .collect();
    PlayLog::from_parts(last_played, play_count, first_played, refs)
}

// -- Load/save --------------------------------------------------------------

/// Outcome of loading `index.json` (mirrors `settings::LoadOutcome`).
#[derive(Debug, Clone, PartialEq)]
pub struct LoadIndexOutcome {
    pub index: LibraryIndex,
    pub warning: Option<LoadWarning>,
}

/// Outcome of loading `play_log.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadPlayLogOutcome {
    pub play_log: PlayLog,
    pub warning: Option<LoadWarning>,
}

fn write_atomic<T: Serialize>(path: &Path, tmp_name: &str, value: &T) -> std::io::Result<()> {
    let serialized = serde_json::to_vec(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_file_name(tmp_name);
    {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(&serialized)?;
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

fn read_schema_checked<T: for<'de> Deserialize<'de>>(
    path: &Path,
    schema_of: impl Fn(&serde_json::Value) -> Option<u32>,
) -> Result<T, Option<LoadWarning>> {
    if !path.exists() {
        return Err(None);
    }
    let content = fs::read(path).map_err(|_| Some(LoadWarning::Unreadable))?;
    let value: serde_json::Value =
        serde_json::from_slice(&content).map_err(|_| Some(LoadWarning::Unreadable))?;
    if let Some(schema) = schema_of(&value)
        && schema > SCHEMA_VERSION
    {
        return Err(Some(LoadWarning::NewerSchema));
    }
    serde_json::from_value(value).map_err(|_| Some(LoadWarning::Unreadable))
}

/// Load `index.json` (contracts/library-and-search-core.md §5): missing ⇒
/// empty index, no warning; unreadable/malformed ⇒ empty + `Unreadable`;
/// newer `schema` ⇒ empty + `NewerSchema` (matching `settings::store`'s
/// read rules).
pub fn load_index(paths: &LibraryPaths) -> LoadIndexOutcome {
    match read_schema_checked::<IndexFile>(&paths.index_path, |v| {
        v.get("schema")
            .and_then(serde_json::Value::as_u64)
            .map(|n| n as u32)
    }) {
        Ok(file) => LoadIndexOutcome {
            index: index_from_file(file),
            warning: None,
        },
        Err(warning) => LoadIndexOutcome {
            index: LibraryIndex::new(),
            warning,
        },
    }
}

/// Save `index.json` atomically (temp-file + `sync_all` + `rename`, `0o600`
/// on Unix).
pub fn save_index(paths: &LibraryPaths, index: &LibraryIndex) -> std::io::Result<()> {
    write_atomic(&paths.index_path, INDEX_TMP_NAME, &index_to_file(index))
}

/// Load `play_log.json` — same degrade rules as [`load_index`].
pub fn load_play_log(paths: &LibraryPaths) -> LoadPlayLogOutcome {
    match read_schema_checked::<PlayLogFile>(&paths.play_log_path, |v| {
        v.get("schema")
            .and_then(serde_json::Value::as_u64)
            .map(|n| n as u32)
    }) {
        Ok(file) => LoadPlayLogOutcome {
            play_log: play_log_from_file(file),
            warning: None,
        },
        Err(warning) => LoadPlayLogOutcome {
            play_log: PlayLog::new(),
            warning,
        },
    }
}

/// Save `play_log.json` atomically.
pub fn save_play_log(paths: &LibraryPaths, log: &PlayLog) -> std::io::Result<()> {
    write_atomic(
        &paths.play_log_path,
        PLAY_LOG_TMP_NAME,
        &play_log_to_file(log),
    )
}

/// Delete both files (design note 8, FR-8.1.6 spirit: sign-out clears
/// everything). Missing files are not an error.
pub fn delete_all(paths: &LibraryPaths) {
    let _ = fs::remove_file(&paths.index_path);
    let _ = fs::remove_file(&paths.play_log_path);
}

/// Load both files on a background thread so launch never blocks
/// (design note 2, research R6).
pub fn spawn_background_load(
    paths: LibraryPaths,
) -> Receiver<(LoadIndexOutcome, LoadPlayLogOutcome)> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let index = load_index(&paths);
        let log = load_play_log(&paths);
        let _ = tx.send((index, log));
    });
    rx
}

/// A dirty snapshot to persist, sent to the background writer thread
/// (design note 2: a frame must never call `File` I/O itself).
pub enum PersistJob {
    SaveIndex(Box<LibraryIndex>),
    SavePlayLog(Box<PlayLog>),
    DeleteAll,
}

/// Spawn the long-lived background persistence thread (contracts/library-
/// and-search-core.md §5 "flushes dirty persistence <= 1 s later on the
/// persistence thread"). The controller sends a [`PersistJob`] whenever its
/// `PERSIST_DEBOUNCE` window says it's time to flush.
///
/// The returned `JoinHandle` lets the controller's `shutdown` wait for the
/// last queued write (the loop ends once every `Sender` is dropped), so a
/// save issued moments before quitting is not cut off by process exit.
pub fn spawn_writer(paths: LibraryPaths) -> (Sender<PersistJob>, std::thread::JoinHandle<()>) {
    let (tx, rx) = std::sync::mpsc::channel::<PersistJob>();
    let handle = std::thread::spawn(move || {
        for job in rx {
            match job {
                PersistJob::SaveIndex(index) => {
                    let _ = save_index(&paths, &index);
                }
                PersistJob::SavePlayLog(log) => {
                    let _ = save_play_log(&paths, &log);
                }
                PersistJob::DeleteAll => delete_all(&paths),
            }
        }
    });
    (tx, handle)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_paths() -> LibraryPaths {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-library-persist-test-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = fs::create_dir_all(&dir);
        LibraryPaths::with_dir(dir)
    }

    #[test]
    fn missing_index_file_loads_empty_with_no_warning() {
        let paths = temp_paths();
        let outcome = load_index(&paths);
        assert_eq!(outcome.index, LibraryIndex::new());
        assert_eq!(outcome.warning, None);
    }

    #[test]
    fn index_round_trips_through_save_and_load() {
        let paths = temp_paths();
        let mut index = LibraryIndex::new();
        index.merge_page(modplayer_audio_source::LibraryPage {
            set: LibrarySet::Playlists,
            items: vec![modplayer_audio_source::LibraryItem::Playlist(PlaylistRef {
                id: PlaylistId::new("spotify:playlist:a").unwrap(),
                name: "Road Trip".to_string(),
                owner_name: "Alex".to_string(),
                editable: true,
                artwork_url: None,
                track_count: 3,
                revision: None,
            })],
            next_page: None,
            sync_token: Some("token-1".to_string()),
        });
        save_index(&paths, &index).expect("save");
        let outcome = load_index(&paths);
        assert_eq!(outcome.warning, None);
        assert_eq!(outcome.index, index);
    }

    #[test]
    fn a_crash_mid_write_leaves_the_prior_index_file_intact() {
        let paths = temp_paths();
        let index = LibraryIndex::new();
        save_index(&paths, &index).expect("save");
        fs::write(paths.index_path.with_file_name(INDEX_TMP_NAME), "garbage").expect("write tmp");
        let outcome = load_index(&paths);
        assert_eq!(outcome.warning, None);
        assert_eq!(outcome.index, index);
    }

    #[test]
    fn a_newer_index_schema_loads_empty_with_exactly_one_warning() {
        let paths = temp_paths();
        fs::create_dir_all(paths.index_path.parent().unwrap()).unwrap();
        fs::write(&paths.index_path, r#"{"schema":99}"#).expect("write");
        let outcome = load_index(&paths);
        assert_eq!(outcome.warning, Some(LoadWarning::NewerSchema));
        assert_eq!(outcome.index, LibraryIndex::new());
    }

    #[test]
    fn garbage_index_file_loads_empty_with_unreadable_warning() {
        let paths = temp_paths();
        fs::create_dir_all(paths.index_path.parent().unwrap()).unwrap();
        fs::write(&paths.index_path, "not json").expect("write");
        let outcome = load_index(&paths);
        assert_eq!(outcome.warning, Some(LoadWarning::Unreadable));
    }

    #[test]
    fn play_log_round_trips_through_save_and_load() {
        let paths = temp_paths();
        let mut log = PlayLog::new();
        log.record(
            &TrackRef::new(
                TrackId::new("spotify:track:a").unwrap(),
                "Söng",
                vec!["Ärtist".to_string()],
                None,
                None,
                1000,
                Availability::Available,
            ),
            42,
        );
        save_play_log(&paths, &log).expect("save");
        let outcome = load_play_log(&paths);
        assert_eq!(outcome.warning, None);
        assert_eq!(outcome.play_log.recent(), log.recent());
        assert_eq!(outcome.play_log.recent_tracks(), log.recent_tracks());
    }

    #[test]
    fn sign_out_deletes_both_files() {
        let paths = temp_paths();
        save_index(&paths, &LibraryIndex::new()).expect("save index");
        save_play_log(&paths, &PlayLog::new()).expect("save play log");
        assert!(paths.index_path.exists());
        assert!(paths.play_log_path.exists());
        delete_all(&paths);
        assert!(!paths.index_path.exists());
        assert!(!paths.play_log_path.exists());
    }
}
