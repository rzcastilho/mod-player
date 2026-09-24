# Implementation Plan: List Row, Tab Strip, and Panel Card Components

**Branch**: `016-list-row-and-panel-components` | **Date**: 2026-09-23 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/016-list-row-and-panel-components/spec.md`

## Summary

Turn the app's three repeating building blocks into real components. The
**list row** becomes a fixed three-column grid — artwork, a truncating
title over a secondary line, and a trailing column holding the duration in
`mono` figures ahead of the "…" menu — and gains the interaction it never
had: a single click selects, a double click or Enter opens, and a tooltip
says which. The **tab strip** becomes an underlined navigation row with a
live per-tab count. The **panel** becomes a card on `surface.raised` with a
`lg` inset, an `md` corner and a small uppercase header, drawn by one
shared helper, with its open/closed state finally surviving a restart.

The spec arrived already clarified — sixteen decisions settled, no open
markers — so Phase 0's job was verifying the toolkit and the existing code
can deliver them. Four findings shaped the design, and each was read out of
the vendored egui 0.36.2 / accesskit 0.24.1 source or this worktree, not
assumed:

1. **The tab strip must become a host widget, not a restyle** (research
   R6). `selectable_label`'s fill comes from `Visuals::selection.bg_fill`,
   a single app-wide field; removing it would silently delete the selection
   fill from the nav rail and the Settings category list — the two surfaces
   the spec's Scope boundary explicitly excludes. A host `tab` widget is
   the only way to restyle Library's five tabs without reaching them. The
   corollary is a *preservation* requirement hiding inside FR-034:
   `response.rs:976` shows egui already maps `WidgetInfo::selected` to
   accesskit's `Toggled`, so today's tabs report it for free — a
   hand-rolled widget reports nothing unless it says so.

2. **FR-029 is already true for the "…" button, and only for it**
   (research R3). `hit_test.rs:76-80` — *"In tie, pick last = topmost"* —
   and `hit_test_on_close` picks exactly one click target. The row
   registers first (`rows.rs:514`), the button later (`rows.rs:457`), so
   the button wins and `row_response.clicked()` is false. No suppression
   code is needed; a contract test pins the toolkit rule instead, because a
   later in-row control would break it silently.

3. **FR-012's reconciliation must be O(1)** (research R2). "Clear when that
   index no longer holds that id" reads like a scan, and a scan every frame
   destroys the virtualization property `rows::virtualized_list` exists to
   provide (a 50 000-row list costing O(visible), `rows.rs:597-607`). The
   API is therefore `reconcile(list, key_at: impl FnOnce(usize) -> ...)`,
   called at most once, at exactly the stored index. Selection carries a
   **third** component the spec's pair does not name — the list salt —
   because Search's four simultaneously-visible groups share one selection
   across four different index spaces.

4. **The panel wrapper would have rendered two headers** (research R7).
   Markers and Effect Chain already draw a `section_label` *inside their
   own first `ui.horizontal`*, beside other controls
   (`markers.rs:544-548`, `effects_view.rs:99-103`). Wrapping without
   deleting those lines duplicates the header. Also verified: the three
   keyboard toggles can move from `ctx` to `controller` for free, because
   `actions::invoke` already holds a `&mut PlaybackController` — had it
   not, FR-019 would have forced a refactor of 007's action plumbing.

Nothing touches the real-time path, the plugin API, the audio source or any
credential surface. One `settings.toml` section is added, optional, with
`SCHEMA_VERSION` unchanged at `1` — `[onboarding]`'s precedent exactly.
Three Fluent keys are added; zero semantic roles.

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`;
edition 2024) — unchanged from 001–015.

**Primary Dependencies**: existing only — egui/eframe **0.36.2**
(+`accesskit` **0.24.1**), `epaint`/`ecolor` 0.36.2 (transitive). APIs this
feature relies on, each read in the vendored source: `egui::Frame`
(fill/inner_margin/corner_radius), `Response::on_hover_text`,
`Context::accesskit_node_builder`, `hit_test`'s tie-break
(`hit_test.rs:76-80`), `Response`'s `WidgetInfo::selected → Toggled`
mapping (`response.rs:976`), `accesskit::Node::set_selected`
(`lib.rs:2133`) and `set_toggled` (`lib.rs:2138`), `Fonts::glyph_width`,
`Color32::blend`, `egui::__run_test_ctx`. **No new crate, no new
dependency, no new feature flag, no dev-dependency** (Constitution X);
no `Cargo.toml` changes. Explicitly rejected: `egui_kittest` / any
screenshot-diff harness (015 research R16, inherited).

