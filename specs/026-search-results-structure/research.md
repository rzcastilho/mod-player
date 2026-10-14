# Research: Search Results Structure and Feedback

**Feature**: 026-search-results-structure | **Date**: 2026-09-30 | **Plan**: [plan.md](./plan.md)

Every Technical Context unknown is resolved here. Sources: the spec's Clarifications (Session 2026-09-30), the current code (`crates/modplayer-ui/src/search_view.rs`, `crates/modplayer-ui/src/app.rs` `Section::Search`, `crates/modplayer-ui/src/rows.rs` `virtualized_list*`, `crates/modplayer-ui/src/section_memory.rs`, `crates/modplayer-core/src/search.rs`, `crates/modplayer-core/src/library/sync.rs`, `crates/modplayer-core/src/controller.rs`), egui 0.36 APIs, and the 004/015/016/019/020 contracts.

---

## R1 — One results scroll area: who owns it

**Decision**: Move the Search section's scroll area *out of* `app.rs` and *into* `search_view::show`. `app.rs` stops wrapping the whole view in `section_memory.scroll_area(&ViewKey::Search).show(...)`; instead it passes `&mut SectionMemory` to `search_view::show`, which draws the fixed chrome (field row, count line, status strip) in the plain `Ui`, then builds the results `ScrollArea` from `section_memory.scroll_area(&ViewKey::Search)` and calls `section_memory.record(...)` with its offset. The per-group `virtualized_list(... Some(GROUP_VISIBLE_ROWS …))` calls and the `GROUP_VISIBLE_ROWS` constant are deleted.

**Rationale**: FR-001 requires the field row, count and status to stay fixed above exactly one vertical results area. Today the field scrolls away with the results *and* each group nests its own capped `ScrollArea` (the defect in US1). Keeping `SectionMemory` as the offset source preserves 020's section-return/sign-out reset behaviour (`reset_session_ui` already covers `ViewKey::Search`) with no new API — this is the same "caller supplies the pre-configured area" shape `rows::virtualized_list_in` introduced in 020.

**Alternatives considered**:
- Keep `app.rs`'s outer area and just delete the inner ones — rejected: the field row would still scroll away (FR-001 "fixed above"), and sticky headers must be computed against the *results* viewport, not one that includes the field.
- A new `ViewKey::SearchResults` — rejected: `ViewKey::Search` already means "the Search section's scroll offset"; adding a second key is a parallel path (Principle X).

## R2 — Virtualising four groups inside one scroll area

**Decision**: Use `ScrollArea::show_viewport` with a pure, precomputed flat layout (`ResultsLayout`, data-model §3). Each shown group contributes a header segment, *n* fixed-height row segments (height = `skeleton_shape(kind).height`, the same height `list_row` already paints at), and an optional "Show more" footer segment; groups are separated by `space::LG`. The closure sets the content's min height to `layout.total_height`, then, for each group whose extent intersects the viewport, draws only rows in `visible_rows(group, viewport)` (intersecting rows ± 1, clamped) via `ui.put`/`ui.scope_builder(UiBuilder::new().max_rect(row_rect))` at their computed rect. Headers and footers are drawn only when they intersect the viewport.

**Rationale**: FR-001 keeps virtualisation (hundreds of rows per group after repeated "Show more"). `show_rows` only virtualises one homogeneous list; four groups with different row heights plus headers/footers need `show_viewport` + our own index math. Making the math a pure function (no `Ui`) lets it be unit- and property-tested (Principle VIII) and makes SC-006/SC-002 checkable without rendering.

**Alternatives considered**:
- Four consecutive `show_rows` bodies inside one outer `ScrollArea` — rejected: nested `ScrollArea`s are exactly the defect; `show_rows` without its own area is not an egui API.
- Laying out every row every frame (no virtualisation) — rejected by the spec clarification ("Must long groups still be virtualised? → yes").
- Measuring row heights dynamically — rejected: `list_row` already paints at fixed per-kind heights (016); fixed heights reserve the full extent cheaply.

