// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Settings category row (020-shell-navigation-and-gates, US2,
//! contracts/settings-category-row.md): the eleven categories drawn on one
//! `ui.horizontal` line at every width ≥ 960 px, with whatever does not fit
//! collapsed into a keyboard-operable "More" overflow menu. Replaces
//! `settings/mod.rs`'s old `ui.horizontal_wrapped` category block
//! (research.md R6/R7).

use egui::accesskit::Role;
use egui::{Align, Id, Key, Layout, Modifiers, Popup, PopupKind, Response, SetOpenCommand, Ui};
use modplayer_core::settings_registry::SettingsCategory;
use modplayer_core::tr;

use crate::actions::{self, Claim};
use crate::theme::{self, controls};
use crate::widgets;

/// One frame's split of `SettingsCategory::ALL` into what is drawn inline,
/// what is pinned just before "More" (the selected category, when it would
/// otherwise overflow), and what is reachable only through the "More" menu
/// (contracts/settings-category-row.md R1-R4). Every index is a position
/// into `SettingsCategory::ALL`, in canonical (display) order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowPartition {
    pub visible: Vec<usize>,
    pub pinned: Option<usize>,
    pub overflow: Vec<usize>,
}

/// Pure. `widths[i]` = measured width of `SettingsCategory::ALL[i]`'s item
/// (research.md R6): all fit ⇒ everything visible, no "More"; otherwise the
/// longest canonical prefix that fits alongside "More" is shown, with the
/// selected category pinned immediately before "More" whenever it would
/// otherwise have overflowed (shrinking the prefix further if needed so the
/// pinned item and "More" both still fit).
///
/// ```
/// # use modplayer_ui::settings::category_row::partition;
/// // Three items, `available` too narrow for all three plus "More": only
/// // the first fits inline, so the rest overflow.
/// let widths = [40.0, 40.0, 40.0];
/// let result = partition(&widths, 0, 20.0, 4.0, 70.0);
/// assert_eq!(result.visible, vec![0]);
/// assert_eq!(result.pinned, None);
/// assert_eq!(result.overflow, vec![1, 2]);
/// ```
pub fn partition(
    widths: &[f32],
    selected: usize,
    more_width: f32,
    gap: f32,
    available: f32,
) -> RowPartition {
    let n = widths.len();
    if n == 0 {
        return RowPartition {
            visible: Vec::new(),
            pinned: None,
            overflow: Vec::new(),
        };
    }

    let all_fit_total: f32 = widths.iter().sum::<f32>() + gap * (n as f32 - 1.0);
    if all_fit_total <= available {
        return RowPartition {
            visible: (0..n).collect(),
            pinned: None,
            overflow: Vec::new(),
        };
    }

    // Largest prefix length `k` (0..=n) with
    // `Σw[0..k] + gap·k + more_width ≤ available`.
    let mut k = 0usize;
    let mut prefix_sums = vec![0.0f32; n + 1];
    for i in 0..n {
        prefix_sums[i + 1] = prefix_sums[i] + widths[i];
        let candidate = prefix_sums[i + 1] + gap * (i as f32 + 1.0) + more_width;
        if candidate <= available {
            k = i + 1;
        } else {
            break;
        }
    }

    if selected < k {
        return RowPartition {
            visible: (0..k).collect(),
            pinned: None,
            overflow: (k..n).collect(),
        };
    }

    // `selected ≥ k`: shrink the prefix to the largest `j ≤ k` that still
    // leaves room for the pinned `selected` item and "More" (floor 0).
    let mut j = 0usize;
    for (candidate_j, &prefix_sum) in prefix_sums.iter().enumerate().take(k + 1) {
        let total = prefix_sum + widths[selected] + gap * (candidate_j as f32 + 1.0) + more_width;
        if total <= available {
            j = candidate_j;
        } else {
            break;
        }
    }

    let visible: Vec<usize> = (0..j).collect();
    let overflow: Vec<usize> = (0..n).filter(|&i| i >= j && i != selected).collect();
    RowPartition {
        visible,
        pinned: Some(selected),
        overflow,
    }
}

