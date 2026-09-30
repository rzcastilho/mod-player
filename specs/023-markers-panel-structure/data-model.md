# Data Model: Markers Panel Structure

**Feature**: 023-markers-panel-structure | **Date**: 2026-09-29 | **Research**: [research.md](./research.md)

This feature adds **no persisted data** and **no core model change**: `modplayer_core::markers::{TrackMarkers, Marker, LoopRegion, CueSlot, PaletteIndex}` and every `PlaybackController` method are used as they are. Everything below is UI-side:

- a per-frame, read-only projection (§1–§3)
- a per-frame intent list (§4)
- two new session-only fields on `WaveformState` (§5)

All types are private to `crates/modplayer-ui/src/markers.rs` unless marked `pub`.

## 1. `PanelModel` (per-frame snapshot, pure)

Built once per `panel` call from `Option<&TrackMarkers>` and the source sample rate, before any widget renders (research R2, R3). It holds no borrow of `controller`.

| Field | Type | Rule |
|---|---|---|
| `loop_blocks` | `Vec<LoopBlock>` | One per `TrackMarkers::regions()` entry, sorted by `(earliest present boundary position, RegionId)` (FR-003) |
| `points` | `Vec<RowData>` | Every `MarkerKind::Point`, sorted by `(position, MarkerId)` (FR-005) |
| `cues` | `[CueSlotRow; 8]` | Index `i` ↔ `CueSlot::new(i + 1)` (FR-016) |
| `total` | `usize` | `TrackMarkers::count()`; `0` when no `TrackMarkers` |

Derived (methods):

| Method | Returns | Rule |
|---|---|---|
| `is_empty()` | `bool` | `total == 0`, which selects the empty state (FR-020) |
| `loop_count()` | `usize` | `loop_blocks.len()` (FR-002; an incomplete region counts 1) |
| `point_count()` | `usize` | `points.len()` |
| `cue_count()` | `usize` | Number of `CueSlotRow::Occupied` (FR-002; never 8 unless all are set) |
| `show_points_group()` | `bool` | `!is_empty() && point_count() > 0` (FR-001) |
| `show_loop_group()` / `show_cues_group()` | `bool` | `!is_empty()` (FR-004, FR-016) |
| `visual_order()` | `Vec<MarkerId>` | Populated rows in render order: each block's A, B → points → occupied cues by slot (research R11) |

**Invariants** (proptest, research R15):

- **P1**: The multiset of ids across `loop_blocks[*].{a,b}`, `points` and occupied `cues` equals the set of `TrackMarkers::markers()` ids. No marker is missing or duplicated.
- **P2**: Every row's `kind` matches its group. `RegionStart`/`RegionEnd` appear only in `loop_blocks`, `Point` only in `points`, and `Cue{slot}` only in `cues[slot-1]` (FR-006).
- **P3**: `visual_order()` is a permutation of P1's set.
- **P4**: Inside a block, `a` (if any) is ordered before `b` (if any).

## 2. `RowData` (one populated row)

This replaces today's `MarkerRowData`. Its `region` field is dropped, because block membership now carries it.

| Field | Type | Source |
|---|---|---|
| `id` | `MarkerId` | `Marker::id` |
| `kind` | `MarkerKind` | role label (`marker-role-a`/`-b`/`-point`/`-cue`) |
| `name` | `String` | `Marker::name`; empty shows the `markers-name-placeholder` in the panel only (FR-012) |
| `color` | `PaletteIndex` | swatch fill; popover's current selection (FR-013) |
| `position` | `u64` | frames; rendered `format_mmss_millis` via `mono_text`, or `MarkerDrag::live` while dragging |
| `clamped` | `bool` | ⚠ glyph with `marker-clamped-desc` tooltip (FR-007) |

## 3. `LoopBlock` and `CueSlotRow`

**`LoopBlock`**:

| Field | Type | Notes |
|---|---|---|
| `region` | `RegionId` | |
| `a` | `Option<RowData>` | `LoopRegion::a` resolved |
| `b` | `Option<RowData>` | `LoopRegion::b` resolved |
| `cells` | `RegionCells` | Today's `RegionRow` renamed: `armed`, `crossfade_ms`, `repeat_times: Option<u16>`, `armable`, `complete`, `clamped` (either endpoint). Rendered after the last present boundary row (research, baseline table; Clarification 12) |

Sort key: `a.or(b).position`, then `region`.

**`CueSlotRow`**:

```text
enum CueSlotRow {
    Occupied(RowData),   // full row: swatch, role "Cue n", name/placeholder, position, actions
    Empty(CueSlot),      // muted `markers-cue-empty { $slot }`, non-interactive (FR-015)
}
```

