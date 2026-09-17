---

description: "Task list for Catalog Search and Library Browsing"

---

# Tasks: Catalog Search and Library Browsing

**Input**: Design documents from `/specs/004-search-and-library-browse/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Included. quickstart.md names required tests per FR/SC and each contract has a "Tests pinning this contract" section — this is an explicit test requirement, not the template default.

**Organization**: Tasks are grouped by user story (US1 = Search, P1 · US2 = Library, P2 · US3 = Scale & network trouble, P3) per spec.md, preceded by Setup and Foundational phases per the design note "Seam first, UI last" (plan.md §Design notes #1).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no unmet dependency)
- **[Story]**: US1 / US2 / US3 — omitted for Setup, Foundational and Polish tasks
- Every task names its exact file path(s)

## Path Conventions

Cargo workspace, paths as laid out in plan.md §Project Structure:
`crates/modplayer-audio-source/` (trait crate) · `crates/modplayer-audio-source-synthetic/` (scripted/synthetic doubles) · `crates/modplayer-audio-source-connect/` (receiver, maintainer sign-off) · `crates/modplayer-core/` (host state) · `crates/modplayer-ui/` (presentation) · `crates/modplayer/` (binary) · `locales/en-US/` (Fluent strings).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Workspace dependency and lint-surface changes needed before any catalog code compiles.

- [X] T001 Add `image` 0.25 (`default-features = false`, `features = ["jpeg"]`) to the workspace `Cargo.toml` and as a dependency of `crates/modplayer-ui/Cargo.toml` (research R13)
- [X] T002 [P] Add `crates/modplayer-ui/Cargo.toml` dependency on the workspace `ureq` (new in-workspace user, research R13)
- [X] T003 [P] Add `crates/modplayer-core/Cargo.toml` dependency on the workspace `serde_json` (new in-workspace user, research R13)
- [X] T004 Run `cargo deny check` against T001–T003 and update `deny.toml` only if `image`'s dependency tree surfaces a licence not yet allow-listed (plan.md expects none)
- [X] T005 [P] Create `locales/en-US/library.ftl` with a header comment as the target file for all new keys (contracts/ui-surface.md §9); no keys yet

**Checkpoint**: workspace builds with new dependencies; no behavior yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Trait-crate types, the scripted double, receiver dispatch skeleton, shared queue ops and the one shared row/artwork/skeleton UI primitive that every user story renders through (FR-004 "rows are one widget"). No story's tests can pass until this phase compiles.

**⚠️ CRITICAL**: No user story work may begin until this phase is complete.

### Trait crate (types + seam contract)

- [X] T006 [P] Widen `Availability` in `crates/modplayer-audio-source/src/types.rs`: rename `Unavailable` → `UnavailableRegion`, add `Removed`; update the two existing 003 call sites (data-model.md §1.2)
- [X] T007 [P] Extend `TrackRef` in `crates/modplayer-audio-source/src/types.rs`: add `artist_ids`, `album_id`, `explicit`, `release_date` fields, `ReleaseDate { year, month, day }`, and `TrackRef::with_extras(TrackRefExtras)`; keep the existing constructor signature and "Unknown title" fallback (data-model.md §1.3)
- [X] T008 [P] Add `TrackId`/`AlbumId`/`ArtistId`/`PlaylistId` newtypes and `AlbumRef`, `ArtistRef`, `PlaylistRef` in `crates/modplayer-audio-source/src/catalog.rs` (data-model.md §1.4–1.6)
- [X] T009 Add catalog payload types `LibrarySet`, `SearchKind`, `TrackListSource`, `SearchHit`, `SearchGroupPage`, `SearchPage`, `LibraryItem`, `LibraryPage`, `TrackList`, `CatalogError` in `crates/modplayer-audio-source/src/catalog.rs` (data-model.md §1.7; depends on T008)
- [X] T010 Add `SourceCommand` variants `SearchCatalog`, `FetchLibrary`, `FetchTrackList`, `HydrateRefs`, `CancelCatalog` and `SourceEvent` variants `SearchResult`, `LibraryPage`, `TrackList`, `Hydrated` in `crates/modplayer-audio-source/src/lib.rs`, additive only — `ListAccountTracks`/`AccountTracks` stay for now, removed atomically with T061 (contracts/catalog-source.md §1–2)
- [X] T011 [P] Unit tests for id-newtype invariants and `request_id` (`generation << 8 | kind`) round-trip in `crates/modplayer-audio-source/src/catalog.rs` (contracts/catalog-source.md §5)

### Scripted / synthetic doubles

- [X] T012 Implement `script_search`, `script_library`, `script_track_list`, `script_hydrate`, `script_catalog_delay`, `record_commands()` on `ScriptedHost` in `crates/modplayer-audio-source-synthetic/src/scripted.rs` (contracts/catalog-source.md §4; depends on T009, T010)
- [X] T013 [P] `SyntheticHost` answers every new catalog command with `Err(CatalogError::Unsupported)` in `crates/modplayer-audio-source-synthetic/src/host.rs`
- [X] T014 [P] `crates/modplayer-audio-source-synthetic/tests/host.rs`: scripted catalog replies honour delay/order; synthetic returns `Unsupported` (contracts/catalog-source.md §5)

### Receiver dispatch skeleton

- [X] T015 Wire new catalog command arms in `crates/modplayer-audio-source-connect/src/worker.rs` to `runtime.spawn`ed tasks behind a `tokio::sync::Semaphore(4)`, following the existing `ListAccountTracks` pattern (contracts/catalog-source.md §3 "Threads")
- [X] T016 Create `crates/modplayer-audio-source-connect/src/catalog/mod.rs`: dispatch table + error classification per contracts/catalog-source.md §2 rule 4 (I/O-class→`Offline`, 429→`RateLimited`, 404→`NotFound`, refused→`Unsupported`, else redacted `Unavailable`); wire the module tree in `crates/modplayer-audio-source-connect/src/lib.rs`

### Core: queue additions + event-routing scaffold

- [X] T017 [P] `Queue::replace_context_at(tracks, cursor)` and `Queue::play_next_tracks(tracks)` in `crates/modplayer-core/src/queue.rs` (contracts/library-and-search-core.md §2)
- [X] T018 [P] Extend the existing queue operation-sequence proptest with both new operations in `crates/modplayer-core/tests/queue_proptest.rs`
- [X] T019 Add catalog-event routing scaffold in `crates/modplayer-core/src/controller.rs`: dispatch `SearchResult`/`LibraryPage`/`TrackList`/`Hydrated` by `request_id` to stub handlers (never the transport reducer), plus a persistence-thread hook point (contracts/library-and-search-core.md §1; depends on T010)

### UI: shared row / artwork / skeleton primitives

- [X] T020 [P] Row model types `RowEntity`, `RowOrigin`, `RowAction`, `ActingList` in `crates/modplayer-ui/src/rows.rs` (data-model.md §4)
- [X] T021 [P] `initials(name)` helper and `widgets/initials.rs` (two-letter rule, music-glyph fallback) in `crates/modplayer-ui/src/widgets/initials.rs`
- [X] T022 [P] Skeleton row widget (`Role::Status`, name "Loading") in `crates/modplayer-ui/src/widgets/skeleton.rs`; wire into `crates/modplayer-ui/src/widgets/mod.rs`
- [X] T023 `ArtworkCache` — off-thread `ureq` fetch pool, `image` decode, in-memory LRU, negative cache, `ArtworkState { Ready | Loading | Failed }` in `crates/modplayer-ui/src/artwork.rs` (contracts/ui-surface.md §6; depends on T001, T002, T021)
- [X] T024 `rows::ListRow` widget rendering Track/Album/Artist/Playlist content, accessible name incl. availability reason, six-action `Menu` in fixed order (contracts/ui-surface.md §5) in `crates/modplayer-ui/src/rows.rs` (depends on T020–T023)
- [X] T025 `rows::acting_list()` implementing the acting-list rule (contracts/library-and-search-core.md §3) in `crates/modplayer-ui/src/rows.rs` (depends on T024, T017)
- [X] T026 Shell: add `Section::Search` to the rail between Library and Now Playing, `Ctrl/Cmd+1..5`, `Ctrl/Cmd+F`/`/` focuses the search box, drop the `library_placeholder` wiring point in `crates/modplayer-ui/src/shell.rs` (contracts/ui-surface.md §1)
- [X] T027 [P] Seed `locales/en-US/library.ftl` with the shared row/action keys: `row-explicit`, `row-actions`, `row-unavailable-region`, `row-removed`, `action-play-now`, `action-play-next`, `action-add-to-queue`, `action-add-to-playlist`, `action-save-to-library`, `action-pin-offline`, `coming-soon`, `loading`, `detail-back` (contracts/ui-surface.md §9)

**Checkpoint**: Foundation ready — user story implementation can now begin.

---

## Phase 3: User Story 1 - Find and play any track by searching (Priority: P1) 🎯 MVP

**Goal**: Free-text search returns Tracks/Albums/Artists/Playlists groups live as the user types (150 ms debounce), first group painted ≤ 300 ms, non-music excluded, 20-per-page with Show more, race-safe, offline/no-results states, and all six row actions work per the acting-list rule.

**Independent Test**: With a signed-in account, type a query that matches tracks, albums, artists and playlists; verify all four groups populate within budget, each result's six actions behave as specified, and no non-music result ever appears.

### Tests for User Story 1 ⚠️ write first, confirm failing before implementing

- [X] T028 [P] [US1] `search_session.rs`: debounce (149 ms → no request, 150 ms → request), trim + empty → no request/reset, stale-generation discard, group order + omission, show-more offsets, rate-limit keeps stale + refreshing, offline → no request, in-flight + offline → discarded, in `crates/modplayer-core/tests/search_session.rs`
- [X] T029 [P] [US1] `catalog_mapping.rs::non_music_hits_are_dropped` fixture test in `crates/modplayer-audio-source-connect/tests/catalog_mapping.rs`
- [X] T030 [P] [US1] `live.rs::search_probe` (`#[ignore = "manual"]`) in `crates/modplayer-audio-source-connect/tests/live.rs`
- [X] T031 [P] [US1] `search_view.rs`: skeleton while pending, group omission, no-results vs. offline state, refreshing keeps stale rows, show-more paging, `first_group_paints_within_budget` (scripted 100 ms reply, injected clock, SC-001 proxy) in `crates/modplayer-ui/tests/search_view.rs`
- [X] T032 [P] [US1] `rows.rs`: acting-list rule for the search Tracks group, cursor placement, six actions incl. placeholders inert (`coming-soon` raised, no command recorded) in `crates/modplayer-ui/tests/rows.rs`
- [X] T033 [P] [US1] `accessibility.rs`: search box, four group headers, Show more button, row menu — role + name + Enter/menu-key behaviour, in `crates/modplayer-ui/tests/accessibility.rs`

