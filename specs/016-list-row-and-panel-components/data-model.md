# Phase 1 Data Model: List Row, Tab Strip, and Panel Card Components

**Feature**: 016-list-row-and-panel-components | **Date**: 2026-09-23

The value set, the state shapes and the complete call-site map. Every
number here is either an existing token or a named derivation of one
(FR-024); there is no pixel literal in this document that a call site is
allowed to type.

---

## 1. Row geometry — the fixed three-column grid (FR-001–FR-003)

| Column | Width | Source | Content |
|---|---|---|---|
| Leading (artwork) | `ARTWORK_SIZE` = 40.0 | `rows.rs:43`, **unchanged** (FR-001) | texture / neutral square / initials |
| Middle (text) | remainder | computed | title line above secondary line, both truncating |
| Trailing (duration) | `theme::duration_measure(ctx)` | **new**, §2 | `mono` duration, right-aligned; empty for non-Track |
| Trailing (menu) | `ACTIONS_RESERVED_WIDTH` = 40.0 | `rows.rs:46`, **unchanged** | the "…" button |

Middle column width, replacing `rows.rs:570`:

```
text_width = (available_width - ACTIONS_RESERVED_WIDTH - duration_measure(ctx)).max(0.0)
```

**Invariant (FR-003)**: the duration column is subtracted for **every**
row kind, Track or not. An Album/Artist/Playlist row reserves it and draws
nothing into it. This is what puts the "…" menu at one x-position in a
mixed list.

**Row heights are unchanged**: `ROW_HEIGHT` 56.0 (Track),
`WIDE_ROW_HEIGHT` 72.0 (Album/Artist/Playlist) — `skeleton.rs:17,19`,
selected by `rows::row_height` (`rows.rs:230-235`). This feature changes
column structure, not vertical rhythm.

### 2. `duration_measure` — the one new measure

```rust
// theme/tokens.rs, beside body_measure (tokens.rs:160-165)
pub const DURATION_FIGURES: f32 = 7.0;   // "1:04:15" — FR-033's widest string

pub fn duration_measure(ctx: &egui::Context) -> f32 {
    DURATION_FIGURES * ctx.fonts_mut(|f| f.glyph_width(&mono_font_id(), '0'))
}
```

Mirrors `body_measure`'s `72.0 × body '0' advance` exactly (research R4).
Runtime, not `const`, so a future `text_scale()` change widens the column
with the figures.

### 3. `format_duration` — the rollover (FR-033)

Replacing `rows.rs:303-306`:

| Input | Output | Chars |
|---|---|---|
| 0 | `0:00` | 4 |
| 237 000 | `3:57` | 4 |
| 3 599 000 | `59:59` | 5 |
| **3 600 000** | **`1:00:00`** | **7** |
| 3 855 000 | `1:04:15` | 7 |
| 7 445 000 | `2:04:05` | 7 |

Rule: `< 3_600_000 ms` → `{m}:{ss:02}`; otherwise
`{h}:{mm:02}:{ss:02}`. Minutes and seconds are zero-padded in the hour
form; minutes are not in the `m:ss` form (unchanged from today).

`format_duration` is also made `pub` (`#[must_use]`), like `entity_key`:
Constitution VII's doc example for this rollover only runs under
`cargo test --doc` if rustdoc can see the item, and it is private today
(`rows.rs:303`). The table above is that example's body.

---

## 4. Row selection state (FR-006–FR-008, FR-012, FR-028–FR-032)

```rust
// rows.rs
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowSelection(Option<SelectedRow>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedRow {
    list: String,    // the virtualized_list id salt — §5
    key: String,     // rows::entity_key(entity) — today's private entity_salt
    index: usize,    // display index in that list's current order
}

impl RowSelection {
    pub fn is_selected(&self, list: &str, key: &str, index: usize) -> bool;
    pub fn select(&mut self, list: &str, key: &str, index: usize);
    pub fn clear(&mut self);
    /// FR-012. O(1): `key_at` is called at most once, at the stored index,
    /// and only when `list` matches the stored salt.
    pub fn reconcile(&mut self, list: &str, key_at: impl FnOnce(usize) -> Option<String>);
}
```

`entity_key` is today's `entity_salt` (`rows.rs:294-301`) made `pub` — it
already returns each entity's id `&str`.

