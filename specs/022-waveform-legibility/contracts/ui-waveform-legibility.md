# Contract: Waveform Paint & Hover (UI)

**Feature**: 022-waveform-legibility | Extends [005 contracts/ui-waveform.md](../../005-now-playing-waveform/contracts/ui-waveform.md) §4 and [006 contracts/ui-markers.md](../../006-markers-loops-and-cues/contracts/ui-markers.md) §1/§5. Types: [data-model.md](../data-model.md).

Contract IDs `WL1…` are referenced by tests and tasks.

## WL1 — Layer order (per view, overview and detail identically)

Bottom → top, inside `space.rect`:

1. Background `surface_base` (005).
2. Columns: two-tone fill (WL2) or placeholder band (005, unchanged) or "analysis unavailable" label (005, unchanged).
3. Overview only: detail-window highlight `accent × DETAIL_HIGHLIGHT_ALPHA` (005, unchanged value).
4. `overlays` hook (006/011): marker lines → loop shading (WL4) → plugin overlay layers.
5. **Hover indicator (WL5)** — new.
6. **Playhead (WL3)** — casing then core; always topmost.

`overview()`/`detail()` public signatures are unchanged except via the `WaveformPaint` struct (new field `hover_suppressed`); `overlays` hook signature is unchanged.

## WL2 — Two-tone, played/unplayed fill

For each present column `(min, max, rms)` at pixel column `col`:
- **Played** iff `playhead` is `Some(p)` and `col_left + 0.5 < space.x_of(p)`; else unplayed. `playhead` is the displayed position (drag preview while dragging).
- Peak rect `[mid − max·h, mid − min·h]` (hairline ≥ 1 px when flat, 005 rule) in `played_peak` / `unplayed_peak`.
- Average band `[mid − rms·h, mid + rms·h] ∩ peak rect`, drawn only if ≥ 1 px tall, in `played_average` / `unplayed_average`. (`h = half_height / 127`.)
- Placeholder columns and the whole-area placeholder/unavailable states are unchanged — no played/unplayed or two-tone treatment there.

Assertions: shape capture shows ≥ 2 distinct fill colours left vs right of the playhead x (SC-006); a column with `rms > 0` emits exactly two rects of the two tone colours of its side.

## WL3 — Playhead

- Drawn iff `WaveformPaint::playhead` is `Some`.
- Two `line_segment`s at `x = space.x_of(frame)`, full rect height: `PLAYHEAD_CASING_WIDTH` in `playhead_casing`, then `PLAYHEAD_CORE_WIDTH` in `playhead_core`.
- Contrast: `max(ratio(core, b), ratio(casing, b)) ≥ 3.0` for every backdrop in [data-model §6](../data-model.md#6-playhead-backdrop-set-research-r9), all four appearances (SC-001).

## WL4 — Loop-region shading

Implemented in `markers::paint_overlay` (signature unchanged: `painter, space, markers, loop_state, focused, roles`).
- Every region with `span() == Some((a, b))` is shaded over `[x_of(a), max(x_of(b), x_of(a)+1)]` full height; regions without a complete span draw nothing.
- Treatment = `loop_shade(region.armed, loop_state)` ([data-model §7](../data-model.md#7-loopshade--modplayer-uisrcmarkersrs-research-r10)):
  - `ArmedActive`: `rect_filled(palette × LOOP_ARMED_FILL_ALPHA)`; HC adds `marker_outline` stroke (017 O5, unchanged).
  - `ArmedInactive`: diagonal hatch, `LOOP_HATCH_SPACING`, stroke `palette × LOOP_HATCH_ALPHA` (006, unchanged).
  - `Idle`: `rect_filled(palette × LOOP_IDLE_FILL_ALPHA)` + `rect_stroke(LOOP_OUTLINE_WIDTH, palette, Inside)`; HC adds `marker_outline` stroke.
- Order: all `Idle` first, armed region last.
- `markers.current_region()` is not read by this function.
- Marker lines, glyphs, clamped-warning triangles: unchanged (006 FR-026, 017).

## WL5 — Hover scrub indicator

Shown on a view iff **all** hold: the view is `enabled` (a track is loaded and seek is available); `response.hover_pos()` is `Some`; `!paint.hover_suppressed`.
- Line: `HOVER_LINE_WIDTH` in `hover_line`, full rect height, at the pointer x.
- Label: text `m:ss.mmm` (`format_mmss_millis`, e.g. `1:23.500`) in `theme::mono_font_id()`, colour `hover_label_text`, on a `hover_label_bg` pill (`radius::SM`, padding `space::XS`), top-aligned at `rect.top() + space::XS`, placed at `x + space::XS`; if that overflows `rect.right()`, placed so its right edge is `x − space::XS`; finally clamped fully inside `rect`.
- Only the hovered view shows it; the other view does not mirror.
- Never returns a `WaveformEvent`; never changes playback, transport, playhead or `WaveformState` (SC-004).
- Never calls `widget_info` / `accesskit_node_builder` differently from 005 (FR-016, FR-019): the slider name, value text and description are byte-identical with and without hover.
- Suppression: `now_playing.rs` sets `hover_suppressed = waveform.drag.is_some() || waveform.marker_drag.is_some()` for both views every frame.

## WL6 — Unchanged surfaces (regression contract, FR-010/SC-007)

- 005 click/drag seek, `Esc` cancel, keyboard seek/zoom/pan rows, slider accessibility (`transport-seek`, `waveform-detail`, descriptions).
- Elapsed/remaining labels (`theme::mono_text`).
- 006 marker lane, glyphs, drag, list-row format (`format_mmss_millis` output byte-identical to the former `format_mmss_millis_frames`).
- `layout::waveform_heights` formula and minimums.
- No new Fluent keys (hover text is numeric).