### Implementation for User Story 1

- [X] T034 [US1] `SearchSession`/`GroupState` state machine — debounce deadline, generation, fixed group order, offline gate (data-model.md §2.1) in `crates/modplayer-core/src/search.rs`
- [X] T035 [US1] `PlaybackController::search()/search_mut()/search_show_more()`; `tick()` issues `SearchCatalog` commands and routes `SearchResult` replies by `request_id` in `crates/modplayer-core/src/controller.rs` (depends on T034, T019)
- [X] T036 [US1] Receiver `catalog/search.rs`: strategy list — mercury `searchview` then context-resolve fallback (gate V1), non-music filtering (contracts/catalog-source.md §2 rule 5) in `crates/modplayer-audio-source-connect/src/catalog/search.rs` (depends on T016)
- [X] T037 [US1] Receiver `catalog/hydrate.rs`: batched `get_extended_metadata` mapping → `TrackRef`/`AlbumRef`/`ArtistRef`, availability (gate V4), artwork URL (gate V5) in `crates/modplayer-audio-source-connect/src/catalog/hydrate.rs` (depends on T036)
- [X] T038 [US1] `search_view.rs`: search box, four group headers/lists rendered through `rows::ListRow`, Show more, no-results/offline states (contracts/ui-surface.md §2) in `crates/modplayer-ui/src/search_view.rs` (depends on T024, T025, T034)
- [X] T039 [US1] Wire `Section::Search` into `crates/modplayer-ui/src/app.rs` — view dispatch, hydrate-visible hints, artwork cache lifetime (depends on T026, T038)
- [X] T040 [US1] Add search fluent keys — `nav-search`, `search-placeholder`, `search-offline`, `search-no-results`, `search-group-tracks|albums|artists|playlists`, `search-show-more`, `refreshing` — to `locales/en-US/library.ftl`; add `nav-search` to `locales/en-US/app.ftl`
- [X] T041 [US1] `fluent_keys.rs` extended: assert every search key from T040 exists in `crates/modplayer-ui/tests/fluent_keys.rs`