/// Which control keyboard focus returns to once the "More" menu closes
/// (data-model.md §5): the menu's own opener, or the category the user just
/// selected (which is not drawn until the *next* frame's `show`, so this is
/// applied then, not immediately).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusTarget {
    More,
    Selected,
}

/// Per-`SettingsScreen` state for [`show`] (data-model.md §5): last frame's
/// partition (to detect a resize while the menu is open, contract R10), a
/// mirror of the "More" popup's own open memory (used only to know whether
/// it *was* open a moment ago), and a deferred focus target applied on the
/// next draw once its widget exists.
#[derive(Debug, Default)]
pub struct CategoryRowState {
    last_partition: Option<RowPartition>,
    menu_open: bool,
    focus_after_close: Option<FocusTarget>,
}

/// The "More" popup's fixed id (never derived from the opener button's own
/// auto id): needed so a resize can be detected and the popup closed
/// *before* the opener itself has been drawn this frame (contract R10).
fn more_menu_id() -> Id {
    Id::new("settings-category-row-more-menu")
}

/// Draw the eleven Settings categories on one `ui.horizontal` line
/// (contracts/settings-category-row.md, never `horizontal_wrapped`),
/// collapsing whatever does not fit into a keyboard-operable "More" menu.
/// Returns `Some(category)` the frame the user picks one, either inline or
/// from the menu.
pub fn show(
    ui: &mut Ui,
    selected: SettingsCategory,
    state: &mut CategoryRowState,
) -> Option<SettingsCategory> {
    let selected_index = SettingsCategory::ALL
        .iter()
        .position(|category| *category == selected)
        .unwrap_or(0);

    let more_label = tr("settings-more");
    let more_width = measure_button_width(ui, &more_label);
    let widths: Vec<f32> = SettingsCategory::ALL
        .iter()
        .map(|category| measure_category_width(ui, &tr(category.label_key())))
        .collect();
    let gap = ui.spacing().item_spacing.x;
    let available = ui.available_width();
    let partition = partition(&widths, selected_index, more_width, gap, available);
    // Contract R5: every drawn item — every category, selected or not, and
    // "More" — is pinned to the same explicit height, so the row draws on
    // one baseline. Left to their own natural sizing, a *selected*
    // category (`Button::selectable`'s own accessibility-bounds handling)
    // and a plain `Button` ("More") each resolve a couple of pixels taller
    // or shorter than an *unselected* category, which is exactly the
    // misalignment R5 forbids.
    let row_height = row_item_height(ui);

    // Contract R10: the partition changed while the menu was open (a
    // resize) — force it closed this frame and defer focus to whichever
    // control the *new* partition still draws.
    if state.menu_open && state.last_partition.as_ref() != Some(&partition) {
        Popup::close_id(ui.ctx(), more_menu_id());
        state.focus_after_close = Some(if partition.overflow.is_empty() {
            FocusTarget::Selected
        } else {
            FocusTarget::More
        });
    }

    let mut chosen = None;
    ui.horizontal(|ui| {
        for &idx in &partition.visible {
            let category = SettingsCategory::ALL[idx];
            if draw_category_item(ui, category, category == selected, row_height, state) {
                chosen = Some(category);
            }
        }
        if let Some(pinned_idx) = partition.pinned {
            let category = SettingsCategory::ALL[pinned_idx];
            if draw_category_item(ui, category, true, row_height, state) {
                chosen = Some(category);
            }
        }
        if !partition.overflow.is_empty()
            && let Some(category) = draw_more(ui, &partition, row_height, state)
        {
            chosen = Some(category);
        }
    });

    state.menu_open = Popup::is_id_open(ui.ctx(), more_menu_id());
    state.last_partition = Some(partition);
    chosen
}