**Storage**: one new **optional** `settings.toml` table,
`[now_playing_panels]`, with three `#[serde(default)]` booleans. Absent
section ⇒ all three `false` ⇒ every panel closed, byte-identical to
today's `unwrap_or(false)` (`now_playing.rs:94-104`). `SCHEMA_VERSION`
stays `1` (`settings/model.rs:69`) — an absent optional table is not a
schema change, exactly as `[onboarding]`'s addition established. No
migration. Row selection is explicitly **not** persisted (FR-028).

**Testing**: `cargo test --workspace`. New/extended suites in
`crates/modplayer-ui/tests/{rows, library_view, search_view, now_playing,
queue_view, transport_view, markers, fluent_keys,
design_token_contrast}.rs`, plus `#[cfg(test)]` modules in
`src/rows.rs`, `src/theme/tokens.rs`, `src/theme/controls.rs`,
`src/widgets/controls.rs` and
`crates/modplayer-core/src/settings/model.rs`. Everything is asserted from
**values**, headlessly, through `egui::__run_test_ctx` / `Context::run_ui`
— the route 014 and 015 both used, no rendering harness. The regression net
is the *unmodified* existing suites: `accessibility.rs` (FR-025/SC-009 —
`Role::Tab` names are the binding case), `design_token_literals.rs` (must
still report **0** — SC-008), `design_token_roles.rs`,
`interaction_states.rs`, `control_variants.rs`, `control_inventory.rs`, and
the capability gateway's `api_reference.rs` regeneration diff. Manual
scenarios M1–M11 in [quickstart.md](quickstart.md), executed by the
implementing agent (Governance › Manual Scenario Sign-Off). CI gates
unchanged (fmt, clippy `-D warnings`, test, deny, licence headers, API
reference diff) on ubuntu / macos / windows.

**Target Platform**: Desktop macOS, Windows 10+, Linux. Every value is
identical on all three — the column measure, the underline width, the card
tokens and the persisted flags are computed from the same token tables and
the same 4 px scale everywhere (Constitution X).

**Project Type**: Desktop application — the 13-crate Cargo workspace is
unchanged. Two crates are modified (`crates/modplayer-ui`,
`crates/modplayer-core`).

**Performance Goals**: zero work on the audio thread — no engine or effects
file is touched. Per-frame UI cost added: one `glyph_width` lookup per row
list for the duration measure (egui caches glyph metrics); one O(1)
`reconcile` per view per frame; one `Frame` per open panel; one extra
label node per tab. The reconciliation's O(1)-ness is a **stated
constraint, not an accident** — an O(rows) implementation would void
`virtualized_list`'s 50 000-row guarantee, so contract S5 tests the
invocation count. The 30 Hz repaint budget (`REPAINT_INTERVAL` 33 ms) is
unaffected.

**Constraints**: Constitution I — no real-time path edit; PR note "N/A".
II/III — no plugin-facing change. V — no sample data anywhere. VI — no
credential, network, asset or telemetry surface; the one new persisted
value is three booleans about panel geometry. VII — `forbid(unsafe_code)`
holds; no `unwrap`/`expect` outside tests; SPDX headers unchanged (no new
file); public items documented. VIII — the NFRs this feature promises
(NFR-6.1 keyboard, NFR-6.2 roles/states, NFR-6.4 colour never sole carrier,
NFR-6.5 contrast) are pinned by tests that already exist and must pass
unmodified, plus the new value suites. IX — untriggered:
`crates/modplayer-capability-gateway/api/v1.toml` and `docs/plugin-api/v1.md`
byte-identical. X — no crate, trait or flag; identical on three platforms.
**Out of scope** (FR-026, Scope boundary): which lists exist, Search's group
arrangement, Now Playing's block order, the nav rail, the Settings category
list, and the window's initial/minimum size (research R16).