**Checkpoint**: User Story 1 is fully functional and independently testable — search, play now/next/add-to-queue, placeholders.

---

## Phase 4: User Story 2 - Browse and play from your own library (Priority: P2)

**Goal**: Library view mirrors the account (Saved Tracks, Saved Albums, Followed Artists, Playlists, Recently Played) from a locally kept-in-sync index, syncing on launch / 15 min / reconnect without blocking the UI; album/playlist/artist detail views; Recently Played is ModPlayer's own play log; the 003 "Play from account" scaffold is removed.

**Independent Test**: With a signed-in account that has saved tracks, saved albums, followed artists, and both an owned and a followed playlist, open the Library view and verify every section lists correctly, a followed playlist shows no edit affordances, Recently Played lists past plays newest-first, and every row's six actions behave exactly as in Search.

### Tests for User Story 2 ⚠️ write first, confirm failing before implementing

- [X] T042 [P] [US2] `library_sync.rs`: launch/interval/reconnect triggers, page walking, rate-limit backoff, silent failure with snapshot, first-sync-failed without snapshot, resume after offline, partial outcome on `Unsupported`, in `crates/modplayer-core/tests/library_sync.rs`
- [X] T043 [P] [US2] `library_index.rs`: merge semantics, lazy hydration queue, 50 000 × 1 000 fixture load/lookup under budget (SC-002 proxy: index ops ≤ 1 ms), in `crates/modplayer-core/tests/library_index.rs`
- [X] T044 [P] [US2] `play_log.rs`: record on `TrackStarted → Playing` only, dedupe, 100-window, `last_played` outlives the window, restart round-trip, Connect-transfer recorded, unavailable-skipped not recorded, in `crates/modplayer-core/tests/play_log.rs`
- [X] T045 [P] [US2] `persist.rs`: atomic write, crash-mid-write keeps prior file, newer schema ⇒ empty + one warning, sign-out deletes both files, plus proptests `index_file_round_trips_any_index` (`proptest!` over arbitrary `LibraryIndex` contents — every set, refs incl. Unicode names, optional fields, every `Availability`, sync meta: save → load == original) and `play_log_file_round_trips_any_log` (arbitrary `last_played`/`play_count`/`first_played` maps and `refs`: save → load == original, `recent` re-derives identically), in `crates/modplayer-core/tests/persist.rs` *(Constitution VIII — state serialization proptest, contracts/library-and-search-core.md §6)*
- [X] T046 [P] [US2] `controller_streaming.rs` extended: catalog event routing, connectivity derivation, in `crates/modplayer-core/tests/controller_streaming.rs`
- [X] T047 [P] [US2] `catalog_mapping.rs` additions: `collection` URI-prefix split, rootlist → `PlaylistRef` mapping, error classification without leaking raw text, in `crates/modplayer-audio-source-connect/tests/catalog_mapping.rs`
- [X] T048 [P] [US2] `live.rs::library_sets_probe` and `::availability_probe` (`#[ignore = "manual"]`) in `crates/modplayer-audio-source-connect/tests/live.rs`
- [X] T049 [P] [US2] `library_view.rs`: tab order/initial tab, each empty state + its action, FR-021 first-sync-failed state, owner label, no edit controls, offline snapshot, `detail_*` view tests, in `crates/modplayer-ui/tests/library_view.rs`
- [X] T050 [P] [US2] `developer_removed.rs`: no "Play from account" control renders, in `crates/modplayer-ui/tests/developer_removed.rs`
- [X] T051 [P] [US2] `fluent_keys.rs` extended: library/detail/action keys exist, scaffold keys gone, in `crates/modplayer-ui/tests/fluent_keys.rs`

