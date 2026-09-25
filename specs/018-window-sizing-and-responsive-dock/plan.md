# Implementation Plan: Window Sizing and Responsive Plugin Dock

**Branch**: `018-window-sizing-and-responsive-dock` | **Date**: 2026-09-24 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/018-window-sizing-and-responsive-dock/spec.md`

**Requirement IDs**: FR-001–FR-015 (incl. FR-004a, FR-011a); NFR-7.4, NFR-6.1, NFR-6.2, NFR-7.1; review UX-01, UX-23 (heights only).

## Summary

ModPlayer currently launches with `eframe::NativeOptions::default()` (no size),
draws plugin panels in a fixed 280-pt, non-resizable `Panel::right` whose
single-row header elides its own buttons, and sizes the waveform with fixed
72/120-pt constants. This feature:

1. Opens the main window at **1200 × 820** with a **960 × 640** minimum via
   `ViewportBuilder::with_inner_size/with_min_inner_size`, restoring a
   persisted inner size from a new additive `[window]` section of
   `settings.toml` (debounced 500 ms save + exit flush; maximized/fullscreen
   ignored).
2. Makes the dock **continuously resizable** (240–480, default 280, further
   limited at render time to keep ≥ 560 pt of host content) through a custom
   focusable **splitter** (drag + ←/→/Home/End, AccessKit `Splitter`), with the
   width persisted in `[window] dock_width`.
3. **Auto-hides** the docked column below a 1024-pt window width, surfacing a
   **"Panels"** toggle in the transport row that opens the dock as a
   right-anchored **overlay** (session-only; Esc/toggle/widen/last-panel-gone
   dismiss; outside clicks don't).
4. Reworks the **panel header**: title truncates with tooltip (hover + focus),
   buttons never elide and wrap to extra rows; host status text wraps.
5. Scales waveform heights to `max(64, round(0.08 H))` / `max(120, round(0.22 H))`.
6. Verifies NFR-7.4 with a thread-local **+40 % pseudo-localization** test hook.

Technical approach and rejected alternatives: [research.md](./research.md) (R1–R14).

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned in `rust-toolchain.toml`; workspace `rust-version = "1.95"`)

**Primary Dependencies**: eframe/egui 0.36 (with `accesskit`), `toml` 0.9 and `serde` (settings), `fluent-templates` (i18n) — all existing; **no new crates**

**Storage**: existing `settings.toml` via `modplayer-core::settings::SettingsStore`; new optional `[window]` table (research R4)

**Testing**: `cargo test` — core integration tests (`crates/modplayer-core/tests/`), `proptest` (already a dev-dependency of core and ui) for state serialization and layout math, headless egui `Context::run` + AccessKit harness in `crates/modplayer-ui/tests/` (pattern from `plugin_panels.rs`); manual scenarios M1–M13 per constitution sign-off

**Target Platform**: macOS, Windows, Linux desktop (egui logical points; OS enforces min size)

**Project Type**: desktop app (Cargo workspace, crate per component)

**Performance Goals**: no added per-frame disk I/O (shadow state); splitter width tracks the pointer every frame (SC-003); UI remains at existing ≥ 60 Hz repaint while playing

**Constraints**: ≤ 1 window-size write per 500 ms; zero changes to the audio/real-time path; no hard-coded colours (tokens only); all new strings in `locales/en-US`

**Scale/Scope**: 2 crates changed materially (`modplayer-core` settings/controller, `modplayer-ui` dock/now-playing/waveform/app) + `modplayer` binary launch options; ~3 new locale keys; 1 new UI module (`layout.rs`), 1 new core module (`settings/window.rs`)

No open unknowns remain: all product values are fixed in spec Clarifications; all engineering unknowns resolved in research.md.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Principles touched: VII, VIII, X (directly); I, III, IV (structurally, no change). Initial check and post-design re-check both **PASS**.

- [x] **I. Real-Time Path Is Sacred** — N/A: all changes are UI-thread layout and settings I/O in `modplayer-ui`/`modplayer-core` settings; no engine, effect or audio-source code is touched, no "real-time safety" PR note needed.
- [x] **II. Plugins Are Guests** — Pass: dock/overlay only re-lays out host chrome around existing 011 panel views; no plugin code runs while drawing (011 P4 unchanged), no Capability Gateway change.
- [x] **III. Host Primitives, Plugin Behaviors** — Pass: dock width, presentation and header layout are host concepts owned by `modplayer-ui`/`modplayer-core`; plugins gain no layout API.
- [x] **IV. Audio Source Replaceable and Isolated** — N/A: no source crate dependency added; all tests use `ScriptedHost`/`FakeBackend` synthetic sources as today.
- [x] **V. No Audio Ever Leaves the Engine** — N/A: feature handles no audio data, cache or buffers.
- [x] **VI. Security and Privacy by Default** — N/A: persists only three numeric geometry values; no credentials, telemetry, network or plugin package handling.
- [x] **VII. Rust Quality Gates** — Pass: no new crate; no `unsafe`; no `unwrap/expect` outside tests (`toml::Value` conversions return `Option`); new public items (`WindowSettings`, setters, `layout` fns) get doc comments with runnable examples; fmt/clippy `-D warnings`/test/deny gates run in quickstart; leftover `eprintln!` TRACE calls in `plugin_panels.rs` removed (R14).
- [x] **VIII. Test What the NFRs Promise** — Pass: `[window]` state serialization gets proptests (W4.4/W4.5, required for state serialization); layout math proptests (D10.2/D10.3); automated NFR-7.4 pseudo-localization test (D10.8); manual scenarios M1–M13 executed by the implementing agent.
- [x] **IX. One Plugin API Definition** — N/A: plugin API/schema untouched (no new widget kinds, manifest fields or calls); panel bodies are drawn exactly as 011 defines.
- [x] **X. Simplicity, Portability, User's Override** — Pass: no new trait, crate or Cargo feature (pseudo-loc is a thread-local test hook, R12); only egui cross-platform APIs, no platform-specific code; splitter and "Panels" toggle are keyboard-operable with accessible names (NFR-6.1/6.2); all new strings externalized (NFR-7.1); persistence reuses `settings.toml` (no second mechanism, R1); Disable/Close remain one-action and are never hidden by truncation (strengthens the user-override guarantee).
- [x] **Governance traceability** — Pass: every design element cites FR/NFR IDs (contracts D1–D10, W1–W4); ui-panels.md L1/L5 supersession recorded in `contracts/ui-responsive-dock.md`.

## Project Structure

### Documentation (this feature)

```text
specs/018-window-sizing-and-responsive-dock/
├── spec.md
├── plan.md                        # this file
├── research.md                    # Phase 0 (R1–R14)
├── data-model.md                  # Phase 1: WindowSettings, tracker, presentation, widths, heights, keys
├── quickstart.md                  # Phase 1: automated commands + manual scenarios M1–M13
├── contracts/
│   ├── window-settings.md         # [window] wire format, controller API, launch options (W1–W4)
│   └── ui-responsive-dock.md      # dock presentation/width/splitter/toggle/overlay/header/waveform (D1–D10)
└── tasks.md                       # Phase 2 (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
crates/
├── modplayer/src/main.rs                      # NativeOptions.viewport: restored inner size + 960×640 min (W3)
├── modplayer-core/
│   ├── src/settings/
│   │   ├── mod.rs                             # re-export window types/consts
│   │   ├── window.rs                          # NEW: WindowSettings, consts, sanitize/clamp (FR-014)
│   │   └── model.rs                           # AudioSettings.window, RawWindow (Option<toml::Value>), from/into
│   ├── src/controller.rs                      # window shadow field, window_settings(), set_dock_width(), set_window_inner_size()
│   ├── src/i18n.rs                            # with_pseudo_expansion thread-local test hook (R12)
│   └── tests/
│       ├── settings.rs                        # W4.1–W4.3
│       ├── settings_window_proptest.rs        # NEW: W4.4–W4.5
│       └── controller_window.rs               # NEW: W4.6
└── modplayer-ui/
    ├── src/
    │   ├── lib.rs                             # pub mod layout
    │   ├── layout.rs                          # NEW: thresholds, effective_dock_width, dock_presentation, waveform_heights, WindowSizeTracker (+unit/proptests)
    │   ├── plugin_panels.rs                   # dock: effective width, splitter, overlay, header rework, wrap text; drop DOCK_WIDTH const & TRACE eprintlns
    │   ├── now_playing.rs                     # H capture, waveform heights, horizontal_wrapped transport row, "Panels" toggle
    │   ├── waveform/mod.rs                    # overview/detail take height; remove OVERVIEW_HEIGHT/DETAIL_HEIGHT
    │   ├── shell.rs                           # PLUGIN_DOCK_OVERLAY_ORDER
    │   └── app.rs                             # WindowSizeTracker per frame, repaint deadline, on_exit flush
    └── tests/
        ├── responsive_dock.rs                 # NEW: D10.5–D10.8
        ├── waveform.rs                        # D10.9 (update fixed-height assertions)
        ├── plugin_panels.rs                   # update header/dock expectations
        └── fluent_keys.rs                     # D10.10