**Scale/Scope**: `rows.rs` ≈ 180 LOC changed + ≈ 150 LOC unit tests;
`widgets/controls.rs` ≈ 90 LOC added (`tab`, `panel_card`);
`theme/tokens.rs` + `theme/controls.rs` ≈ 30 LOC added; `library_view.rs`
≈ 60 LOC changed; `search_view.rs`/`detail_view.rs`/`app.rs` ≈ 80 LOC
changed (the two new view states); `now_playing.rs` ≈ 70 LOC changed;
four panel files ≈ 40 LOC changed; `settings/model.rs` + `controller.rs`
≈ 90 LOC added; new/extended integration tests ≈ 600 LOC. New semantic
roles: **0**. New crates/dependencies/feature flags: **0**. Locale keys:
**3**. Persisted fields: **3** (one optional table). Plugin API: **0**.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | No | ✅ PASS | No file under `crates/modplayer-engine/` or `crates/modplayer-effects/` is modified. Nothing here allocates, locks or blocks on the audio thread: the whole feature is UI-thread layout, paint and one settings write. `peak_meter`/`chain_meters` keep reading what their caller already reads; the Effect Chain panel's re-derived reserved height (FR-023) is a layout figure, not a DSP one. PR real-time note: "N/A — no real-time path changes." |
| II. Plugins Are Guests (non-negotiable) | No | N/A | No gateway, runtime, permission, budget or refusal path is touched. `plugin_panels.rs`'s dock and floated windows (`now_playing.rs:85, 214`) are drawn unchanged and are **not** among FR-017's four host panels — a plugin panel does not get the host card, and no plugin gains or loses a capability. |
| III. Host Primitives, Plugin Behaviors | **Yes** (preserved) | ✅ PASS | Markers, loop regions, effect nodes and transport focus stay host primitives owned by core crates; this feature changes only the chrome around their panels and the columns of the rows that list them. No DSP, no scripting-tier involvement, no threshold logic moved. |
| IV. Audio Source Is Replaceable and Isolated | No | N/A | No source crate, trait or implementation is touched. Every automated test here is a value test over `RowEntity`/token/settings data, driven by the synthetic path the existing UI suites already use — no live Connect session is required by any of them. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | No | N/A | No sample buffer, decoded frame, cache file or export path appears anywhere in this feature. The only new persisted data is three booleans describing whether a panel is open. |
| VI. Security and Privacy by Default | No | ✅ PASS | No credential, network call, bundled asset or telemetry surface. `settings.toml` gains three booleans about panel geometry — no secret, nothing scrubbed. No font or image added, so `cargo deny`'s licence surface is unchanged. The quickstart's live-token line is quoted from the constitution for completeness and is used by no scenario here. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | No new crate and no new file, so `#![forbid(unsafe_code)]` and the SPDX header check are unaffected. No `unwrap`/`expect` outside tests: `reconcile` takes `FnOnce -> Option<String>` and handles `None` as the clear case; `duration_measure` is total over any `ctx`; `format_duration` is total over `u32`; the panel accessor is a total `match` over a three-variant enum. Public items (`RowSelection`, `RowEvent::Select`, `entity_key`, `tab`, `panel_card`, `duration_measure`, `NowPlayingPanel`, the controller pair) carry doc comments. Two of them carry a doc example rustdoc really collects, which is only true of public items: `format_duration` is **made `pub`** (it is private at `rows.rs:303` today, so a doctest on it would never run) and its rollover example uses data-model.md §3's table, and `RowSelection` gets a `select`/`is_selected`/`reconcile` example that needs no `egui::Context`. Both mirror the runnable doctest already on `rows::acting_list` (`rows.rs:171-194`); `rtk cargo test -p modplayer-ui --doc` must report new doctests, not zero. `cargo deny` unaffected (zero dependency change). |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | **NFR-6.2** by `accessibility.rs`'s existing role/name assertions passing **unmodified** (T9 is binding: `find_one(&nodes, Role::Tab, &tr(key))` at `:900`), plus FR-011's and FR-034's new state assertions. **NFR-6.1** by the unchanged `has_focus() && Enter` branch (S12) and the untouched `actions.rs`/`controls.rs` suites. **NFR-6.4** by the tab's selected/toggled state beside the underline (T11) and by the selected row's secondary line staying distinguishable by the type scale after colour is equalised (A3, already pinned by `body_and_secondary_are_visibly_different`). **NFR-6.5** by A4's contrast sweep over the widest set of runs a row can show. **Property-based tests (the MUST for new serialized state)**: the new `[now_playing_panels]` table is state serialization, so it gets a `proptest!` round-trip over an arbitrary `(effect_chain_open, transport_open, queue_open)` triple in `crates/modplayer-core/tests/settings.rs` (contract P9, tasks T038a), beside the existing `plugin_panels_round_trip_proptest` — the example-based P4/P6 cases do not discharge this principle on their own. No marker/loop arithmetic or manifest parsing is touched, so no other proptest is owed. **Test-first (the MUST, in full)**: every suite lands red against data-model.md's values before the code — 014's Phase 2 inverted this once and had to record it, and 015's plan warned about it; `tasks.md` must sequence it correctly from the start. |
| IX. One Plugin API Definition | No | ✅ PASS | **Untriggered by construction**: no request, event, DTO, permission or schema line changes; plugin panels are not among the four host panels this feature cards. `crates/modplayer-capability-gateway/api/v1.toml` and `docs/plugin-api/v1.md` must be byte-identical, and the existing `api_reference.rs` regeneration-diff test in CI is the check. No written change request is required or included. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | **No new trait, crate, feature flag or dependency**; two functions added to a widgets module that already holds five. **YAGNI**: a `bool` parameter rather than a builder struct (R1), one enum'd controller pair rather than three copy-pasted ones (R10), no arrow-key list navigation (FR-030 forbids it), no new collapse control (FR-035), no scanner widening (R15, inheriting 015's D8), no screenshot harness. **Portability**: identical values on all three platforms; the one platform-specific thing in this feature is the manual-walk recipe, which the constitution already scopes to macOS. **Externalized strings**: three new keys, both new row hints and the Queue header, in `locales/en-US` with the `fluent_keys.rs` inventory extended; no string is uppercased in a catalogue (`section_label` uppercases at draw time). **User override**: every existing one-action control keeps its behaviour (FR-022); the user can still take transport focus back and disable any plugin in one action. |
| Governance: area-maintainer sign-off | No | N/A | No change under `crates/modplayer-engine/`, `crates/modplayer-capability-gateway/` or `crates/modplayer-plugin-runtime/` (GOV-3.2), so no additional sign-off beyond normal review. Requirement ids (FR-, SC-, US-, NFR-, C-, GOV-) are referenced throughout spec, plan, contracts, data model and test names. |
| Governance: Manual Scenario Sign-Off | **Yes** | ⚠️ PLANNED, AT RISK | [quickstart.md](quickstart.md) § 3 defines M1–M11, to be executed by the implementing agent against the real build on macOS with `screencapture` plus point-sampling evidence; each result is recorded on its `tasks.md` task and any deviation written back into quickstart.md/research.md. **Known host risk, and this feature is more exposed than 015**: 015 recorded M1–M10 *all* not executed — the process wedged inside `AccountService::launch_resolve_session` → `KeyringSecureStore::get` → `SecKeychainFindGenericPassword` before `eframe::run_native` was reached, so no window ever formed (015 Complexity Tracking D11); 014 hit the sign-in gate one step later. Here, M1–M8 and M11 all need Library or Search behind that gate and M9/M10 need a loaded track, so there is no equivalent of 015's M1 (Welcome) reachable regardless. If the block recurs, the required outcome is to record each scenario **not executed** with the reason and mark the affected checkpoints **not reached** — never to sign off on automated evidence alone, never to fabricate a signed-in state, and never to modify the host's real Keychain to route around it (quickstart § 4). |