### Implementation for User Story 2

- [X] T052 [US2] `library::Connectivity` derived from `SourceHealth` + `Registered` (research R5) in `crates/modplayer-core/src/library/connectivity.rs`
- [X] T053 [US2] `library::persist` — JSON DTOs, atomic temp-file + rename, background load, data-dir resolution for `index.json` + `play_log.json` (data-model.md §3.4) in `crates/modplayer-core/src/library/persist.rs`
- [X] T054 [US2] `LibraryIndex`/`LibraryStatus` facade — sets, refs, `track_lists`, merge, hydration queue (data-model.md §3.1) in `crates/modplayer-core/src/library/index.rs` and `crates/modplayer-core/src/library/mod.rs` (depends on T053)
- [X] T055 [US2] `SyncScheduler` — pure state machine, injectable clock, launch/15-min/reconnect triggers, backoff (data-model.md §3.3) in `crates/modplayer-core/src/library/sync.rs` (depends on T054)
- [X] T056 [US2] `PlayLog` — record/recent-100/persistence DTO (data-model.md §3.5) in `crates/modplayer-core/src/library/play_log.rs` (depends on T053)
- [X] T057 [US2] Receiver `catalog/collection.rs`: hand-encoded `collection2v2` `PageRequest`/`PageResponse`, set split, saved-tracks context fallback (gate V2) in `crates/modplayer-audio-source-connect/src/catalog/collection.rs` (depends on T016)
- [X] T058 [US2] Receiver `catalog/playlists.rs`: rootlist raw proto + `Playlist::get` → `PlaylistRef`, folding `fetch_account_tracks`; delete `crates/modplayer-audio-source-connect/src/account_read.rs`
- [X] T059 [US2] Receiver `catalog/track_lists.rs`: album discs, playlist tracks, artist top-10 mapping in `crates/modplayer-audio-source-connect/src/catalog/track_lists.rs` (depends on T037)
- [X] T060 [US2] `PlaybackController` library surface — `library()`, `library_status()`, `library_retry_sync()`, `library_track_list()`, `library_hydrate_visible()`, `recently_played()`; route `LibraryPage`/`TrackList`/`Hydrated`; play-log hook on `TrackStarted → Playing`; persistence flush ≤ 1 s; `clear_for_sign_out()` additions (contracts/library-and-search-core.md §1) in `crates/modplayer-core/src/controller.rs` (depends on T054, T055, T056)
- [X] T061 [US2] Remove `ListAccountTracks`/`AccountTracks` from `crates/modplayer-audio-source/src/lib.rs` and every implementor (`modplayer-audio-source-synthetic`, `modplayer-audio-source-connect`, `modplayer-core`) — atomic with T057/T058 landing (plan.md design note 9)
- [X] T062 [US2] Remove the "Play from account" scaffold — `PlayFromAccountState`, `developer.play_from_account` descriptor, its `.ftl` keys — from `crates/modplayer-ui/src/settings/developer.rs`, `crates/modplayer-core/src/settings_registry.rs`, `locales/en-US/playback.ftl`, `locales/en-US/settings.ftl` (depends on T061; FR-022)
- [X] T063 [US2] `library_view.rs` — five tabs, empty/first-sync-failed/refreshing states, virtualised lists, `library_hydrate_visible` wiring (contracts/ui-surface.md §3) in `crates/modplayer-ui/src/library_view.rs` (depends on T024, T025, T060)
- [X] T064 [US2] `detail_view.rs` — album/playlist/artist detail views (contracts/ui-surface.md §4) in `crates/modplayer-ui/src/detail_view.rs` (depends on T024, T060)
- [X] T065 [US2] Wire `Section::Library` → `library_view`, add detail-navigation stack in `crates/modplayer-ui/src/app.rs` and `crates/modplayer-ui/src/shell.rs` (depends on T063, T064)
- [X] T066 [US2] Add library/detail fluent keys — `library-tab-*` (×5), `library-empty`, `library-empty-albums`, `library-empty-artists`, `library-empty-playlists`, `library-empty-recent`, `library-first-sync-failed`, `library-retry`, `action-search`, `action-create`, `playlist-owner`, `playlist-no-tracks` — to `locales/en-US/library.ftl`

