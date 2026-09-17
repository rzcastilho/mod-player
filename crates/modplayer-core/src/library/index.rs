// SPDX-License-Identifier: MIT OR Apache-2.0

//! `LibraryIndex`: the in-memory mirror of the account's library sets plus
//! hydrated ref maps and cached full track lists (data-model.md §3.1,
//! research R6). Merging is lazy (research R4/design note 7): a
//! `FetchLibrary` page only ever records identity + `added_at` in the set
//! lists (`saved_tracks`/`saved_albums`/`followed_artists`) and queues the
//! id for hydration — except `Playlists`, whose page already carries a
//! fully-resolved `PlaylistRef` (playlists.rs/`Playlist::get`, verified in
//! 003), so no hydration entry is queued for it. `tracks`/`albums`/
//! `artists` therefore double as "has this id been hydrated?" — a row with
//! no entry there renders a skeleton (contracts/ui-surface.md §3).

use std::collections::{HashMap, HashSet, VecDeque};

use modplayer_audio_source::{
    AlbumId, AlbumRef, ArtistId, ArtistRef, LibraryItem, LibraryPage, LibrarySet, PlaylistId,
    PlaylistRef, TrackId, TrackList, TrackListSource, TrackRef,
};

/// A library entry alongside when it was added, in service order
/// (data-model.md §3.1).
#[derive(Debug, Clone, PartialEq)]
pub struct SavedEntry<Id> {
    pub id: Id,
    pub added_at: Option<u64>,
}

/// How a sync page's outcome rolls up across the whole cycle (data-model.md
/// §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncOutcome {
    #[default]
    Never,
    Ok,
    Partial,
    RateLimited,
    Failed,
}

/// `MODPLAYER_LIBRARY_FIXTURE=large` (quickstart M15, US3 T072, debug
/// builds only): read fresh by `PlaybackController::tick_library` so a
/// 50 000 saved-track / 1 000-playlist library (SC-002, FR-014) can be
/// rehearsed without a real account that large. Compiled out of release
/// builds (`debug_assertions`) so it can never fire for a real user, like
/// `modplayer-audio-source-connect::health`'s
/// `MODPLAYER_CONNECT_FORCE_UNAVAILABLE`.
#[cfg(debug_assertions)]
pub fn large_fixture_requested() -> bool {
    std::env::var_os("MODPLAYER_LIBRARY_FIXTURE").is_some_and(|v| v == "large")
}

#[cfg(not(debug_assertions))]
pub fn large_fixture_requested() -> bool {
    false
}

/// The synthetic fixture itself (quickstart M15): 50 000 saved tracks
/// merged page by page like a real sync and then hydrated in place (the
/// fixture ids exist on no service, so leaving them to the live hydration
/// sweep would only ever resolve them to `Removed` — the 2026-09-17
/// manual walk saw 50 000 skeletons instead of rows), plus 1 000
/// fully-resolved playlists (a `Playlists` page is always fully resolved
/// on merge, contracts/catalog-source.md §3). Mirrors the shape
/// `tests/library_index.rs::large_fixture_merges_and_looks_up_under_budget`
/// already exercises.
pub fn large_fixture() -> LibraryIndex {
    const SAVED_TRACKS: u32 = 50_000;
    const PLAYLISTS: u32 = 1_000;
    const CHUNK: u32 = 500;

    let mut index = LibraryIndex::new();
    let mut hydrated = Vec::with_capacity(SAVED_TRACKS as usize);
    for chunk in 0..CHUNK {
        let items = (0..SAVED_TRACKS / CHUNK)
            .map(|i| {
                let id = chunk * (SAVED_TRACKS / CHUNK) + i;
                let track = fixture_track(id);
                hydrated.push(track.clone());
                LibraryItem::Track {
                    track,
                    added_at: Some(u64::from(id)),
                }
            })
            .collect();
        index.merge_page(LibraryPage {
            set: LibrarySet::SavedTracks,
            items,
            next_page: None,
            sync_token: None,
        });
    }
    index.apply_hydrated(hydrated, Vec::new(), Vec::new(), Vec::new());
    let playlists = (0..PLAYLISTS)
        .map(|i| LibraryItem::Playlist(fixture_playlist(i)))
        .collect();
    index.merge_page(LibraryPage {
        set: LibrarySet::Playlists,
        items: playlists,
        next_page: None,
        sync_token: None,
    });
    index
}

