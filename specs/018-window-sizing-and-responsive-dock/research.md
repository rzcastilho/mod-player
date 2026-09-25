# Research: Window Sizing and Responsive Plugin Dock

**Feature**: 018-window-sizing-and-responsive-dock | **Date**: 2026-09-24 | **Plan**: [plan.md](./plan.md)

All Technical Context unknowns are resolved below. The spec's Clarifications
section already fixed every product number (1200 × 820, 960 × 640, 280/240/480,
1024, 560, 16, 500 ms, 8 %/22 %); this document records the *engineering*
decisions needed to implement them in the existing codebase (eframe/egui 0.36,
`settings.toml` via `modplayer-core::settings`).

Findings from reading the current code (baseline):

- `crates/modplayer/src/main.rs:129` uses `eframe::NativeOptions::default()` —
  no initial or minimum size is set (root cause of UX-01).
- `crates/modplayer-ui/src/plugin_panels.rs:34` `pub const DOCK_WIDTH: f32 = 280.0`,
  drawn with `egui::Panel::right(..).exact_size(DOCK_WIDTH).resizable(false)`;
  the header is a single `ui.horizontal` (icon, section-label title, Float/Dock,
  Close, destructive gap, Disable) — at narrow widths egui clips/elides the
  trailing buttons.
- `crates/modplayer-ui/src/waveform/mod.rs:24,97` fixed `OVERVIEW_HEIGHT = 72.0`,
  `DETAIL_HEIGHT = 120.0` (UX-23).
- `crates/modplayer-ui/src/now_playing.rs:91` transport row is a plain
  `ui.horizontal` of 4 buttons + 3 switches (no wrapping).
- Now Playing is drawn directly inside `CentralPanel` (no outer `ScrollArea`),
  after `Panel::left("shell-nav-rail")` (`app.rs:268,319`).
- `settings.toml` sections are `Raw*` serde structs with `#[serde(default)]`,
  converted to the domain `AudioSettings` in `RawSettings::into_settings`
  (`settings/model.rs`); a *type* mismatch in any typed field fails the whole
  `toml` deserialize and yields the "unreadable file → defaults + one warning"
  path (`tests/settings.rs::garbage_file_loads_defaults_with_exactly_one_warning`).
- `plugin_panels.rs` still contains two `eprintln!("TRACE …")` debug calls
  (lines 236, 469) left over from 011; they are in a file this feature rewrites.

---

## R1 — Setting the initial and minimum window size

**Decision**: In `main.rs`, build `eframe::NativeOptions { viewport:
egui::ViewportBuilder::default().with_title("ModPlayer").with_inner_size(restored)
.with_min_inner_size([960.0, 640.0]), ..Default::default() }`, where `restored`
is the persisted `[window]` inner size clamped per FR-014, or 1200 × 820 when
absent. The size is read from `PlaybackController::window_settings()` (the
controller already loads `settings.toml` before `run_native`).

**Rationale**: `with_inner_size`/`with_min_inner_size` are egui's viewport
API in logical points and map to winit's `inner_size`/`min_inner_size`, which
every desktop OS enforces natively (FR-002: the user cannot drag smaller).
Reading through the controller keeps one settings loader (no second
`SettingsStore::load` in `main`).

**Alternatives considered**:
- eframe's built-in `persist_window` (requires the `persistence` feature and a
  RON file in eframe's own storage dir) — rejected: introduces a second
  persistence mechanism, and also persists position/maximized state, which the
  spec explicitly excludes (Constitution X, spec Assumptions).
- Sending `ViewportCommand::InnerSize` on the first frame — rejected: visible
  resize flash on launch and a frame at the wrong size.

## R2 — Observing and persisting the window size (debounce, maximized)

**Decision**: A pure `WindowSizeTracker` (in `crates/modplayer-ui/src/layout.rs`)
fed once per frame from `ctx.input(|i| i.viewport())`: `inner_rect` (logical
points), `maximized`, `fullscreen`. Rules:

1. If `maximized == Some(true)` or `fullscreen == Some(true)`, the observation
   is ignored (the last non-maximized size remains the candidate).
2. Otherwise, a size differing from the last *saved* size by > 0.5 pt becomes
   `pending` with `changed_at = now`.
3. When `pending` exists and `now − changed_at ≥ 500 ms` **and**
   `now − last_write ≥ 500 ms`, the tracker yields the size to save;
   `App` calls `controller.set_window_inner_size(w, h)`. While `pending`
   exists, `App` requests a repaint after the remaining delay so the save
   fires even with no further input.
