# Contract: Results Layout — one scroll area, pinned headers (US1)

**Covers**: FR-001–FR-006, SC-001, SC-002, SC-006 | **Data**: [data-model.md §1, §3](../data-model.md) | **Research**: R1–R5

UI contract for `search_view::show` (`crates/modplayer-ui/src/search_view.rs`) and the pure `search_layout` module (`crates/modplayer-ui/src/search_layout.rs`). Test homes: `crates/modplayer-ui/tests/search_view.rs` (headless `Context::run_ui` + accesskit tree), unit/proptest in `search_layout.rs`.

## Structure

```text
Search view (Ui given by app.rs, NOT wrapped in a ScrollArea any more)
├── field row          [fixed]  TextEdit · Spinner? · Clear(×)?      → search-feedback.md
├── count line?        [fixed]  "<n> results" (Role::Status)          → search-feedback.md
├── status strip?      [fixed]  ⚠ stale / rate-limited (Role::Status) → search-status.md
└── results ScrollArea [the ONLY ScrollArea; SectionMemory ViewKey::Search]
    ├── Tracks    header · rows… · Show more?
    ├── Albums    header · rows… · Show more?
    ├── Artists   header · rows… · Show more?
    └── Playlists header · rows… · Show more?
    (+ at most one pinned header painted last at the viewport top)
```

Offline / no-results replace the scroll area's content with their message (no groups).

## Rules

| ID | Rule | Verification |
|---|---|---|
| RL1 | Exactly **one** `ScrollArea` is created by the Search view per frame; `app.rs` no longer wraps `search_view::show` in one; `GROUP_VISIBLE_ROWS` and the per-group `virtualized_list` calls are gone. | Test: render with 4 loaded groups; count scroll-area states in `ctx.memory` / assert only the `("section-scroll", ViewKey::Search, epoch)` id holds scroll state. grep test: `GROUP_VISIBLE_ROWS` absent. |
| RL2 | The field row, count line and status strip are laid out **above** the scroll area and do not move when it scrolls. | Test: scroll results by 2 000 pt (injected `MouseWheel`); field node's bounds unchanged. |
| RL3 | Group order is Tracks, Albums, Artists, Playlists; omitted groups (`Idle`/`Empty`/`Unsupported`/`RateLimited{stale:None}`) contribute no header, rows, footer or gap. | Test: Albums `Empty`, Artists `Unsupported` → headers found are exactly [Tracks, Playlists] in y order. |
| RL4 | Only rows intersecting the viewport ±1 are laid out; the content height equals `ResultsLayout.total_height`. | Test: Tracks with 400 rows → < 20 `list_row` nodes in tree; scroll area content size == layout total. Unit: L1/L2. |
| RL5 | At 960 × 640 with all four groups loaded (20 rows each), scrolling the results area alone brings the Playlists header into view (SC-001). | Test at `screen_rect` 960 × 640 (minus nav/transport per 021 layout): wheel-scroll to max → Playlists header visible. Manual M1. |
| RL6 | Pinned header: when group *g*'s in-flow header is above the viewport top and the viewport top is still inside *g*'s content, *g*'s header is painted at `y = min(viewport_top, g.content_end − header_h)` over the rows; at most one is pinned; it never overlaps the next group's header (SC-002). | Proptest P1–P4 below; UI test: scroll into the middle of Tracks → Tracks `Role::Header` node's rect top == scroll viewport top. |
| RL7 | While pinned, the in-flow copy of that header is not drawn — the tree has exactly one `Role::Header` per shown group. Rows under the pinned header receive no clicks. | Test: count headers == shown groups while pinned; click at pinned header rect → no `RowEvent::Select`. |
| RL8 | Header text = `section_label(group name)` + count in `text_secondary`; accessible label `search-group-header` ("Tracks, 20 results"); `Pending` → group-name-only label, no count. Count = `GroupState::items().len()` (SC-006). | Test: Loaded 20 → label "Tracks, 20 results"; after Show more reply +20 → "Tracks, 40 results"; `RateLimited{stale:Some(20)}` → 20; `Pending` → "Tracks". |
| RL9 | "Show more" (existing `search-show-more` label, disabled while `loading_more`) is the last element of its group, directly after the last row. | Test: Show more button's rect top ≥ last row's bottom and < next header top. |
| RL10 | Single-column at every width ≥ 960 px (no columns). | Test at 960 and 1 920 widths: all headers share the same left x. |
| RL11 | The scroll offset resets to 0 on the frame the effective query changes; it is **not** reset by Show more, rate-limit or retry transitions. | Test: scroll to 800, change query → offset 0; scroll to 800, Show more reply → offset 800. |
| RL12 | Section memory still works: leaving Search and returning restores the offset (020 contract M3); sign-out resets it (M4). | Existing `tests/section_memory.rs` cases for `ViewKey::Search` keep passing unchanged. |

## Pure layout properties (`search_layout.rs`, proptest)

Generated input: 0–4 groups, `rows ∈ 0..=500`, `row_h ∈ {56, 72}`, `header_h ∈ 16..40`, footer optional, viewport top in `0..total`, viewport height `100..1 000`.

- **P1** `header_top`s strictly increase; `total_height` equals the L1 sum.
- **P2** `visible_rows(g, top, bottom)` ⊆ `0..rows`, covers every row intersecting `[top,bottom)`, and has length ≤ `ceil(h/row_h)+3`.
- **P3** `pinned_header(top)` returns `Some(g)` iff `header_top(g) < top < content_end(g)`; never two.
- **P4** if pinned at `y`, `y ≤ top`, `y ≥ header_top(g)`, and `y + header_h ≤ header_top(g+1)` when a next group exists.
