---

description: "Task list for 018-window-sizing-and-responsive-dock"
---

# Tasks: Window Sizing and Responsive Plugin Dock

**Input**: Design documents from `/specs/018-window-sizing-and-responsive-dock/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md) (R1–R14), [data-model.md](./data-model.md), [contracts/window-settings.md](./contracts/window-settings.md) (W1–W4), [contracts/ui-responsive-dock.md](./contracts/ui-responsive-dock.md) (D1–D10)

**Tests**: Included — Constitution VIII ("Test What the NFRs Promise") and the plan's Constitution Check require proptests for `[window]` serialization (W4.4/W4.5) and layout math (D10.2/D10.3), plus the NFR-7.4 pseudo-localization test (D10.8). Manual scenarios M1–M13 (quickstart.md) are executed by the implementing agent, not written as automated tests.

**Organization**: Tasks are grouped by user story per plan.md §"Implementation Phasing". US1's header rework (D7) lands in Foundational-adjacent US1 work because 3 of its 4 acceptance scenarios need the header fix; US1 acceptance scenario 3 (overlay-based check) can only be *verified* once US3 lands — this is called out at the US1 checkpoint.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1/US2/US3/US4 — maps to spec.md priorities P1/P1/P2/P3
- Every task names exact file path(s)

## Path Conventions

Cargo workspace, crate-per-component (plan.md "Project Structure"):
- `crates/modplayer-core/src/...`, `crates/modplayer-core/tests/...`
- `crates/modplayer-ui/src/...`, `crates/modplayer-ui/tests/...`
- `crates/modplayer/src/main.rs`
- `locales/en-US/plugins.ftl`

---

## Phase 1: Setup

**Purpose**: No new dependencies, crates or scaffolding are needed (plan.md: "no new crates"). This phase only confirms the toolchain gate that every later phase and quickstart.md rely on.

- [X] T001 Confirm toolchain pin and baseline gates are green before touching any file: `RUSTUP_TOOLCHAIN=1.95.0 rtk cargo fmt --check && RUSTUP_TOOLCHAIN=1.95.0 rtk cargo clippy --workspace --all-targets --all-features -- -D warnings && RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test --workspace` (no file changes; establishes the pre-feature baseline referenced by quickstart.md)

**Checkpoint**: Baseline confirmed green; proceed to Foundational.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: `WindowSettings` persistence (core), the controller shadow/setters, the pure `modplayer-ui::layout` module, and locale keys. Every user story reads or writes through these. Per plan.md phasing: "Foundation (blocks all stories)."

**⚠️ CRITICAL**: No user story task may start until this phase's checkpoint is reached.

- [X] T002 [P] Create `WindowSettings` domain type, `Default` impl, and constants `DEFAULT_INNER_SIZE`, `MIN_INNER_SIZE`, `DOCK_WIDTH_DEFAULT`, `DOCK_WIDTH_MIN`, `DOCK_WIDTH_MAX` in new `crates/modplayer-core/src/settings/window.rs`, with per-field sanitize/clamp functions per data-model.md §1 validation rules (absent → default; non-Float/Integer → default; non-finite or ≤0 → default; else clamp: width `max(v,960)`, height `max(v,640)`, dock `clamp(v,240,480)`) — doc comments with runnable examples on every public item (Constitution VII)
- [X] T003 [P] Add `RawWindow` struct (all fields `Option<toml::Value>`) and wire it into `crates/modplayer-core/src/settings/model.rs`: `AudioSettings.window: WindowSettings`, `RawSettings.window: RawWindow`, `RawSettings::into_settings` (independent per-field conversion via T002's sanitizers, silent fallback, no `InvalidField` warning), `AudioSettings::from(&self) -> RawSettings`/equivalent write path always emitting all three keys with current clamped values — additive, no `schema_version` bump (contract W1, research R4)
- [X] T004 Re-export `WindowSettings` and the window constants from `crates/modplayer-core/src/settings/mod.rs` (depends on T002, T003)
- [X] T005 [US-shared] Add `window: WindowSettings` shadow field to `PlaybackController` in `crates/modplayer-core/src/controller.rs`, initialized from `settings.window` at construction (pattern: `now_playing_panels`, `controller.rs:714`); implement `pub fn window_settings(&self) -> WindowSettings` (shadow read, no I/O), `pub fn set_dock_width(&mut self, width: f32)` (clamp [240,480]; ignore non-finite/≤0 input; `persist_settings` only when clamped value differs from shadow), `pub fn set_window_inner_size(&mut self, width: f32, height: f32)` (clamp ≥960×640; same ignore/write-iff-changed rules) — contract W2 (depends on T004)
- [X] T006 [P] Add `#[doc(hidden)] pub fn with_pseudo_expansion<R>(percent: u32, f: impl FnOnce() -> R) -> R` thread-local test hook to `crates/modplayer-core/src/i18n.rs`: while active, `tr`/`tr_args` pad each resolved string by `max(1, ceil(len * percent / 100))` characters (research R12); zero cost when inactive
- [X] T007 [P] Create `crates/modplayer-ui/src/layout.rs` with: constants `AUTO_HIDE_THRESHOLD = 1024.0`, `HOST_CONTENT_FLOOR = 560.0`, `DOCK_KEY_STEP = 16.0`, waveform proportions (`0.08`/`64.0`, `0.22`/`120.0`), `SAVE_DEBOUNCE = Duration::from_millis(500)`; `enum DockPresentation { None, Docked, Hidden, Overlay }`; `pub fn dock_presentation(window_width: f32, docked_count: usize, overlay_open: bool) -> DockPresentation` (data-model §3 truth table); `pub fn effective_dock_width(stored_or_live: f32, content_width: f32) -> f32` = `clamp(stored_or_live, 240.0, max(240.0, min(480.0, content_width - 560.0)))` (data-model §4); `pub fn waveform_heights(h: f32) -> (f32, f32)` = `(max(64, round(0.08*h)), max(120, round(0.22*h)))` (data-model §5); `pub struct WindowSizeTracker` with fields `last_saved: Vec2, pending: Option<Vec2>, changed_at: Option<Instant>, last_write: Option<Instant>` and methods `observe(&mut self, size, maximized_or_fullscreen: bool, now: Instant)`, `poll(&mut self, now: Instant) -> Option<Vec2>`, `next_deadline(&self) -> Option<Instant>`, `flush(&mut self) -> Option<Vec2>` (data-model §2, research R2) — doc comments with runnable examples on every public item (depends on T004 for constant parity, but layout module itself has no core dependency so file creation can start immediately)
- [X] T008 Register `pub mod layout;` in `crates/modplayer-ui/src/lib.rs` (depends on T007)
- [X] T009 [P] Add locale keys to `locales/en-US/plugins.ftl`: `plugin-dock-panels-toggle` = "Panels", `plugin-dock-resize` = "Resize plugin dock", `plugin-dock-resize-value` = "{ $width } points" (data-model §7, FR-015)
- [X] T010 [P] Write W4.1–W4.3 unit tests in `crates/modplayer-core/tests/settings.rs`: absent `[window]` → defaults 1200/820/280 (W4.1); below-min/out-of-[240,480] values clamp (W4.2); `"wide"`, `nan`, `inf`, `-5`, `0` per key → default for that key only, other sections intact, no `InvalidField` notification (W4.3) (depends on T003)
- [X] T011 [P] Write W4.4–W4.5 proptests in new `crates/modplayer-core/tests/settings_window_proptest.rs`: any valid `WindowSettings` round-trips exactly through save/load (W4.4); any arbitrary `f64`/string per key loads to a value within the valid post-load ranges (W4.5) (depends on T003)
- [X] T012 [P] Write W4.6 test in new `crates/modplayer-core/tests/controller_window.rs`: setters persist only on change and the persisted value survives a new controller constructed over the same store (depends on T005)
- [X] T013 [P] Write D10.1–D10.4 unit/proptest coverage as `#[cfg(test)]` in `crates/modplayer-ui/src/layout.rs`: `dock_presentation` truth table including the 1023.9/1024.0 boundary and zero-panels case (D10.1); proptest `effective_dock_width` ∈ [240,480], ≤ `max(240, content-560)`, monotone in stored width (D10.2); proptest `waveform_heights` formula and monotonicity (D10.3); `WindowSizeTracker` debounce/maximized-ignore/exit-flush behavior (D10.4) (depends on T007)
- [X] T014 [P] Write new-key resolution assertions for `plugin-dock-panels-toggle`, `plugin-dock-resize`, `plugin-dock-resize-value` in `crates/modplayer-ui/tests/fluent_keys.rs` (D10.10) (depends on T009)

