# Contract: Hover, Focus, and Pressed

**Feature**: 015-control-variants | **Spec**: [../spec.md](../spec.md) |
**Plan**: [../plan.md](../plan.md) |
**Data model**: [../data-model.md](../data-model.md) §3, §4, §7

Interaction feedback is **app-wide** (§ 4.5 describes the gap as
app-wide), so every rule below is stated over *every* interactive control
and row, not over the FR-008 conversion set. This feature owns the
interaction-state *visuals*; 003-list-row-and-panel-components owns row
*geometry* (FR-018).

---

## I — The three states

| id | Rule | Pinned by |
|---|---|---|
| **I1** | `hover_fill` = `text_primary` at 4 % alpha, computed in `theme::controls` from the role, never at a call site. | `theme::controls::tests::hover_fill_is_four_percent_foreground` |
| **I2** | `pressed_fill` = `text_primary` at 8 % alpha — exactly double I1. | `theme::controls::tests::pressed_fill_is_double_hover` |
| **I3** | **Normative (FR-011)**: for every variant × theme, resting, hover and pressed are three **pairwise distinct** composited fills whose overlay alphas are strictly increasing. The ordering is the contract; 4 %/8 % are not. | `tests/interaction_states.rs::three_states_are_three_increasing_fills` |
| **I4** | The five `Visuals::widgets` slots are re-differentiated per data-model §4: `hovered` carries I1, `active`/`open` carry I2, `noninteractive`/`inactive` stay at `surface_raised`. This happens **once per frame in `theme::style::build_style`** — not per widget, not per view. | `theme::style::tests::widget_slots_are_re_differentiated` (FR-011a) |
| **I5** | A view that calls a plain `ui.button(...)`/`ui.label(...)` gets I4's states with **no call-site edit**. | I4 + the unedited FR-005 call sites |
| **I6** | Hover and pressed reach every interactive list row — queue, plugin, marker, search/library/detail — through `row_frame`, which changes **no** layout: the fill is written into a shape index reserved before the row's content. | `tests/interaction_states.rs::row_frame_adds_no_layout`; quickstart **M5** |
| **I7** | Host controls resolve their state from the `Response`'s own fields (`hovered`, `is_pointer_button_down_on`), never from `Response::widget_state()`, which folds focus into `Active`. | `tests/interaction_states.rs::host_controls_do_not_use_widget_state` |

---

## F — The focus ring

| id | Rule | Pinned by |
|---|---|---|
| **F1** | The ring is `Stroke::new(2.0, accent)`. | `theme::controls::tests::focus_ring_is_two_px_accent` |
| **F2** | It is drawn on `rect.expand(FOCUS_RING_GAP)` with `StrokeKind::Outside`, `FOCUS_RING_GAP == 1.0` — one pixel of the underlying surface sits between control and ring. | `theme::controls::tests::focus_ring_is_offset` |
| **F3** | Ring and selection stay separable **structurally**, not chromatically: the selection indicator remains interior (`accent` fill, `text.on-accent` label, unchanged from 014 FR-010b) and the ring remains exterior. Neither replaces, hides or merges with the other. | F2 + `tests/interaction_states.rs::ring_and_selection_do_not_overlap`; quickstart **M4** (SC-004) |
| **F4** | The ring is painted by **one** app-level pass — `widgets::controls::paint_focus_ring(ctx)`, the last statement of `App::ui` — reading `Memory::focused()` and `Context::read_response`. No view file paints a ring. | `tests/interaction_states.rs::focus_ring_is_painted_once_app_wide` |
| **F5** | No ring is drawn when nothing is focused, or when the focused widget was not visible this pass or last (`read_response` returns `None`) — so a scrolled-away row leaves no stranded ring. | `tests/interaction_states.rs::no_ring_without_a_visible_focus` |
| **F6** | A disabled control can hold neither pointer interaction nor focus, so it renders no hover, no pressed and no ring (FR-020). | `tests/control_variants.rs::disabled_controls_show_no_feedback` |

---

## N — Known toolkit behaviour, recorded not hidden

| id | Statement |
|---|---|
| **N1** | egui 0.36.2's `Response::widget_state()` maps `has_focus()` to `WidgetState::Active` (`widget_style.rs:104-116`). Built-in widgets this feature does **not** replace — the FR-008b selection controls, `ComboBox`, `Slider`, `DragValue`, and `Default`-variant `ui.button` call sites — therefore render I2's pressed fill while merely focused. They still receive F1–F4's ring, so focus is never ambiguous. There is no `Style` field that separates the two in this toolkit version; host-drawn controls avoid it via I7. Recorded in plan.md § Complexity Tracking. |
| **N2** | A host control's fill settles one frame after a hover begins, because its state is read from the previous pass (egui's own `Checkbox` idiom, `checkbox.rs:69-72`). This is a sampling artefact, not an animation: FR-021 holds, `animation_time` is untouched. |

---

## Y — No animation (FR-021)

| id | Rule | Pinned by |
|---|---|---|
| **Y1** | No easing, transition or duration is introduced; a state changes on the frame its input changes. `Style::animation_time` and `Visuals::interaction` stay exactly as 014 left them. | 014's `style::tests::no_geometry_or_interaction_field_changes`, passing **verbatim** |