4. `App::on_exit` flushes `pending` unconditionally (clean exit, FR-003).

The tracker takes `Instant` as a parameter so it is unit-testable without
sleeping.

**Rationale**: "Settles" = no change for 500 ms; this naturally caps writes at
≤ 1 per 500 ms (FR-003). The viewport info is already provided by eframe each
frame; no platform code needed (Constitution X portability).

**Alternatives considered**: saving every frame the size changes (hundreds of
writes during a drag — rejected, disk churn); saving only on exit (loses size
on crash — rejected, spec says "when a resize settles *and* on clean exit").

## R3 — Where window/dock numbers live

**Decision**: Persistence bounds live in core, layout rules live in UI:

- `modplayer-core::settings::window` (new module): `DEFAULT_INNER_SIZE =
  (1200, 820)`, `MIN_INNER_SIZE = (960, 640)`, `DOCK_WIDTH_DEFAULT = 280`,
  `DOCK_WIDTH_MIN = 240`, `DOCK_WIDTH_MAX = 480`, plus the `WindowSettings`
  domain type and its clamp/sanitize functions (FR-014).
- `modplayer-ui::layout` (new module): `AUTO_HIDE_THRESHOLD = 1024`,
  `HOST_CONTENT_FLOOR = 560`, `DOCK_KEY_STEP = 16`, waveform proportions
  (`0.08/64`, `0.22/120`), `SAVE_DEBOUNCE = 500 ms`, and pure functions
  `effective_dock_width`, `dock_presentation`, `waveform_heights`.
- `main.rs` imports `MIN_INNER_SIZE` from core.

**Rationale**: core must clamp on load (FR-014) without depending on UI;
UI-only render rules stay out of core. Pure functions make SC-003/SC-006
property-testable without rendering.

**Alternatives**: all constants in UI (core would then duplicate the clamp
bounds — rejected); a `LayoutConfig` struct/trait (single implementor —
Constitution X rejects).

## R4 — `[window]` wire format and corrupt-value tolerance

**Decision**: New `RawWindow` section, all fields `Option<toml::Value>`:

```toml
[window]
inner_width = 1200.0
inner_height = 820.0
dock_width = 280.0
```

`into_settings` converts each field independently: accept `Float` or
`Integer`, reject anything else, then reject non-finite or `≤ 0` → treat as
absent (default). Valid values are then clamped (width/height up to
960/640, dock into [240, 480]). No `InvalidField` warning is raised — silent
fallback, matching the `connect_device_id` "dropped silently" precedent
(the spec says "treated as absent", not "warned").

**Rationale**: typing the fields as `Option<f32>` (as `RawPanel` does) would
make `inner_width = "wide"` a whole-file deserialize failure → every setting
reset + "unreadable" warning, violating FR-014 ("without failing the settings
load" / "the rest of settings.toml still loads"). `toml::Value` is already a
dependency of core (`toml.workspace = true`), so no new crate.

`inf`/`nan` are valid TOML floats, so the finite check is required, not
defensive noise.

**Alternatives considered**: `#[serde(deserialize_with = lenient_f32)]`
helper — equivalent behaviour, more code; chosen form is simpler to read and
proptest. Raising an `InvalidField::Window` notification — rejected: spec
does not ask for a user-facing warning and a stale window size is harmless.

**Serialization**: `from_settings` always writes the `[window]` table with
the current (clamped, finite) values, same as every other section. A file
without `[window]` loads defaults; the next save adds it (additive; no
`SCHEMA_VERSION` bump — matching how `[now_playing_panels]` was added in 016).

## R5 — Controller surface and shadow state

**Decision**: `PlaybackController` gains a `window: WindowSettings` shadow
field initialised from `settings.window` at construction (exactly like
`now_playing_panels`, `controller.rs:714`), and:

- `pub fn window_settings(&self) -> WindowSettings` — per-frame read, no disk I/O.
- `pub fn set_dock_width(&mut self, width: f32)` — clamps into [240, 480],
  updates shadow, `persist_settings(|s| s.window.dock_width = w)` only when
  the clamped value differs from the shadow.
- `pub fn set_window_inner_size(&mut self, width: f32, height: f32)` — clamps
  up to 960 × 640, same write-only-on-change rule.

**Rationale**: reuses the one `persist_settings` read-modify-write path and
its `settings-save-failed` notification; per-frame reads never hit disk (016
precedent, contract P3). Off the real-time path entirely (UI thread only).

## R6 — Dock resize: custom splitter vs. egui's resizable panel

**Decision**: Keep `egui::Panel::right("plugin-panel-dock").exact_size(effective)
.resizable(false)`, and draw a **custom splitter** along its left edge:
a 6-pt-wide hit rect (cursor `ResizeHorizontal`) allocated with
`Sense::click_and_drag()` and made focusable. Drag: `live_width =
drag_start_width − pointer_delta_x`, clamped to [240, effective_max], held in
egui temp memory while dragging and used as the panel width that frame
(SC-003: tracks the pointer every frame). On `drag_stopped()` →
`controller.set_dock_width(live_width)`. Keyboard (while focused): `←` +16,
`→` −16, `Home` → 240, `End` → effective max; each press commits immediately
via `set_dock_width`. The splitter registers a focus claim for
`Left/Right/Home/End/Tab/Shift+Tab` through `actions::register_claim` so the
global action dispatcher does not also act on them (007 SC-010 precedent,
same as `plugin_panels::numeric_claims`). AccessKit: role `Splitter`,
label `tr("plugin-dock-resize")`, numeric value = current width, min/max =
240/effective max, set through `ctx.accesskit_node_builder`.

**Rationale**: egui's built-in resizable panel handle is pointer-only (not
focusable, no AccessKit node) → fails FR-004a / NFR-6.1; it also persists width
in egui memory, not `settings.toml`, and gives no drag-end signal. Using
`exact_size` with our own computed width keeps the render-time limit
(FR-004) from ever being written back.