**Checkpoint**: `WindowSettings` persists and clamps correctly; controller exposes `window_settings()`/`set_dock_width()`/`set_window_inner_size()`; `modplayer-ui::layout` provides presentation/width/height pure functions plus the size tracker; locale keys resolve. Run `RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-core --test settings --test settings_window_proptest --test controller_window && RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui --lib layout --test fluent_keys` — all green before any user story starts.

---

## Phase 3: User Story 1 - Window opens at a size that renders correctly (Priority: P1) 🎯 MVP

**Goal**: First launch opens at 1200×820 (or restored size), never shrinks below 960×640, and every panel header/transport control renders without clipped or overlapping text — the root problem this feature exists to fix.

**Independent Test** (spec.md): Launch with a fresh config directory (no saved `[window]` state); measure opening inner size; visually confirm dock headers/transport controls render without clipped/overlapping text.

**Note**: Acceptance scenario US1-3 (960×640 with dock auto-hidden, opened via "Panels" toggle) needs the overlay mechanics from Phase 5 (US3) to verify end-to-end; the header fix that scenario depends on lands here. US1-4 (docked at 240, ≥1024 wide) is fully verifiable within this phase once T017 lands.

### Implementation for User Story 1

- [X] T015 [US1] Wire `NativeOptions.viewport` in `crates/modplayer/src/main.rs`: `ViewportBuilder::default().with_title("ModPlayer").with_inner_size([w.inner_width, w.inner_height]).with_min_inner_size([960.0, 640.0])` where `w = controller.window_settings()` (no position/maximized/fullscreen set — OS decides); replace `NativeOptions::default()` (contract W3, research R1) — FR-001, FR-002
- [X] T016 [US1] Add `WindowSizeTracker` (from T007) as a field on `App` in `crates/modplayer-ui/src/app.rs`, initialized from the restored size; each frame call `observe()` with `ctx.input(|i| i.viewport())`'s `inner_rect`/`maximized`/`fullscreen`, then `poll()` — on `Some(size)` call `controller.set_window_inner_size(size.x, size.y)`; call `ctx.request_repaint_after(deadline)` when `next_deadline()` is `Some`; in `App::on_exit`, call `flush()` and persist unconditionally if `Some` (contract D9, research R2) — FR-003
- [X] T017 [US1] Rework the panel header in `crates/modplayer-ui/src/plugin_panels.rs` per contract D7 / research R9: measure button widths (Float/Dock, Close, destructive gap, Disable) built with `TextWrapMode::Extend` (never elided); compute `title_budget = available - icon - spacing - buttons_w`; if `title_budget >= TITLE_MIN_WIDTH` draw one row (icon, title, buttons), else draw title row + `horizontal_wrapped` button row(s); title = `Label::new(tr_args("plugin-panel-header", ..)).truncate()`, focusable, `on_hover_text`/focus-tooltip with full text, AccessKit label = full untruncated text (FR-010, FR-011); suspended-placeholder and other host status text switched to `Label::wrap()` (FR-011a) — remove the two leftover `eprintln!("TRACE …")` calls (lines ~236, ~469) as part of this rewrite (research R14)
- [X] T018 [US1] Change the Now Playing transport row from `ui.horizontal` to `ui.horizontal_wrapped` in `crates/modplayer-ui/src/now_playing.rs`, with button/switch labels using `TextWrapMode::Extend` so whole controls wrap rather than eliding/overlapping under +40% text at 960×640 (research R10) — partial FR-013 (full FR-013 coverage completes once the "Panels" toggle exists, T024)
- [X] T019 [US1] Update `crates/modplayer-ui/tests/plugin_panels.rs` header expectations for the new truncating-title/wrapping-buttons layout (replace any assertions that assumed the old single-row `ui.horizontal` header) (depends on T017)
- [X] T020 [P] [US1] Add/adjust `crates/modplayer-ui/tests/first_launch.rs` (or equivalent) coverage confirming the app launches with a restored/default `WindowSettings`-driven viewport size is exercised via `controller.window_settings()` (unit-level; full visual confirmation is manual scenario M1)