**Pre-Phase-0 result**: PASS (no violations).

**Post-Phase-1 re-check**: PASS. The design adds no crate, trait, flag,
dependency or `unsafe`; the real-time path, the plugin API, the audio
source and every credential surface are untouched. Phase 1 surfaced four
things the re-check would otherwise have flagged, all recorded in
Complexity Tracking: the third selection component the spec's pair does not
name (D1), the widget-signature change the spec implies but does not state
(D2), the double-header the naive panel wrapping would have produced (D3),
and the fact that the window sizes two success criteria sample at are not
set anywhere in the codebase (D6). None is a principle violation; each is a
place where the spec's letter and the code's reality needed reconciling in
writing rather than in a surprise during implementation.

## Project Structure

### Documentation (this feature)

```text
specs/016-list-row-and-panel-components/
├── plan.md                      # This file
├── spec.md                      # Feature specification (input, Clarified 2026-09-23)
├── research.md                  # Phase 0: decisions R1–R16, verified against egui 0.36.2,
│                                #   accesskit 0.24.1 and this worktree
├── data-model.md                # Phase 1: the value set — row grid, duration measure,
│                                #   selection state machine, tab/card values, settings shape,
│                                #   full call-site and file map
├── quickstart.md                # Phase 1: automated gates (named suites) + manual scenarios M1–M11
├── contracts/
│   ├── list-row.md              # L1–L7, S1–S12, A1–A8 with the test that pins each
│   ├── tab-strip.md             # T1–T13, incl. the two assertions that must pass verbatim
│   └── panel-card.md            # C1–C13 (the card) and P1–P9 (the persisted flags)
├── checklists/
│   └── requirements.md          # (created by /speckit-checklist)
└── tasks.md                     # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

The existing layout (001–015) is kept. No file is created and none is
deleted; `~` marks modified, `=` marks unchanged-but-load-bearing as a
gate.

```text
crates/
├── modplayer-core/
│   ├── src/
│   │   ├── settings/model.rs   ~ NowPlayingPanels + RawNowPlayingPanels beside
│   │   │                         RawOnboarding (:494-500); AudioSettings/RawSettings
│   │   │                         fields; SCHEMA_VERSION unchanged at 1 (:69)
│   │   └── controller.rs       ~ NowPlayingPanel enum; now_playing_panel_open /
│   │                             set_now_playing_panel_open shaped like
│   │                             focus_policy/set_focus_policy (:2448-2458);
│   │                             persist_settings (:4684) stays private
│   └── tests/
│       └── settings.rs         ~ P9 — the [now_playing_panels] proptest round-trip
│                                 beside plugin_panels_round_trip_proptest (:682-710)
│                                 (Constitution VIII; proptest already a dev-dep)
└── modplayer-ui/
    ├── Cargo.toml                (unchanged — no new dependency)
    ├── src/
    │   ├── theme/
    │   │   ├── tokens.rs       ~ DURATION_FIGURES, duration_measure() beside
    │   │   │                     body_measure (:160-165)
    │   │   ├── controls.rs     ~ TAB_UNDERLINE_WIDTH, tab_underline() beside
    │   │   │                     FOCUS_RING_WIDTH (:103)
    │   │   ├── contrast.rs       (unchanged — reused by SC-011)
    │   │   ├── style.rs          (unchanged — Visuals::selection.bg_fill stays put, R6)
    │   │   └── markers.rs        (unchanged)
    │   ├── widgets/
    │   │   ├── controls.rs     ~ tab(), panel_card() beside button()/switch()/
    │   │   │                     row_frame()/paint_focus_ring()
    │   │   ├── skeleton.rs       (unchanged — FR-032 holds structurally, R14)
    │   │   └── {initials,knob,peak_meter,volume,chain_meters}.rs  (unchanged)
    │   ├── rows.rs             ~ RowSelection; entity_key made pub; RowEvent::Select;
    │   │                         `selected` parameter; three-column grid;
    │   │                         format_duration rollover + made pub (its doc
    │   │                         example must run); set_selected; tooltip;
    │   │                         selection-aware text colours
    │   ├── library_view.rs     ~ tab strip → widgets::controls::tab + counts;
    │   │                         LibraryViewState.selection; clear-on-tab-change
    │   ├── search_view.rs      ~ new SearchViewState (selection + last_query);
    │   │                         one selection across the four groups
    │   ├── detail_view.rs      ~ new DetailViewState (selection + last_target)
    │   ├── app.rs              ~ owns the two new view states beside library_view
    │   │                         and library_detail (:95-100, :173-174)
    │   ├── now_playing.rs      ~ controller-backed panel flags (the three
    │   │                         panel_open_id helpers and six memory calls deleted);
    │   │                         EFFECTS_PANEL_RESERVED_HEIGHT re-derived from tokens
    │   ├── markers.rs          ~ panel_card; in-row header deleted (:544-548)
    │   ├── effects_view.rs     ~ panel_card; in-row header deleted (:99-103);
    │   │                         toggle takes controller; panel_open_id deleted
    │   ├── transport_view.rs   ~ panel_card; ui.heading() → helper's section header
    │   │                         (:48); toggle takes controller; panel_open_id deleted
    │   ├── queue_view.rs       ~ panel_card + new "Queue" header
    │   ├── actions.rs          ~ three dispatcher arms (:518, :531, :532) pass
    │   │                         controller; invoke's own signature unchanged
    │   ├── shell.rs            = untouched (Scope boundary — nav rail is not a tab strip)
    │   └── settings/mod.rs     = untouched (Scope boundary — categories are not tabs)
    └── tests/
        ├── rows.rs             ~ L1–L7, S1–S12, A1–A3, A5, A7
        ├── library_view.rs     ~ T1, T4–T8, T10, S8 — existing assertions unmodified
        ├── search_view.rs      ~ S2 across the four groups, S8
        ├── now_playing.rs      ~ C5, C6, C9, C10, C13, P3 + FR-037 seed migration
        ├── effects_view.rs     ~ FR-037 seed migration (the only other edited helper)
        ├── {queue_view,transport_view,markers}.rs ~ C7, C8, C11, C12
        ├── design_token_contrast.rs ~ A4 (SC-011)
        ├── fluent_keys.rs      ~ three new keys
        ├── accessibility.rs    = unmodified but for T11's added state (FR-025, SC-009)
        ├── design_token_literals.rs = must still report 0 (SC-008)
        └── {design_token_roles,interaction_states,control_variants,
             control_inventory}.rs = unchanged gates

