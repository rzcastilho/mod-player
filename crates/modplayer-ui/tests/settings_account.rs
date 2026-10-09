// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! 028-settings-fields-and-account (contracts/account-and-signout.md):
//! the Account summary (A1, A2, A5–A7). Each frame is drawn headlessly at
//! 960×640 inside a scroll area (as the Settings content is) with AccessKit
//! on, and its nodes are read back.

mod common;

use egui::accesskit::Role;
use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};
use modplayer_account::AccountService;
use modplayer_core::i18n::with_pseudo_expansion;
use modplayer_core::tr;
use modplayer_ui::settings::account;

#[derive(Debug, Clone)]
struct Node {
    role: Role,
    text: String,
    bounds: Option<Rect>,
    disabled: bool,
}

const VIEWPORT: Rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(960.0, 640.0));

fn fresh_ctx() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx.enable_accesskit();
    ctx
}

fn frame(ctx: &Context, service: &mut AccountService) -> Vec<Node> {
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(VIEWPORT),
            ..Default::default()
        },
        |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let _ = account::show(ui, service, None);
            });
        },
    );
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit enabled");
    output.drop_without_applying_deltas();
    update
        .nodes
        .iter()
        .map(|(_, node)| Node {
            role: node.role(),
            text: node
                .label()
                .or_else(|| node.value())
                .unwrap_or_default()
                .to_string(),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
            disabled: node.is_disabled(),
        })
        .collect()
}

fn settled(service: &mut AccountService) -> Vec<Node> {
    let ctx = fresh_ctx();
    let _ = frame(&ctx, service);
    frame(&ctx, service)
}

fn find<'a>(nodes: &'a [Node], text: &str) -> &'a Node {
    nodes
        .iter()
        .find(|n| n.text == text)
        .unwrap_or_else(|| panic!("no node named `{text}` in {nodes:#?}"))
}

fn y(nodes: &[Node], text: &str) -> f32 {
    find(nodes, text).bounds.unwrap().min.y
}

fn starts_with<'a>(nodes: &'a [Node], prefix: &str) -> &'a Node {
    nodes
        .iter()
        .find(|n| n.text.starts_with(prefix))
        .unwrap_or_else(|| panic!("no node starting `{prefix}` in {nodes:#?}"))
}

/// A1, A2: the first block is the summary card; identity, tier and
/// last-verified follow its heading in that order.
#[test]
fn a1_a2_summary_card_leads_with_identity_tier_and_last_verified() {
    let mut service = common::active_account("a1");
    let nodes = settled(&mut service);

    let headings: Vec<&Node> = nodes.iter().filter(|n| n.role == Role::Heading).collect();
    assert_eq!(
        headings.first().map(|h| h.text.clone()),
        Some(tr("account-summary-title"))
    );

    let title = y(&nodes, &tr("account-summary-title"));
    let identity = y(&nodes, "Alex");
    let tier = y(&nodes, &tr("account-tier"));
    let verified = y(&nodes, &tr("account-last-verified"));
    assert!(title < identity && identity < tier && tier < verified);

    find(&nodes, &tr("tier-premium"));
    // The sign-in fixture stamps a validation time, so a formatted date
    // (not the "never" fallback, not the RFC 3339 form) follows the label.
    assert!(nodes.iter().all(|n| n.text != tr("account-never-verified")));
    assert!(
        nodes
            .iter()
            .all(|n| !(n.text.contains('T') && n.text.contains("+00:00"))),
        "no RFC 3339 text: {nodes:#?}"
    );
}

/// A5: Re-check (standard) then Sign out (destructive) sit beneath the card.
#[test]
fn a5_actions_follow_the_summary_card_in_order() {
    let mut service = common::active_account("a5");
    let nodes = settled(&mut service);

    let verified = y(&nodes, &tr("account-last-verified"));
    let recheck = y(&nodes, &tr("account-recheck"));
    let sign_out = y(&nodes, &tr("account-sign-out"));
    assert!(verified < recheck, "Re-check below the summary");
    assert!(recheck < sign_out, "Sign out below Re-check");
    assert!(!find(&nodes, &tr("account-recheck")).disabled);
    // The old in-line sentences are gone.
    assert!(nodes.iter().all(|n| !n.text.starts_with("Signed in as")));
}

/// A6: at 960×640 and scroll offset 0 the whole summary card is visible.
#[test]
fn a6_summary_card_is_inside_the_viewport() {
    let mut service = common::active_account("a6");
    let nodes = settled(&mut service);
    for text in [
        tr("account-summary-title"),
        "Alex".to_string(),
        tr("account-tier"),
        tr("account-last-verified"),
    ] {
        let bounds = find(&nodes, &text).bounds.unwrap();
        assert!(
            VIEWPORT.contains_rect(bounds),
            "`{text}` outside {VIEWPORT:?}: {bounds:?}"
        );
    }
}

