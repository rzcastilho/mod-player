// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `settings::category_row` tests (020-shell-navigation-and-gates, US2,
//! contracts/settings-category-row.md R1-R11): the pure `partition()`
//! invariants (proptest), one-line geometry at 960 px, no truncation, the
//! "More" overflow control's AccessKit surface, full keyboard reachability
//! (SC-003), focus-claim registration so no global binding fires while a
//! menu item has focus, and closing the menu when a resize changes the
//! partition.

use egui::accesskit::{NodeId, Role};
use egui::{Context, Event, Key, Modifiers, Pos2, RawInput, Rect, vec2};
use modplayer_core::i18n::with_pseudo_expansion;
use modplayer_core::settings_registry::SettingsCategory;
use modplayer_core::tr;
use modplayer_ui::settings::category_row::{self, CategoryRowState};

const NARROW_WIDTH: f32 = 960.0;
const NARROW_HEIGHT: f32 = 640.0;
const WIDE_WIDTH: f32 = 1600.0;
/// Narrower than the 960 px minimum window (018) on purpose: the keyboard
/// tests need at least two overflowed categories to exercise clamped
/// `ArrowUp`/`ArrowDown` navigation (contract R8), which 960 px alone does
/// not force for every `selected` category. `partition()` is a pure
/// function with no notion of a minimum window width, so exercising it at
/// this width is still a faithful test of the row's own keyboard logic.
const KEYBOARD_TEST_WIDTH: f32 = 520.0;

fn screen_input(width: f32, height: f32) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, height))),
        ..Default::default()
    }
}

fn fresh_context() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx.enable_accesskit();
    ctx
}

/// One AccessKit node's accessibility-relevant fields, plus its bounds —
/// mirrors `tests/high_contrast.rs`'s identically-named helper type.
#[derive(Debug, Clone)]
struct AccessNode {
    id: NodeId,
    role: Role,
    label: Option<String>,
    expanded: Option<bool>,
    bounds: Option<Rect>,
}

fn render(
    ctx: &Context,
    selected: SettingsCategory,
    state: &mut CategoryRowState,
    width: f32,
    height: f32,
    events: Vec<Event>,
) -> (Option<SettingsCategory>, Option<NodeId>, Vec<AccessNode>) {
    let mut input = screen_input(width, height);
    input.events = events;
    let mut chosen = None;
    let mut output = ctx.run_ui(input, |ui| {
        chosen = category_row::show(ui, selected, state);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .unwrap_or_else(|| unreachable!("accesskit_update should be populated once enabled"));
    let focus = update.focus;
    output.drop_without_applying_deltas();

    let nodes = update
        .nodes
        .iter()
        .map(|(id, node)| AccessNode {
            id: *id,
            role: node.role(),
            label: node.label().map(str::to_string),
            expanded: node.is_expanded(),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
        })
        .collect();
    (chosen, Some(focus), nodes)
}

fn find_all<'a>(nodes: &'a [AccessNode], role: Role, name: &str) -> Vec<&'a AccessNode> {
    nodes
        .iter()
        .filter(|node| node.role == role && node.label.as_deref() == Some(name))
        .collect()
}

fn find_one<'a>(nodes: &'a [AccessNode], role: Role, name: &str) -> &'a AccessNode {
    let matches = find_all(nodes, role, name);
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one {role:?} node named `{name}`, found {}: {nodes:?}",
        matches.len()
    );
    matches[0]
}