**Checkpoint**: Window opens at 1200×820 by default, enforces 960×640 minimum, size settles/persists/flushes on exit, and panel headers never truncate their buttons. Run `RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui --test plugin_panels --test now_playing`. US1 is independently testable now except acceptance scenario 3, which is confirmed once Phase 5 lands.

---

## Phase 4: User Story 2 - Plugin dock resizes and remembers its width (Priority: P1)

**Goal**: The dock's inner edge is drag- and keyboard-resizable between 240 and its effective maximum, and the chosen width persists across restarts.

**Independent Test** (spec.md): With the app running and ≥1 panel docked, drag the dock's inner edge to a new width, confirm reflow, restart, confirm the dock reopens at that width.

### Implementation for User Story 2

- [X] T021 [US2] In `crates/modplayer-ui/src/plugin_panels.rs`, compute `effective = layout::effective_dock_width(live_or_stored, content_width)` each frame (`content_width` = Now Playing content `Ui`'s `max_rect().width()`, research R7) and draw the docked column with `egui::Panel::right("plugin-panel-dock").exact_size(effective).resizable(false)`, removing the old `pub const DOCK_WIDTH: f32 = 280.0` constant (contract D2, FR-004)
- [X] T022 [US2] Add a custom focusable splitter along the dock's left edge in `crates/modplayer-ui/src/plugin_panels.rs`: 6-pt hit rect, `Sense::click_and_drag()`, `CursorIcon::ResizeHorizontal`; drag computes `live_width = drag_start_width - pointer_delta_x` clamped to `[240, effective_max]`, held in egui temp memory and used as that frame's panel width (tracks pointer every frame, SC-003); on `drag_stopped()` call `controller.set_dock_width(live_width)`; keyboard while focused: `←` +16, `→` −16, `Home` → 240, `End` → effective max, each committing immediately via `set_dock_width`; register `Left/Right/Home/End/Tab/Shift+Tab` focus claims via `actions::register_claim` (007 SC-010 pattern, same as `plugin_panels::numeric_claims`) (contract D3, research R6) — FR-004a, FR-005
- [X] T023 [US2] Wire AccessKit for the splitter in `crates/modplayer-ui/src/plugin_panels.rs`: role `Splitter`, label `tr("plugin-dock-resize")`, numeric value = current effective width, min 240 / max effective max, value text `tr_args("plugin-dock-resize-value", width)` (contract D3, NFR-6.1/6.2) (depends on T022)
- [X] T024 [P] [US2] Write D10.7 splitter test in `crates/modplayer-ui/tests/responsive_dock.rs` (new file): drag each frame tracks pointer; width persists once on release; ←/→/Home/End change width correctly; clamp holds at 240 and at effective max (depends on T021–T023)

**Checkpoint**: Dock is continuously drag- and keyboard-resizable within [240, effective max], width persists on release, render-time clamp never rewrites the stored value. Run `RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui --test responsive_dock`. US2 independently testable (manual M5/M6).

---

## Phase 5: User Story 3 - Narrow window keeps plugin panels reachable (Priority: P2)

**Goal**: Below 1024 pt window width the dock auto-hides; a "Panels" toggle in the transport row opens it as a dismissible right-anchored overlay.

**Independent Test** (spec.md): With a panel docked, narrow the window below 1024, confirm the dock disappears and "Panels" toggle appears, reopen via the toggle, confirm the same panel is available.

### Implementation for User Story 3

- [X] T025 [US3] In `crates/modplayer-ui/src/plugin_panels.rs` (or `app.rs`, wherever the dock is invoked from), compute `window_width = ctx.content_rect().width()` and call `layout::dock_presentation(window_width, docked_count, overlay_open)` each frame to choose `None`/`Docked`/`Hidden`/`Overlay` (contract D1) — `overlay_open` stored as session-only egui temp-memory bool under `Id::new("plugin-dock-overlay-open")` (research R8), force-cleared whenever presentation resolves to `Docked` or `None` — FR-006, FR-009
- [X] T026 [US3] Add `shell::PLUGIN_DOCK_OVERLAY_ORDER` in `crates/modplayer-ui/src/shell.rs` and draw the overlay in `crates/modplayer-ui/src/plugin_panels.rs` with `egui::Area::new("plugin-dock-overlay").order(Order::Middle)`, fixed at the right edge of the Now Playing content rect, full content height, width = `effective_dock_width`, `ui.visuals().panel_fill` frame, same splitter (T022) + `ScrollArea` body as the docked column, containing exactly the docked panels in existing order/state (contract D1/D2, research R8) — FR-008
- [X] T027 [US3] Implement overlay dismissal in `crates/modplayer-ui/src/plugin_panels.rs`: toggle pressed again → close; `Escape` while focus is inside the overlay → close + move focus to the "Panels" toggle (register an `Escape` claim so the dispatcher doesn't also fire a global action); last docked panel closed/floated → close; window widened ≥1024 → close; outside pointer clicks do **not** close it (contract D5) — FR-008
- [X] T028 [US3] Implement focus continuity in `crates/modplayer-ui/src/plugin_panels.rs`: if the previous frame's focused widget was inside the dock/overlay and this frame's presentation is `Hidden`/`None`, move focus to the "Panels" toggle id (or surrender if the toggle is absent, i.e. `None`); Docked↔Overlay transitions keep the same widget id focused (contract D6, data-model §3 "Focus rule")
- [X] T029 [US3] Add the "Panels" toggle to the Now Playing transport row in `crates/modplayer-ui/src/now_playing.rs`: `widgets::controls::switch(ui, SwitchKind::Toggle, &mut overlay_open, &tr("plugin-dock-panels-toggle"))`, last item of the row, shown only when presentation ∈ {Hidden, Overlay}, AccessKit `Toggled::True` iff Overlay, no new global shortcut (contract D4, FR-007) — completes FR-013 coverage from T018 (depends on T025)
- [X] T030 [P] [US3] Write D10.6 test in `crates/modplayer-ui/tests/responsive_dock.rs`: window 960×640 with 2 panels → no dock drawn, toggle present; activating toggle → overlay with same panels in order; Esc → overlay closed, focus on toggle; widen ≥1024 → docked, toggle gone (depends on T025–T029)
- [X] T031 [US3] Write the D10.8 / SC-007 pseudo-localization test in `crates/modplayer-ui/tests/responsive_dock.rs`: using `with_pseudo_expansion(40, ..)` (T006) and a 60-character fixture panel title, run `egui::Context::run` at (a) ~1100×800 with dock at 240 and (b) 960×640 overlay open, with two fixture panels docked; assert every button/toggle label's galley is not elided and no two interactive rects overlap (research R12) — FR-013, NFR-7.4 (depends on T006, T017, T029, T030)
- [X] T032 [P] [US3] Write the D10.5 test in `crates/modplayer-ui/tests/responsive_dock.rs`: window 1100 wide, dock at 240, 2 panels — every header button's AccessKit name present and label not elided; title tooltip appears on keyboard focus (depends on T017, T021)

**Checkpoint**: Dock auto-hides below 1024, "Panels" toggle surfaces/opens a dismissible overlay with correct focus handling, and the +40% pseudo-localization assertion holds. Run `RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui --test responsive_dock`. US1 acceptance scenario 3 is now verifiable end-to-end (re-run manual M3). US3 independently testable (manual M7–M10).

---

## Phase 6: User Story 4 - Waveform height scales with window height (Priority: P3)

**Goal**: Waveform overview/detail heights track window content height (`max(64, 8% H)` / `max(120, 22% H)`) instead of fixed 72/120 constants.

**Independent Test** (spec.md): With a track loaded, resize the window taller/shorter, confirm overview/detail heights follow the formulas without clipping or disappearing.

### Implementation for User Story 4

- [X] T033 [US4] Capture `H = ui.max_rect().height()` as the first statement of `crates/modplayer-ui/src/now_playing.rs::show` (CentralPanel inner height, unaffected by dock/scroll) and call `layout::waveform_heights(H)` once per frame (research R11) — FR-012
- [X] T034 [US4] Change `crates/modplayer-ui/src/waveform/mod.rs::overview`/`detail` to take an explicit `height: f32` parameter; remove `OVERVIEW_HEIGHT`/`DETAIL_HEIGHT` constants; update all callers (now_playing.rs from T033, and any test fixtures) to pass the computed heights (contract D8) — FR-012
- [X] T035 [P] [US4] Update `crates/modplayer-ui/tests/waveform.rs` fixed-height assertions to pass explicit heights (72/120 where old geometry was asserted) and add D10.9: overview/detail rect heights equal `waveform_heights(H)` for two distinct window heights (depends on T034)

**Checkpoint**: Waveform heights scale with content height, never below 64/120, no upper bound. Run `RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui --test waveform`. US4 independently testable (manual M11).

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Full gate sweep, remaining cleanup, and the manual scenario matrix required by Constitution "Manual Scenario Sign-Off."

- [X] T036 Confirm no leftover `eprintln!("TRACE …")` calls remain in `crates/modplayer-ui/src/plugin_panels.rs` (should already be gone per T017/research R14); grep the file to verify
- [X] T037 Run the full automated validation sequence from quickstart.md: `RUSTUP_TOOLCHAIN=1.95.0 rtk cargo fmt --check && RUSTUP_TOOLCHAIN=1.95.0 rtk cargo clippy --workspace --all-targets --all-features -- -D warnings && RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-core --test settings --test settings_window_proptest --test controller_window && RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui --lib layout && RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test -p modplayer-ui --test responsive_dock --test waveform --test plugin_panels --test fluent_keys --test now_playing && RUSTUP_TOOLCHAIN=1.95.0 rtk cargo test --workspace && RUSTUP_TOOLCHAIN=1.95.0 rtk cargo deny check`
- [X] T038 Execute manual scenarios M1–M13 from quickstart.md (launch via `RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer`, Quartz window location, `screencapture` evidence); record pass/deviation + screenshot path for each of M1 (first-launch size), M2 (min size), M3 (overlay at min size), M4 (dock at 240), M5 (drag+persist), M6 (keyboard resize), M7 (auto-hide), M8 (widen back), M9 (outside click no-op), M10 (last panel leaves), M11 (waveform scaling), M12 (window size persistence incl. maximized-quit), M13 (corrupt values) in this tasks.md's checkpoint notes; feed any behavior deviation back into research.md/quickstart.md (depends on T015–T035 all complete)
- [X] T039 [P] Verify governance traceability: every FR/NFR cited in plan.md's Constitution Check and contracts D1–D10/W1–W4 has a corresponding implemented task above and a passing test; note the `ui-panels.md` L1/L5 supersession is recorded in `contracts/ui-responsive-dock.md` (already true — confirm no drift was introduced)

**Checkpoint**: All gates green (T037, re-run 2026-09-25 after the T038 fixes: fmt, clippy `-D warnings`, 1873 workspace tests, `cargo deny`), traceability confirmed (T039).

**T038 manual walk (2026-09-25, macOS, scratch config `target/manual-walk/cfg`, screenshots in `target/manual-walk/shots/`)** — the earlier "screen locked" block was actually the agent shell running in launchd's `Background` session; see the quickstart.md launch notes. Four deviations were found and fixed (research.md R15), each with a regression test:

| # | Result | Evidence / notes |
|---|---|---|
| M1 | Pass | Fresh config: Quartz 1200 × 848 = 1200 × 820 inner + 28 pt title bar; welcome screen text unclipped (`m1.png`). Relaunch with a real config and no `[window]`: same size, `[window]` written 1200/820/280. |
| M2 | Pass | Corner dragged far up-left: stops at 960 × 640 inner (`m2.png`). |
| M3 | Pass | 960 × 640 with 2 docked panels: dock hidden, "Panels" toggle shown; overlay shows both panels, header buttons wrap to their own row, every Float/Close/Disable readable, body scrolls (`m3-overlay.png`). |
| M4 | Pass | 1200 wide, splitter dragged right: stops at 240 (`dock_width = 240.0`); titles truncated with "…", buttons on a wrapped row (`m4.png`); hovering the title shows the full-text tooltip (`m4-tooltip-crop.png`). |
| M5 | Pass after fix | The dock tracks the pointer mid-drag (≈300 at half-way, `m5-middrag.png`). Width is saved once on release (354) and restored on relaunch. **Deviation fixed (R15-2)**: at 354 the header's full title pushed Float/Close/Disable past the dock edge (`m5-relaunch2.png` → `m5-relaunch-fixed.png`). |
| M6 | Pass after fix | Tab ×7 reaches the splitter (focus ring). ←×3, →, Home, End → 370→418→402→240→480, each saved immediately; 480 survives relaunches. **Deviation fixed (R15-3)**: before the fix only the first ← landed, then egui's arrow navigation moved focus away. |
| M7 | Pass | At 1000 wide the dock disappears and the toggle appears in the same frame. Focus on the splitter moves to "Panels" (`m7-narrow.png`). The overlay shows both panels in order (`m7-overlay.png`). Esc with focus inside closes it, with the focus ring on "Panels" (`m7-esc.png`). A first attempt showed only one panel: the Keychain prompt had blocked startup, so the Section Loop plugin timed out registering (`host_busy`). After a relaunch both panels were present (environmental, not the feature). |
| M8 | Pass after fix | With the overlay open, widening to 1100 re-docks the panels at the remembered width (clamped to the effective max) and removes the toggle (`m8.png`). **Deviation fixed (R15-4)**: "Transport" ran ~7 pt across the dock edge (`m8-crop.png`). Verified by `transport_row_never_crosses_docked_edge`. Not re-walked live: another rebuild would have needed another Keychain approval. |
| M9 | Pass | Overlay open, clicked Play: overlay stays open (`m9.png`). There was no track loaded, so Play itself was a no-op. |
| M10 | Pass | With one docked panel, overlay open, clicked Float: the overlay closes, the toggle disappears and the floated window appears (`m10-c.png`). |
| M11 | Pass | Track playing, volume muted. 1200 × 792: overview 64, detail ≈ 170. Zoomed (≈ 997 tall): overview ≈ 78–80, detail ≈ 214 (`m11-zoom.png`). 640 tall: overview 64, detail ≈ 137 (`m11-640.png`). |
| M12 | Pass after fix | 1400 × 900 saved within ~1 s and restored on relaunch. **Deviation fixed (R15-1)**: after a zoom, 1680 × 997 was saved. After the fix, the zoomed size is never saved and relaunch opens at 1400 × 900. Re-checked during M11: zoom left `[window]` unchanged. |
| M13 | Pass | `inner_width = "wide"`, `dock_width = 9999`: opens at 1200 × 900 (saved height kept), dock ≈ 480, theme/volume unchanged, no settings-unreadable notice (`m13.png`). The next write rewrote `inner_width = 1200.0`. |

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Phase 1. BLOCKS all user stories (T002–T014 must all land; T005/T007 in particular are consumed by every story).
- **US1 (Phase 3)**: Depends on Foundational. No dependency on other stories for implementation; acceptance scenario 3 needs US3 (Phase 5) to *verify*, not to implement.
- **US2 (Phase 4)**: Depends on Foundational. Independent of US1/US3/US4 for implementation; shares `plugin_panels.rs` with US1's header rework (T017) and US3's overlay work (T025–T028) — sequence T017 → T021–T024 → T025–T029 to avoid rebasing the same file repeatedly (see Note below).
- **US3 (Phase 5)**: Depends on Foundational; its splitter reuse depends on US2's splitter (T022) existing; its "Panels" toggle depends on US1's transport-row wrapping (T018). Per plan.md phasing, sequence after US2.
- **US4 (Phase 6)**: Depends on Foundational only (`now_playing.rs`, `waveform/mod.rs`) — independent of US1/US2/US3, could run in parallel with them by a different developer.
- **Polish (Phase 7)**: Depends on all four user stories being complete.

**Note on `plugin_panels.rs` contention**: T017 (US1 header), T021–T023 (US2 dock/splitter) and T025–T028 (US3 overlay) all edit `crates/modplayer-ui/src/plugin_panels.rs`. They are not marked `[P]` against each other for this reason, even though they belong to different stories — implement in the plan.md-specified order (US1 header → US2 width/splitter → US3 presentation/overlay) within that file.

### User Story Dependencies

- **US1 (P1)**: Foundational only, for implementation. Full acceptance verification needs US3.
- **US2 (P1)**: Foundational only. Independently testable (manual M5/M6) without US1/US3.
- **US3 (P2)**: Foundational + reuses US2's splitter (T022) + US1's wrapped transport row (T018).
- **US4 (P3)**: Foundational only. Fully independent of US1/US2/US3.

### Parallel Opportunities

- Within Foundational: T002, T003 (after T002 lands, since `RawWindow`→`into_settings` needs the sanitizers), T006, T007, T009 can start in parallel; T010–T014 (tests) are `[P]` against each other once their respective implementation task lands.
- T020 (US1 test) is `[P]` against T017–T019 impl work (different file).
- T024 (US2 test) depends on T021–T023 but is otherwise isolated.
- T030, T032 (US3 tests, D10.6/D10.5) are `[P]` against each other; T031 (D10.8) depends on more precursors (T006, T017, T029) so sequence it after.
- US4 (Phase 6, T033–T035) can be executed by a different developer in parallel with US1–US3 once Foundational is done, since it touches only `now_playing.rs` (H capture — additive to US1/US3's transport-row edits, watch for merge overlap) and `waveform/mod.rs`.
- T039 (Polish) is `[P]` against T036–T038.

---

## Parallel Example: Foundational Phase

```bash
# After T002 lands, launch together:
Task: "Add RawWindow struct and wire settings.rs conversion (T003)"
Task: "Add with_pseudo_expansion test hook to i18n.rs (T006)"
Task: "Create modplayer-ui/src/layout.rs pure functions + WindowSizeTracker (T007)"
Task: "Add locale keys to plugins.ftl (T009)"
```

## Parallel Example: User Story 3 tests

```bash
Task: "D10.6 overlay/toggle/Esc/widen test in responsive_dock.rs (T030)"
Task: "D10.5 header-at-240 AccessKit test in responsive_dock.rs (T032)"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001).
2. Complete Phase 2: Foundational (T002–T014) — CRITICAL, blocks everything.
3. Complete Phase 3: User Story 1 (T015–T020).
4. **STOP and VALIDATE**: window opens at 1200×820, min 960×640 enforced, headers never truncate buttons (US1-1, US1-2, US1-4 fully verifiable; US1-3 pending US3).
5. This is the smallest shippable increment that fixes the root UX-01 problem.

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. US1 (Phase 3) → validate independently → root launch-size bug fixed (MVP).
3. US2 (Phase 4) → validate independently → dock is resizable and persists.
4. US3 (Phase 5) → validate independently (also closes out US1-3) → narrow-window panels story complete.
5. US4 (Phase 6) → validate independently → waveform polish complete.
6. Polish (Phase 7) → full gates + manual M1–M13 → ship.

### Suggested Team Split

With 2 developers after Foundational: Developer A takes US1 → US2 → US3 sequentially (shared file `plugin_panels.rs`, see contention note); Developer B takes US4 in parallel, then joins Polish.

---

## Notes

- [P] tasks touch different files or are read-only test additions with no cross-task ordering constraint.
- [Story] label maps each task to spec.md's US1–US4 for traceability back to acceptance scenarios and FR/NFR IDs.
- `plugin_panels.rs` is touched by US1, US2 and US3 — implement in plan.md's specified order (header → width/splitter → presentation/overlay) to avoid repeated rebasing.
- Every FR/NFR/SC/D/W ID from spec.md, data-model.md and the two contracts is covered by at least one task above; Phase 7 T039 is the closing traceability check.
- Manual scenarios M1–M13 (quickstart.md) are executed once per full pass at T038, and again narrowly wherever a story's checkpoint says a scenario becomes verifiable (e.g. M3 after Phase 5).
