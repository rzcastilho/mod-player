# Research: Shell Navigation and Launch Gates

**Feature**: `020-shell-navigation-and-gates` | **Date**: 2026-09-25 | **Plan**: [plan.md](./plan.md)

All product values are fixed in [spec.md](./spec.md) § Clarifications (items 1–15). This document
resolves the remaining *engineering* unknowns found while reading the code as of this branch
(`crates/modplayer-ui/src/{app.rs,shell.rs,settings/mod.rs,library_view.rs,rows.rs,search_view.rs}`,
`crates/modplayer-account/src/launch_flow.rs`, `crates/modplayer-core/src/i18n.rs`, egui 0.36.2
sources). Format per item: Decision / Rationale / Alternatives considered.

---

## R1 — Where the rail-visibility decision lives (FR-001, FR-001a, FR-001b, FR-004)

**Finding**: `App::ui` (`app.rs:314`) adds `Panel::left("shell-nav-rail")` unconditionally every
frame, *before* computing `step` (`app.rs:381`). The dispatcher gate (`app.rs:291`) already uses
`launch_step() == Main && device_check.is_none()`. `App` itself cannot be constructed in a
headless test (it needs `eframe::CreationContext`), so any decision left inline in `App::ui` is
untestable.

**Decision**: Add a pure, `Copy` value `shell::Chrome` computed once per frame by
`Chrome::for_frame(step: LaunchStep, device_check_open: bool)`:

| `step` | `device_check_open` | `rail` | `gate` (step indicator) |
|---|---|---|---|
| `Welcome` | any | `false` | `Some(GateStep::Welcome)` |
| `SignIn` | any | `false` | `Some(GateStep::SignIn)` |
| `DeviceCheck` | any | `false` | `Some(GateStep::AudioOutputCheck)` |
| `Main` | `true` (Settings preview) | `false` | `None` |
| `Main` | `false` | `true` | `None` |

`App::ui` computes `step` and `chrome` **once, near the top** (before dispatch), uses
`chrome.navigation_enabled()` (≡ `chrome.rail`) as the dispatcher gate, and calls
`shell::show_chrome(ui, chrome, &mut self.shell)` which adds the left `Panel` only when
`chrome.rail`, and the top `Panel` hosting the step indicator only when `chrome.gate.is_some()`.
The same `step` value is used for the `CentralPanel` match (no second `launch_step()` call within
the frame).

**Rationale**: One predicate drives the rail *and* the dispatcher, so FR-001b cannot drift from
FR-001. The table is exhaustively unit-testable; `show_chrome` is testable with `Context::run_ui`
and the AccessKit tree (no `App`). Not adding the `Panel` at all (rather than drawing it empty)
means egui never allocates its width, so `CentralPanel` spans the window (FR-004).

**Subtlety**: `step` is currently computed *after* account events drain; a sign-out event drained
this frame would otherwise hide the rail one frame late. Computing `chrome` *after*
`account.tick()` and before any panel is added keeps it single-sourced. Dispatch runs before
`account.tick()` today; it keeps using the pre-tick step (it consumes input for *this* frame, and
a gate step that begins this frame simply stops dispatch from next frame). Both are evaluated from
`launch_step()`; the plan orders the code as: `claims` → `step_before = launch_step()` →
dispatch gated on `Chrome::for_frame(step_before, …)` → `account.tick()` → `step = launch_step()`
→ `chrome = Chrome::for_frame(step, …)` → panels.

**Alternatives considered**:
- *Draw the rail but disable its buttons (`add_enabled(false, …)`)* — rejected: still five
  controls that look like navigation and are exposed to AT; the spec requires not rendering.
- *Put the predicate on `LaunchStep` in `modplayer-account`* — rejected: rail/indicator are UI
  concepts; `modplayer-account` stays UI-free (design note 4 of 002).
- *Zero-width rail* — rejected by FR-004 (egui still reserves separator/margin space).

## R2 — Step indicator model, placement and accessibility (FR-002, FR-003)

**Decision**:
- `GateStep` enum in `modplayer-ui::shell` — `Welcome`, `SignIn`, `AudioOutputCheck` — with
  `const ALL: [GateStep; 3]`, `position() -> u8` (1..=3), `label_key()` and
  `from_launch_step(LaunchStep) -> Option<GateStep>` (1:1, `Main → None`). `GATE_STEP_TOTAL: u8 = 3`
  is a constant, never derived from tier (Clarification 4).
