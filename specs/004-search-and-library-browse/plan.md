# Implementation Plan: Catalog Search and Library Browsing

**Branch**: `004-search-and-library-browse` | **Date**: 2026-09-16 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/004-search-and-library-browse/spec.md`

## Summary

Let a signed-in user find and start music without a second player: a
**Search** section (free text, 150 ms debounce, four groups Tracks / Albums /
Artists / Playlists, 20 per page with Show more, non-music never shown) and a
real **Library** section (Saved Tracks, Saved Albums, Followed Artists,
Playlists, Recently Played, plus album / playlist / artist detail views),
every row carrying the same six actions — Play now / Play next / Add to
queue (functional, on 003's unchanged Queue contract) and Add to playlist /
Save to library / Pin for offline (inert "Coming soon" placeholders). Lists
are virtualised, loading uses skeleton rows, rate-limits degrade to stale
data plus "Refreshing…", unavailable tracks are greyed with a reason,
failed artwork becomes initials, and the 003 "Play from account" developer
scaffold is removed.

Technical approach (details in [research.md](research.md)): all catalog
and library reads go through the **existing `SourceHost` seam** as new
`SourceCommand`/`SourceEvent` pairs (R1), fulfilled by the receiver crate
on its tokio worker against the same librespot session that streams audio
— the only credential that works under 003's single-credential finding.
Search uses an ordered strategy list inside the receiver (mercury
searchview, then context-resolve tracks-only) behind a live verification
gate (R2); library sets use collection-v2 paging with hand-encoded protos
and a documented fallback (R3); identities are hydrated in batches (R4).
The host side is pure, testable state in `modplayer-core`: a
`SearchSession` (R7), a `LibraryIndex` persisted as JSON and loaded
off-thread, a `SyncScheduler` with launch / 15-min / reconnect triggers
(R6), a `PlayLog` recorded on the `TrackStarted → Playing` transition
(R11), and a derived connectivity fact (R5). The UI adds one shared
virtualised `ListRow` widget for every list (R9) and an artwork cache that
fetches the public CDN URL with `ureq` and decodes with `image` (R8).

Assumptions taken headlessly (all recorded in Complexity Tracking or
research): the grouped search endpoint is unverified for 2026 and gated by
a manual probe with a tracks-only fallback; collection-v2 paging is likewise
gated; "online" is derived from source health rather than an OS
reachability API; the library index is JSON, not SQLite.

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001–003

**Primary Dependencies**: existing eframe/egui 0.36 (+accesskit), librespot-core/-metadata/-protocol 0.8.0 (receiver crate only), tokio 1 (receiver only), protobuf 3.7 (receiver only), fluent-templates 0.15, serde + serde_json, directories 6, ureq 3.4, time 0.3, proptest (dev); **new**: `image` 0.25 (`default-features = false`, `jpeg`) in `modplayer-ui`; `ureq` and `serde_json` gain new in-workspace users (`modplayer-ui`, `modplayer-core`) — research R13

**Storage**: `<data_local_dir>/ModPlayer/library/index.json` (library mirror + sync meta, ≈ 15 MB at 50 000 tracks) and `library/play_log.json` (per-track first/last played + count), both atomic temp-file + rename, user-only permissions, deleted on sign-out; search results in-memory only; artwork textures in an in-memory LRU (no disk image cache in this slice); no decoded audio, no credentials

**Testing**: `cargo test --workspace` with `ScriptedHost` (extended to script catalog replies with delays), `FakeBackend`, injected clocks; proptest for the two new queue operations and for the two new persisted state formats (`index.json` / `play_log.json` save → load round-trip over arbitrary `LibraryIndex` / `PlayLog` contents, Constitution VIII state serialization — `tests/persist.rs::index_file_round_trips_any_index`, `::play_log_file_round_trips_any_log`); 50 000 × 1 000 synthetic fixture for the index and the virtualised view; live protocol probes `#[ignore = "manual"]` in the receiver crate; manual scenarios M1–M16 in [quickstart.md](quickstart.md); CI gates unchanged (fmt, clippy `-D warnings`, test, deny, licence headers) on ubuntu / macos / windows

**Target Platform**: Desktop macOS, Windows 10+, Linux — identical behaviour and shortcuts; platform differences stay in the existing adapter crates (`directories` paths, keyring)

