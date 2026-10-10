// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SearchSession`: pure, host-owned free-text catalog search state
//! (data-model.md §2.1, contracts/library-and-search-core.md §1/§4,
//! research R7). No I/O, no egui — `tick(now)` returns the
//! `SourceCommand`s to issue and `apply_reply` folds a
//! `SourceEvent::SearchResult` back in; `PlaybackController` (`search()`/
//! `search_mut()`/`search_show_more()`, `tick()`) is the only caller.

use std::time::{Duration, Instant};

use crate::backoff::backoff_delay;
use modplayer_audio_source::{
    CatalogError, SearchGroupPage, SearchHit, SearchKind, SearchPage, SourceCommand,
    pack_request_id, unpack_request_id,
};

/// FR-001's debounce window: a request is issued 150 ms after the last
/// edit, never on every keystroke.
pub const SEARCH_DEBOUNCE: Duration = Duration::from_millis(150);
/// FR-002's page size ("20 per page with Show more").
pub const SEARCH_PAGE: u8 = 20;

/// The four search groups, fixed display/request order (FR-001/002,
/// data-model.md §2.1).
pub const GROUP_ORDER: [SearchKind; 4] = [
    SearchKind::Track,
    SearchKind::Album,
    SearchKind::Artist,
    SearchKind::Playlist,
];

fn kind_index(kind: SearchKind) -> usize {
    GROUP_ORDER.iter().position(|k| *k == kind).unwrap_or(0)
}

fn kind_from_byte(byte: u8) -> Option<SearchKind> {
    GROUP_ORDER.into_iter().find(|k| *k as u8 == byte)
}

/// One search group's state (data-model.md §2.1).
#[derive(Debug, Clone, PartialEq)]
pub enum GroupState {
    Idle,
    Pending,
    Loaded {
        items: Vec<SearchHit>,
        /// `Some` when a further "Show more" page exists.
        next_offset: Option<u32>,
        /// A "Show more" page request is outstanding for this group.
        loading_more: bool,
    },
    /// The reply answered with zero hits — omitted from the view unless
    /// every group is `Empty` (the no-results state, FR-017/018).
    Empty,
    /// The strategy in use could not fulfil this kind (research R2/R3) —
    /// omitted from the view.
    Unsupported,
    /// The newest request for this group was rate-limited: `stale` is the
    /// previous `Loaded` snapshot, if any (FR-015).
    RateLimited {
        stale: Option<Vec<SearchHit>>,
        /// The "Show more" offset that was rate-limited; `None` when the
        /// combined request was rate-limited (nothing on screen).
        resume_offset: Option<u32>,
    },
}

/// A scheduled automatic retry of a rate-limited request (data-model.md §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Retry {
    /// When the retry may be issued.
    due: Instant,
    /// Consecutive rate limits of this target in the current generation.
    attempt: u8,
    /// The retry command was sent and its reply is outstanding.
    issued: bool,
}

impl GroupState {
    /// The items to render for this group: `Loaded`'s own items, or a
    /// `RateLimited` reply's stale snapshot; empty otherwise.
    pub fn items(&self) -> &[SearchHit] {
        match self {
            GroupState::Loaded { items, .. } => items,
            GroupState::RateLimited {
                stale: Some(items), ..
            } => items,
            _ => &[],
        }
    }
}

/// Free-text catalog search, debounced and race-safe (data-model.md §2.1,
/// research R7). Pure state: the controller drives `tick`/`apply_reply` and
/// forwards the `SourceCommand`s `tick`/`show_more` return.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchSession {
    raw_query: String,
    query: String,
    debounce_deadline: Option<Instant>,
    generation: u64,
    groups: [GroupState; 4],
    offline: bool,
    /// The current combined 4-kind request, if one is outstanding
    /// (research R2: one round trip answers every group).
    pending_request_id: Option<u64>,
    /// Per-kind "Show more" request in flight, if any.
    show_more_request_ids: [Option<u64>; 4],
    /// Scheduled/issued retry of the combined request for `generation`.
    combined_retry: Option<Retry>,
    /// Scheduled/issued retry of a rate-limited "Show more" page, per kind.
    show_more_retries: [Option<Retry>; 4],
}

impl Default for SearchSession {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchSession {
    pub fn new() -> Self {
        Self {
            raw_query: String::new(),
            query: String::new(),
            debounce_deadline: None,
            generation: 0,
            groups: [
                GroupState::Idle,
                GroupState::Idle,
                GroupState::Idle,
                GroupState::Idle,
            ],
            offline: false,
            pending_request_id: None,
            show_more_request_ids: [None, None, None, None],
            combined_retry: None,
            show_more_retries: [None; 4],
        }
    }