/// One inline category item — the same `Button::selectable` (what
/// `ui.selectable_label` builds) styled through `theme::section_label` the
/// old `horizontal_wrapped` block already drew (research.md R11),
/// accessible name pinned back to the exact, un-uppercased label (FR-019,
/// mirrors `shell::nav_rail`'s own note), now with an explicit `row_height`
/// (contract R5) — left to its own natural size, a *selected* item resolves
/// a couple of pixels taller than an unselected one. Applies a deferred
/// `FocusTarget::Selected` request (contract R8's "Enter selects → … focus
/// on it") the frame this item is drawn selected.
fn draw_category_item(
    ui: &mut Ui,
    category: SettingsCategory,
    selected: bool,
    row_height: f32,
    state: &mut CategoryRowState,
) -> bool {
    let label = tr(category.label_key());
    let response = ui.add(
        egui::Button::selectable(selected, theme::section_label(&label))
            .min_size(egui::vec2(0.0, row_height)),
    );
    ui.ctx().accesskit_node_builder(response.id, |b| {
        b.set_label(label.clone());
    });
    if selected && state.focus_after_close == Some(FocusTarget::Selected) {
        response.request_focus();
        state.focus_after_close = None;
    }
    response.clicked()
}

/// The "More" control and its overflow menu (contracts/settings-category-
/// row.md R7-R9, research.md R7): a quiet-variant button named
/// `tr("settings-more-a11y")` with `expanded` reflecting the menu's open
/// state, opening a `PopupKind::Menu` of `Role::MenuItem`s in overflow
/// (canonical) order. Keyboard: opening by `Enter`/`Space` focuses the
/// first item; `↑`/`↓` move focus between items, clamped at the ends;
/// `Enter` selects (egui's own focused-widget activation, no extra
/// handling needed); `Escape` closes via the popup's own built-in handling
/// and this function returns focus to "More". `row_height` (contract R5)
/// is applied by briefly raising `spacing.interact_size.y` — the same
/// floor `Button`'s own min-height clamp already reads — around the one
/// `widgets::controls::button` call, rather than growing that shared
/// widget's own signature for this one caller.
fn draw_more(
    ui: &mut Ui,
    partition: &RowPartition,
    row_height: f32,
    state: &mut CategoryRowState,
) -> Option<SettingsCategory> {
    let menu_id = more_menu_id();
    let more_text = tr("settings-more");
    let a11y_name = tr("settings-more-a11y");

    let previous_interact_height = ui.spacing().interact_size.y;
    ui.style_mut().spacing.interact_size.y = row_height;
    let opener = widgets::controls::button(ui, controls::Variant::Quiet, more_text);
    ui.style_mut().spacing.interact_size.y = previous_interact_height;
    if state.focus_after_close == Some(FocusTarget::More) {
        opener.request_focus();
        state.focus_after_close = None;
    }

    let was_open = Popup::is_id_open(ui.ctx(), menu_id);
    let opened_by_keyboard = !was_open
        && opener.has_focus()
        && ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Space));

    let command = if opener.clicked() {
        Some(SetOpenCommand::Toggle)
    } else {
        None
    };

    let arrow_down = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown));
    let arrow_up = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp));
    // The item to move *from* — captured once, before any item in this
    // pass is drawn or re-focused. Checking `response.has_focus()` live,
    // item by item, would also catch the item this same pass just handed
    // focus *to* (a `request_focus()` call takes effect immediately), so
    // a single key press would cascade focus all the way to the last item
    // instead of moving it by one.
    let originally_focused = ui.memory(|memory| memory.focused());

    let mut chosen = None;
    Popup::new(menu_id, ui.ctx().clone(), &opener, opener.layer_id)
        .kind(PopupKind::Menu)
        .layout(Layout::top_down_justified(Align::Min))
        .open_memory(command)
        .show(|ui| {
            let mut pending_forward_focus = opened_by_keyboard;
            let mut previous_response: Option<Response> = None;

            for &idx in &partition.overflow {
                let category = SettingsCategory::ALL[idx];
                let label = tr(category.label_key());
                let response = ui.button(label.clone());

                ui.ctx().accesskit_node_builder(response.id, |b| {
                    b.set_role(Role::MenuItem);
                    b.set_label(label);
                });
                actions::register_claim(
                    ui.ctx(),
                    response.id,
                    Claim::Keys(actions::category_menu_item_claims()),
                );
                // Contract R8: `↑`/`↓` move focus *between menu items*, not
                // egui's own built-in spatial "focus the nearest widget in
                // that direction" navigation (`memory/mod.rs`'s
                // `FocusDirection`), which would otherwise steal focus to
                // whatever category button happens to sit above/below this
                // popup on screen (mirrors `effects_view.rs`'s reorder
                // handle, `markers.rs`'s glyph — the same reason, the same
                // fix).
                ui.memory_mut(|memory| {
                    memory.set_focus_lock_filter(
                        response.id,
                        egui::EventFilter {
                            vertical_arrows: true,
                            ..egui::EventFilter::default()
                        },
                    );
                });

                if pending_forward_focus {
                    response.request_focus();
                    pending_forward_focus = false;
                }

                if Some(response.id) == originally_focused {
                    if arrow_down {
                        pending_forward_focus = true;
                    } else if arrow_up && let Some(prev) = &previous_response {
                        prev.request_focus();
                    }
                }

                if response.clicked() {
                    chosen = Some(category);
                }

                previous_response = Some(response);
            }
        });

    if chosen.is_some() {
        Popup::close_id(ui.ctx(), menu_id);
        state.focus_after_close = Some(FocusTarget::Selected);
    } else if was_open && !Popup::is_id_open(ui.ctx(), menu_id) {
        // Closed without a selection (Escape, or a click outside) — the
        // popup's own built-in handling already closed it.
        state.focus_after_close = Some(FocusTarget::More);
    }

    // `expanded` reflects *this frame's* resulting open state (contract
    // R7) — read after `.show()`, not the pre-toggle `was_open`, so a
    // click/Enter that opens or closes the menu is reflected the same
    // frame it happens.
    let is_open_now = Popup::is_id_open(ui.ctx(), menu_id);
    ui.ctx().accesskit_node_builder(opener.id, |b| {
        b.set_label(a11y_name);
        b.set_expanded(is_open_now);
    });

    chosen
}

