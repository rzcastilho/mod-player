// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PlayLog`: Recently Played, ModPlayer's own play log (data-model.md
//! §3.5, FR-012, research R11) — recorded by the controller on the
//! `TrackStarted -> Playing` transition, never merged with the queue's own
//! in-memory `history`. `recent()` is always *derived* from `last_played`
//! (never itself persisted, contracts/library-and-search-core.md §6 "recent
//! re-derives identically"), so a save -> load round trip can never drift
//! from what a fresh `recent()` call recomputes.

use std::collections::{HashMap, HashSet};

use modplayer_audio_source::{TrackId, TrackRef};

/// contracts/library-and-search-core.md §4.
pub const RECENT_WINDOW: usize = 100;

/// Recently Played state (data-model.md §3.5).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayLog {
    last_played: HashMap<TrackId, u64>,
    play_count: HashMap<TrackId, u32>,
    first_played: HashMap<TrackId, u64>,
    /// Refs for the `RECENT_WINDOW` most-recently-played entries only, so
    /// Recently Played rows render offline (data-model.md §3.5) — trimmed
    /// to the current `recent()` set on every `record`/`load`.
    refs: HashMap<TrackId, TrackRef>,
    dirty: bool,
}

impl PlayLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a play (FR-012, research R11): upsert `last_played`/
    /// `play_count`/`first_played` (unbounded — every track ever played),
    /// store `track`'s ref, and trim `refs` back down to the current
    /// `RECENT_WINDOW`.
    ///
    /// ```
    /// use modplayer_audio_source::{Availability, TrackId, TrackRef};
    /// use modplayer_core::library::PlayLog;
    ///
    /// let mut log = PlayLog::new();
    /// let track = TrackRef::new(
    ///     TrackId::new("spotify:track:a").expect("valid"),
    ///     "Song",
    ///     vec![],
    ///     None,
    ///     None,
    ///     1000,
    ///     Availability::Available,
    /// );
    /// log.record(&track, 1_000);
    /// log.record(&track, 2_000);
    /// assert_eq!(log.recent(), vec![track.id.clone()]);
    /// assert_eq!(log.play_count(&track.id), 2);
    /// ```
    pub fn record(&mut self, track: &TrackRef, now_ms: u64) {
        let id = track.id.clone();
        self.last_played.insert(id.clone(), now_ms);
        *self.play_count.entry(id.clone()).or_insert(0) += 1;
        self.first_played.entry(id.clone()).or_insert(now_ms);
        self.refs.insert(id, track.clone());
        self.trim_refs_to_window();
        self.dirty = true;
    }

    fn trim_refs_to_window(&mut self) {
        let recent: HashSet<TrackId> = self.recent().into_iter().collect();
        self.refs.retain(|id, _| recent.contains(id));
    }

    /// The top `RECENT_WINDOW` track ids by `last_played`, newest first,
    /// distinct (data-model.md §3.5) — always recomputed, never stored.
    pub fn recent(&self) -> Vec<TrackId> {
        let mut entries: Vec<(&TrackId, &u64)> = self.last_played.iter().collect();
        entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        entries
            .into_iter()
            .take(RECENT_WINDOW)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The Recently Played row content (FR-012), newest first — only the
    /// entries whose ref is known render; any temporarily missing ref (a
    /// window edge just after a restore) is skipped rather than shown
    /// blank.
    pub fn recent_tracks(&self) -> Vec<TrackRef> {
        self.recent()
            .into_iter()
            .filter_map(|id| self.refs.get(&id).cloned())
            .collect()
    }

    pub fn last_played_at(&self, id: &TrackId) -> Option<u64> {
        self.last_played.get(id).copied()
    }

    pub fn play_count(&self, id: &TrackId) -> u32 {
        self.play_count.get(id).copied().unwrap_or(0)
    }

    pub fn first_played_at(&self, id: &TrackId) -> Option<u64> {
        self.first_played.get(id).copied()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    pub fn last_played_map(&self) -> &HashMap<TrackId, u64> {
        &self.last_played
    }

    pub fn play_count_map(&self) -> &HashMap<TrackId, u32> {
        &self.play_count
    }

    pub fn first_played_map(&self) -> &HashMap<TrackId, u64> {
        &self.first_played
    }

    pub fn refs_map(&self) -> &HashMap<TrackId, TrackRef> {
        &self.refs
    }

    /// Rebuild directly from raw parts (`library::persist`'s load path and
    /// its round-trip proptests, contracts/library-and-search-core.md §6):
    /// no re-derivation needed for `last_played`/`play_count`/
    /// `first_played` (they round-trip verbatim, assumed to share the same
    /// keyset — the only shape `record`/persistence ever produces);
    /// `refs` is trimmed to the current `recent()` window the same way
    /// `record` trims it, so a value carrying extra refs never leaks them
    /// into memory.
    pub fn from_parts(
        last_played: HashMap<TrackId, u64>,
        play_count: HashMap<TrackId, u32>,
        first_played: HashMap<TrackId, u64>,
        refs: HashMap<TrackId, TrackRef>,
    ) -> Self {
        let mut log = Self {
            last_played,
            play_count,
            first_played,
            refs,
            dirty: false,
        };
        log.trim_refs_to_window();
        log
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use modplayer_audio_source::Availability;

    fn track(id: &str) -> TrackRef {
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

    #[test]
    fn recording_a_new_track_sets_first_and_last_played_and_count_one() {
        let mut log = PlayLog::new();
        let a = track("spotify:track:a");
        log.record(&a, 100);
        assert_eq!(log.last_played_at(&a.id), Some(100));
        assert_eq!(log.first_played_at(&a.id), Some(100));
        assert_eq!(log.play_count(&a.id), 1);
    }

    #[test]
    fn replaying_the_same_track_updates_last_played_but_keeps_first_played() {
        let mut log = PlayLog::new();
        let a = track("spotify:track:a");
        log.record(&a, 100);
        log.record(&a, 500);
        assert_eq!(log.first_played_at(&a.id), Some(100));
        assert_eq!(log.last_played_at(&a.id), Some(500));
        assert_eq!(log.play_count(&a.id), 2);
    }

    #[test]
    fn recent_lists_newest_first() {
        let mut log = PlayLog::new();
        let a = track("spotify:track:a");
        let b = track("spotify:track:b");
        log.record(&a, 100);
        log.record(&b, 200);
        assert_eq!(log.recent(), vec![b.id.clone(), a.id.clone()]);
    }

    #[test]
    fn replaying_the_first_track_moves_it_back_to_the_top_once() {
        let mut log = PlayLog::new();
        let a = track("spotify:track:a");
        let b = track("spotify:track:b");
        log.record(&a, 100);
        log.record(&b, 200);
        log.record(&a, 300);
        assert_eq!(log.recent(), vec![a.id.clone(), b.id.clone()]);
    }

    #[test]
    fn recent_is_capped_at_the_window_even_though_last_played_is_unbounded() {
        let mut log = PlayLog::new();
        for i in 0..(RECENT_WINDOW + 10) {
            log.record(&track(&format!("spotify:track:{i}")), i as u64);
        }
        assert_eq!(log.recent().len(), RECENT_WINDOW);
        // last_played itself outlives the window (data-model.md §3.5).
        assert_eq!(log.last_played_map().len(), RECENT_WINDOW + 10);
        assert!(
            log.last_played_at(&TrackId::new("spotify:track:0").unwrap())
                .is_some()
        );
    }

    #[test]
    fn refs_are_trimmed_to_the_window_but_the_count_maps_are_not() {
        let mut log = PlayLog::new();
        for i in 0..(RECENT_WINDOW + 5) {
            log.record(&track(&format!("spotify:track:{i}")), i as u64);
        }
        assert_eq!(log.refs_map().len(), RECENT_WINDOW);
        assert_eq!(log.play_count_map().len(), RECENT_WINDOW + 5);
    }

    #[test]
    fn recent_tracks_returns_refs_in_recent_order() {
        let mut log = PlayLog::new();
        let a = track("spotify:track:a");
        let b = track("spotify:track:b");
        log.record(&a, 100);
        log.record(&b, 200);
        let tracks = log.recent_tracks();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].id, b.id);
        assert_eq!(tracks[1].id, a.id);
    }

    #[test]
    fn recording_marks_dirty_and_mark_clean_resets_it() {
        let mut log = PlayLog::new();
        assert!(!log.is_dirty());
        log.record(&track("spotify:track:a"), 1);
        assert!(log.is_dirty());
        log.mark_clean();
        assert!(!log.is_dirty());
    }

    #[test]
    fn from_parts_re_derives_recent_identically_to_a_freshly_recorded_log() {
        let mut original = PlayLog::new();
        let a = track("spotify:track:a");
        let b = track("spotify:track:b");
        original.record(&a, 100);
        original.record(&b, 200);

        let restored = PlayLog::from_parts(
            original.last_played_map().clone(),
            original.play_count_map().clone(),
            original.first_played_map().clone(),
            original.refs_map().clone(),
        );
        assert_eq!(restored.recent(), original.recent());
        assert_eq!(restored.recent_tracks(), original.recent_tracks());
    }
}