- `StepState` for each item is derived purely by comparing positions:
  `< current → Complete`, `== current → Current`, `> current → Upcoming`. Because `next_step`
  is first-match-wins in exactly this order, "earlier step satisfied on a previous launch" is
  always "position < current" — no extra persisted flag needed.
- Placement: `Panel::top("gate-step-indicator")` added only while `chrome.gate.is_some()`, so the
  gate screens' own `CentralPanel` content (welcome/sign-in/device check) is untouched — the spec
  keeps their copy/layout out of scope.
- Paint (tokens only): `Complete` = small filled `accent` dot + `text_secondary` label;
  `Current` = `accent` ring (focus-ring stroke width from `theme::controls`) + `text_primary`
  label; `Upcoming` = `divider` ring + `text_secondary` label. Items joined by a divider-coloured
  connector. No new colour role.
- Accessibility: one non-interactive node for the whole indicator, `Role::ProgressIndicator`,
  label = `tr_args("gate-step-progress", n, total, label)` → "Step 2 of 3: Sign in",
  `numeric_value = n`, `max_numeric_value = 3`. The three painted item labels are plain `Label`s
  (not focusable) so Tab order into the gate content is unchanged. The existing
  `shell.rs` a11y helper already requires every `ProgressIndicator` to have a name.

**Rationale**: Positions derived from `LaunchStep` alone make FR-003 true by construction:
retries/sub-views (Decline, Privacy notice, Authorizing, Checking, Failed, StoreUnavailable,
tier-result) never change `LaunchStep`, so they cannot change `n`.

**Alternatives considered**: drawing the indicator inside each gate screen (three edits,
three places to drift); a path-dependent total (2 for non-Premium) — rejected by Clarification 4;
`Role::Group`/`Role::Status` — `ProgressIndicator` with value/max is the closest AccessKit role
for "n of total" and is already covered by the project's a11y sweep helper.

## R3 — Rail selected state (FR-006)

**Finding**: `Shell::nav_rail` uses `ui.selectable_label`, whose selected state is a filled
`selection.bg_fill` — reads as a pressed button. 016 already settled "a stroke, never a filled
accent background" for tabs (`widgets::controls::tab`, `theme::controls::tab_underline`).

**Decision**: New host widget `widgets::controls::nav_item(ui, selected, label) -> Response`:
`Label::new(theme::section_label(label)).sense(Sense::click())` laid out at full rail width,
hover fill from the existing interaction-state table (015) only when unselected & hovered, and
when selected paints a vertical bar on the item's left edge: width `theme::controls::NAV_INDICATOR_WIDTH`
(3.0 normal, 4.0 high contrast — read via a `nav_indicator(roles) -> Stroke` selector, like
`focus_ring`), full item height, colour `roles.accent`. Label colour `text_primary` selected /
`text_secondary` unselected. AccessKit: `Role::Button` kept (it activates navigation),
`set_selected(true)` on the selected item, label pinned to the exact un-uppercased `tr(key)`
(unchanged from 014 FR-019).

**Contrast check** (rail surface = `panel_fill` = `surface_base`): light `#0a63c9` on `#ffffff`
≈ 5.9:1; dark `#5aa9ff` on `#141417` ≈ 7.6:1; high-contrast accents (`#074a96` light,
`#74b6ff` dark) are darker/lighter still. All ≥ 3:1; a test in `design_token_contrast.rs`
asserts it for all four `Roles` via `theme::contrast::ratio`.

**Alternatives considered**: keeping `selectable_label` with a custom `selection.bg_fill` —
still a filled button; underline (the tab treatment) — a vertical rail reads better with a
leading-edge bar and keeps tab vs. nav visually distinct; `Role::Tab` — the rail is not a tab
list controlling a panel in AT terms, and changing role would break 014/007 a11y expectations.

## R4 — Retaining sub-view and scroll per section (FR-007)

**Findings**:
- Sub-views are **already** retained across section switches, because their state lives in
  `App` fields that section switches never touch: `library_view.tab`, `library_detail`,
  `search_view` + `controller.search()` (query/results), `settings.category`. Only sign-out
  resets part of it (`clear_for_sign_out` resets search/library; `App` resets artwork/waveform
  but *not* `library_view`, `library_detail`, `search_view`, `settings`, `shell`).