**Checkpoint**: User Stories 1 AND 2 both work independently — search and library browsing/playback, scaffold gone.

---

## Phase 5: User Story 3 - Stay usable and honest at real-world scale and under network trouble (Priority: P3)

**Goal**: No visible scroll/search degradation at 50 000 saved tracks / 1 000 playlists; rate-limited responses degrade to stale + "Refreshing…"; unavailable tracks are greyed with reason; failed artwork becomes initials; skeleton rows everywhere loading occurs.

**Independent Test**: Load a library fixture at 50 000 saved tracks / 1 000 playlists and confirm no visible scroll or search degradation; separately simulate a rate-limited response, a region-locked/removed track, and a failed artwork load, and confirm each degrades exactly as specified rather than erroring.

### Tests for User Story 3 ⚠️ write first, confirm failing before implementing

- [X] T067 [P] [US3] `rows.rs::initials_*` and artwork-failure ⇒ initials, in `crates/modplayer-ui/tests/rows.rs`
- [X] T068 [P] [US3] `catalog_mapping.rs::availability_*` — region-locked and removed mapping, in `crates/modplayer-audio-source-connect/tests/catalog_mapping.rs`
- [X] T069 [P] [US3] `library_view.rs::large_fixture_frame_budget` — 50 000 × 1 000 fixture, frame ≤ 8 ms in `cargo test --release` (SC-002 proxy; debug asserts only row-count), in `crates/modplayer-ui/tests/library_view.rs`
- [X] T070 [P] [US3] `accessibility.rs` full pass — every new element across Search/Library/detail has role + name + state (SC-009), in `crates/modplayer-ui/tests/accessibility.rs`