fn fixture_track(id: u32) -> TrackRef {
    TrackRef::new(
        TrackId::new(format!("spotify:track:fixture-{id}")).unwrap_or_else(|_| unreachable!()),
        format!("Fixture Track {id}"),
        vec!["Fixture Artist".to_string()],
        None,
        None,
        200_000,
        modplayer_audio_source::Availability::Available,
    )
}

fn fixture_playlist(id: u32) -> PlaylistRef {
    PlaylistRef {
        id: PlaylistId::new(format!("spotify:playlist:fixture-{id}"))
            .unwrap_or_else(|_| unreachable!()),
        name: format!("Fixture Playlist {id}"),
        owner_name: "Fixture Owner".to_string(),
        editable: true,
        artwork_url: None,
        track_count: 0,
        revision: None,
    }
}

/// Sync bookkeeping persisted alongside the index (data-model.md §3.2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SyncMeta {
    pub last_synced_at: Option<u64>,
    pub last_outcome: SyncOutcome,
    pub sync_tokens: HashMap<LibrarySet, String>,
}

impl SyncMeta {
    /// FR-021's inline state: a `Failed` outcome with no prior successful
    /// sync ever recorded (data-model.md §3.3 "Derived view flags").
    pub fn first_sync_failed(&self) -> bool {
        self.last_outcome == SyncOutcome::Failed && self.last_synced_at.is_none()
    }
}

/// One id queued for lazy hydration (research R4, design note 7).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum HydrationTarget {
    Track(TrackId),
    Album(AlbumId),
    Artist(ArtistId),
}

/// Up to `HYDRATE_BATCH` ids to resolve next (contracts/library-and-
/// search-core.md §4).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HydrateBatch {
    pub tracks: Vec<TrackId>,
    pub albums: Vec<AlbumId>,
    pub artists: Vec<ArtistId>,
}

impl HydrateBatch {
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty() && self.albums.is_empty() && self.artists.is_empty()
    }

    fn len(&self) -> usize {
        self.tracks.len() + self.albums.len() + self.artists.len()
    }
}

/// The in-memory mirror of the account library (data-model.md §3.1).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibraryIndex {
    pub(crate) saved_tracks: Vec<SavedEntry<TrackId>>,
    pub(crate) saved_albums: Vec<SavedEntry<AlbumId>>,
    pub(crate) followed_artists: Vec<ArtistId>,
    pub(crate) playlists: Vec<PlaylistId>,
    pub(crate) tracks: HashMap<TrackId, TrackRef>,
    pub(crate) albums: HashMap<AlbumId, AlbumRef>,
    pub(crate) artists: HashMap<ArtistId, ArtistRef>,
    pub(crate) playlist_refs: HashMap<PlaylistId, PlaylistRef>,
    pub(crate) track_lists: HashMap<TrackListSource, Vec<TrackId>>,
    pub(crate) meta: SyncMeta,
    hydration_queue: VecDeque<HydrationTarget>,
    hydration_pending: HashSet<HydrationTarget>,
    /// O(1) dedup/update indices into `saved_tracks`/`saved_albums`, and
    /// O(1) membership for `followed_artists`/`playlists` (SC-002: a
    /// 50 000-track sync must not be O(n^2) — these are always rebuilt
    /// from the Vecs above, in [`Self::from_parts`] and incrementally in
    /// [`Self::merge_page`], so they never affect `PartialEq`-observable
    /// state).
    saved_track_positions: HashMap<TrackId, usize>,
    saved_album_positions: HashMap<AlbumId, usize>,
    followed_artist_set: HashSet<ArtistId>,
    playlist_set: HashSet<PlaylistId>,
}