## R3 — Sticky ("pinned") headers in egui

**Decision**: After drawing the viewport's content, compute `layout.pinned_header(viewport.min.y)`: the group *g* whose in-flow header top is above the viewport top while `viewport.min.y < g.content_end`; its draw y is `min(viewport.min.y, g.content_end − header_h)` (so the next header pushes it out and it never overlaps the next group). Draw it **last** at that rect with an opaque `roles.surface` fill behind the `theme::section_label` text, so it paints over the rows and — because egui hit-tests the most recently added widget on top — rows beneath it take no pointer input. When a header is pinned, its in-flow copy is not drawn (it is above the viewport anyway), so the accessibility tree contains exactly one `Role::Header` node per shown group.

**Rationale**: egui 0.36 has no sticky-header primitive. Draw-last within the same layer is the smallest mechanism that meets FR-003 ("at most one pinned, pushed out by the next") and keeps the header inside the scroll area's clip rect. The push-out rule is pure arithmetic and proptest-able (P1–P4 in [contracts/results-layout.md](./contracts/results-layout.md)).

**Alternatives considered**:
- A separate `egui::Area` (foreground layer) for the pinned header — rejected: it escapes the scroll area's clip, needs its own z-order management against popups (row "…" menus), and duplicates the accessible header node.
- Drawing the pinned header in the fixed chrome above the scroll area — rejected: it would shift the results viewport height as groups change, and "push-out" animation would be impossible.

## R4 — Header content and accessible name

**Decision**: Header = `theme::section_label(group name)` followed by the count in `text_secondary` (same row, `space::SM` gap). Count = `group.items().len()` — i.e. `Loaded` items (including appended pages) or `RateLimited { stale: Some }` items (SC-006). `Pending` → name only. Accessible node: `Role::Header`, label = `tr_args("search-group-header", group, count)` → "Tracks, 20 results"; `Pending` keeps today's label (group name only). Header height is a single constant derived from the `section` text style row height + `space::SM` vertical padding, computed once per frame from `ui.fonts` (no per-group measurement).

**Rationale**: Spec clarification "What does a header's count count?". The service returns no totals; the rendered-row count is the only honest number.

**Alternatives considered**: "20+" or total counts — rejected (no total from service; "Show more" already signals more).

## R5 — Scroll-to-top on a new effective query

**Decision**: `SearchViewState` already records `last_query` and clears the selection when `session.query()` differs. The same branch sets a `reset_scroll` flag; when set, the results `ScrollArea` gets `.vertical_scroll_offset(0.0)` for that frame. "Show more", stale and retry transitions do not change `query()` and so do not reset. The per-group `("search-group", kind, query)` scroll ids that did this before disappear with the nested areas.

**Rationale**: Spec clarification "Scroll position on a new query?" (004 walk M2). Reuses the existing query-change detection.

**Alternatives considered**: Salting the area id with the query — rejected: it would bypass `SectionMemory`'s id and break 020's section-return restore.

## R6 — Single field label, clear control, focus

**Decision**: Delete `ui.label(tr("search-placeholder"))` + `.labelled_by(...)`. The `TextEdit` gets `.hint_text(tr("search-hint"))` and its accessible label is set explicitly via `ui.ctx().accesskit_node_builder(response.id, |b| b.set_label(tr("search-field-label")))` (same pattern the view already uses for headers/status). The field row is `ui.horizontal`: field (desired width = available − trailing controls), spinner (R7) when in flight, then the clear button when `!raw_query().is_empty()`: `widgets::controls::button(ui, Variant::Quiet, "×")` with its accessible label overridden to `tr("search-clear")`. Activation (`clicked()` — egui already maps Enter/Space on a focused button to `clicked`) calls the same path Escape uses — `controller.search_mut().set_query(String::new(), now)` — and then `field_response.request_focus()` next frame (store the field id; request focus via `ui.memory_mut(|m| m.request_focus(id))`). Tab order is naturally field → clear because the button is added right after the field. The empty-state "Clear search" button is a `Variant::Secondary` button with the same handler.