    /// The query exactly as typed (drives the search box's own text).
    pub fn raw_query(&self) -> &str {
        &self.raw_query
    }

    /// The trimmed, effective query (FR-001) — empty before any search.
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn offline(&self) -> bool {
        self.offline
    }

    /// FR-009: the search-field spinner predicate — `true` while the
    /// debounce is armed or the first combined request is outstanding.
    /// `false` for "Show more" pages and once a reply is applied. Callers
    /// hide the spinner while [`offline`](Self::offline).
    ///
    /// ```
    /// use std::time::Instant;
    ///
    /// use modplayer_core::SearchSession;
    ///
    /// let mut session = SearchSession::new();
    /// assert!(!session.in_flight());
    /// session.set_query("abba", Instant::now());
    /// assert!(session.in_flight());
    /// ```
    pub fn in_flight(&self) -> bool {
        self.debounce_deadline.is_some()
            || (self.pending_request_id.is_some() && self.combined_retry.is_none())
    }

    /// `true` while any rate-limited request has a retry waiting or
    /// outstanding (the status strip only claims a refresh that is
    /// actually scheduled).
    ///
    /// ```
    /// use modplayer_core::SearchSession;
    ///
    /// assert!(!SearchSession::new().retry_scheduled());
    /// ```
    pub fn retry_scheduled(&self) -> bool {
        self.combined_retry.is_some() || self.show_more_retries.iter().any(Option::is_some)
    }

    fn clear_retries(&mut self) {
        self.combined_retry = None;
        self.show_more_retries = [None; 4];
    }

    /// The current query generation: bumped whenever a new effective query
    /// fires or the query is cleared, never by "Show more". The view uses
    /// it to announce a settled result count once per query (FR-010).
    ///
    /// ```
    /// use std::time::Instant;
    ///
    /// use modplayer_core::SearchSession;
    ///
    /// let mut session = SearchSession::new();
    /// let before = session.generation();
    /// session.set_query("", Instant::now());
    /// assert!(session.generation() > before);
    /// ```
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// FR-015: any group is `RateLimited` (stale results + "Refreshing…").
    pub fn refreshing(&self) -> bool {
        self.groups
            .iter()
            .any(|g| matches!(g, GroupState::RateLimited { .. }))
    }

    /// FR-017/018: a real (non-empty) query whose four groups all came back
    /// empty — the no-results state, distinct from the offline state.
    pub fn is_no_results(&self) -> bool {
        !self.query.is_empty() && self.groups.iter().all(|g| matches!(g, GroupState::Empty))
    }

    pub fn group(&self, kind: SearchKind) -> &GroupState {
        &self.groups[kind_index(kind)]
    }

    /// Every group with its kind, in the fixed display order.
    pub fn groups(&self) -> impl Iterator<Item = (SearchKind, &GroupState)> {
        GROUP_ORDER
            .into_iter()
            .map(move |kind| (kind, &self.groups[kind_index(kind)]))
    }

    /// Edit the raw query (FR-001): trims, and re-arms the 150 ms debounce
    /// deadline for a changed, non-empty effective query. A trim-empty
    /// query resets to `Idle` immediately and bumps `generation`, poisoning
    /// any in-flight reply (data-model.md §2.1).
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    ///
    /// use modplayer_core::SearchSession;
    ///
    /// let mut session = SearchSession::new();
    /// let now = Instant::now();
    /// session.set_query("abba", now);
    /// // Debounce not yet elapsed: no command issued.
    /// assert!(session.tick(now).is_empty());
    /// // 150 ms later, the debounce fires.
    /// assert!(!session.tick(now + Duration::from_millis(150)).is_empty());
    /// ```
    pub fn set_query(&mut self, raw_query: impl Into<String>, now: Instant) {
        self.raw_query = raw_query.into();
        let trimmed = self.raw_query.trim().to_string();
        if trimmed.is_empty() {
            self.query = trimmed;
            self.reset_to_idle();
            return;
        }
        if trimmed == self.query {
            // Unchanged effective query (e.g. a trailing-space edit): leave
            // the current debounce/state alone.
            return;
        }
        self.query = trimmed;
        self.cancel_retries();
        self.debounce_deadline = Some(now + SEARCH_DEBOUNCE);
    }

