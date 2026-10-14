# Data Model: Search Results Structure and Feedback

**Feature**: 026-search-results-structure | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Nothing is persisted. §1–§2 are changes to the pure core state in `crates/modplayer-core/src/search.rs` (extends 004 data-model §2.1); §3–§5 are UI-local value types/state in `crates/modplayer-ui/src/search_view.rs` (plus one new module, `search_layout.rs`).

---

## 1. `GroupState` (core, changed)

| Variant | Change | Fields |
|---|---|---|
| `Idle`, `Pending`, `Empty`, `Unsupported` | unchanged | — |
| `Loaded` | unchanged | `items: Vec<SearchHit>`, `next_offset: Option<u32>`, `loading_more: bool` |
| `RateLimited` | **+ `resume_offset`** | `stale: Option<Vec<SearchHit>>` — rows of the **current** generation kept on screen; `resume_offset: Option<u32>` — the "Show more" offset that was rate-limited (`None` for a combined-request rate limit) |

`GroupState::items()` unchanged (returns `Loaded.items` or `RateLimited.stale`). **Header count = `items().len()`** (FR-004, SC-006).

**Validation rules**
- V1: `RateLimited { stale: Some(v) }` ⇒ `!v.is_empty()` and `v` came from this same generation (only `show_more`'s rate-limit produces `Some`).
- V2: `RateLimited { stale: None }` ⇒ `resume_offset == None` (combined request rate-limited; nothing on screen).
- V3: A group is **shown** iff `Pending`, `Loaded`, or `RateLimited { stale: Some }` (FR-002). `Idle`/`Empty`/`Unsupported`/`RateLimited { stale: None }` are omitted.

## 2. `SearchSession` (core, extended)

New private fields:

| Field | Type | Meaning |
|---|---|---|
| `combined_retry` | `Option<Retry>` | Scheduled/issued retry of the combined 4-kind request for `generation` |
| `show_more_retries` | `[Option<Retry>; 4]` | Scheduled/issued retry of a rate-limited "Show more" page, per kind |

```text
Retry { due: Instant, attempt: u8, issued: bool }
```

- `due = now + backoff_delay(attempt, retry_after_ms)` (shared `crate::backoff`, research R9).
- `attempt` counts consecutive rate limits of that target within the generation (0-based, saturating); drives exponential growth, capped at `SYNC_BACKOFF_MAX`.
- `issued = true` once `tick` has sent the retry command and its reply is outstanding.

New read-only accessors (FR-013 permits):

| Accessor | Returns |
|---|---|
| `in_flight() -> bool` | `debounce_deadline.is_some() \|\| (pending_request_id.is_some() && combined_retry.is_none())` — spinner predicate (FR-009): true while debouncing or while the *first* combined request is outstanding; false while a retry is waiting or issued |
| `generation() -> u64` | Current generation (for once-per-query announcement) |
| `retry_scheduled() -> bool` | Any `combined_retry`/`show_more_retries` is `Some` (tests + status-strip invariant S5) |

Changed signature: `apply_reply(&mut self, request_id, result, now: Instant)` (controller passes `self.now()`).

### State transitions (additions to 004's machine, per group *g* / per session)

| From | Event | To | Side effect |
|---|---|---|---|
| any `Pending` (combined outstanding) | combined reply `Err(RateLimited{ra})` | all `Pending` → `RateLimited { stale: None, resume_offset: None }` | `combined_retry = Retry{due: now+delay(0,ra), attempt:0, issued:false}` |
| `Loaded { items, next_offset: Some(o), loading_more: true }` | show-more reply `Err(RateLimited{ra})` | `RateLimited { stale: Some(items), resume_offset: Some(o) }` | `show_more_retries[g] = Retry{attempt: prev+1 or 0, …}` |
| any, `combined_retry{issued:false}` | `tick(now ≥ due)`, online | unchanged (groups stay `RateLimited`) | emit `SearchCatalog{kinds: all, offset:0}` with new request id, same generation; `pending_request_id = Some(id)`; `issued = true` |
| `RateLimited{stale:Some, resume_offset:Some(o)}`, retry not issued | `tick(now ≥ due)`, online | unchanged | emit `SearchCatalog{kinds:[g], offset:o}`; `show_more_request_ids[g] = Some(id)`; `issued = true` |
| `RateLimited` (combined retry issued) | reply `Ok(page)` | per 004 `apply_page` (Loaded/Empty/Unsupported) | `combined_retry = None` |
| `RateLimited` (combined retry issued) | reply `Err(RateLimited{ra})` | unchanged | `combined_retry = Retry{due: now+delay(attempt+1,ra), attempt+1, issued:false}` |
| `RateLimited` (combined retry issued) | reply other `Err` | → `Empty` | `combined_retry = None` (004 FR-018) |
| `RateLimited{stale:Some(s), resume_offset:Some(o)}` (show-more retry issued) | reply `Ok(page)` | `Loaded { items: s ++ page.items, next_offset: page.next_offset, loading_more: false }` | `show_more_retries[g] = None` |
| same | reply `Err(RateLimited{ra})` | unchanged | reschedule with `attempt+1` |
| same | reply other `Err` | `Loaded { items: s, next_offset: Some(o), loading_more: false }` | retry cleared (matches today's non-rate-limit show-more failure) |
| any | `set_query` → trim-empty (`reset_to_idle`) / Escape / clear | all `Idle` | all retries cleared, generation bump (existing poisoning) |
| any | `set_query` → changed non-empty effective query (debounce armed) | groups unchanged until the debounce fires (004 behaviour) | all retries cleared immediately (FR-011a "a query edit … MUST cancel"); the strip therefore disappears and the spinner shows |
| any | debounce fires (new generation) | all `Pending` | all retries cleared |
| any | `set_offline(true)` | `RateLimited{stale:Some(s),resume_offset:Some(o)}` → `Loaded{items:s,next_offset:Some(o),loading_more:false}`; `RateLimited{stale:None}` (combined) → `Idle` with `debounce_deadline = Some(combined_retry.due)` | all retries cleared (FR-011a); the re-armed deadline re-runs the query on reconnect (004 offline rule), so no false "refreshing" or "no results" claim (research R9) |

## 3. `ResultsLayout` (UI, new — `crates/modplayer-ui/src/search_layout.rs`)

Pure geometry, no `Ui`; built each frame from the session snapshot.

```text
GroupBlock  { kind: SearchKind, header_h: f32, row_h: f32, rows: usize,
              skeleton: bool,            // Pending → 3 skeleton rows
              footer_h: Option<f32> }    // "Show more" when next_offset.is_some()
PlacedGroup { kind, header_top: f32, rows_top: f32, footer_top: Option<f32>,
              content_end: f32 }         // content_end = bottom of last row/footer
ResultsLayout { groups: Vec<PlacedGroup>, total_height: f32 }
```

- Built by `ResultsLayout::new(blocks: &[GroupBlock], gap: f32)`; groups in `GROUP_ORDER`, only shown groups (V3), separated by `gap = space::LG`.
- `visible_rows(&self, g, viewport_top, viewport_bottom) -> Range<usize>` — rows intersecting `[top, bottom)` extended by one row either side, clamped to `0..rows` (FR-001).
- `pinned_header(&self, viewport_top) -> Option<(usize, f32)>` — group index and draw-y per research R3.

**Invariants** (property-tested, contract [results-layout.md](./contracts/results-layout.md)):
- L1 `total_height = Σ(header_h + rows·row_h + footer_h?) + gap·(n−1)`; monotonic `header_top`.
- L2 `visible_rows` never returns more than `ceil(viewport_h/row_h) + 3` rows.
- L3 at most one pinned header; its rect never intersects the next group's `header_top` (`y + header_h ≤ next.header_top`).
- L4 pinned header exists iff some group has `header_top < viewport_top < content_end`.

## 4. `SearchViewState` (UI, extended)

| Field | Status | Meaning |
|---|---|---|
| `selection: RowSelection` | existing | unchanged |
| `last_query: String` | existing | query-change detection (also triggers R5 scroll reset) |
| `announced_generation: Option<u64>` | **new** | generation whose settled count was already announced (`Live::Polite` once) |
| `refocus_field: bool` | **new** | set by any clear action; consumed next frame by `request_focus` on the field id |

`reset_session_ui` (app.rs) already resets `SearchViewState` to `Default` on sign-out — new fields default to `None`/`false`.

## 5. Derived view status (UI, pure fn)

```text
enum SearchStatus {
  Idle,                                // query empty
  Offline,                             // session.offline()
  InFlight,                            // session.in_flight()
  NoResults { shown_query: Cow<str> }, // is_no_results(); truncate_query(…)
  RateLimited { stale: bool },         // refreshing(); stale = any shown RateLimited{Some}
  Settled { count: usize },            // settled_count() is Some
}
```

`fn view_status(&SearchSession) -> SearchStatus` — precedence Offline > Idle > InFlight > NoResults > RateLimited > Settled. `RateLimited` requires `refreshing() && retry_scheduled()` (the strip never claims a refresh that is not scheduled). A retry never shows the spinner because `in_flight()` is false while `combined_retry` is `Some` (FR-009); a new query edit cancels retries and arms the debounce, so the spinner replaces the strip (edge case "query changes while a retry is pending"). Decides which of spinner / count line / status strip / empty state render; each is exclusive except that `RateLimited{stale:true}` still draws groups. `truncate_query` per research R11.
