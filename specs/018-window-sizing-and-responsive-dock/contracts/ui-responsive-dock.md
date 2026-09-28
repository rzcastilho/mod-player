# Contract: Responsive plugin dock, panel header, "Panels" toggle, waveform sizing

**Feature**: 018-window-sizing-and-responsive-dock | Requirements: FR-002, FR-004–FR-013, FR-015; NFR-6.1, NFR-6.2, NFR-7.1, NFR-7.4 | Model: [data-model.md §2–§7](../data-model.md)

Supersedes `specs/011-plugin-ui-contributions/contracts/ui-panels.md` **L1**
("docked in a fixed-width column") and **L5**'s single-row header; all other
011 rules (L2–L4, P1–P5, A1–A4, §3 widget table) are unchanged.

## D1 — Presentation

Computed each frame by `layout::dock_presentation(window_width, docked_count,
overlay_open)` (data-model §3). `window_width = ctx.content_rect().width()`.

| Presentation | Docked column | Overlay | "Panels" toggle |
|---|---|---|---|
| None | not drawn | not drawn | absent |
| Docked (≥ 1024) | `Panel::right`, drawn first in Now Playing | not drawn | absent |
| Hidden (< 1024) | not drawn | not drawn | shown, off |
| Overlay (< 1024) | not drawn | `Area`, right edge of Now Playing content, full content height | shown, on |

No hysteresis: 1023.x → narrow, 1024.0 → wide. Floated panels are unaffected in
every presentation.

## D2 — Width

- Docked and overlay both use `layout::effective_dock_width` (data-model §4).
- Width is never below 240; host content column (docked) is ≥ 560 whenever
  `content_width ≥ 800`.
- The render-time limit never calls `set_dock_width`.

## D3 — Splitter (resize edge)

- Present on the left edge of both the docked column and the overlay.
- Pointer: `ResizeHorizontal` cursor; drag tracks the pointer every frame;
  clamped to [240, effective max]; persisted once on release.
- Keyboard: focusable (in Tab order before the first panel). `←` +16,
  `→` −16, `Home` min (240), `End` effective max; each press persists.
  Claims `Left/Right/Home/End/Tab/Shift+Tab` so global actions don't fire.
- AccessKit: role `Splitter`, label `tr("plugin-dock-resize")`, numeric value
  = current effective width, min 240, max effective max, value text
  `tr_args("plugin-dock-resize-value", width)`.

## D4 — "Panels" toggle

- `widgets::controls::switch(.., SwitchKind::Toggle, .., tr("plugin-dock-panels-toggle"))`,
  last item of the Now Playing transport row.
- AccessKit: toggle role, name "Panels", `Toggled::True` iff Overlay.
- The transport row is `horizontal_wrapped`; its controls wrap as whole
  controls, never elide (FR-013).
- No new global shortcut.

## D5 — Overlay dismissal

Closes (→ Hidden or → Docked/None) on exactly: toggle off; `Escape` while focus
is inside the overlay (focus then moves to the toggle); last docked panel
closed/floated; window ≥ 1024. Pointer clicks outside the overlay do **not**
close it. `overlay_open` is never persisted.

## D6 — Focus continuity

If the previously focused widget was inside the dock/overlay and the dock is
no longer drawn, focus moves to the "Panels" toggle (Hidden) — never left on
an undrawn widget. Docked ↔ Overlay keeps the same widget focused (ids are
container-independent).

## D7 — Panel header (replaces 011 L5 layout)

- Row 1: icon · title. Title = `tr_args("plugin-panel-header", {plugin, title})`,
  single line, truncated with "…" when short of space; focusable; tooltip
  with full text on hover and on keyboard focus; AccessKit label = full text.
- Buttons `[Float|Dock] [Close] ‹destructive gap› [Disable]`: labels built
  with `TextWrapMode::Extend` — **never** elided. On row 1 when
  `available − icon − spacing − buttons_w ≥ TITLE_MIN_WIDTH`, otherwise on
  row 2 (and further rows) via `horizontal_wrapped`.
- Suspended placeholder (`plugin-panel-suspended`) and other host status text
  in the dock wrap (`Label::wrap()`).

## D8 — Waveform heights

`waveform::overview(ui, .., height, ..)` / `waveform::detail(ui, .., height, ..)`
take an explicit `height`. `now_playing::show` computes
`layout::waveform_heights(H)` once per frame, `H` = Now Playing content
`max_rect().height()`. Constants `OVERVIEW_HEIGHT`/`DETAIL_HEIGHT` are removed.

## D9 — Window

Initial inner size and min inner size per [window-settings.md W3](./window-settings.md).
Window size observed each frame via `ctx.input(|i| i.viewport())` into
`WindowSizeTracker`; settled sizes (≥ 500 ms stable, ≤ 1 write / 500 ms) and
the exit flush go to `controller.set_window_inner_size`. Maximized/fullscreen
observations are ignored.

## D10 — Test obligations

| ID | Assertion | Location |
|---|---|---|
| D10.1 | `dock_presentation` truth table incl. 1023.9/1024.0 and zero panels | `crates/modplayer-ui/src/layout.rs` unit tests |
| D10.2 | proptest: `effective_dock_width` ∈ [240, 480], ≤ `max(240, content−560)`, monotone in stored width | same |
| D10.3 | proptest: `waveform_heights` formula and monotonicity (SC-006) | same |
| D10.4 | `WindowSizeTracker` debounce, maximized ignore, exit flush | same |
| D10.5 | window 1100 wide, dock 240, 2 panels: every header button AccessKit name present and label not elided; title tooltip on focus | `crates/modplayer-ui/tests/responsive_dock.rs` |
| D10.6 | window 960 × 640, 2 panels: no dock, toggle present; toggle → overlay with same panels in order; Esc → closed, focus on toggle; widen → docked, toggle gone | same |
| D10.7 | splitter drag (each frame width tracks pointer; persisted once on release); ← / → / Home / End; clamp at 240 and effective max | same |
| D10.8 | +40 % pseudo-localization and 60-char title at (a) dock 240 and (b) 960 × 640 overlay: no elided button/toggle label, no overlapping interactive rects (SC-007) | same |
| D10.9 | overview/detail rect heights equal `waveform_heights(H)` for two window heights | `crates/modplayer-ui/tests/waveform.rs` |
| D10.10 | new keys resolve | `crates/modplayer-ui/tests/fluent_keys.rs` |
