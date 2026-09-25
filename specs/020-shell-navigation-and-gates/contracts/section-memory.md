# Contract: Section Memory (sub-view + scroll retention)

**Covers**: FR-007 · SC-004 · US3 AS2–AS4
**Module**: `crates/modplayer-ui/src/section_memory.rs` (new); `rows.rs` (new
`virtualized_list_in`), `library_view.rs`, `detail_view.rs`, `settings/mod.rs` (header/content
split), `app.rs` (wiring + sign-out resets)
**Tests**: `crates/modplayer-ui/tests/section_memory.rs` (new)

## Public surface

```rust
pub enum LibraryViewKey { Tab(LibraryTab), Detail(DetailTarget) }
pub enum ViewKey { Library(LibraryViewKey), Search, Settings(SettingsCategory), Plugins }

#[derive(Default)]
pub struct SectionMemory { /* epoch, offsets, shown_last_frame */ }
impl SectionMemory {
    pub fn scroll_area(&mut self, key: &ViewKey) -> egui::ScrollArea;
    pub fn record(&mut self, key: ViewKey, offset_y: f32);
    pub fn end_frame(&mut self, drawn: Option<&ViewKey>);
    pub fn offset(&self, key: &ViewKey) -> Option<f32>;
    pub fn epoch(&self) -> u64;
    pub fn reset(&mut self);
}

// rows.rs — existing `virtualized_list` becomes a wrapper around this
pub fn virtualized_list_in(ui: &mut Ui, scroll: ScrollArea, row_height: f32, count: usize,
    draw_row: impl FnMut(&mut Ui, usize)) -> (Range<usize>, f32 /* offset.y */);

// settings/mod.rs — `show` keeps its signature; internally:
fn show_header(..) /* search box + results + category row */;
fn show_content(..) /* selected category body, inside memory.scroll_area(Settings(cat)) */;
```

`settings::show` gains a `&mut SectionMemory` parameter (or `App` passes a pre-built
`ScrollArea` and receives the offset) — the task phase picks the lighter of the two; the
clauses below are what must hold.

## Attachment points

| Section | Scrollable view | Key |
|---|---|---|
| Library (tab) | active tab's `virtualized_list` | `Library(Tab(tab))` |
| Library (detail) | detail track list `virtualized_list` | `Library(Detail(target))` |
| Search | whole section body (group lists keep bounded `max_height`) | `Search` |
| Settings | selected category content (search box + row stay fixed above) | `Settings(category)` |
| Plugins | whole section body | `Plugins` |
| Now Playing | — none (fixed layout, 018); inner effect-chain area salt gains `epoch` | — |

## Clauses

| # | Clause | Verified by |
|---|---|---|
| M1 | Scroll view K to offset y, draw a frame of another section, draw K again: K's offset after the return frame is within ±1 px of y. Holds for 2 consecutive round trips. For each key in the table. | `section_memory.rs` (headless, `ScrollArea` wheel/`scroll_to` input or `record` then draw) |
| M2 | Returning to Library restores the same tab / open detail target, Search the same query and results, Settings the same category (sub-view retention) — in addition to M1. | `section_memory.rs` |
| M3 | If content shrank while away (e.g. library list 200 → 20 rows), the restored offset == `max(0, content − viewport)` (egui clamp), never beyond. | `section_memory.rs` |
| M4 | `reset()` → every `offset(key) == None`, `epoch` incremented; the next draw of any key starts at 0. `App` calls it, and resets `shell`, `library_view`, `library_detail`, `search_view`, `settings.category`, on `AccountEvent::SignedOut` and `SessionRevoked`. | unit + `section_memory.rs` (a test-visible `App`-free helper `reset_session_ui(..)` that `App` calls, so it is testable) |
| M5 | Switching Library *tabs* within the Library section is not a round trip: behaviour unchanged (each tab has its own key; the list starts where that tab was last left). | `section_memory.rs` |
| M6 | Nothing from `SectionMemory` is serialized; eframe `persistence` stays disabled. | code review (no `serde` derive on these types) |
| M7 | No section frame takes longer due to memory: operations are O(1) hash lookups per frame. | code review |