- egui 0.36 stores `ScrollArea` state with `insert_persisted` keyed by the area's id
  (`containers/scroll_area.rs:76–80`); it survives frames where the area is not drawn, and eframe
  here is built without the `persistence` feature, so nothing reaches disk. So a *stable-id*
  scroll area already keeps its offset in memory — but: (a) Settings, Plugins and Search have **no
  section-level scroll area** at all (content below the fold is clipped, and there is nothing to
  restore); (b) stale egui state would survive sign-out (violates US3-AS4); (c) there is no
  test-visible handle to assert ±1 px.

**Decision**: New module `crates/modplayer-ui/src/section_memory.rs`:
- `SectionMemory { epoch: u64, offsets: HashMap<ViewKey, f32>, shown_last_frame: Option<ViewKey> }`
  owned by `App`.
- `ViewKey` identifies *the scrollable view*, not just the section: `Library(LibraryViewKey)`
  where `LibraryViewKey = Tab(LibraryTab) | Detail(DetailTarget)`, `Search`,
  `Settings(SettingsCategory)`, `Plugins`. (Now Playing: see R5.)
- `SectionMemory::scroll_area(&mut self, key) -> ScrollArea` returns a vertical `ScrollArea`
  with `id_salt(("section-scroll", key, epoch))`; if `key` was *not* the view shown last frame
  and an offset is stored, it applies `.vertical_scroll_offset(stored)` exactly once (egui clamps
  to the new max on show, satisfying "clamped if content shrank"). `record(key, output.state.offset.y)`
  stores the offset after `show`.
- `SectionMemory::reset()` clears `offsets`, bumps `epoch` (so every egui-side persisted state
  keyed by the old salt becomes unreachable) — called from the `SignedOut` and `SessionRevoked`
  arms of `App::handle_account_event`, alongside new resets of `shell`, `library_view`,
  `library_detail`, `search_view`, and `settings.category` (US3-AS4: "every section starts at
  its default view scrolled to the top").
- Where the scroll area is attached:
  - **Library** (tab lists and detail list): these are `rows::virtualized_list` (fill-height
    `show_rows`) and cannot be nested in an outer scroll area. `virtualized_list` gains an
    `impl Into<ListScroll>` style parameter — concretely a new
    `rows::virtualized_list_in(ui, scroll: ScrollArea, row_height, count, draw_row) -> (Range<usize>, f32)`
    that takes the pre-configured area, and the existing `virtualized_list` becomes a thin wrapper
    (no behaviour change for Search/detail callers that are not wired to memory).
    `library_view::show`/`detail_view::show` accept `&mut SectionMemory` (or a
    `ScrollArea` + returned offset) for their single main list.
  - **Search**, **Plugins**: the section body is wrapped in `memory.scroll_area(key).show(…)` in
    `App::show_main`. Search's per-group `virtualized_list`s have a fixed `max_height`, so nesting
    them in an outer vertical area is safe (bounded height).
  - **Settings**: the search box and the category row stay fixed at the top (the row must always
    be visible, FR-010); only the selected category's *content* is wrapped, keyed per category.
    This needs `settings::show` to split into `header` + `content`; see contracts/section-memory.md.

**Rationale**: Explicit offsets make ±1 px restoration and sign-out reset deterministic and
directly assertable (`SectionMemory::offset(key)`), while still letting egui clamp. The epoch in
the id salt is the cheapest correct way to discard egui's own persisted state without reaching
into `ctx.data_mut` internals.

**Alternatives considered**:
- *Rely purely on egui's persisted `ScrollArea` state* — rejected: no reset on sign-out, no
  section-level area for Settings/Plugins/Search, and ids silently change if a parent layout
  changes (e.g. the Getting Started card).
- *`ctx.data_mut(|d| d.remove_by_type::<scroll_area::State>())` on sign-out* — rejected: the
  `State` type is private to egui.
- *One offset per section (ignoring sub-view)* — rejected: returning to Library on a different
  tab/detail than the stored offset belongs to would jump to a meaningless position.

## R5 — Now Playing and "no scrollable content" (FR-007 edge case)

**Decision**: Now Playing keeps its fixed, responsive layout (018): it has no section-level
scroll area, so there is nothing to restore (spec edge case: "no observable change"). Its inner
effect-chain `ScrollArea` (`now-playing-effect-chain-scroll`, bounded `max_height`) already keeps
its egui-persisted offset; the plan adds the section-memory epoch to that salt so it too resets
on sign-out. No `ViewKey::NowPlaying`.

**Rationale**: Wrapping Now Playing in an outer vertical area would break
`effects_panel_reserved_height`/`available_height()` sizing (an outer scroll area reports
unbounded height), regressing 018's responsive dock.

**Alternatives considered**: outer area with a fixed max height — duplicates 018's layout maths.

## R6 — Settings category row partition (FR-008–FR-010, FR-012)

**Finding**: `settings::show` draws the eleven categories with `ui.horizontal_wrapped`
(`settings/mod.rs:144`) as `selectable_label(theme::section_label(label))`.

**Decision**: New module `crates/modplayer-ui/src/settings/category_row.rs` with:
1. A **pure** function
   `partition(widths: &[f32], selected: usize, more_width: f32, gap: f32, available: f32) -> RowPartition`
   returning `{ visible: Vec<usize>, pinned: Option<usize>, overflow: Vec<usize> }`:
   - if `Σw + gap·(n−1) ≤ available` → all visible, `pinned = None`, `overflow = []` (no More);
   - else `k` = largest prefix length with `Σw[0..k] + gap·k + more_width ≤ available`;
   - if `selected < k` → `visible = 0..k`;
   - else `j` = largest `j ≤ k` with `Σw[0..j] + w[selected] + gap·(j+1) + more_width ≤ available`
     (floor 0), `visible = 0..j`, `pinned = Some(selected)`;
   - `overflow` = every other index in canonical order.
2. **Measurement before drawing**: each item's width = its painted galley width (the
   *uppercased* `section_label` text laid out with `layout_no_wrap` in the `section` text style)
   + `2 × spacing.button_padding.x`; `more_width` measured the same way from `tr("settings-more")`;
   `gap = spacing.item_spacing.x`; `available = ui.available_width()`. Drawn inside
   `ui.horizontal` (never `horizontal_wrapped`). Labels are laid out with no wrap and no
   truncation, so a drawn item is never ellipsized.