**Alternatives**: `Panel::resizable(true)` + reading back the rect each frame
(no keyboard, no drag-end, silently rewrites the stored width when the window
shrinks — rejected).

## R7 — Effective dock width: measuring the host column

**Decision**: `effective_dock_width(stored, content_width) =
clamp(stored, 240, max(240, min(480, content_width − 560)))`, where
`content_width` is the Now Playing content `Ui`'s `max_rect().width()` at the
moment `show_dock` runs (i.e. central-panel inner width, which is already
`window − nav rail − central panel margins`).

**Rationale**: this is the spec's formula with `windowInnerWidth −
navRailWidth` measured directly instead of reconstructed; it is equal or
stricter (the few points of panel margin are also subtracted), so the ≥ 560
host-column guarantee holds exactly. The nav rail's width is not a constant
(`Panel::left` sizes to content), so measuring avoids a hard-coded rail
width.

**Alternative**: `ctx.content_rect().width() − rail_rect.width()` threaded
from `app.rs` — same number, more plumbing; rejected.

## R8 — Auto-hide, overlay and the "Panels" toggle

**Decision**:

- Window width for the threshold = `ctx.content_rect().width()` (whole
  window inner width in logical points — `content_rect` is already used by
  `plugin_panels::show_floated`).
- Pure `dock_presentation(window_width, docked_count, overlay_open) ->
  DockPresentation::{None, Docked, Hidden, Overlay}` (see data-model.md §3).
- `overlay_open` is session-only: an `egui` temp-memory bool under
  `Id::new("plugin-dock-overlay-open")` (eframe's `persistence` feature is
  not enabled, so temp memory is never written to disk). It is force-cleared
  whenever presentation resolves to `Docked` or `None` (widening, last panel
  closed/floated) — so narrowing again starts `Hidden` (spec clarification).
- Overlay drawn with `egui::Area::new("plugin-dock-overlay")
  .order(Order::Middle)` (same layer policy as floated windows via a new
  `shell::PLUGIN_DOCK_OVERLAY_ORDER`), fixed at the right edge of the Now
  Playing content rect, full content height, width = effective dock width
  (≥ 240), with a filled `ui.visuals().panel_fill` frame and the same
  splitter + `ScrollArea` body as the docked column. Being an `Area` it never
  reflows host content.
- Esc: when any widget inside the overlay has focus and `Escape` is pressed,
  close the overlay and move focus to the "Panels" toggle. The overlay body
  registers an `Escape` claim for its focused widgets so the dispatcher does
  not also run a global Esc action.
- "Panels" toggle: `switch(ui, SwitchKind::Toggle, &mut overlay_open,
  &tr("plugin-dock-panels-toggle"))` appended to the transport row — reusing
  the existing `widgets::controls::switch` (same control variant as the
  Queue/Effects/Transport toggles, AccessKit `Toggled` state already wired by
  015). Shown only when presentation ∈ {Hidden, Overlay}.
- Focus hand-off (edge case): if focus was on a widget inside the docked
  column/overlay in the previous frame and the column is not drawn this
  frame (Docked → Hidden or Overlay → Hidden), focus is requested on the
  "Panels" toggle's id. Docked ↔ Overlay transitions keep widget ids stable
  (panel widget ids are keyed by plugin/panel/widget, not by container), so
  focus stays on the same widget.

**Alternatives**: `egui::Window` for the overlay (movable/title bar — not
anchored; rejected); modal/`Order::Foreground` with click-outside dismiss
(spec forbids outside-click dismissal and wants transport usable); persisting
overlay state (spec: session-only).

## R9 — Panel header: truncating title, wrapping buttons

**Decision**: Replace the single `ui.horizontal` with a measured two-phase
layout:

1. Measure the three buttons' widths from their galleys
   (`WidgetText::into_galley(.., TextWrapMode::Extend, ..)` + button padding
   + `destructive_gap` width) → `buttons_w`.
2. `title_budget = available − icon − spacing − buttons_w`. If
   `title_budget ≥ TITLE_MIN_WIDTH` (a token: `theme::space`-derived, ~64 pt,
   enough for ~6 glyphs + "…"), draw one row: icon, title, buttons.
   Otherwise draw row 1 = icon + title (full width), row 2+ =
   `ui.horizontal_wrapped` of the buttons.
3. Buttons are built with `.wrap_mode(TextWrapMode::Extend)` so egui can never
   elide their text; `horizontal_wrapped` moves a whole button to the next row
   instead (FR-011).
4. Title: `egui::Label::new(section_label(text)).truncate()` with
   `Sense::focusable_noninteractive()`-style focusability so it is in the Tab
   order; when truncated, `on_hover_text(full)` and, while `has_focus()`, the
   same tooltip is shown via egui's tooltip API; accessible name stays the
   full un-truncated text via the existing `accesskit_node_builder` pinning
   (FR-010).
5. Suspended/placeholder and `marker-list-empty` lines use
   `egui::Label::new(..).wrap()` (FR-011a).

**Rationale**: egui's `horizontal_wrapped` alone would put the title and
buttons in one flow but cannot truncate the title *and* keep buttons whole;
measuring first gives a deterministic, testable decision.

**Alternatives**: shrinking button padding/font (still truncates under +40 %
— rejected); icon-only buttons (loses visible label, out of scope for the
control-variant system from 015).

## R10 — Transport row under +40 % text

**Decision**: Change the transport row from `ui.horizontal` to
`ui.horizontal_wrapped` so the row wraps rather than overflowing/overlapping
at 960 × 640 under pseudo-localization (FR-013). Button/switch labels use
`TextWrapMode::Extend` so they wrap as whole controls.

**Rationale**: at 960 wide with the nav rail, the row (4 buttons + 3
switches + "Panels") at +40 % text exceeds the content width; wrapping is the
only option that neither elides nor overlaps.

## R11 — Waveform height from content height

**Decision**: `now_playing::show` captures `H = ui.max_rect().height()` as its
first statement (CentralPanel inner height; unaffected by the dock, which is a
right side panel, and by scroll — Now Playing has no outer `ScrollArea`).
`layout::waveform_heights(H) -> (overview, detail)` returns
`(max(64, round(0.08 H)), max(120, round(0.22 H)))`. `waveform::overview` and
`waveform::detail` take a new `height: f32` parameter; the
`OVERVIEW_HEIGHT`/`DETAIL_HEIGHT` constants are deleted (FR-012). Existing
callers in tests pass explicit heights (72/120 where they asserted the old
geometry).

**Rationale**: pure function → proptest for SC-006; single measurement point.

## R12 — 40 % pseudo-localization in tests

**Decision**: Add a thread-local test hook to `modplayer-core::i18n`:
`#[doc(hidden)] pub fn with_pseudo_expansion<R>(percent: u32, f: impl FnOnce() -> R) -> R`.
While active, `tr`/`tr_args` append padding so each resolved string grows by
`max(1, ceil(len × percent / 100))` characters (padding glyphs: a repeated
wide-ish Latin-1 char such as `ÿ` bracketed, e.g. `[Close·ÿÿ]`-style, so
elision is visible in screenshots). Thread-local, so parallel tests are
unaffected; production never calls it (default = off, zero cost beyond one
`Cell` read).