locales/en-US/
├── library.ftl                 ~ row-open-hint-track, row-open-hint-entity
└── playback.ftl                ~ queue-panel-title

crates/modplayer-capability-gateway/tests/api_reference.rs
                                = unchanged — must regenerate with no diff (Principle IX)
```

**Structure Decision**: keep the single 13-crate Cargo workspace under
`crates/`, one crate per architectural component (Constitution VII), and
add **no crate and no file**. The feature spans exactly two crates, and the
seam between them is the one Principle VII already draws: **persisted state
and its accessors live in `crates/modplayer-core`**
(`src/settings/model.rs` for the wire shape, `src/controller.rs` for the
public accessor pair), because `persist_settings` is private there and
FR-019 makes `settings.toml` the single source of truth; **everything that
paints lives in `crates/modplayer-ui`**, the crate that already owns every
pixel. Within the UI crate the feature splits along the seam 014
established and 015 reinforced: **values in
`crates/modplayer-ui/src/theme/`, painting in
`crates/modplayer-ui/src/widgets/`**. `theme/tokens.rs` takes
`duration_measure` because it is the same kind of thing as its neighbour
`body_measure` — a measure derived from a role's `'0'` advance;
`theme/controls.rs` takes `TAB_UNDERLINE_WIDTH` because FR-034 names
`FOCUS_RING_WIDTH`, already in that file, as the shape to follow.
`widgets/controls.rs` takes `tab` and `panel_card` because both need `Ui`
access that does not belong in a value module, and because it already holds
`button`/`switch`/`row_frame`/`paint_focus_ring` — the same kind of host
widget. The row itself stays in `crates/modplayer-ui/src/rows.rs`, which is
already the one widget every catalog list renders through, so the
three-column grid and the selection model reach Library, Search and all
three detail views without any of those files gaining row-drawing code. The
two new view-state structs live with the views that own them
(`search_view.rs`, `detail_view.rs`) and are held by
`crates/modplayer-ui/src/app.rs` beside the existing `library_view` and
`library_detail` fields, which is where frame-persistent view state already
lives. Verification sits beside the other UI integration suites in
`crates/modplayer-ui/tests/` and the core unit tests in their own modules.
`crates/modplayer` (the binary), `crates/modplayer-engine`,
`crates/modplayer-effects`, `crates/modplayer-audio-source*`,
`crates/modplayer-capability-gateway`, `crates/modplayer-plugin-runtime`,
`crates/modplayer-secure-store`, `plugins/` and `docs/` are untouched; the
workspace dependency graph is unchanged.

## Design notes that tasks must respect

1. **Tests before the code they pin** (Constitution VIII). The new suites
   and `#[cfg(test)]` modules land first, asserting data-model.md's values,
   so the red→green transition is observed. 014's Phase 2 inverted this
   once and had to record a deviation; 015's plan carried the warning
   forward. Sequence it correctly from the start.