3. **Invariants** (proptest in `settings_category_row.rs`, Constitution VIII property tests):
   disjoint cover of `0..n`; canonical order within `visible` and `overflow`; `selected ∉ overflow`;
   `overflow.is_empty() ⇔ all fit`; drawn width ≤ `available` whenever
   `w[selected] + gap + more_width ≤ available`; maximality of the prefix.

**Rationale**: A pure partition over measured widths is deterministic within one frame (no
"measure last frame, jump this frame" flicker), testable without egui, and the only egui-coupled
part is measurement, which reuses the exact font/padding the draw uses.

**Alternatives considered**: egui sizing pass / `ui.horizontal` + clip detection (one-frame
reflow, not testable purely); horizontal `ScrollArea` for the row (hides categories off-screen
with no affordance; the spec requires an overflow control); a sidebar (explicitly out of scope,
breakdown decision).

## R7 — Overflow control and keyboard model (FR-009, FR-011)

**Finding**: 016's row-actions menu (`rows.rs:626–658`) already uses egui 0.36
`Popup::new(id, ctx, &opener, layer).kind(PopupKind::Menu).open_memory(cmd)` with
`Role::MenuItem` items. egui does not provide arrow-key movement between popup items by itself.

**Decision**: The "More" control is a `widgets::controls::button(ui, Variant::Quiet, …)` with
accessible name `tr("settings-more-a11y")` ("More settings categories") and
`set_expanded(open)`; it opens a `Popup` of `PopupKind::Menu` using the same pattern. Keyboard:
- Tab/Shift+Tab: egui's default focus order = widget creation order = visual order (visible
  categories, then pinned selected, then More), matching the 001 nav-rail note.
- Enter/Space on More toggles the menu and, when opening by keyboard, requests focus on the
  first menu item (`SetOpenCommand::Bool(true)` + `request_focus` on item 0 the frame it opens).
- In the open menu the row handles `ArrowDown`/`ArrowUp` itself (consumed with
  `ctx.input_mut(|i| i.consume_key(…))`, wrap-around off, clamped at ends) by moving focus to
  the neighbour item's id; `Enter` selects the focused item (egui `Button` already activates on
  Enter/Space when focused) → `screen.category = c`, `Popup::close_id`, focus returns to the
  now-pinned category item; `Escape` → `Popup::close_id` + `request_focus(more_id)`.