    /// Cancel every retry and discard any outstanding retry reply (the
    /// request id no longer matches). Groups are left as they are.
    fn cancel_retries(&mut self) {
        if let Some(retry) = self.combined_retry
            && retry.issued
        {
            self.pending_request_id = None;
        }
        for idx in 0..4 {
            if matches!(
                self.show_more_retries[idx],
                Some(Retry { issued: true, .. })
            ) {
                self.show_more_request_ids[idx] = None;
            }
        }
        self.clear_retries();
    }

    fn reset_to_idle(&mut self) {
        self.clear_retries();
        self.generation += 1;
        self.debounce_deadline = None;
        self.pending_request_id = None;
        self.show_more_request_ids = [None, None, None, None];
        self.groups = [
            GroupState::Idle,
            GroupState::Idle,
            GroupState::Idle,
            GroupState::Idle,
        ];
    }

    /// FR-018/research R5: the controller reports the derived online fact
    /// every tick. Going offline never touches the groups themselves — it
    /// only blocks new requests (`tick`) and discards whatever reply
    /// arrives while it holds (`apply_reply`), per data-model.md §2.1.
    ///
    /// The one exception is a rate-limited request with a retry scheduled
    /// (contracts/search-status.md R8): the retry is cancelled and the
    /// groups collapse to a state that never claims "refreshing" or "no
    /// results" — stale rows become `Loaded`, a combined rate limit
    /// re-arms the debounce so the query re-runs on reconnect.
    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
        if !offline || !self.retry_scheduled() {
            return;
        }
        if let Some(retry) = self.combined_retry {
            for state in &mut self.groups {
                if matches!(state, GroupState::RateLimited { stale: None, .. }) {
                    *state = GroupState::Idle;
                }
            }
            self.pending_request_id = None;
            self.debounce_deadline = Some(retry.due);
        }
        for idx in 0..4 {
            if self.show_more_retries[idx].is_none() {
                continue;
            }
            self.show_more_request_ids[idx] = None;
            if let GroupState::RateLimited {
                stale: Some(items),
                resume_offset,
            } = std::mem::replace(&mut self.groups[idx], GroupState::Idle)
            {
                self.groups[idx] = GroupState::Loaded {
                    items,
                    next_offset: resume_offset,
                    loading_more: false,
                };
            }
        }
        self.clear_retries();
    }

    /// Advance the debounce timer (data-model.md §2.1). Once the deadline
    /// has passed, online, with a non-empty query: bumps `generation`,
    /// resets all four groups to `Pending`, and returns the one combined
    /// `SearchCatalog` command to issue (mercury searchview answers all
    /// four groups in a single round trip, research R2). A no-op — and,
    /// importantly, leaves the armed deadline untouched — while offline, so
    /// the same deadline fires as soon as connectivity returns.
    pub fn tick(&mut self, now: Instant) -> Vec<SourceCommand> {
        if self.offline {
            return Vec::new();
        }
        let Some(deadline) = self.debounce_deadline else {
            return self.due_retries(now);
        };
        if now < deadline {
            return Vec::new();
        }
        self.debounce_deadline = None;
        if self.query.is_empty() {
            return Vec::new();
        }
        self.generation += 1;
        self.groups = [
            GroupState::Pending,
            GroupState::Pending,
            GroupState::Pending,
            GroupState::Pending,
        ];
        self.show_more_request_ids = [None, None, None, None];
        self.clear_retries();
        let request_id = pack_request_id(self.generation, SearchKind::Track);
        self.pending_request_id = Some(request_id);
        vec![SourceCommand::SearchCatalog {
            request_id,
            query: self.query.clone(),
            kinds: GROUP_ORDER.to_vec(),
            offset: 0,
            limit: SEARCH_PAGE,
        }]
    }