2. **Values in `theme/`, painting in `widgets/`** (015 design note 2,
   inherited). If a task finds itself typing a number into
   `widgets/controls.rs`, `rows.rs` or a view file, the number belongs in
   `theme/` instead. This is what keeps `design_token_literals.rs` at 0
   with its exclusion list unchanged — no new scan pattern is added.
3. **Delete the in-row headers when wrapping Markers and Effect Chain**
   (R7, C6). Both already draw a `section_label` inside their own first
   `ui.horizontal`. Wrapping without deleting renders the header twice. The
   remaining controls in those rows stay.
4. **`reconcile` must stay O(1)** (R2, S5). `FnOnce`, called at most once,
   at exactly the stored index, only when the list salt matches. Never
   `ids.iter().position(...)`, never a per-frame `Vec<String>` of keys —
   either would void `virtualized_list`'s whole reason for existing.
5. **The selection's list salt is the `virtualized_list` id salt** (R2,
   L2/S2). Reuse the string the call site already passes; do not invent a
   parallel enum. A typo surfaces as a selection that never reconciles, so
   the salt set is asserted.
6. **Never append the count to a tab's accessible name** (T9, T10). The
   count is a sibling label node in the `mono` role.
   `accessibility.rs:900` and `library_view.rs:380` must pass verbatim —
   FR-025 names this as the binding case.
7. **The `tab` widget must set its accessibility state explicitly** (R6,
   T11). `selectable_label` got `Toggled` for free via `response.rs:976`;
   a hand-rolled widget gets nothing. Set `Role::Tab`, the exact
   un-suffixed Fluent label, `set_selected` **and** `set_toggled`.
8. **Do not touch `Visuals::selection.bg_fill`** (R6, T13). It is app-wide;
   changing it silently restyles the nav rail and the Settings category
   list, both out of scope.
9. **A skeleton row needs a test, not an implementation** (R14, S10).
   FR-032 already holds structurally: `skeleton_row` is a different
   function with no `RowEntity`. Budget a regression test only.