### State transitions

| Event | Effect | Requirement |
|---|---|---|
| primary click on row *r* in list *L* at index *i* | `select(L, key(r), i)` — replaces any prior selection in this view | FR-006, FR-007 |
| primary click on a row in a **different** list of the same view | same — the prior selection is overwritten (one field) | FR-007, Clarification 2 |
| primary click in a **different view** | that view's own `RowSelection`; this one untouched | FR-007 |
| `reconcile` finds a different key (or `None`) at the stored index | `clear()` | FR-012 |
| Library active tab changes | `clear()` | FR-028 |
| detail view target changes | `clear()` | FR-028 |
| search query changes | `clear()` | FR-028 |
| "…" click / secondary click / `Shift+F10` | **no change** (research R3: the row never sees the click) | FR-029 |
| skeleton row drawn | **no change** — `list_row` is not called | FR-032, research R14 |
| app restart | selection is gone — never persisted | FR-028 |

**Exclusivity is structural**: one `RowSelection` field per view means "at
most one selected row per view" cannot be violated by a call site.

### 5. The list salt set (the `list` discriminator)

| View | Salt | Source |
|---|---|---|
| Library | `library-saved-tracks` | `library_view.rs:240` |
| Library | `library-saved-albums` | `:333` |
| Library | `library-followed-artists` | `:371` |
| Library | `library-playlists` | `:420` |
| Library | `library-recently-played` | `:254` |
| Search | one per `SearchKind` group | `search_view.rs::show_group` |
| Detail | `detail-tracks` | `detail_view.rs:88` |

These are the strings already passed to `rows::virtualized_list`
(`rows.rs:613`). Contract L2 asserts the set.

### 6. Where each view's selection lives

| View | Owner | Change |
|---|---|---|
| Library | `LibraryViewState` (`library_view.rs:78-89`) | **+ `pub selection: RowSelection`** beside `tab`/`pending_action` |
| Search | **new** `SearchViewState { selection, last_query: String }` | `App` gains the field; `search_view::show` gains the `&mut` parameter |
| Detail | **new** `DetailViewState { selection, last_target: Option<DetailTarget> }` | `App` gains the field; `detail_view::show` gains the `&mut` parameter |

`App` already owns `library_view` and `library_detail`
(`app.rs:95-100, 173-174`); the two new fields sit beside them.
`last_query`/`last_target` are what FR-028's clear-on-replacement compares
against, since neither view has frame-persistent state today.

---

## 7. Selected-row paint (FR-008, FR-031)

| Run | Unselected (today) | Selected |
|---|---|---|
| row fill | none / hover / pressed | `accent`, then hover/pressed blended **over** it |
| title | `text_primary` @ `body` | `text_on_accent` @ `body` |
| title (unavailable) | `.weak()` | `text_on_accent` @ `body` |
| secondary line | `.weak()` → `text_secondary` | `text_on_accent` @ `secondary` |
| duration | `.weak()` + `mono` | `text_on_accent` + `mono` |
| "E" badge | default `text_primary` | `text_on_accent` |
| availability reason | `.weak()` | `text_on_accent` |

Fill computation, replacing `rows.rs:544-554`'s base:

```
base = if selected { roles.accent } else { roles.surface_base }
fill = match () {
    _ if pressed => Some(base.blend(controls::pressed_fill(roles))),   //  8 %
    _ if hovered => Some(base.blend(controls::hover_fill(roles))),     //  4 %
    _ if selected => Some(base),
    _ => None,
}
```

`hover_fill`/`pressed_fill` are unchanged (`theme/controls.rs:82-95`).
**No new colour, alpha or role** — FR-024, and 014's ten-role ceiling holds.

The secondary line stays distinguishable by **size only** once colour is
equalised: `SIZE_BODY` 14.0 vs `SIZE_SECONDARY` 13.0 (`tokens.rs:104-105`),
already pinned by `body_and_secondary_are_visibly_different`
(`tokens.rs:346-350`). That is NFR-6.4 satisfied structurally.

### Accessibility (FR-011)

`rows.rs:524-527`'s existing `accesskit_node_builder` call gains
`b.set_selected(selected)`. Role (`ListItem`) and label
(`rows::accessible_name`) are **unchanged** — FR-025.

