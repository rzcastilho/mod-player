# Research: Library Browsing and Detail View (025)

Phase 0 for [plan.md](./plan.md). All spec Clarifications are taken as settled; items below resolve the remaining *how* questions from reading `crates/modplayer-ui` at `6df0c0f`.

---

## R1 — Header layout primitive and fixed height

**Decision**: One `collection_header(ui, …) -> HeaderEvent` in `detail_view.rs`. Allocate an exact rect `(available_width, HEADER_HEIGHT)` where `HEADER_HEIGHT = back_row_height + space::SM + DETAIL_ARTWORK_SIZE` (back row height = `interact_size.y`, measured from style, not hard-coded). Inside: top row = Quiet Back button; below, `left_to_right(Align::Min)`: 128 px artwork, then a column with the title (`Label::new(...).truncate()`, `text::DISPLAY`), facts (`truncate()`, `text::SECONDARY`, `text_secondary` role), and an actions row (Primary Play + "…"). The column's width is `available - DETAIL_ARTWORK_SIZE - item_spacing.x`, so truncation, not overflow, handles long titles at 960 px.

**Rationale**: `allocate_exact_size` makes height invariant across skeleton/loaded/long-title (H1, K5); the existing row widget already proves truncating labels inside a fixed rect (`tests/rows.rs` L6).

**Alternatives**: `ui.horizontal` + natural height — rejected: height varies with font scale and state, violating FR-001/FR-010.

## R2 — Facts line composition

**Decision**: Pure fn `header_facts(kind_input) -> Vec<String>` returning only present fields, joined by `const FACTS_SEPARATOR: &str = " · "`. Playlist: `detail-owner`(owner) · `playlist-track-count`(ref.track_count) · runtime? Album: `artists.join(", ")` if non-empty · `release.year` if present · `playlist-track-count`(track_count) · runtime? Artist: `detail-kind-artist` · `detail-top-track-count`(len) if `Cached`. Empty strings filtered before join.

**Rationale**: Pure → unit + proptest (no leading/trailing/double separator for any subset). Reusing `playlist-track-count` ("N tracks") for every kind avoids a duplicate plural key; its wording is kind-neutral.

**Alternatives**: Build a Fluent message per kind with optional args — rejected: Fluent has no "omit empty arg with its separator" construct; would need 2^n variants.

## R3 — Scroll restoration after Back (does it already work?)

**Finding**: Every library tab list calls `memory.scroll_area(&Tab key)` then `memory.record(key, offset)` each frame. While a detail is open, `App::show_library` returns the `Detail` key, so `shown_last_frame` becomes `Detail(..)`; on the Back frame `show_library` returns `None` → `end_frame(None)` clears it. Next frame the tab's `scroll_area` sees `shown_last_frame != Tab` and seeds `vertical_scroll_offset(stored)`; egui clamps to content height. `LibraryViewState.tab` is untouched while detail is open. So the mechanism is already in place; what is missing is **proof**: no test covers the Library → Detail → Back round trip, the shrink clamp, or width-only resize.