locales/en-US/plugins.ftl                      # plugin-dock-panels-toggle, plugin-dock-resize, plugin-dock-resize-value
```

**Structure Decision**: Existing Cargo workspace, crate-per-component (Constitution VII). Persistence and clamp bounds go in `crates/modplayer-core` (`src/settings/`, `src/controller.rs`) because core owns `settings.toml` and must clamp on load without depending on UI; per-frame layout rules go in a new pure module `crates/modplayer-ui/src/layout.rs` consumed by `plugin_panels.rs`, `now_playing.rs`, `waveform/mod.rs` and `app.rs`; the binary `crates/modplayer/src/main.rs` only wires launch options. All listed directories exist today; new files are `settings/window.rs`, `layout.rs` and three test files.

## Implementation Phasing (input for /speckit-tasks)

1. **Foundation (blocks all stories)**: `settings/window.rs` + `RawWindow` + `AudioSettings.window` + tests W4.1–W4.5; controller shadow + setters + W4.6; `ui::layout` pure fns + D10.1–D10.4; locale keys; i18n pseudo hook.
2. **US1 (P1)**: `main.rs` viewport sizes; `app.rs` tracker + exit flush; header rework (D7) — US1 acceptance 3/4 depend on header + overlay, so the header lands here and overlay acceptance US1-3 is completed once US3 lands.
3. **US2 (P1)**: effective width + splitter (D2, D3) + persistence; D10.7.
4. **US3 (P2)**: presentation, "Panels" toggle, overlay, Esc/focus (D1, D4–D6); D10.6; D10.8 pseudo-loc run (needs overlay).
5. **US4 (P3)**: waveform heights (D8); D10.9.
6. **Polish**: remove TRACE eprintlns, update 011 test expectations, full gates, manual M1–M13.

## Complexity Tracking

No constitution violations; no entries required.

Assumptions recorded (spec was fully clarified; these are engineering-level choices):

| Assumption | Chosen | Rejected alternative & why |
|---|---|---|
| Host-column width measurement | Measure Now Playing content `Ui` width (window − nav rail − margins) | Reconstruct `window − navRailWidth` from a constant: nav rail width isn't fixed; measured value is equal-or-stricter so ≥ 560 still holds |
| Corrupt `[window]` values | `Option<toml::Value>` fields, silent per-key fallback | Typed `Option<f32>`: a string value would fail the whole-file parse, violating FR-014 |
| Dock resize mechanism | Custom focusable splitter + `exact_size` panel | egui `resizable(true)`: not keyboard-operable, no AccessKit node, no drag-end, rewrites width on shrink |
| Pseudo-localization hook | Thread-local in `i18n` | Cargo feature (Constitution X, no second consumer) or a fake locale bundle (heavier loader change) |
| Invalid `[window]` notification | None (silent default) | `InvalidField::Window` warning: spec says "treated as absent"; stale geometry is harmless |
