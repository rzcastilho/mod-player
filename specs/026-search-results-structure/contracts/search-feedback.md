# Contract: Query Field Feedback — label, clear, spinner, count (US2)

**Covers**: FR-007–FR-010, FR-012a, SC-003 | **Data**: [data-model.md §2, §4, §5](../data-model.md) | **Research**: R6–R8

Test homes: `crates/modplayer-ui/tests/search_view.rs`, `tests/accessibility.rs` (updated search-field case at the former `search-placeholder` assertion), `tests/fluent_keys.rs`; core unit tests in `crates/modplayer-core/src/search.rs`.

## Field

| ID | Rule | Verification |
|---|---|---|
| F1 | No visible label widget above the field; the field's accessible label is exactly `search-field-label` ("Search the catalog") and it is not `labelled_by` another node. | Test: no `Role::Label` node with text "Search" in the view; `TextInput` node label == `tr("search-field-label")`. |
| F2 | Placeholder/hint text is `search-hint` ("Tracks, albums, artists, playlists"). | Test: hint galley text / accesskit `placeholder` == `tr("search-hint")`. |
| F3 | Focus shortcut (`Shell::focus_search_requested`, 007 `NavSearch`) still focuses the field and is consumed. | Existing test kept. |
| F4 | Escape in the focused field keeps its existing behaviour (empties query → idle). | Existing test kept. |

## Clear control

| ID | Rule | Verification |
|---|---|---|
| C1 | Present iff `raw_query()` is non-empty (whitespace-only counts); absent when empty. | Test: `""` → no `Button` named `search-clear`; `" "` → present; `"abba"` → present. |
| C2 | Drawn with `widgets::controls::button(ui, Variant::Quiet, "×")` at the field's trailing edge (right of the spinner when both show); accessible label `search-clear` ("Clear search"). | Test: button node right edge ≥ field right edge; label. `control_inventory` test lists it as Quiet. |
| C3 | Keyboard: Tab from the field lands on it; Enter or Space activates it. | Test: focus field, inject `Tab` → focused node is the clear button; inject `Enter` → query empty. |
| C4 | Activation ⇒ `set_query("", now)` (same path as Escape: groups `Idle`, generation bumped, retries cleared) **and** the field has focus on the next frame. | Test: click → `session.raw_query() == ""`, all groups `Idle`; next frame focused id == field id. |

## In-flight spinner

| ID | Rule | Verification |
|---|---|---|
| S1 | Shown iff `!offline && session.in_flight()` (debounce armed, or first combined request outstanding). Not shown for Show more, not while a rate-limit retry waits or is issued. | Core unit: `in_flight()` truth table over {idle, debouncing, combined outstanding, loaded, show-more outstanding, retry waiting, retry issued, offline+armed}. UI test: node present/absent accordingly. |
| S2 | Appears on the same frame the effective query changes (typing) and disappears on the frame the combined reply is applied (SC-003). | Test with injected clock: frame 0 type → spinner present; scripted reply applied in frame *k* → absent in frame *k*. |
| S3 | Accessible: `Role::ProgressIndicator` (`WidgetType::ProgressIndicator`), label `search-in-flight` ("Searching…"). | Test: find node by role + label. |

## Settled count line

| ID | Rule | Verification |
|---|---|---|
| N1 | Shown iff view status is `Settled { count }` (data-model §5): not idle, not in flight, not offline, not no-results, no group `Pending`, ≥ 1 shown group. | Test matrix per status. |
| N2 | Text `search-result-count` with `$count` = Σ header counts of shown groups (plural-aware: "1 result" / "n results"). Updates after Show more. | Test: 20 tracks + 5 albums → "25 results"; after Show more +20 → "45 results". |
| N3 | Node `Role::Status`; `live = Polite` only on the first frame a generation settles; `live` unset/Off on later frames (so Show more does not re-announce). | Test: frame of settle → `node.live() == Some(Live::Polite)`; next frame and after Show more → not Polite; a new query's settle → Polite again. |
| N4 | While a rate-limit status strip shows, the count line is hidden (the strip is the announcement). | Test. |
