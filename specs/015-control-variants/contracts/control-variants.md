# Contract: Button Variants and the Toggle Switch

**Feature**: 015-control-variants | **Spec**: [../spec.md](../spec.md) |
**Plan**: [../plan.md](../plan.md) |
**Data model**: [../data-model.md](../data-model.md) §2, §5, §7, §8, §9

This contract is internal to `crates/modplayer-ui`: it binds the host's
own control surface, not the plugin API. **No rule here changes
`crates/modplayer-capability-gateway/api/v1.toml`, a DTO, a permission or a manifest field** — Principle IX is
untriggered (FR-008c, 014 A9).

Each rule names the test that pins it. A rule with no test is not a rule.

---

## B — Button variants (FR-001 – FR-006)

| id | Rule | Pinned by |
|---|---|---|
| **B1** | `Variant` has exactly four members: `Primary`, `Default`, `Quiet`, `Destructive`. | `theme::controls::tests::variant_set_is_exactly_four` |
| **B2** | `variant_paint` returns data-model §2's table exactly: `Primary` = `accent` fill / no outline / `text_on_accent`; `Default` = `surface_raised` / 1 px `divider` / `text_primary`; `Quiet` = transparent / none / `text_primary`; `Destructive` = transparent / 1 px `danger` / `danger`. | `theme::controls::tests::variant_paint_matches_the_table` |
| **B3** | The four `(fill, outline.color, outline.width, label)` tuples are **pairwise distinct** in both themes. | `tests/control_variants.rs::four_variants_are_four_distinct_triples` |
| **B4** | Every colour a variant paints is one of 014's ten roles, a data-model §3 derived value, or `TRANSPARENT`; every stroke width comes from `theme::controls`. | `tests/control_variants.rs::variant_colours_come_from_roles` |
| **B5** | `Primary` is applied to `welcome-acknowledge` and appears **at most once** per view — screen + any open modal + the plugin dock. | `tests/control_variants.rs::welcome_has_exactly_one_primary`; quickstart **M1** |
| **B6** | `Destructive` is applied to exactly the FR-003 set: `markers-clear-all`, `markers-clear-yes`, `effects-remove`, `plugin-panel-disable` (**only** while the label reads Disable — `plugins_view.rs` *and* `plugin_panels.rs`), `account-sign-out`, `signout-confirm`. | `tests/control_variants.rs::destructive_sites_are_exactly_fr003`; quickstart **M2**, **M3** |
| **B7** | `Quiet` is applied to the four Queue row actions, and `queue-remove` is `Quiet`, **not** `Destructive`. | `tests/control_variants.rs::queue_row_actions_are_quiet` |
| **B8** | Every button not named by B5–B7 renders `Default`, with its click behaviour, keyboard route and accessible name unchanged (FR-005, FR-017). | the existing view suites, unmodified |
| **B9** | `DESTRUCTIVE_GAP` is a spacing-scale step and is **at least twice** `style.spacing.item_spacing.x`. | `theme::controls::tests::destructive_gap_is_twice_item_spacing` |
| **B10** | A `Destructive` control that shares a row or control group with a non-destructive control is preceded (or followed) by `destructive_gap`; where it has no neighbour, no space is added. | `tests/control_variants.rs::destructive_gap_at_named_instances`; quickstart **M2** |
| **B11** | `Destructive`'s meaning never rests on colour alone: its own label states the action in words and B10's gap is a second non-chromatic signal. No icon or badge is added. | `fluent_keys.rs` (labels unchanged) + B10 |

---

## S — The switch (FR-007, FR-008, FR-008a, FR-008b, FR-008c)