### Tooltip (FR-010)

```
row_response.on_hover_text(tr(match entity {
    RowEntity::Track(_) => "row-open-hint-track",
    _                   => "row-open-hint-entity",
}))
```

---

## 8. `list_row`'s new signature and `RowEvent` (research R1)

```rust
pub enum RowEvent { Action(RowAction), Open, Select }   // + Select

pub fn list_row(
    ui: &mut Ui,
    artwork: &mut ArtworkCache,
    entity: &RowEntity,
    selected: bool,          // new
) -> Option<RowEvent>
```

Precedence within one frame, in the order `list_row` returns:
`Action` (menu item chosen) → `Open`/`Action(PlayNow)` (double-click or
Enter) → `Select` (single click). A double-click's first click already
emitted `Select` on the previous frame; the Edge Case permits that.

**Every call site** (7): `library_view.rs:262, 294, 343, 381, 435`;
`search_view.rs` (`show_group`); `detail_view.rs:94`.

---

## 9. Tab strip (FR-013–FR-016, FR-034)

```rust
// theme/controls.rs, beside FOCUS_RING_WIDTH (controls.rs:103)
pub const TAB_UNDERLINE_WIDTH: f32 = 2.0;
pub fn tab_underline(roles: &Roles) -> Stroke;   // accent, TAB_UNDERLINE_WIDTH

// widgets/controls.rs
pub fn tab(ui: &mut Ui, selected: bool, label: &str) -> Response;
```

`tab` draws the label, strokes the underline along the widget rect's
bottom edge when `selected`, and sets on its accesskit node:

| Property | Value | Why |
|---|---|---|
| role | `Role::Tab` | FR-016 — unchanged from today |
| label | **exactly** `tr("library-tab-…")` | FR-016, FR-025 — no count suffix |
| `set_selected(bool)` | active tab | FR-034 |
| `set_toggled(Toggled::True/False)` | active tab | FR-034 — preserves what `selectable_label` gave for free (research R6) |

The count (FR-014, FR-015) is a **sibling** node beside the tab:
`ui.label(theme::mono_text(count.to_string()))`.

### Count source and visibility

| Tab | Count expression | `library_view.rs` already reads |
|---|---|---|
| Saved Tracks | `index.saved_tracks().len()` | `:232-236, 148` |
| Saved Albums | `index.saved_albums().len()` | `:318-321, 149` |
| Followed Artists | `index.followed_artists().len()` | `:365, 150` |
| Playlists | `index.playlists().len()` | `:403, 151` |
| Recently Played | `controller.recently_played().len()` | `:249` |

| `library_status().loading` | Count |
|---|---|
| `true` | **no count node at all** (FR-014, "not yet known") |
| `false`, list empty | **`0`, shown** (FR-014, Edge Case: a hidden count means "not yet known", never "none") |
| `false`, refreshing | shown, updating live as the sync grows the set |

`status.loading` is read at `library_view.rs:113` and the tab row at
`:115-125`, so the count is available where it is needed with no new
controller call.

---

## 10. Panel card (FR-017, FR-018, FR-021, FR-022, FR-035, FR-036)

```rust
// widgets/controls.rs
pub fn panel_card(ui: &mut Ui, header: &str, add_contents: impl FnOnce(&mut Ui));
```

| Property | Value | Token |
|---|---|---|
| fill | `roles.surface_raised` | existing role |
| padding | `theme::space::LG` (16.0), all four sides | `tokens.rs:132` |
| corner radius | `theme::radius::MD` (8) | `tokens.rs:143` |
| stroke | **none** | FR-017 — § 5.4 names none |
| header | `theme::section_label(header)` | `tokens.rs:154-158` |
| header accessible name | the exact **un-uppercased** `header` | FR-018, mirrors `markers.rs:545-548` |

### The four panels