**Decision**: No production change to `SectionMemory`. Add tests S1–S6 (contracts/skeletons-and-restore.md) driving `library_view::show` / `detail_view::show` with a shared `SectionMemory` over frames, asserting first-visible row index. If a test exposes a defect (e.g. the library's loading/refreshing banner shifting content on the return frame, or `auto_shrink([false,true])` interacting with the seed), fix in `library_view.rs` at the call site, not in `SectionMemory`. Clamp: rely on egui's `ScrollArea` clamp and additionally assert offset ≤ `max_scroll` in S3.

**Outcome (T019, US2)**: S1–S6 added to `tests/section_memory.rs` (shared `SectionMemory`, fixed-height `show_rows` frames, first visible row asserted) and all pass on the first run: round trip, active tab kept, shrink clamps to `max_scroll`, 1400→960 width change keeps the first row, restore applied once, nothing on disk / reset forgets. No production change to `library_view.rs`, `app.rs` or `section_memory.rs`.

**Alternatives**: Store first-visible item id and scroll to it — rejected (spec: no new state type; rows fixed-height → offset ≡ index).

## R4 — Why the "…" can leave its row, and the fix

**Finding**: `list_row` computes `text_width = available_after_artwork − ACTIONS_RESERVED_WIDTH(40) − duration_measure` and forces the text column to exactly that width (`set_min_width`). The trailing `right_to_left` region then gets `available − text_width − item_spacing.x(8)` = `40 + D − 8`, but needs `D + item_spacing.x + button_width("…")` where button_width = glyph + 2·`button_padding.x`(8) ≈ 26–30 at default scale, larger under 014 text scaling. When the demand exceeds supply, the right-to-left layout overflows leftward/wraps and the child `Ui`'s `min_rect` grows beyond the row `rect`; at the narrowest list widths, combined with `set_min_width`, the parent width grows and the opener lands outside/after the row. Existing `tests/rows.rs` L7 only checks the default 1024-wide context.

**Decision**: Replace the constant with `actions_reserved_width(ui) = measured_button_width("…") + 2 · item_spacing.x` (reuse `rows::button_width` helper already present for queue rows). Clamp `text_width ≥ 0`. Extend L7 into a width sweep: list-area widths {`HOST_CONTENT_FLOOR` 560, 480, 400, and 960 − nav rail}, every row kind, widest content, text-scale 100 % and max — assert `row_rect.contains_rect(button_rect)` and `button_rect.max.x ≤ viewport clip`. Narrowest list area: at 960 px the 018 dock is `Overlay`/`Hidden` (< `AUTO_HIDE_THRESHOLD` 1024), so the host list area is the window minus the nav rail; the overlay floats above and is not a layout constraint. Sweep goes lower anyway for margin.

**Alternatives**: Increase constant to 48 — rejected (breaks under text scale; still not measured). Hover-reveal the opener to save space — rejected by spec (always visible).

## R5 — Quiet opener and shared menu

**Decision**: In `actions_menu`, opener = `widgets::controls::button(ui, Variant::Quiet, "…")` (015: transparent fill, no outline, `text_primary`, standard hover/pressed blend, 015 focus ring painted by `paint_focus_ring`). Make `actions_menu` `pub(crate)` with signature `(ui, accessible_name, open_request) -> Option<RowAction>`; header calls it with the collection title. Placeholder handling stays in `apply_row_action` (raises `coming-soon`), so the header inherits identical behaviour.

**Alternatives**: New `row_overflow_button` widget — rejected (X: no parallel paths).

## R6 — Running time cost and formatting

**Decision**: `total_ms: u64 = tracks.iter().map(|t| u64::from(t.duration_ms)).sum()` each frame when `Cached`; `format_runtime(total_ms) -> Option<String>`: `None` for empty list; minutes = `total_ms / 60_000`, floored, min 1 for non-empty; `< 60` → `detail-runtime-minutes`; else `detail-runtime-hours` with `hours`, `minutes % 60`. Use `u64` to avoid overflow on huge playlists.

**Rationale**: A 10k-track playlist sums in microseconds; `TrackListState::Cached` is a borrowed slice — no clone. Distinct from `rows::format_duration` (m:ss per track), which stays.

## R7 — Keyboard path for the menu

**Decision**: Reuse 020's `category_row::draw_more` pattern: before `Popup::show`, `consume_key` ArrowUp/ArrowDown/Home/End while the popup is open; track `focused_index` in egui temp memory keyed by `menu_id`; `request_focus` on item `i`; wrap with `rem_euclid(6)` (spec: wrapping — differs from 020's clamp). Enter/Space on focused item = click (egui button default). Escape: popup built-in close. On close (activate or dismiss) request focus back on the stored "return target" id (row `row_response.id` when opened via row keys, else opener id; header → opener). Open triggers: existing Shift+F10 and secondary-click, plus `Key::ContextMenu` if egui 0.36 exposes it — **verify at implementation**; if absent, document Shift+F10 as the sole key path (it already satisfies FR-008 on all three OSes) and record deviation in quickstart. Add `actions::row_menu_item_claims()` = Up/Down/Home/End/Enter/Space/Escape; add Menu key to `row_claims()` when available.

**Implementation outcome (T024)**: egui 0.36's `Key` enum has no Menu/ContextMenu key (`Key::F10` is the closest), so Shift+F10 is the sole row-menu key path; `row_claims()` is unchanged and `row_menu_item_claims()` was added.

**Alternatives**: egui's built-in menu navigation — insufficient: no wrap, no guaranteed focus return.

## R8 — Localisation

**Decision**: New keys in `locales/en-US/library.ftl`; pt-BR values in new `locales/pt-BR/library.ftl` (only this feature's keys, header comment mirroring `pt-BR/effects.ftl`); `tests/fluent_keys.rs` gains `library_025_keys_have_en_us_and_pt_br_parity` and adds the arg keys to its resolve lists. `detail-back` value changes "Back" → "‹ Library" (visible text label, FR-005). Key list: [contracts/fluent-strings.md](./contracts/fluent-strings.md).

## R9 — Skeleton shape and header skeleton

**Decision**: New `SkeletonShape { height, text_lines }` (data-model §4) and `skeleton_row_shaped(ui, shape)`; `skeleton_row(ui, height)` stays as a thin wrapper (2 lines) so untouched callers keep compiling. Interior: artwork square (`ARTWORK_SIZE`, left, vertically centred, same x as `draw_artwork`) + one bar per text line the replaced row kind draws in `rows::draw_content` — track 2 (title, detail), album 2 (title, artists), playlist 2 (title, owner), artist 1 (title); bar widths 60 % then 40 % of the text column; all `faint_bg_color`. Row-kind height comes from caller: `library_view.rs` loading branch picks `ROW_HEIGHT` for SavedTracks/RecentlyPlayed and `WIDE_ROW_HEIGHT` otherwise, via a new `LibraryTab::row_height()`. New `header_skeleton(ui)` = same exact rect as R1 with live Back button, 128 px square, title bar and facts bar, no Play/"…". `Role::Status` "loading" node kept (one per skeleton).

**Alternatives**: Shimmer animation — out of scope (FR-010 only requires shape/height; animation would need repaint scheduling).

## Baseline (T001)

Recorded 2026-09-30 on the feature branch with all US1–US4 work in the tree.

- `cargo test -p modplayer-ui`: all suites and doctests pass, 0 failures.
- `cargo clippy -p modplayer-ui --all-targets -- -D warnings`: clean.
- Toolchain: the shell exports `RUSTUP_TOOLCHAIN=1.93.1`, which overrides `rust-toolchain.toml` (1.95.0) and fails with "requires rustc 1.95". Run cargo as `cargo +1.95.0 …` (or unset the variable). This is a pre-existing environment issue, not a code regression.