/// A7: without an account the signed-out view shows, never placeholder text.
#[test]
fn a7_no_account_shows_the_signed_out_view() {
    let mut service = common::signed_out_account("a7");
    let nodes = settled(&mut service);
    find(&nodes, &tr("account-signed-out"));
    assert!(
        nodes
            .iter()
            .all(|n| n.text != tr("placeholder-settings-category"))
    );
    let _ = starts_with(&nodes, &tr("account-sign-in"));
}

// ---- US6: sign-out confirmation (S2–S6) ----

fn input() -> RawInput {
    RawInput {
        screen_rect: Some(VIEWPORT),
        ..Default::default()
    }
}

/// One frame of the Account screen; returns nodes, focused node name and
/// whether the account still has a session after the frame.
fn run(ctx: &Context, service: &mut AccountService, raw: RawInput) -> (Vec<Node>, Option<String>) {
    let mut output = ctx.run_ui(raw, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            let _ = account::show(ui, service, None);
        });
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit enabled");
    output.drop_without_applying_deltas();
    let focused = update
        .nodes
        .iter()
        .find(|(id, _)| *id == update.focus)
        .and_then(|(_, n)| n.label().map(str::to_string));
    let nodes = update
        .nodes
        .iter()
        .map(|(_, node)| Node {
            role: node.role(),
            text: node
                .label()
                .or_else(|| node.value())
                .unwrap_or_default()
                .to_string(),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
            disabled: node.is_disabled(),
        })
        .collect();
    (nodes, focused)
}

fn click(ctx: &Context, service: &mut AccountService, pos: Pos2) {
    for pressed in [true, false] {
        let mut raw = input();
        raw.events.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        });
        let _ = run(ctx, service, raw);
    }
}

fn key(ctx: &Context, service: &mut AccountService, key: Key) {
    for pressed in [true, false] {
        let mut raw = input();
        raw.events.push(Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::default(),
        });
        let _ = run(ctx, service, raw);
    }
}

/// Open the dialog by clicking the page's Sign out button; settle two frames.
fn open_dialog(ctx: &Context, service: &mut AccountService) -> (Vec<Node>, Option<String>) {
    let (nodes, _) = run(ctx, service, input());
    let center = find(&nodes, &tr("account-sign-out"))
        .bounds
        .unwrap()
        .center();
    click(ctx, service, center);
    let _ = run(ctx, service, input());
    run(ctx, service, input())
}

fn modal_rect(nodes: &[Node]) -> Rect {
    nodes
        .iter()
        .filter(|n| n.text == tr("signout-confirm-title"))
        .find_map(|n| n.bounds)
        .expect("dialog title");
    // The dialog's outer rect: the union of every node of the dialog.
    let mut rect = Rect::NOTHING;
    for text in [
        tr("signout-confirm-title"),
        tr("signout-confirm-intro"),
        tr("signout-cancel"),
        tr("signout-confirm"),
    ] {
        // The page's own Sign out button shares the dialog button's name;
        // the dialog is painted last, so take the last match.
        if let Some(b) = nodes
            .iter()
            .filter(|n| n.text == text)
            .filter_map(|n| n.bounds)
            .next_back()
        {
            rect = rect.union(b);
        }
    }
    rect
}

struct TestStore(&'static str);

impl modplayer_account::AccountScopedStore for TestStore {
    fn category_key(&self) -> &'static str {
        self.0
    }
    fn clear(&mut self) -> Result<(), modplayer_account::ClearError> {
        Ok(())
    }
}

/// S2, S3: nothing elided; every item sits inside the dialog inside the
/// viewport, at 40 % longer strings.
#[test]
fn s2_s3_every_consequence_item_is_fully_visible() {
    with_pseudo_expansion(40, || {
        let mut service = common::active_account("s3");
        for key in [
            "signout-category-credential",
            "signout-category-account-details",
        ] {
            service.register_store(Box::new(TestStore(key)));
        }
        let ctx = fresh_ctx();
        let (nodes, _) = open_dialog(&ctx, &mut service);
        assert!(nodes.iter().all(|n| !n.text.contains('…')), "{nodes:#?}");
        let modal = modal_rect(&nodes);
        assert!(VIEWPORT.contains_rect(modal), "modal {modal:?}");
        assert!(modal.width() <= 560.0, "{modal:?}");
        assert!(
            nodes.iter().all(|n| n.role != Role::ScrollView
                || n.bounds
                    .is_none_or(|b| !modal.contains_rect(b) || b == modal)),
            "no scroll area inside the dialog"
        );
        let categories = service.signout_categories();
        assert!(!categories.is_empty());
        for category in categories {
            let item = find(&nodes, &tr(category)).bounds.unwrap();
            assert!(
                modal.contains_rect(item),
                "`{category}` {item:?} ⊄ {modal:?}"
            );
        }
    });
}

