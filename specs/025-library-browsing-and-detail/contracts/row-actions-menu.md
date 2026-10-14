# Contract: Row Actions Menu (025)

Surface: `rows::list_row` + `rows::actions_menu` (now `pub(crate)`, shared with the collection header); key claims in `actions.rs`. Types: [data-model.md §5](../data-model.md). Test home: `tests/rows.rs`, `tests/accessibility.rs`, `tests/interaction_states.rs`, `tests/control_variants.rs`.

| ID | Requirement | Verified by |
|----|-------------|-------------|
| RM1 | "…" opener drawn via `widgets::controls::button(ui, Variant::Quiet, "…")`: resting fill transparent, no outline, `text_primary` glyph; hover/pressed use 015 Quiet blend; keyboard focus paints the standard focus ring. Always visible (not hover-revealed). | FR-009, US3-AS5 |
| RM2 | Opener rect ⊂ row rect **and** ⊂ list viewport clip for every row kind, widest content, at list widths {400, 480, 560 (`HOST_CONTENT_FLOOR`), 960 − nav rail, 1400} and text scale 100 % / max. | FR-007, SC-003, US3-AS1 |
| RM3 | Reserved trailing width = measured opener width + 2 · `item_spacing.x` (+ `duration_measure`); text column ≥ 0; row height unchanged by width. | FR-007 (research R4) |
| RM4 | Popup anchored to the opener of the row that opened it (popup rect min.y within ±1 row of that row, x adjacent to that opener) — also when opened via Shift+F10 on a focused row. | FR-007, US3-AS2 |
| RM5 | Open: focused row + Shift+F10, focused row + Menu/ContextMenu key (if exposed by egui 0.36 — research R7), secondary click, or focused opener + Enter/Space. | FR-008, US3-AS3 |
| RM6 | In open menu: ↓/↑ move focus with wrap (5→0, 0→5); Home→0; End→5; Enter/Space activate focused item; Escape closes without action. Keys are claimed (`row_menu_item_claims`) so no global binding fires. | FR-008 |
| RM7 | After activate or dismiss, keyboard focus returns to the row that opened it (or to the header "…" button for the header menu). | FR-008, SC-005 |
| RM8 | The six actions' effects are unchanged: existing `tests/rows.rs` action tests pass unmodified; placeholders still raise `coming-soon`. Unavailable (greyed) rows keep all six. | FR-008, SC-006, US3-AS4 |
| RM9 | Opener accessible name `row-actions{name}`, role Button; items role MenuItem named by their Fluent labels. | FR-012 |