**Rationale**: FR-007/FR-008/FR-012; one handler for Escape, the trailing ×, and the empty-state button guarantees "behaves exactly as". `search-placeholder` is still referenced by the nav/shell? — checked: it is used only in `search_view.rs`, so it is **removed** from `library.ftl` and from `fluent_keys.rs`'s list (the nav item uses its own `nav-search` key, unchanged).

**Alternatives considered**: egui `TextEdit` has no built-in clear affordance; overlaying the × inside the field rect — rejected: complicates hit-testing and focus order for no user benefit at 960 px.

## R7 — "In flight" and the spinner

**Decision**: New read-only `SearchSession::in_flight() -> bool` = `debounce_deadline.is_some() || (pending_request_id.is_some() && self.combined_retry.is_none())` (see R9: a *retried* combined request is represented by the status strip, not the spinner). Plus `SearchSession::generation() -> u64` for the once-per-query announcement (R8). The spinner is `egui::Spinner::new().size(interact_size.y * 0.75)` with `WidgetInfo::labeled(WidgetType::ProgressIndicator, true, tr("search-in-flight"))` — the exact pattern `sign_in.rs` uses. Shown iff `!offline && in_flight()`; `set_query` arms the deadline in the same frame the field changes, so SC-003's "first frame" holds.

**Rationale**: Spec clarification "When is a query in flight?". Read-only accessors are explicitly permitted by FR-013.

**Alternatives considered**: Deriving in-flight from `GroupState::Pending` — rejected: the debounce window (150 ms) has no Pending groups yet, so the spinner would miss SC-003's first frame.

## R8 — Settled count line as a polite live region, announced once

**Decision**: `settled_count(&session) -> Option<usize>`: `None` if offline, `in_flight()`, `is_no_results()`, any group `Pending`, or no group shows rows; else `Some(Σ items().len())` over shown groups. Drawn as a `text_secondary` label with `Role::Status`, label `tr_args("search-result-count", count)`. AccessKit `Live::Polite` is set **only** on the first frame a given `session.generation()` settles (`SearchViewState.announced_generation`), `Live::Off` afterwards — so "Show more" updates the text without a re-announcement.

**Rationale**: FR-010 "announced once per settled query, not on every Show more". AccessKit announces live-region changes; gating `live` by generation is the minimal mechanism. No other view uses `set_live` yet — the tests assert the node's `live()` property directly in the accesskit tree.

**Alternatives considered**: Raising a notification — rejected (spec: never a toast); announcing on every change — rejected by FR-010.

## R9 — Automatic retry after rate-limit (completing 004 FR-015)

