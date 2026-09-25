# Contract: Settings Category Row (single line + overflow)

**Covers**: FR-008, FR-009, FR-010, FR-011, FR-012 · SC-002, SC-003
**Module**: `crates/modplayer-ui/src/settings/category_row.rs` (new), called from
`crates/modplayer-ui/src/settings/mod.rs` in place of the `horizontal_wrapped` block
**Tests**: `crates/modplayer-ui/tests/settings_category_row.rs` (new; unit + proptest + headless egui)

## Public surface

```rust
pub struct RowPartition { pub visible: Vec<usize>, pub pinned: Option<usize>, pub overflow: Vec<usize> }

/// Pure. `widths[i]` = measured width of SettingsCategory::ALL[i]'s item.
pub fn partition(widths: &[f32], selected: usize, more_width: f32, gap: f32, available: f32) -> RowPartition;

#[derive(Default)]
pub struct CategoryRowState { /* last_partition, menu_open, focus_after_close */ }

/// Draws the row on ONE horizontal line; returns Some(category) the frame the user picks one.
pub fn show(ui: &mut egui::Ui, selected: SettingsCategory, state: &mut CategoryRowState)
    -> Option<SettingsCategory>;
```

`settings::show` sets `screen.category` from the return value (and from search results, as
today — a search hit on an overflowed category pins it per R3 on the next draw).

## Clauses

| # | Clause | Verified by |
|---|---|---|
| R1 | `partition` output is a disjoint cover of `0..n`; `visible` and `overflow` are each strictly increasing (canonical `SettingsCategory::ALL` order). | proptest |
| R2 | All fit (`Σw + gap·(n−1) ≤ available`) ⇔ `overflow.is_empty()` ⇔ no More control drawn; then `pinned == None`. | proptest + headless (1600 px wide) |
| R3 | `selected ∉ overflow`. If `selected ≥ k` (outside the maximal fitting prefix) then `pinned == Some(selected)` and it is drawn immediately before More. | proptest + headless |
| R4 | Drawn width `Σ visible w + (pinned w) + more_width + gaps ≤ available` whenever `w[selected] + gap + more_width ≤ available`; `visible` is the longest prefix satisfying it. | proptest |
| R5 | Headless at a 960 × 640 screen with the rail present: every drawn row item (categories + More) has identical `rect.top()` (±0.5 px) and row height == one item height. Repeated inside `with_pseudo_expansion(40, …)` (labels become ⌈1.4·len⌉), and with each of the 11 categories selected. | headless geometry |
| R6 | No drawn category label is elided: each item's galley `elided == false` and its text equals the full (uppercased) label. | headless |
| R7 | More control: `Role::Button`, name `tr("settings-more-a11y")`, `expanded` reflects menu state; painted text `tr("settings-more")`. Menu items: `Role::MenuItem`, name = exact `tr(category.label_key())`, canonical order, only `overflow` indices. | AccessKit |
| R8 | Keyboard: Tab from the settings search box visits visible categories left→right, then the pinned one, then More; Enter on More opens the menu and focuses item 0; ArrowDown/ArrowUp move focus (clamped at ends); Enter selects → menu closed, category selected and pinned, focus on it; Escape closes → focus on More. Every overflowed category is reachable this way (SC-003). | multi-frame headless with `Event::Key` |
| R9 | While a menu item has focus, it registers `Claim::Keys` for Up/Down/Enter/Escape so no global binding on those keys fires. | headless + `tests/actions.rs` pattern |
| R10 | If `partition` differs from `last_partition` while the menu is open, the menu closes that frame and focus goes to More if still drawn, else to the selected category. | headless: open menu at 960 px, then 1600 px frame |
| R11 | Row is built with `ui.horizontal`, never `horizontal_wrapped`; category items keep 014's `section_label` styling and pinned accessible names (FR-019 of 014). | code review + R5/R7 |