**Project Type**: Desktop application — Cargo workspace stays at 10 crates (no new crate)

**Performance Goals**: first search group painted ≤ 300 ms after the last keystroke, 95 % of trials (SC-001; 150 ms debounce + one round trip); no perceptible scroll/search degradation at 50 000 saved tracks / 1 000 playlists (SC-002; only visible rows laid out, index ops ≤ 1 ms); find-and-play a known track in < 10 s (SC-003); `tick()` stays O(events); UI thread never blocks on network, disk or hydration

**Constraints**: real-time path untouched (Constitution I — no engine change in this slice); only the receiver crate speaks the protocol and only the binary depends on it (IV, guard test kept); no audio or cache decryption surface added (V); session token never leaves the receiver crate, never logged (VI); `#![forbid(unsafe_code)]`, no `unwrap`/`expect` outside tests (VII); all strings externalised, en-US only; every new control keyboard-operable with an accessible name/role/state (X); no new crate, trait or feature flag (X)

**Scale/Scope**: trait-crate delta (types + catalog payloads, ~500 LOC), receiver delta (search/library/hydrate mapping + hand-encoded protos + live probes, ~1 200 LOC), core additions (search session, library index/sync/persist/play log/connectivity, controller routing, 2 queue ops, ~2 200 LOC incl. tests), UI (Search view, Library view + 5 tabs, 3 detail views, row widget + menu, artwork cache, skeleton/initials widgets, shell/nav delta, developer scaffold removal, ~2 000 LOC), 1 new `.ftl` (~45 keys), ~90 automated tests; single user / single account / one library

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | No | ✅ N/A | No engine, `Processor`, `ConnectRtSource` or ring change. Every new path (search, sync, hydration, persistence, artwork) runs on the receiver's tokio worker, core's persistence thread, the UI's artwork pool or the UI thread — all off the audio thread; the `SourceHost` RT half is untouched (contracts/catalog-source.md §3 "Threads"). |
| II. Plugins Are Guests | No | ✅ N/A | No plugin runtime exists yet; `library.read`/`library.write` plugin access is explicitly out of scope (spec Scope boundary, FR-023). |
| III. Host Primitives, Plugin Behaviors | **Yes** | ✅ PASS | Library index, search state, play log and the acting-list rule are host primitives in `modplayer-core`/`modplayer-ui`; the Queue contract is reused, not reimplemented (FR-006/007). No DSP. |
| IV. Audio Source Is Replaceable and Isolated | **Yes** | ✅ PASS | All protocol reads (mercury/spclient/metadata) are added only to `crates/modplayer-audio-source-connect`; core and UI see only `SourceCommand`/`SourceEvent` data through the existing seam (research R1); the `ScriptedHost`/`SyntheticHost` implementations answer the new commands so every other crate builds and tests without the receiver; `modplayer tests/single_dependent.rs` guard stays. Artwork is fetched from a public CDN URL by the UI — not a protocol surface (R8). |
| V. No Audio Ever Leaves the Engine (non-negotiable) | No | ✅ N/A | Only metadata, identities and artwork are persisted or exposed; no stream, sample buffer or cache-decryption path is added (contracts/library-and-search-core.md §5). |
| VI. Security and Privacy by Default | **Yes** | ✅ PASS | The session token stays inside the receiver crate; catalog errors are classified and redacted before crossing the seam (contracts/catalog-source.md §2 rule 4); persisted files hold no credentials and are user-only; no telemetry; `credential_leak` test extended to the new event payloads. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | No new crate; `#![forbid(unsafe_code)]` kept in every changed crate; `thiserror` for `CatalogError`; no `unwrap`/`expect` outside tests; doc comments with examples on new public items; one new dependency (`image`) justified in research R13 and licence-checked against `deny.toml`; CI matrix unchanged; `CODEOWNERS` already routes the receiver crate. |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | Named tests per FR/SC in quickstart.md (search session, sync scheduler, play log, persistence crash-safety, virtualisation at NFR-3.1 scale, accessibility coverage of every new element); proptest extended for the two queue operations; proptest round-trips for both new persisted state formats (`index.json`, `play_log.json`: arbitrary `LibraryIndex`/`PlayLog` survives save → load, in `tests/persist.rs`, matching 002's `state_store.rs`/`credential.rs` precedent); live probes gate the unverified endpoints; loop-seam/latency/no-alloc suites untouched and still required green. |
| IX. One Plugin API Definition | No | ✅ N/A | No plugin API in this slice. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | No new trait (the seam already has 3 implementors), no feature flag, no new crate; each new dependency states why `std`/existing is insufficient (R13); JSON over SQLite and derived connectivity over OS reachability chosen for simplicity (R5, R6); identical behaviour/shortcuts on all three platforms; every new element keyboard-operable with an accessible name (FR-024, contracts/ui-surface.md); strings externalised (FR-025). User override N/A (no plugins). |
| Governance: engine/gateway/runtime sign-off | **Yes (receiver)** | ✅ PASS | No engine change. Receiver-crate changes require the receiver maintainer's sign-off per `CODEOWNERS`. Requirement IDs (FR-, SC-, INT-3.x, EC-2.x, NFR-3.1, DM-2/3) referenced throughout spec, plan, contracts. |

**Pre-Phase-0 result**: PASS (no violations).
**Post-Phase-1 re-check**: PASS — the design adds no unsafe code, no feature
flag, no single-implementor trait, no crate, and no real-time change; the
two protocol uncertainties are isolated inside the receiver crate behind
gated probes with host-invisible fallbacks, and are recorded below as
deviations-in-waiting rather than constitution violations.

## Project Structure

### Documentation (this feature)

```text
specs/004-search-and-library-browse/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R14, dependency justification, verification gates
├── data-model.md        # Phase 1: refs, catalog payloads, search state, library index/sync, play log, row model
├── quickstart.md        # Phase 1: automated gates (named tests), live probes, manual scenarios M1–M16
├── contracts/
│   ├── catalog-source.md            # new SourceCommand/SourceEvent variants, receiver mapping, scripted double
│   ├── library-and-search-core.md   # controller surface, queue additions, acting-list rule, constants, persistence
│   └── ui-surface.md                # shell/nav, Search view, Library view, detail views, row widget, artwork, keys
├── checklists/requirements.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

Existing layout (001–003) is kept; `+` marks new files, `~` modified, `−` removed.

```text
Cargo.toml                                   ~ workspace deps: image 0.25 (default-features = false, features = ["jpeg"])
deny.toml                                    ~ only if `image`'s tree surfaces a licence not yet allowed (expected: none)
locales/en-US/
├── library.ftl                              + search/library/detail/row/action/empty-state keys (contracts/ui-surface.md §9)
├── app.ftl                                  ~ nav-search added; placeholder-library removed
├── playback.ftl                             ~ "Play from account" keys removed (FR-022)
└── settings.ftl                             ~ developer.play_from_account descriptor title removed
crates/
├── modplayer-audio-source/                  ~ trait crate (still no deps)
│   └── src/{lib.rs ~, types.rs ~, catalog.rs +}     TrackRef extras, Availability widened, AlbumRef/ArtistRef/PlaylistRef, ids, Search*/Library*/TrackList payloads, CatalogError; ListAccountTracks/AccountTracks removed
├── modplayer-audio-source-synthetic/
│   └── src/{host.rs ~, scripted.rs ~}                SyntheticHost → Unsupported; ScriptedHost catalog scripting + command recorder
├── modplayer-audio-source-connect/          ~ receiver crate (maintainer sign-off)
│   ├── src/
│   │   ├── worker.rs                        ~ catalog command arms → spawned tasks, semaphore(4)
│   │   ├── account_read.rs                  − replaced by catalog/ (fetch_account_tracks folded into playlists.rs)
│   │   ├── catalog/mod.rs                   + dispatch + error classification (redacted)
│   │   ├── catalog/search.rs                + strategy list: mercury searchview → context-resolve
│   │   ├── catalog/collection.rs            + hand-encoded collection2v2 PageRequest/PageResponse, set split
│   │   ├── catalog/playlists.rs             + rootlist raw proto + Playlist::get → PlaylistRef
│   │   ├── catalog/track_lists.rs           + album discs / playlist tracks / artist top tracks
│   │   ├── catalog/hydrate.rs               + batched extended metadata → TrackRef/AlbumRef/ArtistRef, availability, artwork URL
│   │   └── lib.rs                           ~ module wiring
│   └── tests/{catalog_mapping.rs +, live.rs ~ (search_probe, library_sets_probe, availability_probe)}
├── modplayer-core/
│   ├── Cargo.toml                           ~ serde_json (workspace)
│   ├── src/search.rs                        + SearchSession, GroupState, request-id packing, constants
│   ├── src/library/mod.rs                   + LibraryIndex facade, LibraryStatus
│   ├── src/library/index.rs                 + sets, refs, track_lists, merge, hydration queue
│   ├── src/library/sync.rs                  + SyncScheduler (pure, injectable clock)
│   ├── src/library/play_log.rs              + PlayLog (record, recent-100, persistence DTO)
│   ├── src/library/persist.rs               + JSON DTOs, atomic write, background load, data dir
│   ├── src/library/connectivity.rs          + Connectivity derived from health/registration
│   ├── src/queue.rs                         ~ replace_context_at, play_next_tracks
│   ├── src/controller.rs                    ~ catalog event routing, search/library/recent accessors, play-log hook, persistence thread, sign-out clearing; account-tracks API removed
│   ├── src/settings_registry.rs             ~ developer.play_from_account descriptor removed
│   └── tests/{search_session.rs +, library_sync.rs +, library_index.rs +, play_log.rs +, persist.rs +, queue_proptest.rs ~, controller_streaming.rs ~}
├── modplayer-ui/
│   ├── Cargo.toml                           ~ ureq (workspace), image
│   ├── src/app.rs                           ~ Search/Library sections, detail navigation stack, artwork cache lifetime, hydrate-visible hints
│   ├── src/shell.rs                         ~ Section::Search, Ctrl/Cmd+1..5, Ctrl/Cmd+F; library_placeholder removed
│   ├── src/search_view.rs                   + search box, groups, show more, offline/no-results/refreshing states
│   ├── src/library_view.rs                  + five tabs, empty/first-sync-failed/refreshing states, virtualised lists
│   ├── src/detail_view.rs                   + album / playlist / artist detail
│   ├── src/rows.rs                          + ListRow widget, six-action menu, acting_list, RowOrigin
│   ├── src/artwork.rs                       + ArtworkCache (ureq fetch pool, image decode, LRU, negative cache)
│   ├── src/widgets/{skeleton.rs +, initials.rs +, mod.rs ~}
│   ├── src/settings/developer.rs            ~ "Play from account" removed
│   └── tests/{search_view.rs +, library_view.rs +, rows.rs +, developer_removed.rs +, accessibility.rs ~, fluent_keys.rs ~, credential_leak.rs ~}
└── modplayer/
    └── src/main.rs                          ~ unchanged wiring (receiver already injected); data dir passed to controller
```

**Structure Decision**: keep the single Cargo workspace under `crates/` with
one crate per architectural component (Constitution VII) and add **no**
crate: catalog payload types join the dependency-free seam crate
`crates/modplayer-audio-source` (the only crate both `modplayer-core` and
`modplayer-audio-source-connect` may name, as 003 established for
`TrackRef`); protocol fulfilment lives in a new `catalog/` module tree of
`crates/modplayer-audio-source-connect` (Constitution IV's sole protocol
speaker); host state (search session, library index, sync scheduler, play
log, persistence, connectivity) is a `library/` module tree plus
`search.rs` in `crates/modplayer-core` (core services, Part 9 §3 "Library
Sync"), since they are pure host primitives with no new external
dependency beyond `serde_json`; presentation is new modules of
`crates/modplayer-ui`. The dependency graph remains the 003 DAG
(`modplayer → {ui, core, audio-io, account, secure-store, audio-source-connect}`;
`ui → {core, audio-io, engine, account, secure-store}`;
`core → {engine, audio-io, audio-source, audio-source-synthetic}`;
`audio-source-connect → {audio-source, librespot-*, tokio, rtrb, protobuf}`);
`ui` additionally gains `ureq` + `image`, `core` gains `serde_json`, neither
introducing a new edge between workspace crates. Locale files stay under
the repository-root `locales/en-US/`.

## Design notes that tasks must respect

1. **Seam first, UI last**: trait-crate types → scripted double →
   core state (fully unit-tested against the double) → receiver mapping
   with fixtures → live probes → UI. No UI task may depend on a probe
   outcome; the UI renders `Unsupported` groups/sets as omitted.
2. **Nothing blocks the UI thread**: index load, persistence writes,
   artwork fetch/decode and every catalog request are off-thread;
   `tick()` only drains channels. A frame must never call `ureq`, `File`
   I/O or `image::load`.
3. **Generation discipline**: `request_id = (generation << 8) | kind`;
   any reply whose generation is not current is dropped before it touches
   state (search) or whose set/page is no longer in flight (sync).
4. **Play-log hook is reducer-adjacent, not reducer-internal**: the
   controller observes the `TrackStarted → Playing` transition and calls
   `PlayLog::record`; the pure transport reducer is unchanged (003
   contract intact).
5. **Queue contract unchanged**: only the two convenience operations in
   contracts/library-and-search-core.md §2 are added; `sync_program()`
   semantics, play-next FIFO block and unavailable-skip stay as 003
   specified.
6. **Rows are one widget**: `rows::ListRow` is the sole renderer for
   search groups, library tabs and detail views; any per-surface
   difference is a bug (FR-004).
7. **Hydration is lazy and bounded**: visible ids first, background sweep
   ≤ 2 batches/s, batch ≤ 100; a 50 000-track library must be scrollable
   before hydration finishes (skeleton rows for un-hydrated ids).
8. **Sign-out clears everything**: index, play log, search state, artwork
   cache, files on disk — before 002's `SignedOut` report renders the
   sign-in step (same ordering rule as 003 note 7).
9. **Scaffold removal is atomic with its replacement**: the PR that
   removes `ListAccountTracks`/"Play from account" is the one that lands
   `FetchLibrary{Playlists}` + `FetchTrackList{Playlist}` and the tests
   that replace the scaffold's coverage.
10. **Debug-only overrides** (`MODPLAYER_CATALOG_FORCE_429`,
    `MODPLAYER_ARTWORK_FORCE_FAIL`, `MODPLAYER_LIBRARY_FIXTURE`) are
    `cfg(debug_assertions)`-gated and read fresh per use, like 003's
    `MODPLAYER_CONNECT_FORCE_UNAVAILABLE`.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. The following headless assumptions,
spec-level risks and scope decisions are recorded for traceability:

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Grouped search relies on the mercury `searchview` endpoint, unverified for 2026, with a tracks-only context-resolve fallback that would leave Albums/Artists/Playlists groups omitted (research R2, gate V1) — a **potential** deviation from FR-001's four groups | The only grouped search reachable with the single Keymaster session credential; the public Web API `/v1/search` 429s (003 R8). The strategy list is confined to one receiver-crate function so the probe outcome changes one file. | Building the pathfinder `searchDesktop` GraphQL path up front — its persisted-query hash tracks the official client's releases and would add a brittle, frequently-broken surface before knowing it is needed. Recorded as the documented next strategy if V1 fails. |
| Library sets via `/collection/v2/paging` with hand-encoded protobuf messages (research R3, gate V2) | `librespot-protocol` does not compile `collection2v2.proto`; the two messages are four fields each. Saved-tracks fallback via documented `spotify:user:<u>:collection` context. | Adding `protobuf-codegen` as a build-dependency for two tiny messages — a build script and a new dev surface for ~80 LOC of encoding. |
| Catalog reads ride the `SourceHost` seam (5 commands / 5 events) instead of a dedicated catalog trait (research R1) | Constitution IV forbids a second protocol speaker; Constitution X forbids a second seam with the same implementors; the session is shared with playback. | A `CatalogHost` trait + second generic parameter — duplicates the worker, the channel and `App`'s generics. |
| "Online" derived from `SourceHealth`/`Registered`, not an OS reachability API (research R5) | Matches the spec's "while online" (service reachable) and reuses 003's classification; zero platform code. | Per-platform reachability adapters — three adapters for a fact the session already reports. |
| Library index persisted as JSON (`serde_json`) rather than SQLite (research R6) | Read-mostly mirror; loads off-thread in < 1 s at 50 000 tracks; existing dependency. | `rusqlite` — new C/FFI dependency and `unsafe` crate for no contract benefit in this slice; a later slice may migrate if delta sync needs indexing. |
| Artwork fetched by the UI crate from the public CDN with `ureq` + new `image` dependency (research R8, R13) | `std` cannot decode JPEG; the CDN URL is public and not a protocol surface, so Constitution IV is intact. | Routing pixel bytes through `SourceEvent` from the receiver (`spclient.get_image`) — large payloads on the control channel and receiver lifetime tied to UI cache. |
| `TrackRef`/`Availability` extended in the shared trait crate; `Availability::Unavailable` renamed `UnavailableRegion` (research R10) | FR-008/DM-2 require explicit, release date and a three-valued availability; two 003 call sites update. | Parallel "rich" ref types in core — duplicated identity types across the seam. |
| First-sync hydration is lazy (visible-first + throttled sweep) rather than complete-before-render (research R4) | A 50 000-track library is ~500 metadata batches; rendering must not wait (SC-002, FR-014/016). | Eager full hydration — minutes of skeleton rows on first launch. |
| Enter on album/artist/playlist rows opens the detail view; Play now for those rows is in the menu (contracts/ui-surface.md §4) | The spec defines Enter ⇒ Play now for *track* rows (Clarifications); non-track rows need a keyboard path to their detail (FR-013). | Enter ⇒ Play now on every row — leaves detail views unreachable by keyboard without a second control. |
| Debug-only environment overrides for rate-limit / artwork-failure / large-fixture rehearsal (quickstart M12/M14/M15) | SC-005/007/002 need reproducible manual verification without a real 429 or a 50 000-track account. | Skipping manual rehearsal — the automated proxies cover logic, not the rendered experience. |


## Post-Implementation Findings (T079/T080, 2026-09-17)

The live probes (T079, research.md "R14 outcomes") and the manual walk
(T080, quickstart.md "Manual walk 2026-09-17") surfaced the following,
all fixed in the same run unless marked otherwise:

| # | Finding | Fix |
|---|---|---|
| V1 | `hm://searchview` retired ⇒ Search is Tracks-only (documented deviation from FR-001's four groups; Complexity Tracking row now describes actual behaviour). Search latency ≈ 5–8 s: context-resolve plus one sequential `Track::get` per hit — **SC-001 (300 ms) missed**, open follow-up: batch hydration (`get_extended_metadata`) or the pathfinder search. | not fixed — recorded |
| V2 | `/collection/v2/paging` answers 400 ⇒ Saved Albums / Followed Artists unsupported; Saved Tracks via context fallback. The fallback was gated on `Unsupported` only, so the whole cycle failed. | `collection.rs::strategy_refused` |
| V4 | Relinked tracks carry a catalogue-less empty-allow-list restriction ⇒ a third of the library greyed as region-locked. | `hydrate.rs::availability_from` (catalogue-filtered, mirrors librespot) |
| D1 | Track rows overflowed `ROW_HEIGHT`. | `rows.rs` two-line truncating layout |
| D4/D5/D6/D11 | "⋯" tofu; bare playlist count; search group scroll carry-over; bare headers under rate limit. | `rows.rs`, `detail_view.rs` + `library.ftl`, `search_view.rs` |
| D7 | **003 regression**: `worker.rs` held three unshared `MapperState`s, so every `TrackChanged` was a foreign reveal and the queue collapsed to one item; relinked alternatives (different uri) collapsed it too. | shared `Arc<Mutex<MapperState>>`; `ProgramMap::resolve` relink rule |
| D8 | Wide-row (album/artist/playlist) menu actions were dropped, and any action on an unfetched list was dropped. | `LibraryViewState::pending_action` deferral |
| D9 | `main.rs` never called `with_library_paths` ⇒ no persistence, no snapshot; shutdown didn't flush/join the writer. | `main.rs`; `PlaybackController::shutdown` |
| D10 | A stale test entry (`spotify:track:a`) sat in the real play log (pre-persistence era). Tests verified not to touch the real dir. | entry removed by hand |
| — | `MODPLAYER_LIBRARY_FIXTURE=large` never seeded once a snapshot existed, and its bare ids stayed skeletons. | `controller.rs` seeded flag + no sync under fixture; fixture pre-hydrated |
| — | Account page shows "Signed in as" blank / tier Unknown (003 surface). | not in scope — recorded |