    /// Emit every retry whose backoff has elapsed, once each, in the same
    /// generation (data-model.md §2 transitions).
    fn due_retries(&mut self, now: Instant) -> Vec<SourceCommand> {
        let mut commands = Vec::new();
        if let Some(retry) = &mut self.combined_retry
            && !retry.issued
            && now >= retry.due
        {
            retry.issued = true;
            let request_id = pack_request_id(self.generation, SearchKind::Track);
            self.pending_request_id = Some(request_id);
            commands.push(SourceCommand::SearchCatalog {
                request_id,
                query: self.query.clone(),
                kinds: GROUP_ORDER.to_vec(),
                offset: 0,
                limit: SEARCH_PAGE,
            });
        }
        for (idx, kind) in GROUP_ORDER.into_iter().enumerate() {
            let Some(retry) = &mut self.show_more_retries[idx] else {
                continue;
            };
            if retry.issued || now < retry.due {
                continue;
            }
            let GroupState::RateLimited {
                resume_offset: Some(offset),
                ..
            } = &self.groups[idx]
            else {
                continue;
            };
            retry.issued = true;
            let request_id = pack_request_id(self.generation, kind);
            self.show_more_request_ids[idx] = Some(request_id);
            commands.push(SourceCommand::SearchCatalog {
                request_id,
                query: self.query.clone(),
                kinds: vec![kind],
                offset: *offset,
                limit: SEARCH_PAGE,
            });
        }
        commands
    }

    /// FR-002 "Show more": request the next page of one loaded group, at
    /// `items.len()`. `None` when the group has no further page or a page
    /// request is already outstanding for it.
    pub fn show_more(&mut self, kind: SearchKind) -> Option<SourceCommand> {
        let idx = kind_index(kind);
        let GroupState::Loaded {
            next_offset: Some(offset),
            loading_more,
            ..
        } = &mut self.groups[idx]
        else {
            return None;
        };
        if *loading_more {
            return None;
        }
        *loading_more = true;
        let offset = *offset;
        let request_id = pack_request_id(self.generation, kind);
        self.show_more_request_ids[idx] = Some(request_id);
        Some(SourceCommand::SearchCatalog {
            request_id,
            query: self.query.clone(),
            kinds: vec![kind],
            offset,
            limit: SEARCH_PAGE,
        })
    }

    /// Fold a `SourceEvent::SearchResult` reply (contracts/catalog-
    /// source.md §2) into group state. Discarded outright while offline, or
    /// when `request_id`'s generation is no longer current (data-model.md
    /// §2.1: "poisons in-flight replies"). A rate-limited reply schedules an
    /// automatic retry at `now + backoff_delay(..)` (data-model.md §2);
    /// `tick` issues it.
    pub fn apply_reply(
        &mut self,
        request_id: u64,
        result: Result<SearchPage, CatalogError>,
        now: Instant,
    ) {
        if self.offline {
            return;
        }
        let (generation, kind_byte) = unpack_request_id(request_id);
        if generation != self.generation {
            return;
        }

        if self.pending_request_id == Some(request_id) {
            self.pending_request_id = None;
            // A retry reply: put the placeholder groups back to `Pending`
            // so the normal fold below applies unchanged.
            let previous = self.combined_retry.take();
            if previous.is_some() {
                for state in &mut self.groups {
                    if matches!(state, GroupState::RateLimited { stale: None, .. }) {
                        *state = GroupState::Pending;
                    }
                }
            }
            match result {
                Ok(page) => {
                    self.apply_page(page);
                    // Any group the reply's strategy silently dropped
                    // (neither answered nor named `unsupported`) resolves
                    // to `Empty` rather than hanging as `Pending` forever.
                    for state in &mut self.groups {
                        if matches!(state, GroupState::Pending) {
                            *state = GroupState::Empty;
                        }
                    }
                }
                Err(CatalogError::RateLimited { retry_after_ms }) => {
                    for state in &mut self.groups {
                        if matches!(state, GroupState::Pending) {
                            *state = GroupState::RateLimited {
                                stale: None,
                                resume_offset: None,
                            };
                        }
                    }
                    let attempt = previous.map_or(0, |r| r.attempt.saturating_add(1));
                    self.combined_retry = Some(Retry {
                        due: now + backoff_delay(attempt, retry_after_ms),
                        attempt,
                        issued: false,
                    });
                }
                Err(_) => {
                    for state in &mut self.groups {
                        if matches!(state, GroupState::Pending) {
                            *state = GroupState::Empty;
                        }
                    }
                }
            }
            return;
        }

        let Some(kind) = kind_from_byte(kind_byte) else {
            return;
        };
        let idx = kind_index(kind);
        if self.show_more_request_ids[idx] != Some(request_id) {
            return;
        }
        self.show_more_request_ids[idx] = None;
        let previous = self.show_more_retries[idx].take();
        if previous.is_some()
            && let GroupState::RateLimited {
                stale: Some(_),
                resume_offset,
            } = &self.groups[idx]
        {
            // A retry reply: restore the outstanding-page shape.
            let resume = *resume_offset;
            if let GroupState::RateLimited {
                stale: Some(items), ..
            } = std::mem::replace(&mut self.groups[idx], GroupState::Idle)
            {
                self.groups[idx] = GroupState::Loaded {
                    items,
                    next_offset: resume,
                    loading_more: true,
                };
            }
        }
        match result {
            Ok(page) => self.apply_page(page),
            Err(CatalogError::RateLimited { retry_after_ms }) => {
                if let GroupState::Loaded {
                    items, next_offset, ..
                } = &mut self.groups[idx]
                {
                    let stale = std::mem::take(items);
                    let resume_offset = *next_offset;
                    self.groups[idx] = GroupState::RateLimited {
                        stale: Some(stale),
                        resume_offset,
                    };
                    let attempt = previous.map_or(0, |r| r.attempt.saturating_add(1));
                    self.show_more_retries[idx] = Some(Retry {
                        due: now + backoff_delay(attempt, retry_after_ms),
                        attempt,
                        issued: false,
                    });
                }
            }
            Err(_) => {
                if let GroupState::Loaded { loading_more, .. } = &mut self.groups[idx] {
                    *loading_more = false;
                }
            }
        }
    }

