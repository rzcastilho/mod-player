# Data Model: Shell Navigation and Launch Gates

**Feature**: `020-shell-navigation-and-gates` | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

All entities are **UI-only, in-memory** types in `crates/modplayer-ui`. Nothing is persisted:
no `settings.toml`/`account.toml` key is added, no `schema_version` change. `modplayer-core`
and `modplayer-account` types are consumed unchanged (`LaunchStep`, `next_step`,
`SettingsCategory::ALL`, `LibraryIndex`).

---

## 1. `GateStep` (spec entity "Launch Gate Step") — `shell.rs`

| Variant | `position()` | `label_key()` | From `LaunchStep` |
|---|---|---|---|
| `Welcome` | 1 | `gate-step-welcome` | `Welcome` |
| `SignIn` | 2 | `gate-step-sign-in` | `SignIn` |
| `AudioOutputCheck` | 3 | `gate-step-audio-output-check` | `DeviceCheck` |

- `pub const ALL: [GateStep; 3]` (display order); `pub const GATE_STEP_TOTAL: u8 = 3`.
- `fn from_launch_step(step: LaunchStep) -> Option<GateStep>` — `Main → None`.
- `fn state_relative_to(self, current: GateStep) -> StepState` with
  `StepState = Complete | Current | Upcoming` (by `position()` comparison).

**Validation rules**: total is constant 3 (FR-002, Clarification 4). `position` depends only
on `LaunchStep` (FR-003) — no retry counter, no sub-view input.

## 2. `Chrome` (per-frame shell decision) — `shell.rs`

```text
Chrome { rail: bool, gate: Option<GateStep> }
Chrome::for_frame(step: LaunchStep, device_check_open: bool) -> Chrome
Chrome::navigation_enabled(self) -> bool   // == self.rail
```

Truth table: research.md R1. Invariants: `rail ⇒ gate.is_none()`; `gate.is_some() ⇔ step != Main`;
`rail ⇔ step == Main && !device_check_open` (FR-001, FR-001a). `navigation_enabled` gates
`actions::dispatch` (FR-001b).

## 3. `Shell` (spec entity "Navigation Rail") — `shell.rs`, existing

| Field | Type | Change |
|---|---|---|
| `section` | `Section` | unchanged; now also **reset to `Library`** on sign-out/revocation |
| `focus_search_requested` | `bool` | unchanged; reset on sign-out |

Visibility is **not** a field — it is `Chrome.rail`, recomputed every frame (a stored flag
could go stale on sign-out, Clarification 1). Rail item rendering moves to
`widgets::controls::nav_item` (contracts/shell-chrome.md C5–C8).

## 4. `SectionMemory` (spec entity "Rail Section" state) — new `section_memory.rs`

```text
SectionMemory {
    epoch: u64,                         // bumped by reset(); part of every section scroll id salt
    offsets: HashMap<ViewKey, f32>,     // last observed vertical offset per scrollable view
    shown_last_frame: Option<ViewKey>,  // which view's area was drawn last frame
}

ViewKey =
  | Library(LibraryViewKey)             // LibraryViewKey = Tab(LibraryTab) | Detail(DetailTarget)
  | Search
  | Settings(SettingsCategory)
  | Plugins
```

Operations (contract: contracts/section-memory.md):
- `scroll_area(&mut self, key: ViewKey) -> egui::ScrollArea` — vertical, `auto_shrink([false, false])`
  (Library lists keep their existing `[false, true]`), `id_salt(("section-scroll", &key, epoch))`,
  plus `.vertical_scroll_offset(stored)` iff `shown_last_frame != Some(key)` and `stored` exists.
- `record(&mut self, key: ViewKey, offset_y: f32)` — stores `offset_y`, sets `shown_last_frame`.
- `end_frame(&mut self, drawn: Option<ViewKey>)` — clears `shown_last_frame` when no section
  view drew this frame (gate / preview), so the next Main frame restores.
- `offset(&self, key) -> Option<f32>` — test/diagnostic read.
- `reset(&mut self)` — `offsets.clear()`, `shown_last_frame = None`, `epoch += 1`.

**State transitions**:

```text
          show(key) + record            leave section (other key drawn)
 (none) ─────────────────────▶ Live(key) ─────────────────────────▶ Stored(key, y)
                                  ▲                                     │ show(key):
                                  └──────── restore y (clamped) ────────┘ apply once
 any ── reset() on SignedOut / SessionRevoked ──▶ (none), epoch+1
```

**Validation rules**: restored offset within ±1 logical px of stored (SC-004), clamped to
`max(0, content_height − viewport_height)` by egui on show; memory never written to disk;
cleared on sign-out (FR-007).

Sub-view retention (library tab/detail, search query/results, settings category) stays in the
existing owners (`LibraryViewState.tab`, `App.library_detail`, `controller.search()`,
`SettingsScreen.category`); `App::handle_account_event`'s `SignedOut`/`SessionRevoked` arms now
also reset `library_view`, `library_detail`, `search_view`, `shell`, `settings.category`
(US3-AS4).

## 5. `RowPartition` and `CategoryRowState` (spec entities "Settings Category", "Overflow Control") — new `settings/category_row.rs`

```text
RowPartition {
    visible: Vec<usize>,     // canonical-order prefix of SettingsCategory::ALL indices
    pinned: Option<usize>,   // selected index, drawn after `visible`, before More
    overflow: Vec<usize>,    // remaining indices, canonical order; empty ⇒ no More control
}

CategoryRowState {           // lives in SettingsScreen as `row: CategoryRowState`
    last_partition: Option<RowPartition>,
    menu_open: bool,         // mirrors the egui Popup's open memory for change detection
    focus_after_close: Option<FocusTarget>,  // More | Selected — applied next frame
}
```

`partition(widths, selected, more_width, gap, available) -> RowPartition` — algorithm and
invariants: research.md R6, contracts/settings-category-row.md R1–R6.

**Validation rules**: `visible ∪ pinned ∪ overflow = 0..11`, pairwise disjoint;
`selected ∉ overflow`; one horizontal line (never `horizontal_wrapped`); no label truncation.

## 6. Library tab count (spec entity "Library Tab") — `library_view.rs`, existing, unchanged

Count = `LibraryIndex` list length (or `recently_played().len()`), drawn as a sibling `mono`
label only when `!library_status().loading`; not part of the tab's accessible name (016
FR-014/FR-015). Regression coverage only (FR-013/FR-014).

## 7. Fluent keys (new)

| Key | File | en-US |
|---|---|---|
| `gate-step-welcome` | `app.ftl` | Welcome |
| `gate-step-sign-in` | `app.ftl` | Sign in |
| `gate-step-audio-output-check` | `app.ftl` | Audio output check |
| `gate-step-progress` | `app.ftl` | Step { $current } of { $total }: { $label } |
| `settings-more` | `settings.ftl` | More |
| `settings-more-a11y` | `settings.ftl` | More settings categories |

pt-BR drafts: contracts/fluent-strings.md.
