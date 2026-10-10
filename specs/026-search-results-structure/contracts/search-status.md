# Contract: Stale / Rate-Limited Status, Automatic Retry, Empty State (US3)

**Covers**: FR-011, FR-011a, FR-012, FR-013, SC-004, SC-005 | **Data**: [data-model.md §1, §2, §5](../data-model.md) | **Research**: R9–R12

Two layers: the pure core (`SearchSession` in `crates/modplayer-core/src/search.rs` + shared `crates/modplayer-core/src/backoff.rs`) and the view (`search_view.rs`). Core tests are unit tests with an injected `Instant`; view tests drive `ScriptedHost` replies through `PlaybackController` as `tests/search_view.rs` already does.

## Core: retry state machine (`SearchSession`)

| ID | Rule | Verification |
|---|---|---|
| R1 | A combined request answered `RateLimited{retry_after_ms}` → all `Pending` groups `RateLimited{stale:None, resume_offset:None}` and a combined retry is scheduled at `now + backoff_delay(0, retry_after_ms)`. | Unit. |
| R2 | A Show-more page answered `RateLimited` → that group `RateLimited{stale:Some(items), resume_offset:Some(offset)}`; other groups untouched; retry scheduled for that kind. | Unit (extends existing show-more rate-limit test). |
| R3 | `tick(now)` before `due` emits nothing; at/after `due`, online, emits exactly one `SearchCatalog` with the same query, same generation, the right kinds/offset (combined: all four @0; show-more: `[kind]` @`resume_offset`), and marks the retry issued; a second `tick` does not re-emit. | Unit. |
| R4 | Retry success: combined → groups resolve per 004 `apply_page` (stale-free); show-more → `Loaded{items: stale ++ page, next_offset: page.next_offset, loading_more:false}`; retry cleared; `refreshing()` false. | Unit; SC-004. |
| R5 | Repeated rate limit → rescheduled with `attempt+1`; delay = `retry_after_ms` (capped at `SYNC_BACKOFF_MAX`) when given, else `15 s·2^attempt` capped at 4 min — byte-for-byte the sync policy (one shared `backoff_delay`). | Unit on `backoff_delay` (moved tests from `sync.rs`) + session reschedule test. |
| R6 | Other error on a retry: combined → `Empty`; show-more → `Loaded{items: stale, next_offset: Some(resume_offset), loading_more:false}`. | Unit. |
| R7 | Cancellation: effective-query change, clear/Escape (`reset_to_idle`), a new generation, and `set_offline(true)` each clear every retry; a reply to a cancelled retry is discarded (generation or request-id mismatch). | Unit per trigger. |
| R8 | `set_offline(true)` collapses `RateLimited{stale:Some(s),resume_offset:Some(o)}` → `Loaded{s, Some(o), false}`; a combined rate-limit → groups `Idle` + `debounce_deadline = Some(due)` so the query re-runs on reconnect. Never produces the no-results state. | Unit. |
| R9 | Results of a different, earlier query are never shown: a new generation always starts from `Pending` (004 FR-016 unchanged). | Existing unit test kept. |
| R10 | Library sync behaviour unchanged after `backoff_delay` moves to `backoff.rs`. | Existing `library/sync.rs` tests pass unchanged. |

## View: status strip

| ID | Rule | Verification |
|---|---|---|
| V1 | Strip visible iff view status is `RateLimited` (`refreshing() && retry_scheduled()`, no debounce armed); drawn above the scroll area. | UI test. |
| V2 | Text: `search-stale` ("Showing earlier results — search is busy, refreshing shortly") when any shown group has stale rows; else `search-rate-limited` ("Search is busy — retrying shortly"). | UI tests for both. |
| V3 | Styling: `surface_raised` fill, `radius::SM`, glyph from 019's `severity_glyph(Severity::Warning)` ("⚠") in `severity_color(roles, Warning)` (= `roles.warning`), text `text_primary`; no colour literals. | `design_token_literals` test passes; unit on the strip's resolved colours under light/dark/high-contrast. |
| V4 | Accessible: one `Role::Status` node whose label is the text (glyph excluded); no notification is raised (`notifications().len()` unchanged). | UI test. |
| V5 | With stale rows: the groups keep drawing their stale rows with their header counts; no Show more on a `RateLimited` group. | UI test. |
| V6 | No stale rows: no groups drawn; no spinner; no count line. | UI test (edge case "rate-limited with no previous results"). |
| V7 | The old Search-view `Status` node with `tr("refreshing")` no longer exists (`tests/search_view.rs` ~L361 and `tests/accessibility.rs` ~L1395 updated to the new keys); Library's `refreshing` status is untouched. | Tests updated; `tests/library_view.rs` unchanged and passing. |

## View: empty state

| ID | Rule | Verification |
|---|---|---|
| E1 | When `is_no_results()`: the existing `search-no-results` message with `$query = truncate_query(query)` — first 60 Unicode scalar values + "…" when longer — wrapped within `min(available, body_measure)`. | Unit + proptest on `truncate_query` (≤ 61 scalars; identity for ≤ 60; prefix-preserving). UI test: 200-char query → message node width ≤ body measure. |
| E2 | Followed by a `Variant::Secondary` button labelled `search-clear`; activation behaves exactly as contract C4 (empties query → idle, focuses the field). SC-005: one action. | UI test: click → `raw_query()==""`, field focused next frame. |
| E3 | No count line, no spinner, no strip in the empty state. | UI test. |

## Debug-only live seam (connect crate)

| ID | Rule | Verification |
|---|---|---|
| D1 | `MODPLAYER_CATALOG_FORCE_429=paged-once` (debug builds only): the first search command with `offset > 0` in the process answers `RateLimited{retry_after_ms: None}`; everything after it dispatches normally. Any other value keeps today's rate-limit-everything behaviour; release builds compile both out. | Unit test in `catalog/mod.rs` on the decision fn (env value + atomic). Manual M7. |