    fn apply_page(&mut self, page: SearchPage) {
        for group in page.groups {
            self.apply_group_page(group);
        }
        for kind in page.unsupported {
            self.groups[kind_index(kind)] = GroupState::Unsupported;
        }
    }

    fn apply_group_page(&mut self, group: SearchGroupPage) {
        let idx = kind_index(group.kind);
        let continuation = matches!(
            &self.groups[idx],
            GroupState::Loaded {
                loading_more: true,
                ..
            }
        );
        if continuation {
            if let GroupState::Loaded {
                items,
                next_offset,
                loading_more,
            } = &mut self.groups[idx]
            {
                items.extend(group.items);
                *next_offset = group.next_offset;
                *loading_more = false;
            }
            return;
        }
        self.groups[idx] = if group.items.is_empty() {
            GroupState::Empty
        } else {
            GroupState::Loaded {
                items: group.items,
                next_offset: group.next_offset,
                loading_more: false,
            }
        };
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use modplayer_audio_source::{Availability, TrackId, TrackRef};

    fn track_hit(id: &str) -> SearchHit {
        SearchHit::Track(TrackRef::new(
            TrackId::new(format!("spotify:track:{id}")).unwrap(),
            format!("Title {id}"),
            vec!["Artist".to_string()],
            None,
            None,
            180_000,
            Availability::Available,
        ))
    }

    fn page(tracks: usize, next_offset: Option<u32>) -> SearchPage {
        let group = |kind, items, next_offset| SearchGroupPage {
            kind,
            items,
            next_offset,
        };
        SearchPage {
            groups: vec![
                group(
                    SearchKind::Track,
                    (0..tracks).map(|i| track_hit(&i.to_string())).collect(),
                    next_offset,
                ),
                group(SearchKind::Album, vec![], None),
                group(SearchKind::Artist, vec![], None),
                group(SearchKind::Playlist, vec![], None),
            ],
            unsupported: vec![],
        }
    }

    fn request_id(cmds: &[SourceCommand]) -> u64 {
        match cmds.first() {
            Some(SourceCommand::SearchCatalog { request_id, .. }) => *request_id,
            other => panic!("expected SearchCatalog, got {other:?}"),
        }
    }

    /// S1 truth table: idle.
    #[test]
    fn in_flight_false_when_idle() {
        assert!(!SearchSession::new().in_flight());
    }

    /// S1: debounce armed → in flight; combined outstanding → in flight;
    /// reply applied → not in flight.
    #[test]
    fn in_flight_tracks_debounce_and_combined_request() {
        let now = Instant::now();
        let mut s = SearchSession::new();
        s.set_query("abba", now);
        assert!(s.in_flight(), "debouncing");
        assert!(s.tick(now).is_empty());
        assert!(s.in_flight(), "still debouncing before deadline");
        let cmds = s.tick(now + SEARCH_DEBOUNCE);
        assert!(s.in_flight(), "combined outstanding");
        s.apply_reply(request_id(&cmds), Ok(page(2, None)), now);
        assert!(!s.in_flight(), "loaded");
    }

    /// S1: Show more outstanding does not count as in flight.
    #[test]
    fn in_flight_false_while_show_more_outstanding() {
        let now = Instant::now();
        let mut s = SearchSession::new();
        s.set_query("abba", now);
        let cmds = s.tick(now + SEARCH_DEBOUNCE);
        s.apply_reply(request_id(&cmds), Ok(page(20, Some(20))), now);
        assert!(s.show_more(SearchKind::Track).is_some());
        assert!(!s.in_flight());
    }

    /// S1: offline + armed debounce is not in flight from the session's
    /// view only if the view gates on `offline()`; the session itself
    /// reports the armed deadline so the UI decides (contract S1:
    /// `!offline && in_flight()`).
    #[test]
    fn in_flight_reports_armed_debounce_while_offline() {
        let now = Instant::now();
        let mut s = SearchSession::new();
        s.set_query("abba", now);
        s.set_offline(true);
        assert!(s.in_flight());
        assert!(s.tick(now + SEARCH_DEBOUNCE).is_empty());
        assert!(s.in_flight(), "deadline stays armed while offline");
    }

    /// Clearing the query drops the spinner predicate.
    #[test]
    fn in_flight_false_after_clear() {
        let now = Instant::now();
        let mut s = SearchSession::new();
        s.set_query("abba", now);
        s.set_query("", now);
        assert!(!s.in_flight());
    }

    /// `generation()` bumps when a new effective query fires, and on clear.
    #[test]
    fn generation_bumps_on_new_effective_query_and_clear() {
        let now = Instant::now();
        let mut s = SearchSession::new();
        let g0 = s.generation();
        s.set_query("abba", now);
        assert_eq!(s.generation(), g0, "typing alone does not bump");
        let _ = s.tick(now + SEARCH_DEBOUNCE);
        let g1 = s.generation();
        assert!(g1 > g0);
        s.set_query("abba ", now);
        assert_eq!(s.generation(), g1, "unchanged effective query");
        s.set_query("queen", now);
        let _ = s.tick(now + SEARCH_DEBOUNCE);
        let g2 = s.generation();
        assert!(g2 > g1);
        s.set_query("", now);
        assert!(s.generation() > g2);
    }

    /// Show more does not change the generation.
    #[test]
    fn generation_stable_across_show_more() {
        let now = Instant::now();
        let mut s = SearchSession::new();
        s.set_query("abba", now);
        let cmds = s.tick(now + SEARCH_DEBOUNCE);
        s.apply_reply(request_id(&cmds), Ok(page(20, Some(20))), now);
        let g = s.generation();
        let more = s.show_more(SearchKind::Track).unwrap();
        let SourceCommand::SearchCatalog { request_id, .. } = more else {
            panic!("expected SearchCatalog");
        };
        s.apply_reply(request_id, Ok(page(20, None)), now);
        assert_eq!(s.generation(), g);
    }

    // ---- US3: retry state machine (contracts/search-status.md R1–R9) ----

    fn rate_limited(retry_after_ms: Option<u32>) -> Result<SearchPage, CatalogError> {
        Err(CatalogError::RateLimited { retry_after_ms })
    }

    /// Session with a settled 20-row Tracks page and a rate-limited Show more.
    fn show_more_rate_limited(now: Instant) -> (SearchSession, u64) {
        let mut s = SearchSession::new();
        s.set_query("abba", now);
        let cmds = s.tick(now + SEARCH_DEBOUNCE);
        s.apply_reply(request_id(&cmds), Ok(page(20, Some(20))), now);
        let Some(SourceCommand::SearchCatalog { request_id: id, .. }) =
            s.show_more(SearchKind::Track)
        else {
            panic!("expected show more");
        };
        s.apply_reply(id, rate_limited(Some(1_000)), now);
        (s, id)
    }

    fn combined_rate_limited(now: Instant, retry_after_ms: Option<u32>) -> (SearchSession, u64) {
        let mut s = SearchSession::new();
        s.set_query("abba", now);
        let cmds = s.tick(now + SEARCH_DEBOUNCE);
        let id = request_id(&cmds);
        s.apply_reply(id, rate_limited(retry_after_ms), now);
        (s, id)
    }

    /// R1: combined rate limit schedules a retry and is not "in flight".
    #[test]
    fn r1_combined_rate_limit_schedules_retry() {
        let now = Instant::now();
        let (s, _) = combined_rate_limited(now, Some(2_000));
        for (_, g) in s.groups() {
            assert_eq!(
                g,
                &GroupState::RateLimited {
                    stale: None,
                    resume_offset: None
                }
            );
        }
        assert!(s.retry_scheduled());
        assert!(s.refreshing());
        assert!(!s.in_flight(), "retry waiting is not the spinner");
    }

    /// R2: show-more rate limit keeps stale rows and the resume offset.
    #[test]
    fn r2_show_more_rate_limit_keeps_stale_and_offset() {
        let now = Instant::now();
        let (s, _) = show_more_rate_limited(now);
        let GroupState::RateLimited {
            stale: Some(rows),
            resume_offset: Some(20),
        } = s.group(SearchKind::Track)
        else {
            panic!("unexpected {:?}", s.group(SearchKind::Track));
        };
        assert_eq!(rows.len(), 20);
        assert!(s.retry_scheduled());
        assert!(matches!(s.group(SearchKind::Album), GroupState::Empty));
    }

    /// R3: tick emits exactly one combined retry at `due`.
    #[test]
    fn r3_tick_emits_combined_retry_once() {
        let now = Instant::now();
        let (mut s, id) = combined_rate_limited(now, Some(2_000));
        let gen_before = s.generation();
        assert!(s.tick(now + Duration::from_millis(1_999)).is_empty());
        let cmds = s.tick(now + Duration::from_millis(2_000));
        assert_eq!(cmds.len(), 1);
        let SourceCommand::SearchCatalog {
            request_id: rid,
            query,
            kinds,
            offset,
            ..
        } = &cmds[0]
        else {
            panic!("expected SearchCatalog");
        };
        assert_eq!(*rid, id);
        assert_eq!(query, "abba");
        assert_eq!(kinds, &GROUP_ORDER.to_vec());
        assert_eq!(*offset, 0);
        assert_eq!(s.generation(), gen_before);
        assert!(
            s.tick(now + Duration::from_secs(60)).is_empty(),
            "no re-emit"
        );
    }

    /// R3: show-more retry targets one kind at `resume_offset`.
    #[test]
    fn r3_tick_emits_show_more_retry_at_resume_offset() {
        let now = Instant::now();
        let (mut s, _) = show_more_rate_limited(now);
        assert!(s.tick(now).is_empty());
        let cmds = s.tick(now + Duration::from_secs(1));
        let [SourceCommand::SearchCatalog { kinds, offset, .. }] = cmds.as_slice() else {
            panic!("expected one SearchCatalog, got {cmds:?}");
        };
        assert_eq!(kinds, &vec![SearchKind::Track]);
        assert_eq!(*offset, 20);
        assert!(s.tick(now + Duration::from_secs(9)).is_empty());
    }

    /// R4: combined retry success resolves groups and clears the retry.
    #[test]
    fn r4_combined_retry_success() {
        let now = Instant::now();
        let (mut s, _) = combined_rate_limited(now, Some(1_000));
        let later = now + Duration::from_secs(1);
        let cmds = s.tick(later);
        s.apply_reply(request_id(&cmds), Ok(page(3, None)), later);
        assert!(!s.refreshing());
        assert!(!s.retry_scheduled());
        assert_eq!(s.group(SearchKind::Track).items().len(), 3);
    }

    /// R4: show-more retry success appends to the stale rows.
    #[test]
    fn r4_show_more_retry_success() {
        let now = Instant::now();
        let (mut s, _) = show_more_rate_limited(now);
        let later = now + Duration::from_secs(1);
        let cmds = s.tick(later);
        s.apply_reply(request_id(&cmds), Ok(page(20, None)), later);
        let GroupState::Loaded {
            items,
            next_offset,
            loading_more,
        } = s.group(SearchKind::Track)
        else {
            panic!("expected Loaded");
        };
        assert_eq!(items.len(), 40);
        assert_eq!(*next_offset, None);
        assert!(!loading_more);
        assert!(!s.refreshing());
        assert!(!s.retry_scheduled());
    }

    /// R5: repeated rate limit reschedules with attempt + 1.
    #[test]
    fn r5_repeated_rate_limit_backs_off() {
        let now = Instant::now();
        let (mut s, _) = combined_rate_limited(now, None);
        assert!(s.tick(now + Duration::from_secs(14)).is_empty());
        let t1 = now + Duration::from_secs(15);
        let cmds = s.tick(t1);
        s.apply_reply(request_id(&cmds), rate_limited(None), t1);
        assert!(s.retry_scheduled());
        assert!(s.tick(t1 + Duration::from_secs(29)).is_empty());
        assert_eq!(s.tick(t1 + Duration::from_secs(30)).len(), 1);
    }

    /// R5: `retry_after_ms` is honoured on a repeat.
    #[test]
    fn r5_retry_after_honoured_on_repeat() {
        let now = Instant::now();
        let (mut s, _) = show_more_rate_limited(now);
        let t1 = now + Duration::from_secs(1);
        let cmds = s.tick(t1);
        s.apply_reply(request_id(&cmds), rate_limited(Some(5_000)), t1);
        assert!(s.tick(t1 + Duration::from_millis(4_999)).is_empty());
        assert_eq!(s.tick(t1 + Duration::from_secs(5)).len(), 1);
    }

    /// R6: other error on a combined retry → Empty; on show-more → Loaded(stale).
    #[test]
    fn r6_other_error_on_retry() {
        let now = Instant::now();
        let (mut s, _) = combined_rate_limited(now, Some(1_000));
        let t = now + Duration::from_secs(1);
        let cmds = s.tick(t);
        s.apply_reply(request_id(&cmds), Err(CatalogError::Offline), t);
        assert!(s.is_no_results());
        assert!(!s.retry_scheduled());

        let (mut s, _) = show_more_rate_limited(now);
        let cmds = s.tick(t);
        s.apply_reply(request_id(&cmds), Err(CatalogError::Offline), t);
        assert_eq!(
            s.group(SearchKind::Track).items().len(),
            20,
            "stale rows kept"
        );
        assert!(matches!(
            s.group(SearchKind::Track),
            GroupState::Loaded {
                next_offset: Some(20),
                loading_more: false,
                ..
            }
        ));
        assert!(!s.retry_scheduled());
    }

    /// R7: query edit, clear, new generation and offline each cancel retries;
    /// replies to a cancelled retry are discarded.
    #[test]
    fn r7_cancellation() {
        let now = Instant::now();
        // Query edit.
        let (mut s, _) = combined_rate_limited(now, Some(1_000));
        s.set_query("queen", now);
        assert!(!s.retry_scheduled());
        assert!(s.tick(now + Duration::from_millis(1_000)).len() <= 1);

        // Clear.
        let (mut s, _) = combined_rate_limited(now, Some(1_000));
        s.set_query("", now);
        assert!(!s.retry_scheduled());
        assert!(!s.refreshing());

        // Issued retry then edit: late reply discarded.
        let (mut s, _) = combined_rate_limited(now, Some(1_000));
        let t = now + Duration::from_secs(1);
        let cmds = s.tick(t);
        let id = request_id(&cmds);
        s.set_query("queen", t);
        s.apply_reply(id, Ok(page(3, None)), t);
        assert_eq!(s.group(SearchKind::Track).items().len(), 0);

        // New generation.
        let (mut s, _) = show_more_rate_limited(now);
        s.set_query("queen", now);
        let _ = s.tick(now + SEARCH_DEBOUNCE);
        assert!(!s.retry_scheduled());

        // Offline.
        let (mut s, _) = show_more_rate_limited(now);
        s.set_offline(true);
        assert!(!s.retry_scheduled());
        assert!(s.tick(now + Duration::from_secs(60)).is_empty());
    }

    /// R8: offline collapse rules; never the no-results state.
    #[test]
    fn r8_offline_collapse() {
        let now = Instant::now();
        let (mut s, _) = show_more_rate_limited(now);
        s.set_offline(true);
        assert!(matches!(
            s.group(SearchKind::Track),
            GroupState::Loaded {
                next_offset: Some(20),
                loading_more: false,
                ..
            }
        ));
        assert_eq!(s.group(SearchKind::Track).items().len(), 20);
        assert!(!s.is_no_results());

        let (mut s, _) = combined_rate_limited(now, Some(1_000));
        s.set_offline(true);
        for (_, g) in s.groups() {
            assert_eq!(g, &GroupState::Idle);
        }
        assert!(!s.is_no_results());
        assert!(!s.refreshing());
        // Reconnect: the query re-runs.
        s.set_offline(false);
        let cmds = s.tick(now + Duration::from_secs(1));
        assert_eq!(cmds.len(), 1);
    }

    /// R9: a new query never shows the earlier query's results.
    #[test]
    fn r9_new_generation_starts_pending() {
        let now = Instant::now();
        let (mut s, _) = show_more_rate_limited(now);
        s.set_query("queen", now);
        let _ = s.tick(now + SEARCH_DEBOUNCE);
        for (_, g) in s.groups() {
            assert_eq!(g, &GroupState::Pending);
        }
    }
}