impl LibraryIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a `LibraryIndex` directly from its parts (`library::persist`'s
    /// load path and its round-trip proptests): skips the hydration-queue
    /// bookkeeping `merge_page` performs, since a value built this way is
    /// already exactly what a save -> load round trip should reproduce
    /// (fresh, empty queues on both sides).
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        saved_tracks: Vec<SavedEntry<TrackId>>,
        saved_albums: Vec<SavedEntry<AlbumId>>,
        followed_artists: Vec<ArtistId>,
        playlists: Vec<PlaylistId>,
        tracks: HashMap<TrackId, TrackRef>,
        albums: HashMap<AlbumId, AlbumRef>,
        artists: HashMap<ArtistId, ArtistRef>,
        playlist_refs: HashMap<PlaylistId, PlaylistRef>,
        track_lists: HashMap<TrackListSource, Vec<TrackId>>,
        meta: SyncMeta,
    ) -> Self {
        let saved_track_positions = saved_tracks
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        let saved_album_positions = saved_albums
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        let followed_artist_set = followed_artists.iter().cloned().collect();
        let playlist_set = playlists.iter().cloned().collect();
        Self {
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
            hydration_queue: VecDeque::new(),
            hydration_pending: HashSet::new(),
            saved_track_positions,
            saved_album_positions,
            followed_artist_set,
            playlist_set,
        }
    }

    pub fn saved_tracks(&self) -> &[SavedEntry<TrackId>] {
        &self.saved_tracks
    }

    pub fn saved_albums(&self) -> &[SavedEntry<AlbumId>] {
        &self.saved_albums
    }

    pub fn followed_artists(&self) -> &[ArtistId] {
        &self.followed_artists
    }

    pub fn playlists(&self) -> &[PlaylistId] {
        &self.playlists
    }

    pub fn meta(&self) -> &SyncMeta {
        &self.meta
    }

    pub fn track(&self, id: &TrackId) -> Option<&TrackRef> {
        self.tracks.get(id)
    }

    pub fn album(&self, id: &AlbumId) -> Option<&AlbumRef> {
        self.albums.get(id)
    }

    pub fn artist(&self, id: &ArtistId) -> Option<&ArtistRef> {
        self.artists.get(id)
    }

    pub fn playlist_ref(&self, id: &PlaylistId) -> Option<&PlaylistRef> {
        self.playlist_refs.get(id)
    }

    /// The cached full ordered track list for `source`, if resolved
    /// (contracts/library-and-search-core.md §1 `library_track_list`).
    pub fn track_list(&self, source: &TrackListSource) -> Option<Vec<TrackRef>> {
        let ids = self.track_lists.get(source)?;
        Some(
            ids.iter()
                .filter_map(|id| self.tracks.get(id).cloned())
                .collect(),
        )
    }

    /// Whether *any* set has ever been populated — used to distinguish a
    /// silent sync failure (a snapshot exists) from FR-021's first-sync-
    /// failed state (data-model.md §3.3).
    pub fn has_any_snapshot(&self) -> bool {
        !self.saved_tracks.is_empty()
            || !self.saved_albums.is_empty()
            || !self.followed_artists.is_empty()
            || !self.playlists.is_empty()
    }

    /// Merge one `FetchLibrary` page into the index (data-model.md §3.1,
    /// research R6): records identity + `added_at`/order for every set,
    /// resolves `Playlists` fully immediately, and queues every other set's
    /// ids for lazy hydration (research R4).
    ///
    /// ```
    /// use modplayer_audio_source::{LibraryItem, LibraryPage, LibrarySet, PlaylistId, PlaylistRef};
    /// use modplayer_core::library::LibraryIndex;
    ///
    /// let mut index = LibraryIndex::new();
    /// index.merge_page(LibraryPage {
    ///     set: LibrarySet::Playlists,
    ///     items: vec![LibraryItem::Playlist(PlaylistRef {
    ///         id: PlaylistId::new("spotify:playlist:a").expect("valid"),
    ///         name: "Road Trip".to_string(),
    ///         owner_name: "Alex".to_string(),
    ///         editable: true,
    ///         artwork_url: None,
    ///         track_count: 3,
    ///         revision: None,
    ///     })],
    ///     next_page: None,
    ///     sync_token: None,
    /// });
    /// assert_eq!(index.playlists().len(), 1);
    /// ```
    pub fn merge_page(&mut self, page: LibraryPage) {
        for item in page.items {
            match item {
                LibraryItem::Track { track, added_at } => {
                    let id = track.id.clone();
                    match self.saved_track_positions.get(&id) {
                        Some(&pos) => {
                            if let Some(entry) = self.saved_tracks.get_mut(pos) {
                                entry.added_at = added_at.or(entry.added_at);
                            }
                        }
                        None => {
                            self.saved_track_positions
                                .insert(id.clone(), self.saved_tracks.len());
                            self.saved_tracks.push(SavedEntry {
                                id: id.clone(),
                                added_at,
                            });
                        }
                    }
                    self.enqueue_hydration(HydrationTarget::Track(id));
                }
                LibraryItem::Album { album, added_at } => {
                    let id = album.id.clone();
                    match self.saved_album_positions.get(&id) {
                        Some(&pos) => {
                            if let Some(entry) = self.saved_albums.get_mut(pos) {
                                entry.added_at = added_at.or(entry.added_at);
                            }
                        }
                        None => {
                            self.saved_album_positions
                                .insert(id.clone(), self.saved_albums.len());
                            self.saved_albums.push(SavedEntry {
                                id: id.clone(),
                                added_at,
                            });
                        }
                    }
                    self.enqueue_hydration(HydrationTarget::Album(id));
                }
                LibraryItem::Artist(artist) => {
                    let id = artist.id.clone();
                    if self.followed_artist_set.insert(id.clone()) {
                        self.followed_artists.push(id.clone());
                    }
                    self.enqueue_hydration(HydrationTarget::Artist(id));
                }
                LibraryItem::Playlist(playlist) => {
                    let id = playlist.id.clone();
                    if self.playlist_set.insert(id.clone()) {
                        self.playlists.push(id.clone());
                    }
                    self.playlist_refs.insert(id, playlist);
                }
            }
        }
        if let Some(token) = page.sync_token {
            self.meta.sync_tokens.insert(page.set, token);
        }
    }

    fn enqueue_hydration(&mut self, target: HydrationTarget) {
        let already_hydrated = match &target {
            HydrationTarget::Track(id) => self.tracks.contains_key(id),
            HydrationTarget::Album(id) => self.albums.contains_key(id),
            HydrationTarget::Artist(id) => self.artists.contains_key(id),
        };
        if already_hydrated || self.hydration_pending.contains(&target) {
            return;
        }
        self.hydration_pending.insert(target.clone());
        self.hydration_queue.push_back(target);
    }

    /// UI hint (contracts/library-and-search-core.md §1
    /// `library_hydrate_visible`): move `ids` to the front of the sweep so
    /// the rows actually on screen resolve first (design note 7).
    pub fn prioritize_hydration(&mut self, ids: &[TrackId]) {
        for id in ids {
            if self.tracks.contains_key(id) {
                continue;
            }
            let target = HydrationTarget::Track(id.clone());
            if let Some(pos) = self.hydration_queue.iter().position(|t| *t == target) {
                if let Some(t) = self.hydration_queue.remove(pos) {
                    self.hydration_queue.push_front(t);
                }
            } else if !self.hydration_pending.contains(&target) {
                self.hydration_pending.insert(target.clone());
                self.hydration_queue.push_front(target);
            }
        }
    }

    /// Pop up to `max` ids off the front of the hydration sweep (research
    /// R4, contracts/library-and-search-core.md §4 `HYDRATE_BATCH`),
    /// leaving them `pending` (in flight) until [`Self::apply_hydrated`] or
    /// [`Self::merge_track_list`] resolves them.
    pub fn drain_hydration_batch(&mut self, max: usize) -> HydrateBatch {
        let mut batch = HydrateBatch::default();
        while batch.len() < max {
            let Some(target) = self.hydration_queue.pop_front() else {
                break;
            };
            match target {
                HydrationTarget::Track(id) => batch.tracks.push(id),
                HydrationTarget::Album(id) => batch.albums.push(id),
                HydrationTarget::Artist(id) => batch.artists.push(id),
            }
        }
        batch
    }

    /// Whether anything is still queued or in flight for hydration —
    /// `hydration_pending` (not the queue alone), since a batch already
    /// drained for a `HydrateRefs` request is "not yet hydrated" until its
    /// reply lands (tests; also usable to skip an empty request).
    pub fn has_pending_hydration(&self) -> bool {
        !self.hydration_pending.is_empty()
    }

    /// Fold a `SourceEvent::Hydrated` reply (contracts/catalog-source.md
    /// §2 rule 7): resolved entities are inserted and their pending mark
    /// cleared; a missing *track* uri maps to `Availability::Removed`
    /// (contract rule 7) so it stops being re-queued forever; a missing
    /// album/artist uri is simply cleared from `pending` — nothing else in
    /// this slice renders an album/artist-level "removed" state.
    pub fn apply_hydrated(
        &mut self,
        tracks: Vec<TrackRef>,
        albums: Vec<AlbumRef>,
        artists: Vec<ArtistRef>,
        missing: Vec<String>,
    ) {
        for track in tracks {
            self.hydration_pending
                .remove(&HydrationTarget::Track(track.id.clone()));
            self.tracks.insert(track.id.clone(), track);
        }
        for album in albums {
            self.hydration_pending
                .remove(&HydrationTarget::Album(album.id.clone()));
            self.albums.insert(album.id.clone(), album);
        }
        for artist in artists {
            self.hydration_pending
                .remove(&HydrationTarget::Artist(artist.id.clone()));
            self.artists.insert(artist.id.clone(), artist);
        }
        for uri in missing {
            if let Ok(id) = TrackId::new(uri.clone()) {
                let target = HydrationTarget::Track(id.clone());
                if self.hydration_pending.remove(&target) {
                    self.tracks.insert(
                        id.clone(),
                        TrackRef::new(
                            id,
                            "",
                            Vec::new(),
                            None,
                            None,
                            0,
                            modplayer_audio_source::Availability::Removed,
                        ),
                    );
                    continue;
                }
            }
            if let Ok(id) = AlbumId::new(uri.clone()) {
                self.hydration_pending.remove(&HydrationTarget::Album(id));
                continue;
            }
            if let Ok(id) = ArtistId::new(uri) {
                self.hydration_pending.remove(&HydrationTarget::Artist(id));
            }
        }
    }

    /// Fold a `SourceEvent::TrackList` reply (contracts/library-and-
    /// search-core.md §1 `library_track_list`): caches the ordered id list
    /// and merges every resolved `TrackRef` — a `TrackList` reply already
    /// carries full refs, so no separate hydration is queued for them.
    pub fn merge_track_list(&mut self, list: TrackList) {
        let ids: Vec<TrackId> = list.tracks.iter().map(|t| t.id.clone()).collect();
        for track in list.tracks {
            self.hydration_pending
                .remove(&HydrationTarget::Track(track.id.clone()));
            self.tracks.insert(track.id.clone(), track);
        }
        self.track_lists.insert(list.source, ids);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use modplayer_audio_source::Availability;

    fn track_ref(id: &str) -> TrackRef {
        TrackRef::new(
            TrackId::new(id).unwrap(),
            "Song",
            vec!["Artist".to_string()],
            None,
            None,
            1000,
            Availability::Available,
        )
    }

    fn album_ref(id: &str) -> AlbumRef {
        AlbumRef {
            id: AlbumId::new(id).unwrap(),
            name: "Album".to_string(),
            artists: vec![],
            artwork_url: None,
            release_date: None,
            track_count: 1,
        }
    }

    fn artist_ref(id: &str) -> ArtistRef {
        ArtistRef {
            id: ArtistId::new(id).unwrap(),
            name: "Artist".to_string(),
            artwork_url: None,
        }
    }

    fn playlist_ref(id: &str, editable: bool) -> PlaylistRef {
        PlaylistRef {
            id: PlaylistId::new(id).unwrap(),
            name: "Playlist".to_string(),
            owner_name: "Alex".to_string(),
            editable,
            artwork_url: None,
            track_count: 0,
            revision: None,
        }
    }

    #[test]
    fn merging_a_saved_tracks_page_records_identity_and_queues_hydration() {
        let mut index = LibraryIndex::new();
        index.merge_page(LibraryPage {
            set: LibrarySet::SavedTracks,
            items: vec![LibraryItem::Track {
                track: track_ref("spotify:track:a"),
                added_at: Some(1),
            }],
            next_page: None,
            sync_token: None,
        });
        assert_eq!(index.saved_tracks().len(), 1);
        // Not hydrated yet: the merged page only carries a bare id, real
        // content comes from `apply_hydrated`.
        assert!(
            index
                .track(&TrackId::new("spotify:track:a").unwrap())
                .is_none()
        );
        assert!(index.has_pending_hydration());
    }

    #[test]
    fn merging_the_same_id_twice_updates_added_at_without_duplicating() {
        let mut index = LibraryIndex::new();
        let page = |added_at| LibraryPage {
            set: LibrarySet::SavedTracks,
            items: vec![LibraryItem::Track {
                track: track_ref("spotify:track:a"),
                added_at,
            }],
            next_page: None,
            sync_token: None,
        };
        index.merge_page(page(Some(1)));
        index.merge_page(page(Some(2)));
        assert_eq!(index.saved_tracks().len(), 1);
        assert_eq!(index.saved_tracks()[0].added_at, Some(2));
    }

    #[test]
    fn playlists_are_merged_fully_hydrated_with_no_hydration_queued() {
        let mut index = LibraryIndex::new();
        index.merge_page(LibraryPage {
            set: LibrarySet::Playlists,
            items: vec![LibraryItem::Playlist(playlist_ref(
                "spotify:playlist:a",
                true,
            ))],
            next_page: None,
            sync_token: None,
        });
        assert_eq!(index.playlists().len(), 1);
        assert!(
            index
                .playlist_ref(&PlaylistId::new("spotify:playlist:a").unwrap())
                .is_some()
        );
        assert!(!index.has_pending_hydration());
    }

    #[test]
    fn drain_hydration_batch_respects_the_max_and_leaves_the_rest_queued() {
        let mut index = LibraryIndex::new();
        for i in 0..5 {
            index.merge_page(LibraryPage {
                set: LibrarySet::SavedTracks,
                items: vec![LibraryItem::Track {
                    track: track_ref(&format!("spotify:track:{i}")),
                    added_at: None,
                }],
                next_page: None,
                sync_token: None,
            });
        }
        let batch = index.drain_hydration_batch(3);
        assert_eq!(batch.tracks.len(), 3);
        assert!(index.has_pending_hydration());
        let batch2 = index.drain_hydration_batch(3);
        assert_eq!(batch2.tracks.len(), 2);
        // Both batches are now "in flight" (drained but not yet resolved)
        // rather than gone — `has_pending_hydration` only clears once
        // `apply_hydrated` resolves them.
        assert!(index.has_pending_hydration());
        let mut ids = batch.tracks;
        ids.extend(batch2.tracks);
        let tracks = ids
            .into_iter()
            .map(|id| {
                TrackRef::new(
                    id,
                    "Song",
                    vec![],
                    None,
                    None,
                    1000,
                    Availability::Available,
                )
            })
            .collect();
        index.apply_hydrated(tracks, vec![], vec![], vec![]);
        assert!(!index.has_pending_hydration());
    }

    #[test]
    fn apply_hydrated_resolves_tracks_albums_and_artists() {
        let mut index = LibraryIndex::new();
        index.merge_page(LibraryPage {
            set: LibrarySet::SavedTracks,
            items: vec![LibraryItem::Track {
                track: track_ref("spotify:track:a"),
                added_at: None,
            }],
            next_page: None,
            sync_token: None,
        });
        index.apply_hydrated(
            vec![track_ref("spotify:track:a")],
            vec![album_ref("spotify:album:a")],
            vec![artist_ref("spotify:artist:a")],
            vec![],
        );
        assert!(
            index
                .track(&TrackId::new("spotify:track:a").unwrap())
                .is_some()
        );
        assert!(!index.has_pending_hydration());
    }

    #[test]
    fn a_missing_track_uri_resolves_to_removed_rather_than_re_queueing_forever() {
        let mut index = LibraryIndex::new();
        index.merge_page(LibraryPage {
            set: LibrarySet::SavedTracks,
            items: vec![LibraryItem::Track {
                track: track_ref("spotify:track:gone"),
                added_at: None,
            }],
            next_page: None,
            sync_token: None,
        });
        index.apply_hydrated(
            vec![],
            vec![],
            vec![],
            vec!["spotify:track:gone".to_string()],
        );
        let resolved = index
            .track(&TrackId::new("spotify:track:gone").unwrap())
            .expect("resolved to a placeholder");
        assert_eq!(resolved.availability, Availability::Removed);
        assert!(!index.has_pending_hydration());
    }

    #[test]
    fn merge_track_list_caches_order_and_hydrates_every_track() {
        let mut index = LibraryIndex::new();
        let source = TrackListSource::Album(AlbumId::new("spotify:album:a").unwrap());
        index.merge_track_list(TrackList {
            source: source.clone(),
            tracks: vec![track_ref("spotify:track:a"), track_ref("spotify:track:b")],
        });
        let list = index.track_list(&source).expect("cached");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, TrackId::new("spotify:track:a").unwrap());
    }

    #[test]
    fn prioritize_hydration_moves_a_queued_id_to_the_front() {
        let mut index = LibraryIndex::new();
        for i in 0..3 {
            index.merge_page(LibraryPage {
                set: LibrarySet::SavedTracks,
                items: vec![LibraryItem::Track {
                    track: track_ref(&format!("spotify:track:{i}")),
                    added_at: None,
                }],
                next_page: None,
                sync_token: None,
            });
        }
        index.prioritize_hydration(&[TrackId::new("spotify:track:2").unwrap()]);
        let batch = index.drain_hydration_batch(1);
        assert_eq!(batch.tracks, vec![TrackId::new("spotify:track:2").unwrap()]);
    }

    #[test]
    fn has_any_snapshot_is_false_until_something_is_merged() {
        let index = LibraryIndex::new();
        assert!(!index.has_any_snapshot());
    }

    #[test]
    fn first_sync_failed_only_when_failed_and_never_synced() {
        let mut meta = SyncMeta::default();
        assert!(!meta.first_sync_failed());
        meta.last_outcome = SyncOutcome::Failed;
        assert!(meta.first_sync_failed());
        meta.last_synced_at = Some(1);
        assert!(!meta.first_sync_failed());
    }
}
