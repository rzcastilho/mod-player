# Implementation Plan: Sticky Transport Bar and Panel Layout

**Branch**: `021-transport-bar-and-panel-layout` | **Date**: 2026-09-28 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/021-transport-bar-and-panel-layout/spec.md`

## Summary

The Now Playing screen is rebuilt as two vertical parts inside its content `Ui`:

- **Pinned transport bar.** An `egui::Panel::top(..).show_inside` drawn after
  018's docked column. It holds, as atomic wrap groups:
  - 40 px artwork and a fixed-width title/artist column;
  - skip back, play/pause, stop, skip forward;
  - elapsed and remaining time;
  - master volume over the peak meter;
  - the Queue, Effects and Transport toggles;
  - 018's conditional Panels toggle.
- **One `ScrollArea`** (`SectionMemory`, `ViewKey::NowPlaying`, not animated, no
  drag source). It holds the status line and transfer banner, the waveform
  region at 018's unchanged `waveform_heights(H)`, and then four collapsible
  panel cards in this order: Markers, Effect Chain, Transport focus, Queue.
  They are separated by `space::XL`.

The Effect Chain's nested scroll area and `effects_panel_reserved_height` are
deleted.

Opening a panel from its bar toggle or its existing host action writes the
persisted flag through the single controller setter. It also records a one-shot
`RevealRequest`. When the card is drawn, `layout::reveal_align` picks the scroll
amount: none if the card is already visible, the minimum otherwise, or header to
the top if the card is taller than the viewport. `ui.scroll_to_rect` applies it
in the same pass. A card's header disclosure and its bar toggle share one flag.
A header click calls `request_discard`, so both show the same state in the same
frame.

Markers gains a persisted `markers_open` flag, which defaults to `true`. Queue
rows move to a new `rows::queue_row`, a sibling of `list_row` built from its
layout primitives:
- artwork;
- title over artist;
- always-visible quiet actions;
- a right-aligned 1-based position;
- the current row marked with a ▶ glyph and a leading accent bar (not colour
  alone, not the selection fill), with a "Now playing, …" accessible name.

Technical approach and every rejected alternative: [research.md](./research.md)
R1–R13.

## Technical Context

**Language/Version**: Rust 1.95.0 (`rust-toolchain.toml`, edition from workspace)

**Primary Dependencies**: `egui`/`eframe` 0.36 (with `accesskit`),
`fluent-templates`. Uses the existing `modplayer-core` (controller, settings)
and `modplayer-ui` widgets (`widgets::controls`, `rows`, `layout`,
`section_memory`). **No new crates or dependencies.**

**Storage**: `settings.toml` `[now_playing_panels]`, with one additive key
`markers_open` (absent → `true`). `SCHEMA_VERSION` stays `1`. Scroll offset is
in memory only (`SectionMemory`), never on disk.

**Testing**: `cargo test`. Tests are headless `egui::Context::run_ui` and assert
on AccessKit nodes, bounds, painted shapes and `PanelState`. There are
`proptest` properties for `reveal_align`, `identity_width` and the
`NowPlayingPanels` serialisation round trip. Manual scenarios M1–M14 are
executed by the implementing agent under the constitution's "Manual Scenario
Sign-Off".

**Target Platform**: desktop macOS, Windows and Linux (eframe). Manual walks
run on macOS.

**Project Type**: desktop app (Cargo workspace, one crate per component)

**Performance Goals**: the bar and scroll region add O(1) widgets per frame. The
queue rows stay O(n) for the Queue panel, the same as today; queues are small
and not virtualised. Repaint cadence stays at 16 ms while playing. The reveal
costs one `scroll_to_rect` per opening, not per frame.

**Constraints**:
- Bar height is invariant to window height, scroll, panel state, playback state
  and title length (FR-002).
- There is no elided control label at 960 × 640 with +40 % text (FR-017, 018
  D4).
- Waveform heights follow 018's formula unchanged (FR-003, FR-011).
- The real-time path isn't touched.

**Scale/Scope**: one screen (Now Playing) and the Queue panel.

The feature touches these files:
- **UI**: `now_playing.rs`, `queue_view.rs`, `rows.rs`, `widgets/controls.rs`,
  `layout.rs`, `actions.rs`, `section_memory.rs`, `app.rs`, `effects_view.rs`,
  `transport_view.rs`, `markers.rs`, `waveform/input.rs`, and
  `locales/en-US/*.ftl`.
- **Core**: `controller.rs` and `settings/model.rs`.

All unknowns are resolved. Research resolved every design unknown
(R1–R13).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Pre-research result: **PASS**. Post-design re-check
(after data-model and contracts): **PASS**, with no new violations. The
deviations from earlier feature contracts (016 C10/C11/C13, 014's display-title
surface) are feature-contract supersessions, not constitution deviations. They
are listed in Complexity Tracking.

- [x] **I. Real-Time Path Is Sacred.** N/A. This is UI-only. No engine or
  effects crate changes. The bar reads the peak with the same
  `controller.shared().peak()` atomic load it uses today, and the volume goes
  through the same `set_master_volume` command path.
- [x] **II. Plugins Are Guests.** N/A. No gateway or runtime change. The
  plugin dock, overlay and floated windows (011, 018) keep their contracts, and
  plugin waveform overlays paint through the same `plugin_overlays::paint` call.
- [x] **III. Host Primitives, Plugin Behaviors.** Pass. Panel open state stays a
  host concept owned by `modplayer-core`: `NowPlayingPanel::Markers` and the
  `markers_open` flag are added there. The UI adds no parallel state (016 P1
  still holds, research R6).
- [x] **IV. Audio Source Replaceable and Isolated.** Pass. `QueueRow` gains
  `artwork_url` from the `TrackRef` that is already in the queue. It comes from
  the `modplayer-audio-source` trait crate, not the receiver. Every test uses
  `ScriptedHost` (synthetic).
- [x] **V. No Audio Ever Leaves the Engine.** N/A. No path touches decoded
  audio, sample buffers or the cache.
- [x] **VI. Security and Privacy by Default.** N/A. No credentials, network or
  telemetry. Artwork loads through the existing `ArtworkCache`, and no new
  fetch path is added.
- [x] **VII. Rust Quality Gates.** Pass.
  - No new crate or dependency, and no `unsafe`.
  - No `unwrap` or `expect` outside tests.
  - New public items carry doc comments, and the two pure layout helpers have
    runnable doc examples: `layout::reveal_align`, `layout::identity_width`,
    `widgets::controls::collapsible_panel_card`, `rows::queue_row`,
    `now_playing::request_reveal`.
  - fmt, clippy `-D warnings`, test and deny are the quickstart gates.
- [x] **VIII. Test What the NFRs Promise.** Pass.
  - Test-first obligations are listed per contract rule (T-B*, T-S*, T-C*,
    T-R*, T-Q*, T-N*).
  - The required proptest for state serialisation covers the four-flag
    `NowPlayingPanels` round trip (T-N5).
  - Proptests on `reveal_align` and `identity_width` cover the layout
    arithmetic.
  - NFR-6.1, 6.2, 6.4 and 7.4 each have automated tests (T-B5, T-B6, T-C5,
    T-Q3, T-Q5, T-Q10, T-Q11).
  - No real-time crate is touched, so no new benchmark or soak test is needed.
- [x] **IX. One Plugin API Definition.** N/A. The plugin API schema and its
  version are unchanged. No host function is exposed to plugins.
- [x] **X. Simplicity, Portability, and the User's Override.** Pass.
  - No trait and no feature flag.
  - `queue_row` is a sibling function, not a mode flag on `list_row`
    (research R8).
  - Behaviour is identical on all platforms.
  - Every new control has keyboard access and an accessible name: the
    disclosure buttons, the queue row actions and the reveal via existing
    shortcuts.
  - New strings are externalised in `locales/en-US`: `panel-collapse`,
    `panel-expand`, `now-playing-bar-identity`, `queue-row-name`,
    `queue-row-name-current`, `queue-playing-glyph`. `queue-current` is
    removed.
  - The user override (taking transport focus back, disabling a plugin) is
    unchanged.
- [x] **Governance: traceability.** Pass. Every contract rule cites FR-001–FR-019,
  FR-3.2.2 and NFR-6.1, 6.2, 6.4 and 7.4, and the spec's review IDs (UX-21,
  UX-22, UX-26).
- [x] **Governance: area-maintainer sign-off.** N/A. No change to the engine,
  the Capability Gateway or the Plugin Runtime.
- [x] **Governance: Manual Scenario Sign-Off.** Pass. quickstart.md defines
  M1–M14, which the implementing agent executes with the macOS Quartz recipe.
  Results are recorded in tasks.md.

## Project Structure

### Documentation (this feature)

```text
specs/021-transport-bar-and-panel-layout/
├── plan.md                                   # This file
├── research.md                               # Phase 0 — R1–R13
├── data-model.md                             # Phase 1 — §1–§8
├── quickstart.md                             # Phase 1 — automated gates + manual M1–M14
├── contracts/
│   ├── ui-now-playing-layout.md              # Bar (B), scroll region (S), cards (C), reveal (R)
│   ├── queue-row.md                          # Queue rows (Q)
│   └── settings-now-playing-panels.md        # [now_playing_panels] markers_open (N)
├── checklists/                               # From the specify/clarify phases
└── tasks.md                                  # Phase 2 — /speckit-tasks (not created here)
```

### Source Code (repository root)

```text
crates/
├── modplayer-core/
│   └── src/
│       ├── controller.rs          # NowPlayingPanel::Markers + RENDER_ORDER; now_playing_panel_open/set_… cover Markers;
│       │                          #   QueueRow.artwork_url / artwork_name filled in queue_view()
│       └── settings/model.rs      # NowPlayingPanels/RawNowPlayingPanels.markers_open (default true, hand-written Default);
│                                  #   unit + proptest round-trip (T-N1–T-N5)
└── modplayer-ui/
    ├── src/
    │   ├── now_playing.rs         # show(): H capture → dock → transport bar (Panel::top) → one ScrollArea
    │   │                          #   (status, banner, waveform, 4 cards); request_reveal/take_reveal; delete
    │   │                          #   show_heading, effects_panel_reserved_height, inner effect scroll; wrap_group_before
    │   ├── layout.rs              # reveal_align(), identity_width() + unit/proptests (T-L1)
    │   ├── widgets/controls.rs    # collapsible_panel_card() + CardResponse (panel_card kept for non-collapsible uses)
    │   ├── rows.rs                # queue_row() + QueueRowAction; draw_artwork_url() factored out of draw_artwork
    │   ├── queue_view.rs          # rows via rows::queue_row; header controls unchanged; body only (card owned by now_playing)
    │   ├── markers.rs             # panel body drawn inside collapsible card (card chrome moves to caller)
    │   ├── effects_view.rs        # body inside collapsible card; toggle_effect_chain_panel → returns bool
    │   ├── transport_view.rs      # body inside collapsible card; toggle_transport_panel → returns bool
    │   ├── actions.rs             # invoke(): `_ctx` → `ctx`; Toggle* → request_reveal when opened
    │   ├── section_memory.rs      # ViewKey::NowPlaying
    │   ├── app.rs                 # Section::NowPlaying records scroll offset like Search/Plugins
    │   └── waveform/input.rs      # consume Shift+wheel delta when it pans the detail (R11)
    └── tests/
        ├── now_playing.rs         # T-B1/3/4/6, T-S1/3, T-C2–C6, T-R1/2/4 (016 C10/C13 tests rewritten)
        ├── queue_view.rs          # T-Q1–T-Q12 (queue-current assertion replaced)
        ├── responsive_dock.rs     # T-B5
        ├── waveform.rs            # T-S2
        ├── actions.rs             # T-R5
        ├── markers.rs             # C11 interactive-count assertion updated for the disclosure
        └── fluent_keys.rs         # T-K
locales/
└── en-US/
    ├── playback.ftl               # + now-playing-bar-identity, queue-row-name, queue-row-name-current,
    │                              #   queue-playing-glyph; − queue-current
    └── controls.ftl               # + panel-collapse, panel-expand
```

**Structure Decision**: Use the existing Cargo workspace layout. All UI work is
in `crates/modplayer-ui/src` and `crates/modplayer-ui/tests`. Two additive
model changes are in `crates/modplayer-core/src/controller.rs` and
`crates/modplayer-core/src/settings/model.rs`. Strings go in `locales/en-US`,
which is the only shipped locale directory today. No new crate, module
directory or test target is created: every test extends an existing file under
`crates/modplayer-ui/tests/` or an existing `#[cfg(test)]` module.

## Phase 0 — Research (complete)

See [research.md](./research.md). Summary of decisions:

| # | Topic | Decision |
|---|---|---|
| R1 | Pinning | `Panel::top(..).show_inside` after the dock; one `ScrollArea` below |
| R2 | Scroll area | `SectionMemory` `ViewKey::NowPlaying`, `animated(false)`, `ScrollSource { drag: false, ..ALL }` |
| R3 | Waveform H | Captured first from the content `Ui`; 018 formula unchanged |
| R4 | Bar layout | Atomic wrap groups, pre-measured; fixed identity width `clamp(0.25·w, 160, 320)`; only title and artist truncate |
| R5 | Reveal math | `reveal_align`: none / egui minimum / `Align::Min` when taller than the viewport |
| R6 | Reveal plumbing | One-shot `RevealRequest` in egui temp memory, valid for 2 passes, set by the toggle and by `invoke` |
| R7 | Collapsible card | Header disclosure (`set_expanded`); collapsed = header only (supersedes 016 C10); `request_discard` for same-frame sync |
| R8 | Queue row | New `rows::queue_row` sharing `list_row` primitives; quiet actions; ▶ glyph and accent bar |
| R9 | Row data | `QueueRow.artwork_url` and `artwork_name` added in core |
| R10 | Markers flag | `markers_open`, serde default `true`, hand-written `Default` |
| R11 | Wheel conflict | Waveform consumes the Shift+wheel delta it uses for panning |
| R12 | Removed pieces | `show_heading` (96 px art, display title), standalone volume and meter, inner scroll |
| R13 | Tests | Headless `run_ui`, AccessKit bounds, `PanelState`, proptests |

## Phase 1 — Design & Contracts (complete)

- [data-model.md](./data-model.md): `NowPlayingPanel` (+Markers),
  `NowPlayingPanels` (+`markers_open`), `QueueRow` (+artwork), `RevealRequest`,
  `ViewKey::NowPlaying`, the pure layout functions with their properties,
  `CardResponse` and `QueueRowAction`.
- [contracts/ui-now-playing-layout.md](./contracts/ui-now-playing-layout.md):
  rules B1–B7, S1–S5, C1–C6 and R1–R6, with test obligations.
- [contracts/queue-row.md](./contracts/queue-row.md): Q1–Q11 with test
  obligations.
- [contracts/settings-now-playing-panels.md](./contracts/settings-now-playing-panels.md):
  N1–N5 with test obligations.
- [quickstart.md](./quickstart.md): the automated gates and manual scenarios
  M1–M14.

**Agent context update**: there is no agent-context update script under
`.specify/scripts/bash/` for this project, so it was skipped. The plan
introduces no new technology to record.

## Complexity Tracking

There are no constitution violations. The table below records **assumptions,
supersessions of earlier feature contracts, and rejected alternatives**, as the
headless plan protocol requires.

| Item | Why needed | Simpler alternative rejected because |
|---|---|---|
| Supersede 016 contract **C10** ("collapsed card draws nothing"): collapsed cards now render header-only | FR-004 requires a keyboard-operable collapse control in every card header, and Markers has no bar toggle (Clarification 5). Under C10, a collapsed Markers card could never be reopened | Adding a Markers bar toggle contradicts the clarified spec (only the Queue, Effects and Transport toggles are allowed) |
| Supersede 016 **C11** (Markers: card, no toggle) and **C13** (reserved height) | Markers gains a header disclosure and a persisted flag. C13's reservation existed only because volume and the meter sat below a capped inner scroll, and that layout is gone (FR-003) | Keeping C13 would keep a nested scroll area, which FR-003 forbids |
| 014's single `display`-role surface (the Now Playing title) is removed | The title moves into the fixed-height bar (FR-002, FR-018). A display-size title would add height and conflict with the 40 px identity group | Keeping a display title in the scroll region would duplicate the title, and the review asks for one identity location |
| `request_discard` on a header toggle | Spec edge case: the bar toggle and the card must agree **in the same frame**. The bar is drawn before the cards, so without a discard there is one frame of lag | Accepting a one-frame lag violates the edge case. Drawing the cards before the bar breaks the region order and the dock-first rule |
| **Assumption**: `RevealRequest` is honoured only within 2 passes. A shortcut pressed on another section doesn't reveal the panel on a later visit | Spec Clarification 10 says the existing behaviour (flip the flag) applies off-screen, and the reveal "applies when Now Playing is shown". Read as "shown at the time" | A deferred reveal on later navigation was rejected: the scroll jump would be unexpected and could come minutes after the key press |
| **Assumption**: H for the waveform is the full content height, including the bar | Clarification 2 says the formula is "unchanged" and "independent of scroll position". The full content height keeps 018's D10.9 numbers exactly | H = viewport-below-bar was rejected: it changes 018's numbers and ties waveform height to how the bar wraps |
| **Assumption**: the bar keeps the existing two-line peak-meter widget (label over bar) | It fits the identity group's two-line height without changing 015 `meter_bands` behaviour | A compact meter variant is deferred to a follow-up and is only needed if M8 shows it doesn't fit |
| **Assumption**: queue row actions move to a second line under the text instead of being elided at narrow widths or with +40 % text | FR-017 and NFR-7.4 allow no truncation of control labels. FR-015 requires the actions to be always visible | A hover-revealed actions menu was rejected: FR-015 requires them to be always visible |
| New `ViewKey::NowPlaying` in `SectionMemory` (020) | Now Playing now has a section-level scroll area, and 020 FR-007 requires retention and sign-out reset for every such area | A bare `id_salt` with an epoch was rejected: it resets on sign-out but loses the offset on section switch, so it behaves differently from every other section |
