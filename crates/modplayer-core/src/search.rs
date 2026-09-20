// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SearchSession`: pure, host-owned free-text catalog search state
//! (data-model.md §2.1, contracts/library-and-search-core.md §1/§4,
//! research R7). No I/O, no egui — `tick(now)` returns the
//! `SourceCommand`s to issue and `apply_reply` folds a
//! `SourceEvent::SearchResult` back in; `PlaybackController` (`search()`/
//! `search_mut()`/`search_show_more()`, `tick()`) is the only caller.

use std::time::{Duration, Instant};

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
    },
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
        self.debounce_deadline = Some(now + SEARCH_DEBOUNCE);
    }

    fn reset_to_idle(&mut self) {
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
    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
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
            return Vec::new();
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
    /// §2.1: "poisons in-flight replies").
    pub fn apply_reply(&mut self, request_id: u64, result: Result<SearchPage, CatalogError>) {
        if self.offline {
            return;
        }
        let (generation, kind_byte) = unpack_request_id(request_id);
        if generation != self.generation {
            return;
        }

        if self.pending_request_id == Some(request_id) {
            self.pending_request_id = None;
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
                Err(CatalogError::RateLimited { .. }) => {
                    for state in &mut self.groups {
                        if matches!(state, GroupState::Pending) {
                            *state = GroupState::RateLimited { stale: None };
                        }
                    }
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
        match result {
            Ok(page) => self.apply_page(page),
            Err(CatalogError::RateLimited { .. }) => {
                if let GroupState::Loaded { items, .. } = &mut self.groups[idx] {
                    let stale = std::mem::take(items);
                    self.groups[idx] = GroupState::RateLimited { stale: Some(stale) };
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