The FR-013/SC-007 test (`crates/modplayer-ui/tests/responsive_dock.rs`) runs
`egui::Context::run` with `RawInput.screen_rect` set to 960 × 640 (overlay
open) and to ~1100 × 800 with the dock at 240, two fixture panels docked,
inside `with_pseudo_expansion(40, ..)`, then walks the AccessKit tree /
`ctx` widget rects and asserts: (a) every `Role::Button`/toggle label's
galley was not elided (`Galley::elided == false`, collected via a
`ctx.memory` debug hook or by comparing the laid-out text to the source),
(b) no two interactive rects overlap. A 60-char plugin title is covered by
a fixture panel title override.

**Rationale**: only en-US ships; the spec requires an automated proxy
(NFR-7.4). Thread-local keeps it a test concern without a Cargo feature
(Constitution X: no feature flag without a second consumer).

**Alternatives**: a `pseudo` cargo feature (rejected, X); a fake `.ftl` locale
(needs loader changes and a second langid; rejected as heavier).

## R13 — Test harness

**Decision**: Reuse the headless harness pattern already in
`crates/modplayer-ui/tests/plugin_panels.rs` (fresh `Context` with
`theme::apply_tokens`, real `org.modplayer.fixture.ui-panel` fixture,
`RawInput` driven frames, AccessKit tree inspection). No `egui_kittest`
(not a workspace dependency; adding it needs Constitution X justification
and brings nothing the existing harness lacks). Core serialization uses the
existing `proptest` dev-dependency.