- Partition change while open (FR-011): the row stores last frame's `RowPartition` in
  `SettingsScreen::row_state`; if the new partition differs and the menu is open, close it and
  focus More if still present, else the selected category.
- The dispatcher (`actions::dispatch`) runs *before* widgets and reads last frame's focus
  claims. Each open-menu item registers
  `actions::register_claim(ctx, item_id, Claim::Keys(vec![Up, Down, Enter, Escape chords]))`
  (the existing `Claim::Keys(Vec<ChordPattern>)` variant, `actions.rs:41–44`), so a global
  binding on an arrow key (e.g. a transport/marker nudge) never fires while a menu item has
  focus. No new `Claim` variant.

**Rationale**: Reuses the project's one existing popup-menu pattern and roles; explicit arrow
handling is ~20 lines and fully testable by feeding `egui::Event::Key` into `RawInput`.

**Alternatives considered**: `ui.menu_button` (egui menu bar semantics, less control over
focus return); a `ComboBox` (reads as a value picker, not navigation).

## R8 — Localisation: pt-BR does not exist yet (FR-002, FR-009, FR-012, NFR-7.1)

**Finding**: `locales/` contains only `en-US/`; `modplayer_core::tr` hard-codes `langid!("en-US")`
(`i18n.rs:40–45`), and `settings/language.rs` states "pt-BR ships in a later slice". There is no
runtime locale switch, so "run the 40 % test with the pt-BR locale" and "ship pt-BR translations"
cannot be executed in this feature.

**Decision**:
- Add the new keys to `locales/en-US/app.ftl` (gate steps) and `locales/en-US/settings.ftl`
  ("More"), exercised by `tests/fluent_keys.rs`.
- Record the pt-BR strings in [contracts/fluent-strings.md](./contracts/fluent-strings.md) so the
  pt-BR slice drops them in verbatim; do **not** create a partial `locales/pt-BR/` directory
  (the `static_loader!` would load it, and a half-populated bundle would be mistaken for shipped
  support).
- The "longer translation" guarantee (FR-012, SC-002) is tested with the existing
  `modplayer_core::i18n::with_pseudo_expansion(40, …)` hook (018, research R12), whose padding
  `max(1, ⌈0.4·len⌉)` yields exactly the spec's `⌈1.4 × len⌉` label length. The longest pt-BR
  category drafts (e.g. "Privacidade e diagnóstico" vs "Privacy & diagnostics") are ≤ 40 %
  longer, so the pseudo-expansion run dominates the pt-BR case.

**Rationale**: This is the constitution-compliant reading of NFR-7.1 ("English and pt-BR ship
first") given the project has deliberately deferred pt-BR as a slice; every new string is still
externalized. Recorded as a deviation in plan.md Complexity Tracking.

**Alternatives considered**: build a locale switch now (out of scope; belongs to the pt-BR
slice and touches every screen); create `locales/pt-BR/` with only the new keys (misleading
partial locale).

## R9 — Library tab counts (FR-013, FR-014)

**Finding**: `library_view::show` (`library_view.rs:120–150`) already draws the count as a
sibling `mono` label per tab, only when `!status.loading`, and `tests/library_view.rs` already
covers "no count while loading" and "count shown incl. 0" (016 contract T4–T8).

**Decision**: No production change. Add one regression test for FR-014 (count reflects a change
to the in-memory list on the next drawn frame: sync a page with 0 albums, render, apply a page
with 2, render, assert `"2"` beside Saved albums) if the existing suite does not already assert
a changed count; and a test that the rail carries **no** count badges (Clarification 14).

## R10 — Test harness

**Decision**: Headless egui (`Context::run_ui` + `enable_accesskit`, AccessKit `TreeUpdate`
inspection) exactly as `shell.rs` tests and `tests/library_view.rs` do; geometry via node
`bounds` / `Response::rect`. Screen size for the 960 px tests set through
`RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960.0, 640.0))), .. }`.
Keyboard via `RawInput.events` with `egui::Event::Key { key, pressed: true, .. }`, driving
multiple frames. No new dev-dependencies (`proptest` is already a dev-dependency of
`modplayer-ui`).

**Alternatives considered**: `egui_kittest` — a new dev-dependency with no second consumer
(Constitution X); the existing harness already covers every assertion needed.