/// S4: Cancel holds focus on open; Enter closes it and the session stays.
#[test]
fn s4_cancel_is_focused_and_enter_keeps_the_session() {
    let mut service = common::active_account("s4");
    let ctx = fresh_ctx();
    let (_, focused) = open_dialog(&ctx, &mut service);
    assert_eq!(focused, Some(tr("signout-cancel")));
    key(&ctx, &mut service, Key::Enter);
    let (nodes, _) = run(&ctx, &mut service, input());
    assert!(nodes.iter().all(|n| n.text != tr("signout-confirm-title")));
    assert!(service.account().is_some(), "still signed in");
}

/// S4, manual walk M7 regression: a real mouse click delivers press and
/// release in the same frame, with pointer-move events before them. Cancel
/// must still hold focus once the dialog settles.
#[test]
fn s4_cancel_is_focused_after_a_single_frame_click() {
    let mut service = common::active_account("s4-live");
    let ctx = fresh_ctx();
    let (nodes, _) = run(&ctx, &mut service, input());
    let center = find(&nodes, &tr("account-sign-out"))
        .bounds
        .unwrap()
        .center();
    let mut raw = input();
    raw.events.push(Event::PointerMoved(center));
    let _ = run(&ctx, &mut service, raw);
    let mut raw = input();
    for pressed in [true, false] {
        raw.events.push(Event::PointerButton {
            pos: center,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        });
    }
    let _ = run(&ctx, &mut service, raw);
    let mut focused = None;
    for _ in 0..4 {
        focused = run(&ctx, &mut service, input()).1;
    }
    assert_eq!(focused, Some(tr("signout-cancel")));
}

/// S4, manual walk M7 regression: in the live app the Cancel focus request
/// made on the dialog's opening frame did not survive (first Tab landed on
/// Cancel, Enter did nothing). Model it with a widget drawn after the screen
/// that takes focus on that frame only; Cancel must still end up focused.
#[test]
fn s4_cancel_focus_survives_a_competing_request_on_the_opening_frame() {
    let mut service = common::active_account("s4-steal");
    let ctx = fresh_ctx();
    let (nodes, _) = run(&ctx, &mut service, input());
    let center = find(&nodes, &tr("account-sign-out"))
        .bounds
        .unwrap()
        .center();
    let run_with_thief = |service: &mut AccountService, raw: RawInput, steal: bool| {
        let output = ctx.run_ui(raw, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let _ = account::show(ui, service, None);
            });
            let thief = ui.button("thief");
            if steal {
                thief.request_focus();
            }
        });
        output.drop_without_applying_deltas();
    };
    let mut raw = input();
    for pressed in [true, false] {
        raw.events.push(Event::PointerButton {
            pos: center,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        });
    }
    run_with_thief(&mut service, raw, true);
    run_with_thief(&mut service, input(), false);
    let mut focused = None;
    for _ in 0..2 {
        focused = run(&ctx, &mut service, input()).1;
    }
    assert_eq!(focused, Some(tr("signout-cancel")));
}

/// S5: Escape and backdrop click are Cancel; only Sign out signs out.
#[test]
fn s5_escape_and_backdrop_cancel_only_sign_out_confirms() {
    let mut service = common::active_account("s5");
    let ctx = fresh_ctx();
    let _ = open_dialog(&ctx, &mut service);
    key(&ctx, &mut service, Key::Escape);
    let (nodes, _) = run(&ctx, &mut service, input());
    assert!(nodes.iter().all(|n| n.text != tr("signout-confirm-title")));
    assert!(service.account().is_some());

    let (nodes, _) = open_dialog(&ctx, &mut service);
    let modal = modal_rect(&nodes);
    click(
        &ctx,
        &mut service,
        Pos2::new(modal.min.x - 10.0, modal.min.y - 10.0),
    );
    let (nodes, _) = run(&ctx, &mut service, input());
    assert!(nodes.iter().all(|n| n.text != tr("signout-confirm-title")));
    assert!(service.account().is_some());

    let (nodes, _) = open_dialog(&ctx, &mut service);
    let confirm = nodes
        .iter()
        .filter(|n| n.text == tr("signout-confirm"))
        .filter_map(|n| n.bounds)
        .next_back()
        .unwrap();
    click(&ctx, &mut service, confirm.center());
    assert!(service.account().is_none(), "explicit Sign out signs out");
}

/// S6: Cancel precedes Sign out.
#[test]
fn s6_button_order_is_cancel_then_sign_out() {
    let mut service = common::active_account("s6");
    let ctx = fresh_ctx();
    let (nodes, _) = open_dialog(&ctx, &mut service);
    let cancel = find(&nodes, &tr("signout-cancel")).bounds.unwrap();
    let confirm = nodes
        .iter()
        .filter(|n| n.text == tr("signout-confirm"))
        .filter_map(|n| n.bounds)
        .next_back()
        .unwrap();
    assert!(cancel.min.x < confirm.min.x);
}