## R14 — Incidental cleanup

**Decision**: Remove the leftover `eprintln!("TRACE …")` calls in
`plugin_panels.rs` while rewriting that file (they print on every frame for
every widget). Recorded here so tasks include it; not a spec requirement.

## R15 — Manual walk findings (2026-09-25, T038)

The M1–M13 walk on macOS turned up four behavior deviations from the
decisions above. Each is fixed and has a regression test that fails without
its fix:

1. **R2 — maximized detection on macOS.** egui-winit 0.36 reads
   `is_maximized()` only at viewport creation on macOS (`update_viewport_info`,
   egui#3494 deadlock workaround), so a window zoomed *after* launch reports
   `maximized == Some(false)` and its full-screen size was persisted (M12).
   **Decision**: also treat the window as maximized when its outer rect fills
   the monitor (`layout::fills_monitor`: one axis spans edge to edge, the
   menu bar allowed for, and the other covers ≥ 85 %). `WindowSizeTracker::
   observe` now also drops any pending size while maximized, so the zoom
   animation's in-between frames are never saved. Tests:
   `layout::tests::fills_monitor_detects_zoom_but_not_restored_sizes`,
   `tracker_discards_zoom_animation_frames_once_maximized`.
2. **R9 — single-row header at mid dock widths.** When one row was chosen,
   `Label::truncate()` took the whole rest of the row rather than
   `title_budget`, so at widths between the two extremes (seen at 320–400)
   the full title pushed Float/Close/Disable past the dock's edge (M5).
   **Decision**: cap the title's `Ui` at `title_budget`. Test:
   `responsive_dock::header_buttons_stay_inside_dock_at_every_width`.
3. **R6 — splitter arrow keys vs. egui focus navigation.** On the full
   screen, egui's own arrow-key focus traversal moved focus off the splitter
   after the first `←`, so only one step ever landed (M6). The dock-only
   harness hid this: it has no neighbouring widgets. **Decision**: the
   splitter sets `EventFilter { horizontal_arrows: true }` while focused, and
   `hold_focus_through_escape` keeps that bit for the splitter (the filter is
   replaced, not merged). Test:
   `responsive_dock::keyboard_sequence_keeps_focus_on_full_screen`.
4. **R10 — transport row vs. the docked column.** `horizontal_wrapped`
   never wraps a `switch` by itself (its nested `ui.horizontal` is placed
   before its size is known). R8's pre-measure ran only in `Overlay`, so at
   1024–~1100 pt wide with the dock at its widest, "Transport" ran ~7 pt
   across the dock's edge (M8). **Decision**: pre-measure in every
   presentation, against the docked column's live edge
   (`plugin_panels::live_dock_width`) while `Docked` (`ui.max_rect()` is not
   narrowed by the nested panel) and the row's own right edge otherwise.
   Test: `responsive_dock::transport_row_never_crosses_docked_edge`.