/// Contract R5's shared row height: the taller of the two text styles this
/// row ever draws (`theme::section_label`'s `text::SECTION` for categories,
/// the plain `Button` style for "More"), plus vertical button padding on
/// both sides, floored at the style's own `interact_size.y` — every value
/// read from the current `Ui`'s fonts/spacing, never a literal.
fn row_item_height(ui: &Ui) -> f32 {
    let section_height = ui.text_style_height(&theme::tokens::text::SECTION);
    let button_height = ui.text_style_height(&egui::TextStyle::Button);
    let content_height = section_height.max(button_height) + 2.0 * ui.spacing().button_padding.y;
    content_height.max(ui.spacing().interact_size.y)
}

/// A category item's drawn width (research.md R6 point 2): the uppercased
/// `section_label` galley, laid out with no wrap, plus `spacing.
/// button_padding.x` on both sides — the exact box `ui.selectable_label`
/// draws (`Button::selectable` uses the same padding).
fn measure_category_width(ui: &Ui, label: &str) -> f32 {
    let galley = egui::WidgetText::from(theme::section_label(label)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    );
    galley.size().x + 2.0 * ui.spacing().button_padding.x
}

/// The "More" control's drawn width, measured the same way as
/// [`measure_category_width`] but through the plain `Button` text style
/// `widgets::controls::button` itself draws with (mirrors `plugin_panels.
/// rs`'s `measure_button_width`).
fn measure_button_width(ui: &Ui, label: &str) -> f32 {
    let galley = egui::WidgetText::from(label.to_string()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    galley.size().x + 2.0 * ui.spacing().button_padding.x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_fit_is_one_partition_with_no_more() {
        let widths = [10.0, 10.0, 10.0];
        let result = partition(&widths, 1, 20.0, 4.0, 1000.0);
        assert_eq!(result.visible, vec![0, 1, 2]);
        assert_eq!(result.pinned, None);
        assert!(result.overflow.is_empty());
    }

    #[test]
    fn selected_outside_the_fitting_prefix_is_pinned_before_more() {
        // Each item is 40 wide, gap 4, more 20. Only index 0 fits alongside
        // a pinned selected item + More at this width.
        let widths = [40.0, 40.0, 40.0, 40.0];
        let result = partition(&widths, 3, 20.0, 4.0, 40.0 + 4.0 + 40.0 + 4.0 + 20.0);
        assert_eq!(result.pinned, Some(3));
        assert!(!result.overflow.contains(&3));
        assert!(result.visible.iter().all(|&i| i < 3));
    }
}