### Implementation for User Story 3

- [X] T071 [US3] Virtualisation pass — only visible rows laid out/request artwork/hydrate — across `crates/modplayer-ui/src/rows.rs`, `search_view.rs`, `library_view.rs`, `detail_view.rs` (FR-014)
- [X] T072 [US3] Debug-only env overrides `MODPLAYER_CATALOG_FORCE_429` (in `crates/modplayer-audio-source-connect/src/worker.rs`), `MODPLAYER_ARTWORK_FORCE_FAIL` (in `crates/modplayer-ui/src/artwork.rs`), `MODPLAYER_LIBRARY_FIXTURE=large` (in `crates/modplayer-core/src/library/index.rs`) — `cfg(debug_assertions)`, read fresh per use
- [X] T073 [US3] Confirm unavailable-row greying + reason text/accessible name is wired on every surface (search, all five library sections, all three detail views) in `crates/modplayer-ui/src/rows.rs` (FR-019/SC-006)
- [X] T074 [US3] Confirm artwork-load failure resolves to the initials placeholder on every surface via `ArtworkCache`/`ListRow` (FR-020/SC-007)

**Checkpoint**: All three user stories independently functional at scale and under network trouble.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Whole-workspace gates from quickstart.md and constitution-mandated checks that span every story.

- [X] T075 [P] `credential_leak.rs` extended for the new catalog event payloads (session token never crosses the seam) in `crates/modplayer-ui/tests/credential_leak.rs`
- [X] T076 [P] Confirm `modplayer tests/single_dependent.rs` guard (Constitution IV: only the binary depends on `modplayer-audio-source-connect`) stays green in `crates/modplayer/tests/single_dependent.rs`
- [X] T077 [P] Doc comments with examples on new public items across `modplayer-audio-source`, `modplayer-core::library`, `modplayer-core::search`, `modplayer-ui::rows`/`artwork` (Constitution VII)
- [X] T078 Run the automated gates from quickstart.md: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check`, `scripts/check-license-headers.sh`
- [X] T079 Run the live protocol probes (`MODPLAYER_LIVE=1 cargo test -p modplayer-audio-source-connect --test live -- --ignored search_probe library_sets_probe availability_probe`); record each strategy outcome in `research.md` R14 before treating four search groups / three collection sets as confirmed
- [X] T080 Walk manual scenarios M1–M16 in quickstart.md against a signed-in Premium account; record any deviation — **macOS walk done 2026-09-17** (macOS 12.6, x86_64, debug build, toolchain 1.95.0, live Premium account, driven by synthetic Quartz events + `screencapture` per constitution "Manual Scenario Sign-Off"; evidence in `target/manual-walk/*.png`, not committed). **Pass**: M1 (Tracks group only — expected deviation, research R14 V1), M2, M3 (second page empty → button hides; single-page context-resolve search, R14 V1), M4 (search + library), M5 (playlist Play next; album/playlist search rows unreachable — R14 V1), M6, M7 (skeletons ⇒ rows, snapshot on relaunch), M8, M9 (in-session + across restart), M10 (via `sandbox-exec (deny network*)`), M12, M14, M15, M16. **Not executed**: M11 (runtime reconnect cannot be driven without dropping the agent's own link; covered by `library_sync.rs`/`search_session.rs` reconnect tests), M13 (no known region-locked URI; `catalog_mapping.rs::availability_*` covers the mapping). **Eleven defects found and fixed this run** (D1–D11, all with regression tests where unit-testable): D1 row text overflow, D2 false "Unavailable in your region" on relinked tracks, D4 "⋯" tofu glyph, D5 bare playlist count, D6 search group scroll carry-over, D7 queue collapsing to one item after Play now (three unshared `MapperState`s + relinked-alternative uri, 003 regression), D8 wide-row menu actions dropped, D9 `main.rs` never enabled library persistence + no flush/join on quit, D10 stale test entry in the real play log (removed), D11 bare group headers under rate limit, plus the R14 V2 collection fallback gate (T079). See quickstart.md "Manual walk 2026-09-17" and plan.md "Post-Implementation Findings".

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies — start immediately
- **Foundational (Phase 2)**: depends on Setup — BLOCKS all user stories
- **User Story 1 (Phase 3)**: depends on Foundational only
- **User Story 2 (Phase 4)**: depends on Foundational only; independent of US1 (shares only the Phase 2 primitives, not US1's files)
- **User Story 3 (Phase 5)**: depends on Foundational; T067/T073/T074 also touch rows/artwork already built in Phase 3/4, so land after US1+US2 in practice even though nothing in Phase 5 is a hard file-level dependency on their new files
- **Polish (Phase 6)**: depends on all desired user stories being complete

### User Story Dependencies

- **US1 (P1)**: no dependency on US2/US3
- **US2 (P2)**: no dependency on US1's files; T061 (remove `ListAccountTracks`) is safe once T009/T010 landed in Foundational, independent of US1's own progress
- **US3 (P3)**: exercises rows/library/search surfaces built in US1+US2, so is sequenced last even though its own tasks touch no US1/US2-exclusive file

### Within Each User Story

- Tests (T028–T033, T042–T051, T067–T070) MUST be written and confirmed failing before their story's implementation tasks
- Trait/receiver types before core state; core state before controller wiring; controller wiring before UI
- Story complete before moving to the next priority, per Implementation Strategy below

### Parallel Opportunities

- All `[P]` Setup tasks (T002–T003, T005) run together after T001
- All `[P]` Foundational tasks within a sub-group (e.g. T006–T008, T011, T013–T014, T017–T018, T020–T022, T027) run together — cross-sub-group tasks (T009→T010→T012, T023→T024→T025) are sequential
- Once Foundational is done, US1 and US2 can be staffed in parallel (different files throughout); US3 should follow both since it hardens what they built
- All `[P]` test tasks within a story's test block run together
- All `[P]` Polish tasks (T075–T077) run together before T078

---

## Parallel Example: User Story 1

```bash
# Tests together, before implementation:
Task: "search_session.rs debounce/trim/discard/order/paging/rate-limit/offline in crates/modplayer-core/tests/search_session.rs"
Task: "catalog_mapping.rs::non_music_hits_are_dropped in crates/modplayer-audio-source-connect/tests/catalog_mapping.rs"
Task: "live.rs::search_probe (#[ignore]) in crates/modplayer-audio-source-connect/tests/live.rs"
Task: "search_view.rs skeleton/omission/no-results/offline/refreshing/show-more/budget in crates/modplayer-ui/tests/search_view.rs"
Task: "rows.rs acting-list/cursor/six-actions for search Tracks group in crates/modplayer-ui/tests/rows.rs"
Task: "accessibility.rs search box/headers/show-more/row-menu in crates/modplayer-ui/tests/accessibility.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL — blocks all stories)
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: run T028–T033 against T034–T041; walk quickstart.md M1–M4, M6, M16
5. Demo: search, play now/next/add-to-queue, placeholder toasts

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. Add US1 → validate independently → demo (MVP)
3. Add US2 → validate independently (library sections, detail views, Recently Played, scaffold gone) → demo
4. Add US3 → validate at 50 000×1 000 scale and under simulated network trouble → demo
5. Polish → full quickstart.md gate + manual scenario pass

### Parallel Team Strategy

With multiple developers, after Foundational (Phase 2):

- Developer A: User Story 1 (search) — trait-crate search payloads already land in Foundational; receiver `catalog/search.rs` + `catalog/hydrate.rs`, core `search.rs`, UI `search_view.rs`
- Developer B: User Story 2 (library) — receiver `catalog/{collection,playlists,track_lists}.rs`, core `library/*`, UI `library_view.rs`/`detail_view.rs`, scaffold removal
- Both converge for User Story 3 (scale/network-trouble hardening) once US1 and US2 land

---

## Notes

- `[P]` tasks touch different files with no unmet dependency in this list
- `[Story]` maps every Phase 3+ task to US1/US2/US3 for traceability to spec.md
- Tests are written first within each story and must fail before implementation begins
- Commit after each task or logical group
- Stop at any checkpoint to validate a story independently
- No task in Phase 3–5 is a same-file conflict with a `[P]`-tagged sibling in its own block; cross-story file overlap is limited to shared Foundational files, already sequenced out of the story phases