10. **FR-029 needs a test, not suppression code** (R3, S7). egui's hit test
    already gives the click to the "…" button. Pin the toolkit rule; do not
    write defensive code that is dead today and will rot.
11. **Re-derive `EFFECTS_PANEL_RESERVED_HEIGHT`, do not re-guess it**
    (R8, C13). A sum of `theme::space` tokens and measured heights, plus
    `2 × space::LG` per card now in the reserved region. The 2026-09-19
    manual-walk defect (controls falling off the bottom) is what this
    guards, and C13 tests it at 960×640.
12. **One controller pair with an enum, not three pairs** (R10). Three
    panels with byte-identical logic is what an enum is for; it keeps the
    persisted-field mapping in one `match`.
13. **`invoke`'s signature does not change** (R9, P7). It already holds
    `controller: &mut PlaybackController<B, H>`; only the three arms at
    `actions.rs:518, 531, 532` change.
14. **`settings.toml` is the single source of truth, so delete the mirrors**
    (FR-019, P1). The three `panel_open_id` helpers and all six
    `ui.memory` reads/writes go. A mirror is exactly the divergence FR-019
    exists to prevent.
15. **Only two existing test helpers may be edited** (FR-037, P8): the
    panel-state seeds in `tests/now_playing.rs` and `tests/effects_view.rs`.
    Every other existing assertion passes unmodified, and an edit to one is
    a failure to investigate, not a fix to apply.
16. **The tooltip's text is manual evidence** (R13, A7). Assert the key
    selection and the key's resolution automatically; the rendered string
    is M4. Do not build a pointer-synthesis harness for one string.
17. **Resize the window explicitly in M6/M9/M11** (R16). The app sets no
    initial or minimum size — `NativeOptions::default()`. Capturing at
    whatever size the window manager gives does not answer SC-006 or
    SC-012.