**Decision**: Extend `SearchSession` (pure, injectable clock — like `SyncScheduler`):
- `apply_reply(…, Err(RateLimited { retry_after_ms }))` now takes `now: Instant` (signature change; the controller's `route_search_result` passes `self.now()`), and schedules a `Retry { due, attempt, target }`: `target = Combined` for the combined request, `ShowMore { offset }` per group for a page request. `due = now + backoff_delay(attempt, retry_after_ms)`; `attempt` increments on each consecutive rate-limit of the same target and resets on success or on a new generation.
- `backoff_delay` moves from a private fn in `library/sync.rs` to `pub(crate) fn backoff_delay` in a new `crates/modplayer-core/src/backoff.rs` shared by both (`SYNC_BACKOFF_BASE`/`SYNC_BACKOFF_MAX` move with it, names unchanged). No behaviour change for sync; its existing tests keep passing.
- `tick(now)` (already called every frame by the controller) issues due retries when online and the retry's generation is current: combined → the same `SearchCatalog { kinds: GROUP_ORDER, offset: 0 }` with a fresh `request_id` of the *same* generation, groups stay `RateLimited` until the reply; show-more → `SearchCatalog { kinds: [kind], offset }`.
- `RateLimited` gains the data needed to resume: `RateLimited { stale: Option<Vec<SearchHit>>, resume_offset: Option<u32> }` (`resume_offset` = the rate-limited "Show more" offset). On success: combined → `apply_page` exactly like a first reply (replaces stale); show-more → `Loaded { items: stale ++ page.items, next_offset: page.next_offset, loading_more: false }`.
- Cancellation: `reset_to_idle` (clear/Escape), any `set_query` that changes the effective query, a new debounce firing (generation bump) and `set_offline(true)` clear all retries. Going offline cancels (spec FR-011a) and coming back online does **not** resurrect the retry. *Assumption recorded*: once the retry is cancelled, a status that still says "refreshing shortly" would be a false claim, so `set_offline(true)` also collapses `RateLimited { stale: Some(s), resume_offset: Some(o) }` back to `Loaded { items: s, next_offset: Some(o), loading_more: false }` (the pre-rate-limit state, "Show more" usable again) ; a combined-request rate limit (`RateLimited { stale: None }` everywhere) turns the groups back to `Idle` and re-arms `debounce_deadline = Some(retry.due)` — reusing 004's existing rule that an armed deadline survives offline and fires as soon as connectivity returns, so the query simply re-runs (new generation, skeletons) on reconnect. Collapsing it to `Empty` instead was rejected: all-`Empty` is the no-results state, a false claim. On reconnect the view therefore shows either plain loaded rows (no strip) or a fresh run of the query. Rejected alternative: re-arming the retry on reconnect — more state for a rare path, and FR-011a says cancel.
- Other errors on a retry → same as today for that request kind (combined → `Empty`; show-more → `Loaded` with `loading_more = false`, rows restored from `stale`).

**Rationale**: FR-011/FR-011a and the spec's "Is a refresh really pending?" clarification: the status may only claim a refresh if one is scheduled. Same backoff policy as sync, required verbatim by the spec, so sharing the fn (not copying it) is the Principle X answer. Keeping everything inside `SearchSession::tick/apply_reply` keeps the controller change to one argument.

**Alternatives considered**:
- Retry scheduling in the controller — rejected: splits search state across two owners and loses the pure-state testability of `SearchSession`.
- Re-arming the debounce deadline as the retry — rejected: bumps generation and resets groups to skeletons, violating "keep showing the previous results".
- A 1–2 s search-specific backoff — rejected: spec mandates library sync's policy.

## R10 — Status strip styling

**Decision**: Inline `egui::Frame::new().fill(roles.surface_raised).corner_radius(radius::SM).inner_margin(space::SM)` containing `"⚠"` in `roles.warning` + text in `roles.text_primary`; `Role::Status`, label = the text (glyph decorative, excluded). Key `search-stale` when any shown group is `RateLimited { stale: Some }`, else `search-rate-limited` when any group is `RateLimited { stale: None }` (and nothing stale). The glyph and colour are read from 019's mapping: `notifications::severity_glyph`/`severity_color` become `pub(crate)` so the strip calls them with `Severity::Warning` rather than duplicating `"⚠"`. The strip is visible iff `session.refreshing() && session.retry_scheduled()` and no query edit is debouncing (data-model §5); the old `tr("refreshing")` label is removed from Search only.

**Rationale**: Spec clarification "How is the rate-limit status styled and worded?"; 017 high contrast is inherited through `theme::roles`.

**Alternatives considered**: Notification-centre toast — rejected by spec; copying the glyph literal — rejected (design_token_literals test + Principle X).

## R11 — Empty-state query truncation

**Decision**: Pure `truncate_query(q: &str) -> Cow<str>`: if `q.chars().count() > 60`, `q.chars().take(60).collect::<String>() + "…"`, else borrowed. The message is drawn inside the existing `ui.set_max_width(min(available, body_measure))` scope with wrapping, followed by the Secondary "Clear search" button.

**Rationale**: FR-012 ("60 Unicode scalar values + …"). `chars()` is exactly Unicode scalar values. Proptest: result ≤ 61 scalars, prefix-preserving, identity for ≤ 60.

**Alternatives considered**: Grapheme-aware truncation — rejected: spec says scalar values; no new dependency (`unicode-segmentation`) justified.

## R12 — Live rehearsal of the stale → retry → recovered path

**Decision**: Extend the existing debug-only `MODPLAYER_CATALOG_FORCE_429` toggle (`crates/modplayer-audio-source-connect/src/catalog/mod.rs`, compiled out of release) with one value: `MODPLAYER_CATALOG_FORCE_429=paged-once` rate-limits only the **first** search command with `offset > 0` in the process (a `static AtomicBool`), then behaves normally. Any other value keeps today's "every command" behaviour.

**Rationale**: Constitution Governance requires the agent to execute manual scenarios against the real build; today's toggle rate-limits *every* command, so "stale rows under a status, then recovery" (US3-AS1/AS2) cannot be driven live (env vars cannot be changed in a running process). One debug-only value is the smallest seam; it is `#[cfg(debug_assertions)]` like the existing toggle, so Principle V/VI are unaffected.

**Alternatives considered**: Automated-only coverage for recovery — rejected: 003's lesson (constitution) that live-only defects hide until driven; a separate env var — rejected: two toggles for one concern.

## R13 — Strings

**Decision**: New keys in `locales/en-US/library.ftl` + pt-BR in `locales/pt-BR/library.ftl` (the parity bundle 025 introduced): `search-field-label`, `search-hint`, `search-clear`, `search-in-flight`, `search-result-count` (plural), `search-group-header` (plural, `$group`, `$count`), `search-stale`, `search-rate-limited`. `search-placeholder` removed (unused after R6). `refreshing` kept (Library uses it). Full table: [contracts/fluent-strings.md](./contracts/fluent-strings.md).

**Rationale**: FR-012a, Constitution X (NFR-7.1).

## R14 — Manual walk outcomes (T043)

Differences from the spec found while driving the real build on 2026-09-30; full per-scenario record on T043 in [tasks.md](./tasks.md).

- **Pinned header at max scroll.** egui's content height runs about 2 pt past `ResultsLayout::total_height`, so at the maximum offset the clamped `top` (kept for the 020 M3 over-seeded frame) sat above the real viewport top and the pinned header was clipped by 2 pt. `draw_results` now pins at egui's real `viewport.min.y` while it is inside the content and falls back to the clamped `top` otherwise (RL5's over-seeded frame). Test: `rl6_header_pins_at_viewport_top_at_max_scroll`.
- **Empty state with unsupported groups.** `SearchSession::is_no_results` (004) needs all four groups `Empty`. The live source answers Tracks only and reports the other three `Unsupported` (004 research V1), so a nonsense query left a blank page and SC-005 was unreachable. Unsupported groups are already omitted like empty ones (V3), so the view's own `is_no_results` treats them as absent: at least one group `Empty` and the rest `Unsupported` shows the empty state. The core accessor is unchanged (FR-013). Test: `e1_empty_state_shows_when_other_groups_are_unsupported`.
- **Escape (F4).** Escape in the focused field never emptied the query, on `main` too: egui's `TextEdit` gives up focus on the Escape frame, so the `has_focus()` guard was false. The guard now also accepts `lost_focus()`. F4 claimed an existing test; there was none, so `f4_escape_in_the_field_empties_the_query` adds one.
- **Live-source limits (not defects of this feature).** Only the Tracks group is ever shown, and Show more exhausts at 20 (context-resolve returns one page). Multi-group layout, header push-out and 20 → 40 counts stay covered by RL3/RL5/RL8/P3/P4/N2.
- **Harness.** Synthetic scroll-wheel `CGEvent`s don't reach egui on this host (every unit/flag/tap variant tried; Library static too); the walk dragged the results scrollbar instead. Keycode-0 Unicode keyboard events are dropped; the helper types with real keycodes.