## 4. `PanelIntent` (per-frame, applied after render)

Pushed by widgets during render. `apply_intents` drains the list in two passes: pass 1 applies `CommitRename`/`CancelRename`, and pass 2 applies the rest in emission order (research R2). If any intent was applied, it calls `ctx.request_repaint()`.

| Variant | Emitted by | Applied as | Spec |
|---|---|---|---|
| `NewLoopRegion` | "New loop region" | `controller.new_loop_region()`; result ignored exactly as today (FR-022: no behaviour change) | FR-017 |
| `Focus(MarkerId)` | any row control gaining focus | `waveform.focused_marker = Some(id)`; `controller.select_marker(id)` | R8 |
| `OpenRename(MarkerId)` | name-cell click/activation | `waveform.rename = Some((id, current_name))`; `rename_focus_pending = true` | FR-012 |
| `CommitRename(MarkerId, String)` | TextEdit Enter / `lost_focus` without Esc | `controller.rename_marker(id, &draft)`; `waveform.rename = None` | FR-012 |
| `CancelRename` | TextEdit Esc | `waveform.rename = None` | FR-012 |
| `Recolor(MarkerId, PaletteIndex)` | popover swatch pick | `controller.recolor_marker(id, index)` | FR-013 |
| `Jump(MarkerId)` | jump action | skipped if `marker_drag.marker == id`; otherwise `controller.seek_frames(position)` | FR-009 |
| `Nudge(MarkerId, i8)` | nudge−/nudge+ (`-1` / `+1`) | `controller.nudge_marker(id, dir, 1)` | FR-010 |
| `Remove(MarkerId)` | remove action | compute focus target from `visual_order()` (§5); `controller.delete_marker(id)`; clear `focused_marker`/`rename` if they pointed at `id` | FR-011 |
| `ClearRequest` / `ClearConfirm` / `ClearCancel` | footer | `clear_confirm = true` / `clear_all_markers()` + `clear_confirm = false` / `clear_confirm = false` | FR-019 |

A `Remove` of an id that no longer exists (for example an earlier intent in the same pass already removed the region) is a silent no-op: `delete_marker` returns `NotFound`, which is ignored as today.

## 5. `WaveformState` additions (session-only, `crates/modplayer-ui/src/waveform/state.rs`)

| Field | Type | Default | Lifecycle |
|---|---|---|---|
| `rename_focus_pending` | `bool` | `false` | Set by `OpenRename`. The rename `TextEdit` calls `request_focus()` once and then clears it (research R9). |
| `panel_focus` | `Option<PanelFocus>` | `None` | Set by `Remove`. Consumed on the next `panel` call: the target control calls `request_focus()` and the field is cleared. |

```text
pub enum PanelFocus {
    Row(MarkerId),     // focus that row's swatch (first tab stop); falls back per research R11 if gone
    NewLoopRegion,     // focus the "New loop region" button
}
```

Both fields are reset with the rest of `WaveformState` on track change (existing `last_track` path). Neither is persisted. `WaveformState` keeps `#[derive(Default, PartialEq, Eq, Clone, Debug)]`, and `PanelFocus` derives the same.

**Unchanged fields**: `focused_marker`, `rename`, `clear_confirm`, `marker_status` and `marker_drag` keep their 006 meaning. `rename`'s draft still lives in `(MarkerId, String)`.

## 6. Egui-memory state (not in `WaveformState`)

| State | Where | Notes |
|---|---|---|
| Palette popover open | `egui::Popup` memory under `Id::new(("markers-palette", MarkerId))` | At most one open (egui guarantee). Closes on click-outside, Esc or pick (research R7). |
| Rename-on-Enter ids | `ctx.data_mut` temp `Vec<Id>` under a fixed id, cleared each frame | Written by lane glyphs and name cells. Read by `handle_focused_marker_keys` to gate the `Enter` alias (research R8). |

## 7. Visual state per row type

| Row | Interactive | Tab stops | Hover fill | Text role |
|---|---|---|---|---|
| Group header | no | 0 | no | `section` label + `mono` count in `text_secondary` |
| Populated row | yes | 6 (swatch, name, jump, nudge−, nudge+, remove) | yes (016 row hover) | name `text_primary`, placeholder `text_secondary`, position `mono` |
| Loop cells | yes (as today) | arm, repeat, crossfade | no | unchanged |
| Empty cue slot | no | 0 | no | `text_secondary` |
| Footer | yes | 1 (Clear all) or 2 (Yes/No) | n/a | Destructive variant |