18. **Manual scenarios M1–M11 are executed by the implementing agent**
    (Governance). If the launch block recurs, record "not executed" with
    the reason and mark the checkpoint not reached — do not sign off on
    automated evidence, do not fabricate a session, do not touch the host
    Keychain (quickstart § 4).

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. Recorded below: the places where this plan had
to decide something the spec leaves open, the two spec-implied changes the
spec does not state outright, and the accepted residuals — so `tasks` does
not re-litigate them and a later reader does not file them as bugs.

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| **D1 — Selection carries a third component the spec's pair does not name: the list salt.** `RowSelection { list, key, index }`, where `list` is the `virtualized_list` id salt the call site already passes | Clarification 2 makes selection exclusive across Search's **four simultaneously-visible groups**, and Clarification 3 identifies a selection by `(entity id, display index)`. Those two are incompatible without a discriminator: index `3` means a different row in each of the four groups, so FR-012's "clear when that index no longer holds that id" cannot know which list to re-check. Reusing the existing salt keeps the discriminator a string the call site already types (research R2). | *A bare `(id, index)` pair as written* — under-specified for Search; reconciliation would check the wrong list and clear or keep a selection arbitrarily. *A `ListId` enum over the ten lists* — must be hand-synced with the salts anyway, and forces Search's group kinds and Library's tabs into one enum that belongs to neither module. |
| **D2 — `rows::list_row`'s signature changes and `RowEvent` gains a `Select` variant.** The spec describes the behaviour but never states the API change | FR-006 needs "am I selected?" inbound (for FR-008's fill and FR-031's colours) and "I was clicked" outbound (for FR-007's per-view state). `Option<RowEvent>` is the established outbound channel; a `bool` is the minimum inbound surface, and deliberately excludes the index so the widget stays ignorant of virtualization. Seven call sites, all enumerated in data-model §8 (research R1). | *A `ListRow` builder struct* — idiomatic egui and probably where this ends up at the fourth knob, but a larger diff across seven sites for one `bool`, and it hides the `Option<RowEvent>` the call sites already match on (Constitution X, YAGNI). *A separate `row_clicked()` query* — two functions that must be called in lockstep with the same entity: a correctness trap the type system would not catch. |
| **D3 — Markers' and Effect Chain's existing in-row headers are deleted, not kept.** `markers.rs:544-548` and `effects_view.rs:99-103` | Both already draw a `section_label` header *inside their own first `ui.horizontal`*, beside other controls. `panel_card(header, …)` draws its own, so a naive wrap renders the header twice in two of the four panels. FR-018 requires one consistent header per panel; FR-036 requires the treatment to exist once (research R7). | *Making the helper's header an `Option`* so those two keep their in-row versions — defeats FR-036's entire point, which is that the four cards cannot drift apart. *Leaving the duplicate and hiding one* — two nodes in the accessibility tree for one heading, breaking C6. |
| **D4 — One controller accessor pair with a `NowPlayingPanel` enum, not three pairs.** The spec (Clarification 15) says "accessor/setter pairs" and leaves the count open | Three panels with byte-identical read/persist logic is the case an enum exists for. It keeps the persisted-field mapping in one `match` instead of three copy-pasted `persist_settings` closures, and gives contract P2/P6 one surface to enumerate exhaustively (research R10). | *Three pairs named after the panels* — closer to the spec's plural wording and marginally more discoverable at the call site, but triples the persistence code and makes "all three behave identically" a review claim rather than a compiler-checked one. |
| **D5 — `ARTWORK_SIZE` and `ACTIONS_RESERVED_WIDTH` stay as bare `f32` constants** in a file this feature modifies | FR-024 binds *colour, alpha, spacing and radius* literals, and FR-001 explicitly keeps `ARTWORK_SIZE` "unchanged from today". Both are reserved-geometry values, neither is a pattern `design_token_literals.rs` matches, and SC-008 ("zero **new** hits") is satisfied by placement (research R15). Recorded because a reader of FR-024 may reasonably expect them to move. | *Moving both into `theme/`* — scope creep into a file whose geometry the spec froze, and it would invite widening the literal scan to spacing patterns, which 015 rejected for a stated reason (false positives on legitimate geometry maths push implementers toward `#[allow]`-shaped escapes; 015 D8). |
| **D6 — Accepted gap: the window sizes SC-006 and SC-012 sample at are not set anywhere in this repository** | `crates/modplayer/src/main.rs:129` is `eframe::NativeOptions::default()` — no `ViewportBuilder`, no `inner_size`, no `min_inner_size`; a tree-wide search returns that one line. "1200×820 initial / 960×640 minimum" are figures from the source specification document that the code never enforces. The manual scenarios therefore resize explicitly before capturing (research R16, quickstart § 3.1). | *Setting the sizes in `main.rs` as part of this feature* — a real fix, but window chrome belongs to the Now Playing workbench feature under FR-026 and the Scope boundary. *Capturing at whatever size the window manager gives* — the evidence would not answer the criterion asked, which is the failure mode this note exists to prevent. |
| **D7 — Accepted residual: a double-click's first click leaves a visible selected frame** | egui reports `clicked()` on the first press-release of a double-click and `double_clicked()` on the second, so a row may render selected for one or two frames before it opens. spec.md's Edge Cases permit this explicitly ("nothing requires suppressing the intermediate selected frame"). Recorded so the flash is not filed as a bug. | *Deferring `Select` by egui's double-click interval* — introduces input latency on the far more common single click to hide a sub-100 ms artifact on the rarer gesture. *Suppressing `Select` on any frame that also activates* — does not help: the two clicks land on different frames. |
| **D8 — Verification is values (automated) plus pixels (manual), with no screenshot harness** | SC-001, SC-002, SC-005 and SC-006 are stated as visual judgements, and the rendered tooltip (SC-004) cannot be driven headlessly at all (research R13). Everything derivable from values is asserted through `egui::__run_test_ctx`; the rendered result is evidenced by M1–M11 under Governance › Manual Scenario Sign-Off. | *`egui_kittest` or an image-diff gate* — a new dev-dependency, a new CI surface and a new class of flake, for a feature whose payload is already expressible as values. 014 and 015 rejected the same dependency for the same reason (015 D9); reversing that here would be a workspace-wide decision made inside one feature. |
| **D9 — Manual Scenario Sign-Off is planned but at elevated risk, and the risk is stated before implementation rather than discovered during it** | 015 recorded **M1–M10 all not executed**: the process wedged inside `AccountService::launch_resolve_session` → `KeyringSecureStore::get` → `SecKeychainFindGenericPassword` before `eframe::run_native` was reached, so no window formed (015 D11); 014 was blocked one step later by the sign-in gate. This feature is *more* exposed: M1–M8 and M11 need Library or Search behind that gate, M9/M10 need a loaded track, and unlike 015 there is no Welcome-screen scenario reachable regardless. The block is entirely inside `modplayer_account`/`modplayer_secure_store`, which this feature does not touch. | *Planning as if the walk will succeed and handling it later* — 015's experience says the likely outcome is an unsigned feature discovered at the last task. Naming it now lets `tasks` budget the diagnosis (`sample <pid>`) alongside the walk. *Signing off on automated evidence, fabricating a session, or clearing the host Keychain* — the first substitutes value tests for the pixel judgement Governance requires, the second is explicitly forbidden, and the third is an irreversible action on the operator's real credential store, outside this feature's scope and outside what an implementing agent should do unattended. |