fn key_event(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn more_a11y_name() -> String {
    tr("settings-more-a11y")
}

// -- Contract R1-R4: partition() invariants (proptest) -----------------

mod partition_invariants {
    use modplayer_ui::settings::category_row::partition;
    use proptest::prelude::*;

    fn is_sorted(values: &[usize]) -> bool {
        values.windows(2).all(|w| w[0] < w[1])
    }

    proptest! {
        #[test]
        fn partition_is_a_disjoint_canonical_cover_that_never_overflows_the_selection(
            widths in proptest::collection::vec(1.0f32..200.0, 1..12),
            selected_seed in 0usize..12,
            more_width in 1.0f32..100.0,
            gap in 0.0f32..20.0,
            available in 0.0f32..2500.0,
        ) {
            let n = widths.len();
            let selected = selected_seed % n;
            let result = partition(&widths, selected, more_width, gap, available);

            // R1: disjoint cover of 0..n, canonical order within each part.
            let mut all: Vec<usize> = result.visible.clone();
            if let Some(pinned) = result.pinned {
                all.push(pinned);
            }
            all.extend(result.overflow.iter().copied());
            all.sort_unstable();
            prop_assert_eq!(all, (0..n).collect::<Vec<_>>());
            prop_assert!(is_sorted(&result.visible));
            prop_assert!(is_sorted(&result.overflow));

            // R2: all fit is a two-sided case — no overflow *and* no pin,
            // everything visible in canonical order. (Outside the all-fit
            // case, `overflow` and `pinned` are *not* a biconditional pair
            // on their own: e.g. a single item, too wide to fit even
            // alone, ends up `pinned = Some(0)` with `overflow` empty —
            // there is simply nothing else left to overflow to.)
            let all_fit = widths.iter().sum::<f32>() + gap * (n as f32 - 1.0) <= available;
            if all_fit {
                prop_assert!(result.overflow.is_empty());
                prop_assert_eq!(result.pinned, None);
                prop_assert_eq!(&result.visible, &(0..n).collect::<Vec<_>>());
            }

            // R3: selected is never in overflow; if it was pushed out of
            // `visible` it must be the pinned index.
            prop_assert!(!result.overflow.contains(&selected));
            if !result.visible.contains(&selected) && !all_fit {
                prop_assert_eq!(result.pinned, Some(selected));
            }

            // R4: drawn width fits whenever the base case
            // (`w[selected] + gap + more_width <= available`) does.
            if widths[selected] + gap + more_width <= available {
                let visible_w: f32 = result.visible.iter().map(|&i| widths[i]).sum();
                let pinned_w: f32 = result.pinned.map(|i| widths[i]).unwrap_or(0.0);
                let more_w = if result.overflow.is_empty() { 0.0 } else { more_width };
                let items = result.visible.len()
                    + usize::from(result.pinned.is_some())
                    + usize::from(!result.overflow.is_empty());
                let gaps = if items > 0 { (items - 1) as f32 * gap } else { 0.0 };
                prop_assert!(visible_w + pinned_w + more_w + gaps <= available + 0.01);
            }
        }
    }
}

// -- Contract R5: one-line geometry -------------------------------------

/// Contract R5: at 960 px, every drawn row item (categories + "More")
/// shares the same `rect.top()` (±0.5 px) and row height, for each of the
/// 11 categories selected in turn — and again under 40 % pseudo-expansion
/// (FR-012).
#[test]
fn every_drawn_row_item_shares_one_baseline_and_height_at_960px() {
    for category in SettingsCategory::ALL {
        for pseudo_expand in [false, true] {
            let ctx = fresh_context();
            let mut state = CategoryRowState::default();
            let mut run = || {
                render(
                    &ctx,
                    category,
                    &mut state,
                    NARROW_WIDTH,
                    NARROW_HEIGHT,
                    Vec::new(),
                )
            };
            let (_, _, nodes) = if pseudo_expand {
                with_pseudo_expansion(40, run)
            } else {
                run()
            };

            let buttons: Vec<Rect> = nodes
                .iter()
                .filter(|n| n.role == Role::Button)
                .filter_map(|n| n.bounds)
                .collect();
            assert!(
                buttons.len() >= 2,
                "expected at least the More control plus one category button \
                 (category={category:?}, pseudo_expand={pseudo_expand}): {nodes:?}"
            );
            let top = buttons[0].top();
            let height = buttons[0].height();
            for rect in &buttons {
                assert!(
                    (rect.top() - top).abs() < 0.5,
                    "category={category:?} pseudo_expand={pseudo_expand}: top mismatch {rect:?} vs {top}"
                );
                assert!(
                    (rect.height() - height).abs() < 0.5,
                    "category={category:?} pseudo_expand={pseudo_expand}: height mismatch {rect:?} vs {height}"
                );
            }
        }
    }
}

// -- Contract R6/R2: no truncation; all 11 fit at 1600 px ----------------

/// Contract R6: no drawn category label is elided (no ellipsis painted).
/// Contract R2 at a 1600 px screen: all 11 categories fit inline, so there
/// is no "More" control at all.
#[test]
fn wide_screen_shows_all_eleven_categories_inline_with_no_more_control() {
    let ctx = fresh_context();
    let mut state = CategoryRowState::default();
    let (_, _, nodes) = render(
        &ctx,
        SettingsCategory::ALL[0],
        &mut state,
        WIDE_WIDTH,
        NARROW_HEIGHT,
        Vec::new(),
    );

    assert!(
        find_all(&nodes, Role::Button, &more_a11y_name()).is_empty(),
        "no More control expected once every category fits: {nodes:?}"
    );

    for category in SettingsCategory::ALL {
        let label = tr(category.label_key());
        assert_eq!(
            find_all(&nodes, Role::Button, &label).len(),
            1,
            "expected exactly one inline Button node for {category:?}: {nodes:?}"
        );
    }
}

/// Contract R6, narrow screen: whatever categories *are* drawn inline
/// still carry their full, un-truncated accessible name (elision would
/// show up as a shortened label since the accessible name is pinned to
/// the exact `tr(category.label_key())` string, not to whatever egui's
/// `Label` decided to paint).
#[test]
fn narrow_screen_never_elides_a_drawn_category_label() {
    let ctx = fresh_context();
    let mut state = CategoryRowState::default();
    let (_, _, nodes) = render(
        &ctx,
        SettingsCategory::ALL[0],
        &mut state,
        NARROW_WIDTH,
        NARROW_HEIGHT,
        Vec::new(),
    );

    for node in nodes.iter().filter(|n| n.role == Role::Button) {
        if let Some(label) = &node.label {
            assert!(
                !label.ends_with('…') && !label.ends_with("..."),
                "a drawn Button node's accessible name looks truncated: {label:?}"
            );
        }
    }
}

// -- Contract R7: the More control's AccessKit surface -------------------

/// Contract R7: "More" is `Role::Button` named `tr("settings-more-a11y")`
/// with `expanded` reflecting the menu's open state; once opened, its menu
/// items are `Role::MenuItem`s named the exact `tr(category.label_key())`,
/// in canonical order, covering only the overflowed categories.
#[test]
fn more_control_and_menu_items_report_the_expected_accesskit_surface() {
    let ctx = fresh_context();
    let mut state = CategoryRowState::default();

    // Closed: `expanded == Some(false)`.
    let (_, _, nodes) = render(
        &ctx,
        SettingsCategory::ALL[0],
        &mut state,
        NARROW_WIDTH,
        NARROW_HEIGHT,
        Vec::new(),
    );
    let more = find_one(&nodes, Role::Button, &more_a11y_name());
    assert_eq!(
        more.expanded,
        Some(false),
        "More must report collapsed: {more:?}"
    );
    assert!(
        find_all(&nodes, Role::MenuItem, "").is_empty()
            && !nodes.iter().any(|n| n.role == Role::MenuItem),
        "no menu items should exist while the menu is closed: {nodes:?}"
    );
    let more_rect = more.bounds.expect("More must have bounds");

    // Open it with a real primary click (press, then release — `clicked()`
    // only registers once both have landed, mirrors `high_contrast.rs`'s
    // own checkbox-click pattern).
    let press = Event::PointerButton {
        pos: more_rect.center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    };
    let release = Event::PointerButton {
        pos: more_rect.center(),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    };
    let _ = render(
        &ctx,
        SettingsCategory::ALL[0],
        &mut state,
        NARROW_WIDTH,
        NARROW_HEIGHT,
        vec![press],
    );
    let (_, _, nodes) = render(
        &ctx,
        SettingsCategory::ALL[0],
        &mut state,
        NARROW_WIDTH,
        NARROW_HEIGHT,
        vec![release],
    );
    let more = find_one(&nodes, Role::Button, &more_a11y_name());
    assert_eq!(
        more.expanded,
        Some(true),
        "More must report expanded once open: {more:?}"
    );

    let mut menu_items: Vec<&AccessNode> =
        nodes.iter().filter(|n| n.role == Role::MenuItem).collect();
    assert!(
        !menu_items.is_empty(),
        "expected at least one overflowed category at 960 px: {nodes:?}"
    );
    menu_items.sort_by(|a, b| {
        a.bounds
            .map(|r| r.top())
            .partial_cmp(&b.bounds.map(|r| r.top()))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Every menu item is named the exact label of some category, and none
    // of them duplicates a category already drawn inline as a Button.
    let inline_labels: Vec<String> = nodes
        .iter()
        .filter(|n| n.role == Role::Button && n.label.as_deref() != Some(&more_a11y_name()))
        .filter_map(|n| n.label.clone())
        .collect();
    let mut last_index: Option<usize> = None;
    for item in &menu_items {
        let label = item.label.clone().unwrap_or_default();
        let index = SettingsCategory::ALL
            .iter()
            .position(|c| tr(c.label_key()) == label)
            .unwrap_or_else(|| panic!("menu item name `{label}` is not any category's label"));
        assert!(
            !inline_labels.contains(&label),
            "`{label}` is drawn both inline and in the overflow menu"
        );
        if let Some(previous) = last_index {
            assert!(
                previous < index,
                "menu items must be in canonical SettingsCategory::ALL order: {menu_items:?}"
            );
        }
        last_index = Some(index);
    }
}

// -- Contract R8/R9: keyboard reachability and focus claims --------------

/// Contract R8 (SC-003): starting from nothing focused, repeated `Tab`
/// presses reach every visible category in canonical order, then the
/// pinned selected category, then "More"; `Enter` on "More" opens the menu
/// and focuses the first item; `ArrowDown`/`ArrowUp` move focus between
/// items, clamped at both ends (never wrapping); `Enter` on a focused item
/// selects it (menu closes, category pinned, focus moves to it next
/// frame). Contract R9: every drawn menu item registers `Claim::Keys` for
/// `Up`/`Down`/`Enter`/`Escape` so no global binding on those keys could
/// ever fire while it has focus.
#[test]
fn tab_reaches_every_overflowed_category_and_arrow_keys_navigate_the_menu() {
    let ctx = fresh_context();
    let mut state = CategoryRowState::default();
    // `About` (the last category) is forced to overflow-then-pin at 960 px
    // by selecting it, exercising the full "visible … pinned … More" Tab
    // order (contract R3/R8), not just a plain visible-then-More case.
    let mut selected = SettingsCategory::About;

    let more_name = more_a11y_name();
    let mut focus = None;
    let mut last_nodes = Vec::new();
    for _ in 0..20 {
        let (_, this_focus, nodes) = render(
            &ctx,
            selected,
            &mut state,
            KEYBOARD_TEST_WIDTH,
            NARROW_HEIGHT,
            vec![key_event(Key::Tab)],
        );
        focus = this_focus;
        last_nodes = nodes;
        let more = find_one(&last_nodes, Role::Button, &more_name);
        if focus == Some(more.id) {
            break;
        }
    }
    let more = find_one(&last_nodes, Role::Button, &more_name);
    assert_eq!(
        focus,
        Some(more.id),
        "Tab must eventually reach the More control: {last_nodes:?}"
    );

    // Enter on More: opens the menu and focuses item 0.
    let (_, focus, nodes) = render(
        &ctx,
        selected,
        &mut state,
        KEYBOARD_TEST_WIDTH,
        NARROW_HEIGHT,
        vec![key_event(Key::Enter)],
    );
    let mut items: Vec<&AccessNode> = nodes.iter().filter(|n| n.role == Role::MenuItem).collect();
    assert!(
        items.len() >= 2,
        "need at least 2 overflowed categories to exercise clamping: {nodes:?}"
    );
    items.sort_by(|a, b| {
        a.bounds
            .map(|r| r.top())
            .partial_cmp(&b.bounds.map(|r| r.top()))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    assert_eq!(
        focus,
        Some(items[0].id),
        "opening by keyboard must focus item 0"
    );

    // R9: the focused item claims Up/Down/Enter/Escape, so no global
    // binding on those keys could fire while it has focus. `NodeId`
    // (AccessKit) and `Id` (egui, what claims are keyed by) are different
    // domains — `ctx.memory`'s own focused-widget id is the one to check.
    let focused_egui_id = ctx
        .memory(|m| m.focused())
        .expect("item 0 should hold real egui keyboard focus");
    let claims = modplayer_ui::actions::claims_snapshot(&ctx);
    match claims.claim_for(focused_egui_id) {
        Some(modplayer_ui::actions::Claim::Keys(keys)) => {
            assert_eq!(
                keys,
                &modplayer_ui::actions::category_menu_item_claims(),
                "unexpected claim set for the focused menu item"
            );
        }
        other => panic!("expected Claim::Keys for the focused menu item, got {other:?}"),
    }

    // `draw_more` calls `set_focus_lock_filter` so `↑`/`↓` move focus
    // *between menu items* rather than triggering egui's own built-in
    // spatial "focus the nearest widget" navigation — but per that API's
    // own doc ("you must first give focus to the widget before calling
    // this"), the filter only takes hold once the newly-focused widget has
    // survived one full extra frame. A real key-repeat interval is far
    // longer than one frame, so every `press_and_settle` below sends the
    // key, then renders one idle follow-up frame before returning — the
    // idle frame's `focus`/`nodes` are what every assertion checks.
    let press_and_settle = |key, state: &mut CategoryRowState| {
        let _ = render(
            &ctx,
            selected,
            state,
            KEYBOARD_TEST_WIDTH,
            NARROW_HEIGHT,
            vec![key_event(key)],
        );
        let (chosen, focus, nodes) = render(
            &ctx,
            selected,
            state,
            KEYBOARD_TEST_WIDTH,
            NARROW_HEIGHT,
            Vec::new(),
        );
        (chosen, focus, nodes)
    };

    // item 0's own filter isn't set until it has already held focus for
    // one full frame (see `press_and_settle`'s doc above) — settle once
    // before the very first arrow press, exactly as every later step's
    // own trailing settle frame already does for the item it just moved
    // focus to.
    let _ = render(
        &ctx,
        selected,
        &mut state,
        KEYBOARD_TEST_WIDTH,
        NARROW_HEIGHT,
        Vec::new(),
    );

    // ArrowUp at item 0 is clamped: focus stays on item 0.
    let (_, focus, nodes) = press_and_settle(Key::ArrowUp, &mut state);
    let item0_id = find_one(&nodes, Role::MenuItem, &items[0].label.clone().unwrap()).id;
    assert_eq!(
        focus,
        Some(item0_id),
        "ArrowUp at the first item must be a no-op"
    );

    // ArrowDown walks through every item in order.
    for i in 0..items.len() - 1 {
        let (_, focus, nodes) = press_and_settle(Key::ArrowDown, &mut state);
        let next_label = items[i + 1].label.clone().unwrap();
        let next_id = find_one(&nodes, Role::MenuItem, &next_label).id;
        let focused_label = focus
            .and_then(|f| nodes.iter().find(|n| n.id == f))
            .cloned();
        assert_eq!(
            focus,
            Some(next_id),
            "ArrowDown must move focus to item {} ({next_label:?}); actually focused: {focused_label:?}",
            i + 1
        );
    }

    // ArrowDown at the last item is clamped: focus stays put.
    let last_label = items.last().unwrap().label.clone().unwrap();
    let (_, focus, nodes) = press_and_settle(Key::ArrowDown, &mut state);
    let last_id = find_one(&nodes, Role::MenuItem, &last_label).id;
    assert_eq!(
        focus,
        Some(last_id),
        "ArrowDown at the last item must be a no-op"
    );

    // Enter selects the focused (last) item: the category is returned this
    // same frame (the click that selects it happens during this frame's
    // draw), but — exactly like `Popup`'s own built-in Escape/click-away
    // close — the menu's content was already drawn before the code could
    // react to the selection, so the *closed* state (no `MenuItem` nodes)
    // and the deferred focus (data-model.md §5's `FocusTarget::Selected`)
    // both only show up the *following* frame.
    let (chosen, _, _) = render(
        &ctx,
        selected,
        &mut state,
        KEYBOARD_TEST_WIDTH,
        NARROW_HEIGHT,
        vec![key_event(Key::Enter)],
    );
    let selected_category = SettingsCategory::ALL
        .iter()
        .copied()
        .find(|c| tr(c.label_key()) == last_label)
        .unwrap();
    assert_eq!(chosen, Some(selected_category));
    selected = chosen.unwrap();

    let (_, focus, nodes) = render(
        &ctx,
        selected,
        &mut state,
        KEYBOARD_TEST_WIDTH,
        NARROW_HEIGHT,
        Vec::new(),
    );
    assert!(
        !nodes.iter().any(|n| n.role == Role::MenuItem),
        "the menu must be closed by the frame after a selection is made"
    );
    let selected_node = find_one(&nodes, Role::Button, &last_label);
    assert_eq!(
        focus,
        Some(selected_node.id),
        "focus must return to the newly selected category the frame after it is drawn"
    );
}

/// Contract R8: `Escape` closes the menu without selecting anything, and
/// this function returns focus to "More".
#[test]
fn escape_closes_the_menu_and_returns_focus_to_more() {
    let ctx = fresh_context();
    let mut state = CategoryRowState::default();
    let selected = SettingsCategory::About;
    let more_name = more_a11y_name();

    let mut focus = None;
    let mut last_nodes = Vec::new();
    for _ in 0..20 {
        let (_, this_focus, nodes) = render(
            &ctx,
            selected,
            &mut state,
            NARROW_WIDTH,
            NARROW_HEIGHT,
            vec![key_event(Key::Tab)],
        );
        focus = this_focus;
        last_nodes = nodes;
        let more = find_one(&last_nodes, Role::Button, &more_name);
        if focus == Some(more.id) {
            break;
        }
    }
    let more = find_one(&last_nodes, Role::Button, &more_name);
    assert_eq!(focus, Some(more.id));

    let (_, _, nodes) = render(
        &ctx,
        selected,
        &mut state,
        NARROW_WIDTH,
        NARROW_HEIGHT,
        vec![key_event(Key::Enter)],
    );
    assert!(
        nodes.iter().any(|n| n.role == Role::MenuItem),
        "menu must be open"
    );

    // `Popup`'s own built-in Escape handling closes the menu for the
    // *following* frame (the content this frame was already drawn before
    // egui reacts to the Escape press, exactly like a click-away close).
    let (chosen, _, _) = render(
        &ctx,
        selected,
        &mut state,
        NARROW_WIDTH,
        NARROW_HEIGHT,
        vec![key_event(Key::Escape)],
    );
    assert_eq!(chosen, None, "Escape must not select anything");

    let (_, focus, nodes) = render(
        &ctx,
        selected,
        &mut state,
        NARROW_WIDTH,
        NARROW_HEIGHT,
        Vec::new(),
    );
    assert!(
        !nodes.iter().any(|n| n.role == Role::MenuItem),
        "Escape must close the menu by the following frame"
    );
    let more = find_one(&nodes, Role::Button, &more_name);
    assert_eq!(
        focus,
        Some(more.id),
        "focus must return to More the frame after Escape closes the menu"
    );
}

// -- Contract R10: partition change while the menu is open ---------------

/// Contract R10: if a resize changes the partition while the menu is
/// open, the menu closes that same frame; focus then lands (the frame
/// after) on "More" if it is still drawn, else on the selected category.
/// Widening from 960 to 1600 px makes every category fit, so "More" stops
/// being drawn at all — focus must land on the selected category.
#[test]
fn resizing_while_the_menu_is_open_closes_it_and_moves_focus_to_the_selected_category() {
    let ctx = fresh_context();
    let mut state = CategoryRowState::default();
    let selected = SettingsCategory::About;
    let more_name = more_a11y_name();

    let mut focus = None;
    let mut last_nodes = Vec::new();
    for _ in 0..20 {
        let (_, this_focus, nodes) = render(
            &ctx,
            selected,
            &mut state,
            NARROW_WIDTH,
            NARROW_HEIGHT,
            vec![key_event(Key::Tab)],
        );
        focus = this_focus;
        last_nodes = nodes;
        let more = find_one(&last_nodes, Role::Button, &more_name);
        if focus == Some(more.id) {
            break;
        }
    }
    let more = find_one(&last_nodes, Role::Button, &more_name);
    assert_eq!(focus, Some(more.id));

    let (_, _, nodes) = render(
        &ctx,
        selected,
        &mut state,
        NARROW_WIDTH,
        NARROW_HEIGHT,
        vec![key_event(Key::Enter)],
    );
    assert!(
        nodes.iter().any(|n| n.role == Role::MenuItem),
        "menu must be open"
    );

    // Resize to 1600 px: the partition changes (all 11 now fit).
    let (_, _, nodes) = render(
        &ctx,
        selected,
        &mut state,
        WIDE_WIDTH,
        NARROW_HEIGHT,
        Vec::new(),
    );
    assert!(
        !nodes.iter().any(|n| n.role == Role::MenuItem),
        "the menu must close the same frame the partition changes"
    );
    assert!(
        find_all(&nodes, Role::Button, &more_name).is_empty(),
        "More must not be drawn once every category fits"
    );

    let (_, focus, nodes) = render(
        &ctx,
        selected,
        &mut state,
        WIDE_WIDTH,
        NARROW_HEIGHT,
        Vec::new(),
    );
    let selected_node = find_one(&nodes, Role::Button, &tr(selected.label_key()));
    assert_eq!(
        focus,
        Some(selected_node.id),
        "focus must land on the selected category once More is gone"
    );
}