| id | Rule | Pinned by |
|---|---|---|
| **S1** | The toggle visual is a pill track (`radius::full`) with a circular thumb, at data-model §5's metrics, every metric derived from the 014 spacing scale. | `theme::controls::tests::switch_metrics_come_from_the_scale` |
| **S2** | Off = `surface_raised` track + 1 px `divider` outline + `text_secondary` thumb at the **leading** end. On = `accent` track + no outline + `text_on_accent` thumb at the **trailing** end. | `theme::controls::tests::switch_states_match_the_table` |
| **S3** | The thumb's centre **moves** between states by `track.x − thumb − 2 × inset` > 0, so state is not carried by colour alone (NFR-6.4). | `theme::controls::tests::switch_thumb_position_carries_state` |
| **S4** | The switch's state triples match no button variant's triple, and its `radius::full` corner is used by no variant — a switch is not mistakable for a button in any state. | `tests/control_variants.rs::a_switch_is_not_any_button_variant` |
| **S5** | Every control in the FR-008 acceptance set and the FR-008a app-wide set (data-model §9.3) renders the switch; **no** `toggle_value`, boolean `Checkbox` or boolean `selectable_label` survives at those sites. | `tests/control_inventory.rs::every_boolean_control_is_a_switch` (SC-010) |
| **S6** | Every FR-008b one-of-N selection control (data-model §9.3) is **unconverted** and keeps its current appearance. | `tests/control_inventory.rs::no_selection_control_became_a_switch` (SC-010) |
| **S7** | Plugin-contributed booleans inherit the switch through the host lines that already draw them (`plugin_panels.rs:363`, `settings/plugins.rs:167`). **No plugin API surface changes**; the capability gateway's API-reference regeneration diff is empty. | `modplayer-capability-gateway` `api_reference.rs` (unchanged, must show no diff) |

---

## A — Accessibility and behaviour preservation (FR-016, FR-017, SC-008)

| id | Rule | Pinned by |
|---|---|---|
| **A1** | `SwitchKind::Checkbox` reports `WidgetType::Checkbox` → `Role::CheckBox` + `Toggled`; `SwitchKind::Toggle` reports `WidgetType::SelectableLabel` → `Role::Button` + `Toggled`. | `theme`/`widgets` unit + the suites in A2 |
| **A2** | Every existing role/name/state assertion passes **unmodified**: `accessibility.rs` (`:278`, `:317-342`, `:385-409`, `:1434/1447`, `:1966`, `:2141`), `plugins_view.rs` (`:290/313/458/506`), `plugin_panels.rs` (`:503/1062/1066`), `settings_plugins.rs` (`:318/360`). | those suites, **not edited** |
| **A3** | No Fluent key is added, removed or re-cased; a switch renders the same string its checkbox did. | `fluent_keys.rs` |
| **A4** | No click target, keyboard shortcut, confirmation step or persisted/controller-visible state changes. | `controls.rs`, `actions.rs`, `queue_view.rs`, `effects_view.rs`, `markers.rs`, `plugins_view.rs`, `settings_plugins.rs`, `plugin_panels.rs` suites, **not edited** |
| **A5** | A disabled control renders its variant/switch appearance composed with the existing disabled treatment (`text.disabled`, `disabled_alpha`) and shows no hover, focus or pressed feedback. | `tests/control_variants.rs::disabled_controls_show_no_feedback` (FR-020) |

---

## L — Zero literals (FR-019)

| id | Rule | Pinned by |
|---|---|---|
| **L1** | Every colour, alpha, stroke width and switch metric this feature introduces is defined in `crates/modplayer-ui/src/theme/controls.rs` and consumed by name. `widgets/controls.rs` and the meters contain no numeric visual literal. | `tests/control_variants.rs::widget_module_uses_only_token_values` |
| **L2** | 014's literal scan (`tests/design_token_literals.rs`) reports **0 hits**, with its S3 exclusion list unchanged and no new S2 pattern added. | `design_token_literals.rs` (unchanged, `EXPECTED_BASELINE_HITS == 0`) |
| **L3** | No eleventh semantic role is defined; `theme::tokens::Roles` keeps exactly its ten colour fields. | `theme::tokens::tests::roles_table_matches_the_contract` (unchanged) |