| Panel | Function | Header string | Header change |
|---|---|---|---|
| Markers | `markers::panel` (`markers.rs:530`) | `tr("markers-panel")` | **move** — delete `markers.rs:544-548` from its horizontal row |
| Effect Chain | `effects_view::show` (`effects_view.rs:71`) | `tr("effects-panel-title")` | **move** — delete `effects_view.rs:99-103` from `show_header`'s row |
| Transport | `transport_view::show` (`:42`) | `tr("transport-panel-title")` | **convert** — delete `ui.heading()` at `:48` (FR-018's stated correction) |
| Queue | `queue_view::show` (`:21`) | `tr("queue-panel-title")` | **add** — new Fluent key |

**The double-header trap** (research R7): Markers and Effect Chain already
draw a `section_label` header *inside their own first `ui.horizontal`*,
beside other controls. Wrapping them without deleting those lines renders
the header twice. Each panel's horizontal row keeps its remaining controls
(Markers: "New loop" + clear-all; Effects: CPU %, overloads, over-budget
badge).

**FR-035**: no collapse control is added to any card header. The three
control-row switches (`now_playing.rs:145-159`) and the `Q`/`E`/`T`
shortcuts remain the only affordance; a collapsed panel draws nothing
(`now_playing.rs:174, 194, 207` keep their `if open` guards).

**FR-021**: Markers gets the card but no toggle — it has none today.

### FR-023 — the re-derived reserved height

`now_playing.rs:39`'s `EFFECTS_PANEL_RESERVED_HEIGHT: f32 = 140.0`
becomes a function summing named tokens and measured heights for what sits
below the Effect Chain panel, **plus the new card insets**:

| Element below the panel | Contribution | Site |
|---|---|---|
| gap before Transport | `space::XL` | `:195` |
| Transport card inset (when open) | `2 × space::LG` | new |
| master volume row | measured | `:199` |
| peak meter | measured | `:205` |
| gap before Queue | `space::XL` | `:208` |
| Queue card inset (when open) | `2 × space::LG` | new |

No pixel literal survives (FR-023, FR-024).

---

## 11. Persisted panel state (FR-019, FR-020, FR-037)

### `settings.toml`

```toml
[now_playing_panels]
effect_chain_open = false
transport_open    = false
queue_open        = false
```

Optional table; absent → all `false` → every panel closed, which is
`unwrap_or(false)` at `now_playing.rs:94-104` today. `SCHEMA_VERSION`
stays `1` (`settings/model.rs:69`) — `[onboarding]`'s precedent
(`:300-301, 320, 595, 753`).

This is new serialized state, so Constitution VIII requires a
property-based test over it, not only the example-based P4/P6 cases: a
`proptest!` round-trip of an arbitrary `(effect_chain_open,
transport_open, queue_open)` triple in
`crates/modplayer-core/tests/settings.rs`, beside
`plugin_panels_round_trip_proptest` (`:682-710`). Contract P9.

### Core model (`crates/modplayer-core/src/settings/model.rs`)

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NowPlayingPanels {
    pub effect_chain_open: bool,
    pub transport_open: bool,
    pub queue_open: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RawNowPlayingPanels {         // beside RawOnboarding (:494-500)
    #[serde(default)] pub effect_chain_open: bool,
    #[serde(default)] pub transport_open: bool,
    #[serde(default)] pub queue_open: bool,
}
```

`AudioSettings` gains `pub now_playing_panels: NowPlayingPanels` beside
`getting_started_dismissed` (`:135`); `RawSettings` gains
`#[serde(default)] pub now_playing_panels: RawNowPlayingPanels`.

### Controller surface (`controller.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NowPlayingPanel { EffectChain, Transport, Queue }

impl PlaybackController<B, H> {
    #[must_use] pub fn now_playing_panel_open(&self, panel: NowPlayingPanel) -> bool;
    pub fn set_now_playing_panel_open(&mut self, panel: NowPlayingPanel, open: bool);
}
```

One pair with an enum, not three pairs (research R10). Cached field loaded
at construction, mirroring `getting_started_dismissed`
(`controller.rs:2463-2464`); the setter calls `persist_settings` on
**every** toggle, mirroring `set_focus_policy` (`:2455-2458`).
`persist_settings` stays private.

### The three deletions (FR-019)

| Deleted | Site |
|---|---|
| `now_playing::queue_panel_open_id` | `now_playing.rs:44-46` |
| `effects_view::panel_open_id` | `effects_view.rs:36-38` |
| `transport_view::panel_open_id` | `transport_view.rs:22-24` |

…and the six `ui.memory` reads/writes at `now_playing.rs:93-104, 161-163`.

### Toggle signature change (research R9)

| Function | Was | Becomes |
|---|---|---|
| `now_playing::toggle_queue_panel` | `(&egui::Context)` | `(&mut PlaybackController<B, H>)` |
| `effects_view::toggle_effect_chain_panel` | `(&egui::Context)` | `(&mut PlaybackController<B, H>)` |
| `transport_view::toggle_transport_panel` | `(&egui::Context)` | `(&mut PlaybackController<B, H>)` |

Dispatcher arms `actions.rs:518, 531, 532`. **`invoke`'s own signature is
unchanged** — it already holds `controller: &mut PlaybackController<B, H>`.

### FR-037 — the two migrated test helpers

`tests/now_playing.rs` and `tests/effects_view.rs` seed panel state by
writing the egui memory id. Both migrate to seeding
`[now_playing_panels]` through the controller. This is the one explicit
relaxation of FR-025.

---

## 12. New Fluent keys (3, all `locales/en-US`)

| Key | File | Text (en-US) | Requirement |
|---|---|---|---|
| `row-open-hint-track` | `library.ftl` | a second click or Enter plays this track | FR-010 |
| `row-open-hint-entity` | `library.ftl` | a second click or Enter opens this | FR-010 |
| `queue-panel-title` | `playback.ftl` | Queue | FR-018 |

All three added to `tests/fluent_keys.rs`'s inventory, which already
asserts the catalogues define no key the test does not exercise.

---

## 13. Complete file map

`+` new, `~` modified, `=` unchanged but load-bearing as a gate.

```
crates/modplayer-core/
  src/settings/model.rs        ~ NowPlayingPanels, RawNowPlayingPanels, AudioSettings field
  src/controller.rs            ~ NowPlayingPanel, accessor/setter pair, cached fields
  tests/settings.rs            ~ P9 — [now_playing_panels] proptest round-trip (Const. VIII)

crates/modplayer-ui/
  src/theme/tokens.rs          ~ DURATION_FIGURES, duration_measure
  src/theme/controls.rs        ~ TAB_UNDERLINE_WIDTH, tab_underline
  src/widgets/controls.rs      ~ tab(), panel_card()
  src/rows.rs                  ~ RowSelection, entity_key pub, RowEvent::Select,
                                 selected param, 3-column grid, format_duration
                                 (rollover + made pub, with runnable doc examples
                                 on it and RowSelection), set_selected, tooltip
  src/library_view.rs          ~ tab strip + counts, selection field, clear-on-tab-change
  src/search_view.rs           ~ SearchViewState, selection across 4 groups, clear-on-query
  src/detail_view.rs           ~ DetailViewState, selection, clear-on-target-change
  src/app.rs                   ~ owns the two new view states, passes them down
  src/now_playing.rs           ~ controller-backed panel flags, re-derived reserved height
  src/effects_view.rs          ~ panel_card, header moved, toggle signature
  src/transport_view.rs        ~ panel_card, heading → section_label, toggle signature
  src/queue_view.rs            ~ panel_card, new header
  src/markers.rs               ~ panel_card, header moved
  src/actions.rs               ~ 3 dispatcher arms pass controller

  tests/rows.rs                ~ L1, L3–L5, L7 (row grid, duration, selection, menu)
  tests/library_view.rs        ~ T1–T5 (underline, counts, Tab names verbatim)
  tests/search_view.rs         ~ selection across the four groups
  tests/now_playing.rs         ~ FR-037 seed migration + card assertions
  tests/effects_view.rs        ~ FR-037 seed migration
  tests/accessibility.rs       = must pass unmodified (FR-025, SC-009)
  tests/design_token_literals.rs = must still report 0 (SC-008)
  tests/design_token_contrast.rs ~ SC-011 selected-row runs
  tests/fluent_keys.rs         ~ 3 new keys

locales/en-US/library.ftl      ~ 2 keys
locales/en-US/playback.ftl     ~ 1 key
```

**Not touched**: `crates/modplayer-engine`, `crates/modplayer-effects`,
`crates/modplayer-capability-gateway`, `crates/modplayer-plugin-runtime`,
`crates/modplayer-audio-source*`, `plugins/`, `shell.rs`,
`settings/mod.rs` (Scope boundary), `docs/plugin-api/`.
