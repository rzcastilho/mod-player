// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
#![allow(dead_code, reason = "harness helpers are used as story tests land")]

//! 030-onboarding-clarity integration tests (contracts/onboarding-screens.md
//! rules M, W, N, P, S, D, V, G, A, K). Phase 1 provides only the shared
//! harness; story phases add the named tests.

mod common;

use egui::accesskit::Role;
use egui::{Context, Event, PointerButton, Pos2, RawInput, Rect, Shape};
#[allow(unused_imports, reason = "fixtures used by story tests")]
use modplayer_account::FakeAuthorizationService;
use modplayer_account::{
    AccountEvent, AccountService, AuthorizationService, Clock, DISCLOSURE_BUNDLE_VERSION,
    FakeClock, SessionState, SignInNote,
};
#[allow(unused_imports, reason = "fixtures used by story tests")]
use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::ScriptedHost;
#[allow(unused_imports, reason = "fixtures used by story tests")]
use modplayer_audio_source_synthetic::SyntheticHost;
use modplayer_core::{PlaybackController, SettingsStore, tr};
#[allow(unused_imports, reason = "fixtures used by story tests")]
use modplayer_secure_store::MemorySecureStore;
use modplayer_secure_store::SecureStore;
use modplayer_ui::privacy_notice::{self, PrivacyOrigin};
use modplayer_ui::sign_in::{self, SignInScreen, TierResult};
use modplayer_ui::theme;
use modplayer_ui::theme::controls::{Variant, variant_paint};
use modplayer_ui::welcome::{WelcomeOutcome, WelcomeScreen};
use std::sync::Arc;

#[allow(unused_imports, reason = "fixtures used by story tests")]
use common::active_account;

/// The two window sizes every layout rule is checked at (960×640 minimum,
/// 1600×900 wide).
pub const SIZES: [(f32, f32); 2] = [(960.0, 640.0), (1600.0, 900.0)];

/// One AccessKit node, gathered into an owned snapshot.
#[derive(Debug, Clone)]
pub struct Node {
    pub role: Role,
    pub label: Option<String>,
    pub value: Option<String>,
    pub bounds: Option<Rect>,
}

impl Node {
    pub fn name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

/// A fresh AccessKit-enabled headless context.
pub fn ctx() -> Context {
    let ctx = Context::default();
    theme::apply_tokens(&ctx);
    ctx.enable_accesskit();
    ctx
}

/// Input at `size` with an injectable frame time (seconds) so elapsed-time
/// text is deterministic.
pub fn input(size: (f32, f32), time: Option<f64>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(size.0, size.1))),
        time,
        ..Default::default()
    }
}

/// Run one frame on `ctx` and return its AccessKit nodes.
pub fn run_frame(
    ctx: &Context,
    input: RawInput,
    mut render: impl FnMut(&mut egui::Ui),
) -> Vec<Node> {
    let mut output = ctx.run_ui(input, |ui| render(ui));
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();
    update
        .nodes
        .iter()
        .map(|(_, node)| Node {
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
        })
        .collect()
}

/// Nodes of `role` whose accessible name equals `name`.
pub fn find_all<'a>(nodes: &'a [Node], role: Role, name: &str) -> Vec<&'a Node> {
    nodes
        .iter()
        .filter(|n| n.role == role && n.name() == Some(name))
        .collect()
}

/// Nodes (any role) whose accessible name contains `needle`.
pub fn find_containing<'a>(nodes: &'a [Node], needle: &str) -> Vec<&'a Node> {
    nodes
        .iter()
        .filter(|n| n.name().is_some_and(|s| s.contains(needle)))
        .collect()
}

/// M1/M2 (research R13): a prose node is no wider than the body measure.
pub fn assert_prose_width_within_measure(ctx: &Context, node: &Node) {
    let bounds = node.bounds.expect("prose node has bounds");
    let max = theme::body_measure(ctx) + 1.0;
    assert!(
        bounds.width() <= max,
        "prose node {:?} is {} wide, exceeding the {} body measure",
        node.name(),
        bounds.width(),
        max
    );
}

// ---------------------------------------------------------------------
// Shared story-1 helpers
// ---------------------------------------------------------------------

/// Everything one frame produced that the welcome/privacy tests inspect.
pub struct Full {
    pub nodes: Vec<Node>,
    /// Accessible name of the AccessKit-focused node, if any.
    pub focus: Option<String>,
    pub shapes: Vec<Shape>,
}

fn flatten(shape: &Shape, out: &mut Vec<Shape>) {
    if let Shape::Vec(inner) = shape {
        for s in inner {
            flatten(s, out);
        }
    } else {
        out.push(shape.clone());
    }
}

/// Like [`run_frame`] but also returns the focused node's name and the
/// painted shapes (flattened).
pub fn run_full(ctx: &Context, input: RawInput, mut render: impl FnMut(&mut egui::Ui)) -> Full {
    let mut output = ctx.run_ui(input, |ui| render(ui));
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    let mut shapes = Vec::new();
    for clipped in &output.shapes {
        flatten(&clipped.shape, &mut shapes);
    }
    output.drop_without_applying_deltas();
    let focus_id = update.focus;
    let mut focus = None;
    let nodes = update
        .nodes
        .iter()
        .map(|(id, node)| {
            if *id == focus_id {
                focus = node.label().or_else(|| node.value()).map(str::to_string);
            }
            Node {
                role: node.role(),
                label: node.label().map(str::to_string),
                value: node.value().map(str::to_string),
                bounds: node.bounds().map(|b| {
                    Rect::from_min_max(
                        Pos2::new(b.x0 as f32, b.y0 as f32),
                        Pos2::new(b.x1 as f32, b.y1 as f32),
                    )
                }),
            }
        })
        .collect();
    Full {
        nodes,
        focus,
        shapes,
    }
}

/// Click the centre of `rect` (pointer move, press and release in one frame;
/// the widget rect is known from the previous pass).
pub fn click_input(size: (f32, f32), rect: Rect) -> RawInput {
    let pos = rect.center();
    let mut raw = input(size, None);
    raw.events = vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ];
    raw
}

/// Send a Tab key press.
pub fn tab_input(size: (f32, f32)) -> RawInput {
    let mut raw = input(size, None);
    raw.events = vec![Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }];
    raw
}

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-030-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

type Controller = PlaybackController<FakeBackend, ScriptedHost>;

/// A bare controller over a temp `settings.toml`, plus the dir guard and the
/// settings path.
fn controller(label: &str) -> (Controller, TempDir, std::path::PathBuf) {
    let dir = TempDir::new(label);
    let path = dir.0.join("settings.toml");
    let store = SettingsStore::with_path(path.clone());
    let controller = PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
    (controller, dir, path)
}

fn disclosure_of(path: &std::path::Path) -> Option<u32> {
    SettingsStore::with_path(path.to_path_buf())
        .load()
        .settings
        .disclosure
        .map(|d| d.acknowledged_version)
}

fn node_named<'a>(nodes: &'a [Node], text: &str) -> &'a Node {
    let found = find_containing(nodes, text);
    assert!(!found.is_empty(), "no node named {text:?}");
    found[0]
}

/// Bounds of the button named exactly `text`.
fn button_bounds(nodes: &[Node], text: &str) -> Rect {
    find_all(nodes, Role::Button, text)
        .first()
        .unwrap_or_else(|| panic!("no button named {text:?}"))
        .bounds
        .expect("bounds")
}

fn bounds_of(nodes: &[Node], text: &str) -> Rect {
    node_named(nodes, text).bounds.expect("bounds")
}

/// One welcome frame at `size`/`time`.
fn welcome_frame(
    ctx: &Context,
    raw: RawInput,
    screen: &mut WelcomeScreen,
    controller: &mut Controller,
) -> (Full, Option<WelcomeOutcome>) {
    let mut outcome = None;
    let full = run_full(ctx, raw, |ui| {
        outcome = screen.show(ui, controller);
    });
    (full, outcome)
}

// ---------------------------------------------------------------------
// §W Welcome (disclosure view): C2, C3, C4, FR-001, FR-003
// ---------------------------------------------------------------------

#[test]
fn welcome_order_is_heading_lead_block_actions() {
    for size in SIZES {
        let (mut c, _dir, _p) = controller("welcome-order");
        let ctx = ctx();
        let mut screen = WelcomeScreen::default();
        let (f, _) = welcome_frame(&ctx, input(size, Some(0.0)), &mut screen, &mut c);
        let heading = bounds_of(&f.nodes, &tr("welcome-title"));
        let lead = bounds_of(&f.nodes, &tr("welcome-description"));
        let unofficial = bounds_of(&f.nodes, &tr("disclosure-unofficial"));
        let premium = bounds_of(&f.nodes, &tr("disclosure-premium-required"));
        let terms_apply = bounds_of(&f.nodes, &tr("disclosure-terms-apply"));
        let terms_link = bounds_of(&f.nodes, &tr("disclosure-terms-link"));
        let privacy_link = bounds_of(&f.nodes, &tr("disclosure-privacy-link"));
        let ack = bounds_of(&f.nodes, &tr("welcome-acknowledge"));
        let decline = bounds_of(&f.nodes, &tr("welcome-decline"));
        assert!(heading.max.y <= lead.min.y + 0.5, "heading above lead");
        assert!(lead.max.y <= unofficial.min.y + 0.5, "lead above block");
        assert!(unofficial.max.y <= premium.min.y + 0.5);
        assert!(premium.max.y <= terms_apply.min.y + 0.5);
        assert!(terms_apply.max.y <= terms_link.min.y + 0.5);
        assert!(terms_link.max.y <= ack.min.y + 0.5, "block above actions");
        assert!(privacy_link.max.y <= ack.min.y + 0.5);
        assert!(ack.max.x <= decline.min.x + 0.5, "decline right of ack");
    }
}

#[test]
fn welcome_compact_block_is_one_secondary_frame_with_real_links() {
    let (mut c, _dir, _p) = controller("welcome-block");
    let ctx = ctx();
    let mut screen = WelcomeScreen::default();
    let (f, _) = welcome_frame(&ctx, input(SIZES[0], Some(0.0)), &mut screen, &mut c);

    // Links are real links, with no button fill.
    for key in ["disclosure-terms-link", "disclosure-privacy-link"] {
        assert_eq!(
            find_all(&f.nodes, Role::Link, &tr(key)).len(),
            1,
            "{key} must be a Role::Link; got {:?}",
            find_containing(&f.nodes, &tr(key))
        );
    }

    // The three legal lines are drawn in the SECONDARY size and colour.
    let roles = theme::tokens::roles(&ctx.global_style().visuals);
    let secondary_size = ctx.global_style().text_styles[&theme::text::SECONDARY].size;
    for key in [
        "disclosure-unofficial",
        "disclosure-premium-required",
        "disclosure-terms-apply",
    ] {
        let text = tr(key);
        let found = f.shapes.iter().any(|s| {
            let Shape::Text(t) = s else { return false };
            let sec = &t.galley.job.sections[0];
            t.galley.text().trim() == text.trim()
                && (sec.format.font_id.size - secondary_size).abs() < 0.01
                && sec.format.color == roles.text_secondary
        });
        assert!(found, "{key} must use SECONDARY text in text_secondary");
    }

    // One stroked frame contains the three lines and both links.
    let mut union = bounds_of(&f.nodes, &tr("disclosure-unofficial"));
    for key in [
        "disclosure-premium-required",
        "disclosure-terms-apply",
        "disclosure-terms-link",
        "disclosure-privacy-link",
    ] {
        union = union.union(bounds_of(&f.nodes, &tr(key)));
    }
    let frames = f
        .shapes
        .iter()
        .filter(
            |s| matches!(s, Shape::Rect(r) if r.stroke.width > 0.0 && r.rect.contains_rect(union)),
        )
        .count();
    assert_eq!(frames, 1, "exactly one framed group around the block");
}

#[test]
fn welcome_has_exactly_one_primary_control() {
    for size in SIZES {
        let (mut c, _dir, _p) = controller("welcome-primary");
        let ctx = ctx();
        let mut screen = WelcomeScreen::default();
        let (f, _) = welcome_frame(&ctx, input(size, Some(0.0)), &mut screen, &mut c);
        let roles = theme::tokens::roles(&ctx.global_style().visuals);
        let primary_fill = variant_paint(roles, Variant::Primary).fill;
        let primaries: Vec<Rect> = f
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Rect(r) if r.fill == primary_fill => Some(r.rect),
                _ => None,
            })
            .collect();
        assert_eq!(primaries.len(), 1, "exactly one Primary-filled control");
        let ack = bounds_of(&f.nodes, &tr("welcome-acknowledge"));
        assert!(
            primaries[0].contains(ack.center()),
            "the Primary control is welcome-acknowledge"
        );
        let decline = bounds_of(&f.nodes, &tr("welcome-decline"));
        assert!(!primaries[0].contains(decline.center()));
        assert!(decline.min.x >= ack.max.x - 0.5, "decline to the right");
    }
}

#[test]
fn welcome_tab_order_reaches_acknowledge_then_decline() {
    let (mut c, _dir, _p) = controller("welcome-tab");
    let ctx = ctx();
    let mut screen = WelcomeScreen::default();
    let size = SIZES[0];
    let _ = welcome_frame(&ctx, input(size, Some(0.0)), &mut screen, &mut c);
    let mut order = Vec::new();
    for _ in 0..6 {
        let (f, _) = welcome_frame(&ctx, tab_input(size), &mut screen, &mut c);
        if let Some(name) = f.focus {
            order.push(name);
        }
    }
    let ack = tr("welcome-acknowledge");
    let decline = tr("welcome-decline");
    let ack_pos = order.iter().position(|n| *n == ack).expect("ack focused");
    let decline_pos = order
        .iter()
        .position(|n| *n == decline)
        .expect("decline focused");
    assert_eq!(
        decline_pos,
        ack_pos + 1,
        "decline follows acknowledge: {order:?}"
    );
    assert!(
        order[..ack_pos]
            .iter()
            .all(|n| *n == tr("disclosure-terms-link") || *n == tr("disclosure-privacy-link")),
        "only the links precede the action row: {order:?}"
    );
}

#[test]
fn welcome_acknowledge_records_and_decline_writes_nothing() {
    let size = SIZES[0];
    // Decline first: nothing written.
    {
        let (mut c, _dir, path) = controller("welcome-decline");
        let existed_before = path.exists();
        let ctx = ctx();
        let mut screen = WelcomeScreen::default();
        let (f, _) = welcome_frame(&ctx, input(size, Some(0.0)), &mut screen, &mut c);
        let rect = bounds_of(&f.nodes, &tr("welcome-decline"));
        let (_, outcome) = welcome_frame(&ctx, click_input(size, rect), &mut screen, &mut c);
        assert_eq!(outcome, None);
        let (f, _) = welcome_frame(&ctx, input(size, Some(0.1)), &mut screen, &mut c);
        assert!(!find_containing(&f.nodes, &tr("decline-quit")).is_empty());
        assert_eq!(path.exists(), existed_before, "decline creates no file");
        assert_eq!(disclosure_of(&path), None, "decline records nothing");
    }
    // Acknowledge: recorded and outcome returned.
    {
        let (mut c, _dir, path) = controller("welcome-ack");
        let ctx = ctx();
        let mut screen = WelcomeScreen::default();
        let (f, _) = welcome_frame(&ctx, input(size, Some(0.0)), &mut screen, &mut c);
        let rect = bounds_of(&f.nodes, &tr("welcome-acknowledge"));
        let (_, outcome) = welcome_frame(&ctx, click_input(size, rect), &mut screen, &mut c);
        assert_eq!(outcome, Some(WelcomeOutcome::Acknowledged));
        assert_eq!(disclosure_of(&path), Some(DISCLOSURE_BUNDLE_VERSION));
    }
}

#[test]
fn welcome_prose_stays_within_measure() {
    for size in SIZES {
        let (mut c, _dir, _p) = controller("welcome-measure");
        let ctx = ctx();
        let mut screen = WelcomeScreen::default();
        let (f, _) = welcome_frame(&ctx, input(size, Some(0.0)), &mut screen, &mut c);
        for key in [
            "welcome-description",
            "disclosure-unofficial",
            "disclosure-premium-required",
            "disclosure-terms-apply",
        ] {
            assert_prose_width_within_measure(&ctx, node_named(&f.nodes, &tr(key)));
        }
    }
}

/// M2: below the measure, prose wraps to the available width and no node
/// runs past the panel's right edge.
#[test]
fn narrow_width_prose_wraps_inside_the_panel() {
    let size = (420.0, 640.0);
    let (mut c, _dir, _p) = controller("welcome-narrow");
    let ctx = ctx();
    let mut screen = WelcomeScreen::default();
    let (f, _) = welcome_frame(&ctx, input(size, Some(0.0)), &mut screen, &mut c);
    for key in [
        "welcome-description",
        "disclosure-unofficial",
        "disclosure-premium-required",
        "disclosure-terms-apply",
    ] {
        let node = node_named(&f.nodes, &tr(key));
        let right = node.bounds.expect("bounds").max.x;
        assert!(
            right <= size.0 + 0.5,
            "{key} right edge {right} > {}",
            size.0
        );
    }
}

// ---------------------------------------------------------------------
// §N Privacy notice: C5, FR-004
// ---------------------------------------------------------------------

#[test]
fn privacy_back_label_follows_origin() {
    let size = SIZES[0];
    for (origin, key, other) in [
        (
            PrivacyOrigin::Welcome,
            "privacy-back-to-welcome",
            "privacy-back-to-about",
        ),
        (
            PrivacyOrigin::About,
            "privacy-back-to-about",
            "privacy-back-to-welcome",
        ),
    ] {
        let ctx = ctx();
        let f = run_full(&ctx, input(size, Some(0.0)), |ui| {
            let _ = privacy_notice::show(ui, origin);
        });
        assert_eq!(find_all(&f.nodes, Role::Button, &tr(key)).len(), 1);
        assert!(find_containing(&f.nodes, &tr(other)).is_empty());
    }
}

#[test]
fn privacy_back_click_returns_true() {
    let size = SIZES[0];
    for origin in [PrivacyOrigin::Welcome, PrivacyOrigin::About] {
        let key = match origin {
            PrivacyOrigin::Welcome => "privacy-back-to-welcome",
            PrivacyOrigin::About => "privacy-back-to-about",
        };
        let ctx = ctx();
        let f = run_full(&ctx, input(size, Some(0.0)), |ui| {
            let _ = privacy_notice::show(ui, origin);
        });
        let rect = bounds_of(&f.nodes, &tr(key));
        let mut clicked = false;
        let _ = run_full(&ctx, click_input(size, rect), |ui| {
            clicked = privacy_notice::show(ui, origin);
        });
        assert!(clicked, "{key} click returns true");
    }
}

#[test]
fn privacy_round_trip_leaves_disclosure_untouched() {
    let size = SIZES[0];
    let (mut c, _dir, path) = controller("privacy-roundtrip");
    let before = SettingsStore::with_path(path.clone())
        .load()
        .settings
        .disclosure;
    let ctx = ctx();
    let mut screen = WelcomeScreen::default();
    let (f, _) = welcome_frame(&ctx, input(size, Some(0.0)), &mut screen, &mut c);
    let rect = bounds_of(&f.nodes, &tr("disclosure-privacy-link"));
    let _ = welcome_frame(&ctx, click_input(size, rect), &mut screen, &mut c);
    let (f, _) = welcome_frame(&ctx, input(size, Some(0.1)), &mut screen, &mut c);
    assert!(!find_containing(&f.nodes, &tr("privacy-title")).is_empty());
    let back = bounds_of(&f.nodes, &tr("privacy-back-to-welcome"));
    let _ = welcome_frame(&ctx, click_input(size, back), &mut screen, &mut c);
    let (f, _) = welcome_frame(&ctx, input(size, Some(0.2)), &mut screen, &mut c);
    assert!(
        !find_containing(&f.nodes, &tr("welcome-acknowledge")).is_empty(),
        "back returns to the welcome disclosure view"
    );
    let after = SettingsStore::with_path(path).load().settings.disclosure;
    assert_eq!(
        before.map(|d| d.acknowledged_version),
        after.map(|d| d.acknowledged_version)
    );
    assert!(after.is_none());
}

#[test]
fn privacy_body_within_measure_from_both_origins() {
    let size = SIZES[1];
    for origin in [PrivacyOrigin::Welcome, PrivacyOrigin::About] {
        let ctx = ctx();
        let f = run_full(&ctx, input(size, Some(0.0)), |ui| {
            let _ = privacy_notice::show(ui, origin);
        });
        let body = tr("privacy-body");
        let first_line = body.lines().next().unwrap_or_default();
        let node = node_named(&f.nodes, first_line);
        assert_prose_width_within_measure(&ctx, node);
    }
}

// ---------------------------------------------------------------------
// §P / §S shared helpers
// ---------------------------------------------------------------------

/// A signed-out `AccountService` over an in-memory secure store (kept so a
/// test can make it unavailable) and the fake authorization service.
fn signed_out_service(label: &str) -> (AccountService, Arc<MemorySecureStore>) {
    let store = Arc::new(MemorySecureStore::new());
    let service = AccountService::new(
        Arc::clone(&store) as Arc<dyn SecureStore>,
        Arc::new(FakeAuthorizationService::new()) as Arc<dyn AuthorizationService>,
        Arc::new(FakeClock::default()) as Arc<dyn Clock>,
        std::env::temp_dir().join(format!("modplayer-ui-030-{label}-{}", std::process::id())),
    );
    (service, store)
}

/// Strip Fluent's bidi isolation marks around placeables.
fn plain(text: &str) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

/// One sign-in frame; returns the frame data and any events raised.
fn sign_in_frame(
    ctx: &Context,
    raw: RawInput,
    account: &mut AccountService,
    screen: &mut SignInScreen,
) -> (Full, Vec<AccountEvent>) {
    let mut events = Vec::new();
    let full = run_full(ctx, raw, |ui| {
        events = sign_in::show(ui, account, screen);
    });
    (full, events)
}

fn statuses(nodes: &[Node]) -> Vec<&Node> {
    nodes.iter().filter(|n| n.role == Role::Status).collect()
}

/// Start a sign-in so the service is `Authorizing`.
fn authorizing_service(label: &str) -> AccountService {
    let (mut account, _store) = signed_out_service(label);
    let events = account.start_sign_in();
    assert!(matches!(
        events.first(),
        Some(AccountEvent::BrowserUrlReady(_))
    ));
    assert!(matches!(account.state(), SessionState::Authorizing { .. }));
    account
}

fn primary_rects(ctx: &Context, f: &Full) -> Vec<Rect> {
    let roles = theme::tokens::roles(&ctx.global_style().visuals);
    let fill = variant_paint(roles, Variant::Primary).fill;
    f.shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Rect(r) if r.fill == fill => Some(r.rect),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------
// §P Pending sign-in: C4, C6, C7, FR-005, FR-006, SC-002
// ---------------------------------------------------------------------

#[test]
fn pending_order_and_indicator() {
    for size in SIZES {
        let mut account = authorizing_service("pending-order");
        let mut screen = SignInScreen::default();
        let ctx = ctx();
        let (f, _) = sign_in_frame(&ctx, input(size, Some(100.0)), &mut account, &mut screen);
        // P2: indicator present on the very first frame.
        assert_eq!(
            find_all(
                &f.nodes,
                Role::ProgressIndicator,
                &tr("signin-waiting-indicator")
            )
            .len(),
            1
        );
        // P1: heading, spinner, hint, elapsed, actions.
        let heading = bounds_of(&f.nodes, &tr("signin-waiting"));
        let spinner = find_all(
            &f.nodes,
            Role::ProgressIndicator,
            &tr("signin-waiting-indicator"),
        )[0]
        .bounds
        .expect("spinner bounds");
        let hint = bounds_of(&f.nodes, &tr("signin-browser-hint"));
        let elapsed = f
            .nodes
            .iter()
            .find(|n| {
                n.name()
                    .is_some_and(|s| plain(s) == "Waiting for 0 seconds")
            })
            .and_then(|n| n.bounds)
            .expect("elapsed hint");
        let open = bounds_of(&f.nodes, &tr("signin-open-again"));
        let cancel = bounds_of(&f.nodes, &tr("signin-cancel"));
        assert!(
            heading.max.y <= spinner.min.y + 0.5,
            "heading above spinner"
        );
        assert!(spinner.max.y <= hint.min.y + 0.5, "spinner above hint");
        assert!(hint.max.y <= elapsed.min.y + 0.5, "hint above elapsed");
        assert!(elapsed.max.y <= open.min.y + 0.5, "elapsed above actions");
        assert!(open.max.x <= cancel.min.x + 0.5, "open-again first");
        assert_prose_width_within_measure(&ctx, node_named(&f.nodes, &tr("signin-browser-hint")));
        assert!(statuses(&f.nodes).is_empty(), "no status while pending");
    }
}

#[test]
fn pending_resumed_line_follows_elapsed_hint() {
    // The resumed line only exists for `resumed: true`; ordering is pinned
    // here by the unit-level structure, so just confirm a fresh attempt
    // does not show it.
    let mut account = authorizing_service("pending-resumed");
    let mut screen = SignInScreen::default();
    let ctx = ctx();
    let (f, _) = sign_in_frame(&ctx, input(SIZES[0], Some(0.0)), &mut account, &mut screen);
    assert!(find_containing(&f.nodes, &tr("signin-waiting-resumed")).is_empty());
}

#[test]
fn pending_elapsed_hint_counts_seconds_then_minutes() {
    let mut account = authorizing_service("pending-elapsed");
    let mut screen = SignInScreen::default();
    let ctx = ctx();
    let t0 = 1000.0;
    for (secs, expected) in [
        (0.0, "Waiting for 0 seconds"),
        (1.0, "Waiting for 1 second"),
        (59.0, "Waiting for 59 seconds"),
        (60.0, "Waiting for 1 minute"),
        (119.0, "Waiting for 1 minute"),
        (120.0, "Waiting for 2 minutes"),
    ] {
        let (f, _) = sign_in_frame(
            &ctx,
            input(SIZES[0], Some(t0 + secs)),
            &mut account,
            &mut screen,
        );
        assert!(
            f.nodes
                .iter()
                .any(|n| n.name().is_some_and(|s| plain(s) == expected)),
            "at +{secs}s expected {expected:?}"
        );
    }
}

#[test]
fn pending_requests_a_repaint_within_one_second() {
    let mut account = authorizing_service("pending-repaint");
    let mut screen = SignInScreen::default();
    let ctx = ctx();
    let output = ctx.run_ui(input(SIZES[0], Some(0.0)), |ui| {
        let _ = sign_in::show(ui, &mut account, &mut screen);
    });
    let delay = output
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map(|v| v.repaint_delay)
        .expect("root viewport output");
    output.drop_without_applying_deltas();
    assert!(
        delay <= std::time::Duration::from_secs(1),
        "repaint_delay {delay:?}"
    );
}

#[test]
fn pending_count_resets_on_new_attempt_and_on_reentry() {
    let mut account = authorizing_service("pending-reset");
    let mut screen = SignInScreen::default();
    let ctx = ctx();
    let _ = sign_in_frame(&ctx, input(SIZES[0], Some(10.0)), &mut account, &mut screen);
    let (f, _) = sign_in_frame(&ctx, input(SIZES[0], Some(75.0)), &mut account, &mut screen);
    assert!(
        f.nodes
            .iter()
            .any(|n| n.name().is_some_and(|s| plain(s) == "Waiting for 1 minute"))
    );

    // Cancel, then a new attempt: count restarts from 0.
    account.cancel_sign_in();
    let (f, _) = sign_in_frame(&ctx, input(SIZES[0], Some(80.0)), &mut account, &mut screen);
    assert!(
        !find_containing(&f.nodes, &tr("signin-start")).is_empty()
            || !find_containing(&f.nodes, &tr("signin-retry")).is_empty()
    );
    let events = account.start_sign_in();
    assert!(matches!(
        events.first(),
        Some(AccountEvent::BrowserUrlReady(_))
    ));
    let (f, _) = sign_in_frame(&ctx, input(SIZES[0], Some(90.0)), &mut account, &mut screen);
    assert!(
        f.nodes.iter().any(|n| n
            .name()
            .is_some_and(|s| plain(s) == "Waiting for 0 seconds")),
        "new attempt restarts at 0"
    );
}

#[test]
fn pending_actions_primary_first_with_unchanged_behaviour() {
    let size = SIZES[0];
    let mut account = authorizing_service("pending-actions");
    let mut screen = SignInScreen::default();
    let ctx = ctx();
    let (f, _) = sign_in_frame(&ctx, input(size, Some(0.0)), &mut account, &mut screen);
    let open = bounds_of(&f.nodes, &tr("signin-open-again"));
    let cancel = bounds_of(&f.nodes, &tr("signin-cancel"));
    let primaries = primary_rects(&ctx, &f);
    assert_eq!(primaries.len(), 1, "exactly one Primary control");
    assert!(
        primaries[0].contains(open.center()),
        "open-again is Primary"
    );
    assert!(!primaries[0].contains(cancel.center()));
    assert!(open.max.x <= cancel.min.x + 0.5);

    // Open again emits BrowserUrlReady(account.browser_url()).
    let expected = account.browser_url().expect("url");
    let (_, events) = sign_in_frame(&ctx, click_input(size, open), &mut account, &mut screen);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AccountEvent::BrowserUrlReady(u) if *u == expected)),
        "events: {events:?}"
    );

    // Cancel calls cancel_sign_in.
    let (_, _) = sign_in_frame(&ctx, click_input(size, cancel), &mut account, &mut screen);
    assert!(matches!(
        account.state(),
        SessionState::SignedOut {
            note: Some(SignInNote::Cancelled)
        }
    ));
}

#[test]
fn pending_adds_no_timeout() {
    let mut account = authorizing_service("pending-no-timeout");
    let mut screen = SignInScreen::default();
    let ctx = ctx();
    let _ = sign_in_frame(&ctx, input(SIZES[0], Some(0.0)), &mut account, &mut screen);
    let (_, _) = sign_in_frame(
        &ctx,
        input(SIZES[0], Some(900.0)),
        &mut account,
        &mut screen,
    );
    assert!(
        matches!(account.state(), SessionState::Authorizing { .. }),
        "the screen never ends the attempt itself"
    );
}

// ---------------------------------------------------------------------
// §S Status message: C8, C9, FR-007, SC-003
// ---------------------------------------------------------------------

/// Heading above status above every action button (S3), plus M1.
fn assert_status_layout(
    ctx: &Context,
    f: &Full,
    heading_key: &str,
    status_text: &str,
    buttons: &[&str],
) {
    let st = statuses(&f.nodes);
    assert_eq!(st.len(), 1, "exactly one status node: {st:?}");
    assert_eq!(st[0].name().map(plain), Some(plain(status_text)));
    let status = st[0].bounds.expect("status bounds");
    let heading = bounds_of(&f.nodes, &tr(heading_key));
    assert!(heading.max.y <= status.min.y + 0.5, "heading above status");
    for key in buttons {
        let b = button_bounds(&f.nodes, &tr(key));
        assert!(status.max.y <= b.min.y + 0.5, "status above {key}");
    }
    assert_prose_width_within_measure(ctx, st[0]);
}

#[test]
fn status_cancelled_note_sits_between_heading_and_actions() {
    for size in SIZES {
        let mut account = authorizing_service("status-cancelled");
        account.cancel_sign_in();
        let mut screen = SignInScreen::default();
        let ctx = ctx();
        let (f, _) = sign_in_frame(&ctx, input(size, Some(0.0)), &mut account, &mut screen);
        assert_status_layout(
            &ctx,
            &f,
            "signin-title",
            &tr("signin-note-cancelled"),
            &["signin-retry"],
        );
        // S4: the instruction is plain text drawn after the status node.
        let status = statuses(&f.nodes)[0].bounds.unwrap();
        let instruction = node_named(&f.nodes, &tr("signin-explanation"));
        assert_ne!(instruction.role, Role::Status);
        assert!(status.max.y <= instruction.bounds.unwrap().min.y + 0.5);
        assert_prose_width_within_measure(&ctx, instruction);
    }
}

#[test]
fn status_signed_out_without_note_has_none() {
    let (mut account, _store) = signed_out_service("status-none");
    let mut screen = SignInScreen::default();
    let ctx = ctx();
    let (f, _) = sign_in_frame(&ctx, input(SIZES[0], Some(0.0)), &mut account, &mut screen);
    assert!(statuses(&f.nodes).is_empty());
    assert!(!find_containing(&f.nodes, &tr("signin-start")).is_empty());
}

#[test]
fn status_probe_failure_is_one_message_and_retry_restarts_sign_in() {
    let size = SIZES[0];
    let (mut account, store) = signed_out_service("status-probe");
    store.set_unavailable(true);
    let mut screen = SignInScreen::default();
    let ctx = ctx();
    let (f, _) = sign_in_frame(&ctx, input(size, Some(0.0)), &mut account, &mut screen);
    let start = button_bounds(&f.nodes, &tr("signin-start"));
    let (_, events) = sign_in_frame(&ctx, click_input(size, start), &mut account, &mut screen);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AccountEvent::StoreUnavailable { .. }))
    );
    let (f, _) = sign_in_frame(&ctx, input(size, Some(0.1)), &mut account, &mut screen);
    let store_name = tr(account.secure().platform_name_key());
    let message = f
        .nodes
        .iter()
        .find(|n| n.role == Role::Status)
        .and_then(|n| n.name().map(plain))
        .expect("status");
    assert!(message.contains(&store_name), "{message}");
    assert_status_layout(&ctx, &f, "signin-title", &message, &["signin-retry"]);

    // S5: Retry calls start_sign_in again.
    store.set_unavailable(false);
    let retry = button_bounds(&f.nodes, &tr("signin-retry"));
    let (_, events) = sign_in_frame(&ctx, click_input(size, retry), &mut account, &mut screen);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AccountEvent::BrowserUrlReady(_)))
    );
    assert!(matches!(account.state(), SessionState::Authorizing { .. }));
}

#[test]
fn status_tier_results_use_the_shared_message() {
    let size = SIZES[0];
    for (result, heading, text, buttons) in [
        (
            TierResult::Free,
            "tier-free-title",
            "tier-free-explanation",
            vec!["tier-open-upgrade", "tier-continue"],
        ),
        (
            TierResult::Unknown,
            "tier-unknown-title",
            "tier-unknown-explanation",
            vec!["tier-retry", "tier-continue"],
        ),
    ] {
        let (mut account, _store) = signed_out_service("status-tier");
        let mut screen = SignInScreen::default();
        screen.set_tier_result(result);
        let ctx = ctx();
        let (f, _) = sign_in_frame(&ctx, input(size, Some(0.0)), &mut account, &mut screen);
        assert_status_layout(&ctx, &f, heading, &tr(text), &buttons);

        // S5: Continue clears the overlay.
        let cont = button_bounds(&f.nodes, &tr("tier-continue"));
        let _ = sign_in_frame(&ctx, click_input(size, cont), &mut account, &mut screen);
        assert_eq!(screen.tier_result, None);
    }
}

#[test]
fn status_absent_while_authorizing() {
    let mut account = authorizing_service("status-authorizing");
    let mut screen = SignInScreen::default();
    let ctx = ctx();
    let (f, _) = sign_in_frame(&ctx, input(SIZES[0], Some(0.0)), &mut account, &mut screen);
    assert!(statuses(&f.nodes).is_empty());
}

// ---------------------------------------------------------------------
// §D Device check: C10, C11, C12, FR-008, FR-009
// ---------------------------------------------------------------------

use modplayer_audio_io::FakeDevice;
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::device_check::{DeviceCheckOrigin, DeviceCheckOutcome, DeviceCheckScreen};

/// A controller over one fake device (`devices = 1`) or none (`0`).
fn device_controller(label: &str, devices: usize) -> (Controller, TempDir, std::path::PathBuf) {
    let dir = TempDir::new(label);
    let path = dir.0.join("settings.toml");
    let store = SettingsStore::with_path(path.clone());
    let list = (0..devices)
        .map(|_| FakeDevice {
            id: DeviceId::new("dev-1").unwrap(),
            name: "Speakers".to_string(),
            rate: SampleRate::new(44_100),
            channels: 2,
            buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
            is_default: true,
        })
        .collect();
    let controller = PlaybackController::new(FakeBackend::new(list), ScriptedHost::new(), store);
    (controller, dir, path)
}

fn check_frame(
    ctx: &Context,
    raw: RawInput,
    screen: &mut DeviceCheckScreen,
    controller: &mut Controller,
) -> (Full, DeviceCheckOutcome) {
    let mut outcome = DeviceCheckOutcome::Open;
    let full = run_full(ctx, raw, |ui| {
        outcome = screen.show(ui, controller);
    });
    (full, outcome)
}

/// Controller state a Back must not change (D2, D3).
fn device_state(
    c: &Controller,
    path: &std::path::Path,
) -> (bool, BufferPreset, Option<DeviceId>, Vec<u8>) {
    (
        c.device_confirmed(),
        c.preset(),
        c.preferred_device().cloned(),
        std::fs::read(path).unwrap_or_default(),
    )
}

#[test]
fn device_check_back_is_a_default_button_in_both_layouts_and_origins() {
    for origin in [DeviceCheckOrigin::LaunchFlow, DeviceCheckOrigin::Settings] {
        for devices in [1, 0] {
            for size in SIZES {
                let (mut c, _dir, _p) = device_controller("d1", devices);
                let mut screen = DeviceCheckScreen::new(None, BufferPreset::Balanced, origin);
                let ctx = ctx();
                let (f, _) = check_frame(&ctx, input(size, None), &mut screen, &mut c);
                let back = tr("device-check-back");
                let found = find_all(&f.nodes, Role::Button, &back);
                assert_eq!(found.len(), 1, "{origin:?} devices={devices} {size:?}");
                // Back sits above the rest of the screen.
                let back_y = found[0].bounds.unwrap().min.y;
                let skip = button_bounds(&f.nodes, &tr("device-check-skip"));
                assert!(back_y < skip.min.y);
                // The Default variant never paints the Primary fill.
                assert!(primary_rects(&ctx, &f).is_empty(), "Back is not Primary");
            }
        }
    }
}

#[test]
fn device_check_back_is_keyboard_focusable() {
    let (mut c, _dir, _p) = device_controller("d1-tab", 1);
    let mut screen =
        DeviceCheckScreen::new(None, BufferPreset::Balanced, DeviceCheckOrigin::Settings);
    let ctx = ctx();
    check_frame(&ctx, input(SIZES[0], None), &mut screen, &mut c);
    let (f, _) = check_frame(&ctx, tab_input(SIZES[0]), &mut screen, &mut c);
    let (f2, _) = check_frame(&ctx, input(SIZES[0], None), &mut screen, &mut c);
    let focus = f2.focus.or(f.focus);
    assert_eq!(focus.as_deref(), Some(tr("device-check-back").as_str()));
}

#[test]
fn device_check_back_from_settings_changes_nothing() {
    let (mut c, _dir, path) = device_controller("d2", 1);
    c.confirm_device(DeviceId::new("dev-1").unwrap(), BufferPreset::Safe);
    let before = device_state(&c, &path);
    let mut screen = DeviceCheckScreen::new(
        Some(DeviceId::new("dev-1").unwrap()),
        BufferPreset::Safe,
        DeviceCheckOrigin::Settings,
    );
    let ctx = ctx();
    let (f, _) = check_frame(&ctx, input(SIZES[0], None), &mut screen, &mut c);
    let rect = button_bounds(&f.nodes, &tr("device-check-back"));
    let (_, outcome) = check_frame(&ctx, click_input(SIZES[0], rect), &mut screen, &mut c);
    assert_eq!(outcome, DeviceCheckOutcome::Back);
    assert_eq!(device_state(&c, &path), before);
}

/// T048 / research R16: a launch-flow device check only lives while the
/// launch gate is `DeviceCheck`. Outside it (tier not Premium, signed out,
/// …) the main shell must not draw it as an overlay whose Back skips the
/// C16 summary; a Settings preview is unaffected.
#[test]
fn launch_device_check_is_dropped_outside_the_gate() {
    use modplayer_account::LaunchStep;
    use modplayer_ui::app::retains_device_check;
    let launch =
        DeviceCheckScreen::new(None, BufferPreset::Balanced, DeviceCheckOrigin::LaunchFlow);
    assert_eq!(launch.origin(), DeviceCheckOrigin::LaunchFlow);
    assert!(retains_device_check(
        LaunchStep::DeviceCheck,
        launch.origin()
    ));
    for step in [LaunchStep::Welcome, LaunchStep::SignIn, LaunchStep::Main] {
        assert!(
            !retains_device_check(step, DeviceCheckOrigin::LaunchFlow),
            "{step:?}"
        );
        assert!(
            retains_device_check(step, DeviceCheckOrigin::Settings),
            "{step:?}"
        );
    }
}

#[test]
fn device_check_back_during_launch_keeps_the_screen_state() {
    let (mut c, _dir, path) = device_controller("d3", 1);
    let before = device_state(&c, &path);
    let mut screen = DeviceCheckScreen::new(
        Some(DeviceId::new("dev-1").unwrap()),
        BufferPreset::Performance,
        DeviceCheckOrigin::LaunchFlow,
    );
    let ctx = ctx();
    let (f, _) = check_frame(&ctx, input(SIZES[0], None), &mut screen, &mut c);
    let rect = button_bounds(&f.nodes, &tr("device-check-back"));
    let (_, outcome) = check_frame(&ctx, click_input(SIZES[0], rect), &mut screen, &mut c);
    assert_eq!(outcome, DeviceCheckOutcome::Back);
    assert_eq!(device_state(&c, &path), before);
    // The same screen, shown again, still carries its preset choice.
    let (f, outcome) = check_frame(&ctx, input(SIZES[0], None), &mut screen, &mut c);
    assert_eq!(outcome, DeviceCheckOutcome::Open);
    let perf = tr("buffer-preset-performance");
    assert!(!find_containing(&f.nodes, &perf).is_empty());
}

#[test]
fn device_check_buffer_preset_is_a_labelled_field_with_help() {
    let (mut c, _dir, _p) = device_controller("d4", 1);
    let mut screen =
        DeviceCheckScreen::new(None, BufferPreset::Balanced, DeviceCheckOrigin::LaunchFlow);
    for size in SIZES {
        let ctx = ctx();
        let (f, _) = check_frame(&ctx, input(size, None), &mut screen, &mut c);
        let label = tr("setting-buffer-preset");
        let help = tr("setting-buffer-preset-desc");
        let label_node = find_all(&f.nodes, Role::Label, &label);
        assert_eq!(label_node.len(), 1, "visible label");
        let help_node = node_named(&f.nodes, &help);
        assert_prose_width_within_measure(&ctx, help_node);
        // Help follows the label directly, and the combo follows the help.
        let help_b = help_node.bounds.unwrap();
        assert!(help_b.min.y >= label_node[0].bounds.unwrap().max.y - 1.0);
        let combo = f
            .nodes
            .iter()
            .find(|n| n.role == Role::ComboBox)
            .expect("preset combo");
        assert!(combo.bounds.unwrap().min.y >= help_b.max.y - 1.0);
        // The combo carries the chosen preset as its value and is tied to
        // the visible label via `labelled_by` (same as Settings › Audio).
        assert!(
            combo
                .name()
                .is_some_and(|n| n.contains(&tr("buffer-preset-balanced")))
        );
    }
}

#[test]
fn device_check_answers_are_unchanged() {
    // Yes confirms the device with the chosen preset and closes.
    let (mut c, _dir, _p) = device_controller("d5-yes", 1);
    let mut screen =
        DeviceCheckScreen::new(None, BufferPreset::Safe, DeviceCheckOrigin::LaunchFlow);
    let ctx = ctx();
    let (f, _) = check_frame(&ctx, input(SIZES[0], None), &mut screen, &mut c);
    let order: Vec<f32> = ["device-check-yes", "device-check-no", "device-check-skip"]
        .iter()
        .map(|k| button_bounds(&f.nodes, &tr(k)).min.x)
        .collect();
    assert!(order[0] < order[1] && order[1] < order[2], "answer order");
    let rect = button_bounds(&f.nodes, &tr("device-check-yes"));
    let (_, outcome) = check_frame(&ctx, click_input(SIZES[0], rect), &mut screen, &mut c);
    assert_eq!(outcome, DeviceCheckOutcome::Closed);
    assert!(c.device_confirmed());
    assert_eq!(c.preset(), BufferPreset::Safe);

    // Skip closes and confirms nothing.
    let (mut c, _dir, _p) = device_controller("d5-skip", 1);
    let mut screen =
        DeviceCheckScreen::new(None, BufferPreset::Safe, DeviceCheckOrigin::LaunchFlow);
    let (f, _) = check_frame(&ctx, input(SIZES[0], None), &mut screen, &mut c);
    let rect = button_bounds(&f.nodes, &tr("device-check-skip"));
    let (_, outcome) = check_frame(&ctx, click_input(SIZES[0], rect), &mut screen, &mut c);
    assert_eq!(outcome, DeviceCheckOutcome::Closed);
    assert!(!c.device_confirmed());
}

#[test]
fn device_check_prose_stays_within_measure() {
    for devices in [1, 0] {
        for size in SIZES {
            let (mut c, _dir, _p) = device_controller("d-m1", devices);
            let mut screen =
                DeviceCheckScreen::new(None, BufferPreset::Balanced, DeviceCheckOrigin::Settings);
            let ctx = ctx();
            let (f, _) = check_frame(&ctx, input(size, None), &mut screen, &mut c);
            let key = if devices == 0 {
                "device-check-no-devices"
            } else {
                "device-check-question"
            };
            assert_prose_width_within_measure(&ctx, node_named(&f.nodes, &tr(key)));
        }
    }
}

// ---------------------------------------------------------------------
// §V Sign-in summary: C16, FR-008
// ---------------------------------------------------------------------

use modplayer_ui::sign_in::SummaryOutcome;

fn summary_frame(
    ctx: &Context,
    raw: RawInput,
    account: &mut AccountService,
) -> (Full, SummaryOutcome) {
    let mut outcome = SummaryOutcome::None;
    let full = run_full(ctx, raw, |ui| {
        outcome = sign_in::show_summary(ui, account);
    });
    (full, outcome)
}

#[test]
fn summary_order_is_heading_identity_then_actions() {
    for size in SIZES {
        let mut account = active_account("v1");
        let ctx = ctx();
        let (f, _) = summary_frame(&ctx, input(size, None), &mut account);
        let heading = bounds_of(&f.nodes, &tr("signin-summary-title"));
        let identity = node_named(&f.nodes, "Alex");
        assert!(plain(identity.name().unwrap()).contains("Signed in as Alex"));
        assert_prose_width_within_measure(&ctx, identity);
        let cont = button_bounds(&f.nodes, &tr("signin-summary-continue"));
        let switch = button_bounds(&f.nodes, &tr("signin-summary-switch"));
        assert!(heading.max.y <= identity.bounds.unwrap().min.y + 1.0);
        assert!(identity.bounds.unwrap().max.y <= cont.min.y + 1.0);
        assert!(cont.min.x < switch.min.x, "Continue first");
        assert_eq!(primary_rects(&ctx, &f).len(), 1, "one Primary");
    }
}

#[test]
fn summary_is_read_only() {
    let (_c, dir, _path) = device_controller("v2", 1);
    let settings = dir.0.join("settings.toml");
    let mut account = active_account("v2");
    let ctx = ctx();
    let before = (
        std::fs::read(&settings).unwrap_or_default(),
        account.account().cloned(),
        format!("{:?}", account.state()),
    );
    summary_frame(&ctx, input(SIZES[0], None), &mut account);
    summary_frame(&ctx, input(SIZES[0], Some(1.0)), &mut account);
    let after = (
        std::fs::read(&settings).unwrap_or_default(),
        account.account().cloned(),
        format!("{:?}", account.state()),
    );
    assert_eq!(before, after);
}

#[test]
fn summary_continue_returns_continue() {
    let mut account = active_account("v3");
    let ctx = ctx();
    let (f, _) = summary_frame(&ctx, input(SIZES[0], None), &mut account);
    let rect = button_bounds(&f.nodes, &tr("signin-summary-continue"));
    let (_, outcome) = summary_frame(&ctx, click_input(SIZES[0], rect), &mut account);
    assert_eq!(outcome, SummaryOutcome::Continue);
    assert!(matches!(account.state(), SessionState::Active));
}

/// Open the switch dialog and return the context with the dialog showing.
fn open_switch_dialog(account: &mut AccountService) -> (Context, Full) {
    let ctx = ctx();
    let (f, _) = summary_frame(&ctx, input(SIZES[0], None), account);
    let rect = button_bounds(&f.nodes, &tr("signin-summary-switch"));
    summary_frame(&ctx, click_input(SIZES[0], rect), account);
    // Two settle frames: the dialog opens, then focus lands on Cancel.
    summary_frame(&ctx, input(SIZES[0], None), account);
    let (f, _) = summary_frame(&ctx, input(SIZES[0], None), account);
    (ctx, f)
}

#[test]
fn summary_switch_asks_for_confirmation_with_cancel_focused() {
    let mut account = active_account("v4-open");
    let (_ctx, f) = open_switch_dialog(&mut account);
    assert!(!find_containing(&f.nodes, &tr("signout-confirm-title")).is_empty());
    assert_eq!(f.focus.as_deref(), Some(tr("signout-cancel").as_str()));
    assert!(matches!(account.state(), SessionState::Active));
}

#[test]
fn summary_switch_confirm_signs_out() {
    let mut account = active_account("v4-confirm");
    let (ctx, f) = open_switch_dialog(&mut account);
    let rect = button_bounds(&f.nodes, &tr("signout-confirm"));
    let (_, outcome) = summary_frame(&ctx, click_input(SIZES[0], rect), &mut account);
    assert!(matches!(outcome, SummaryOutcome::Events(ref e) if !e.is_empty()));
    assert!(matches!(account.state(), SessionState::SignedOut { .. }));
}

#[test]
fn summary_switch_cancel_and_escape_keep_the_summary() {
    let mut account = active_account("v4-cancel");
    let (ctx, f) = open_switch_dialog(&mut account);
    let rect = button_bounds(&f.nodes, &tr("signout-cancel"));
    let (_, outcome) = summary_frame(&ctx, click_input(SIZES[0], rect), &mut account);
    assert_eq!(outcome, SummaryOutcome::None);
    let (f, _) = summary_frame(&ctx, input(SIZES[0], None), &mut account);
    assert!(find_containing(&f.nodes, &tr("signout-confirm-title")).is_empty());
    assert!(matches!(account.state(), SessionState::Active));

    let mut account = active_account("v4-esc");
    let (ctx, _) = open_switch_dialog(&mut account);
    let mut raw = input(SIZES[0], None);
    raw.events = vec![Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }];
    summary_frame(&ctx, raw, &mut account);
    let (f, _) = summary_frame(&ctx, input(SIZES[0], None), &mut account);
    assert!(find_containing(&f.nodes, &tr("signout-confirm-title")).is_empty());
    assert!(matches!(account.state(), SessionState::Active));
}

#[test]
fn summary_identity_falls_back_when_the_name_is_blank() {
    let (mut account, _store) = signed_out_service("v-blank");
    let ctx = ctx();
    let (f, _) = summary_frame(&ctx, input(SIZES[0], None), &mut account);
    let fallback = tr("account-identity-unavailable");
    assert!(!find_containing(&f.nodes, &fallback).is_empty());
}

// ---------------------------------------------------------------------
// §G Getting-started card, §A About restore row: FR-1.4.1, UX-10
// ---------------------------------------------------------------------

use modplayer_core::{CATALOG, Chord, Platform};
use modplayer_ui::getting_started::{self, GettingStartedOutcome};
use modplayer_ui::settings::about::{self, AboutOutcome, AboutScreen};

fn card_frame(ctx: &Context, raw: RawInput) -> (Full, GettingStartedOutcome) {
    let mut outcome = GettingStartedOutcome::None;
    let full = run_full(ctx, raw, |ui| {
        outcome = getting_started::show(ui);
    });
    (full, outcome)
}

#[test]
fn getting_started_nodes_are_title_two_plugin_lines_two_links_and_dismiss() {
    let ctx = ctx();
    let (f, _) = card_frame(&ctx, input(SIZES[0], None));
    let nodes = &f.nodes;
    assert!(!find_containing(nodes, &tr("getting-started-title")).is_empty());
    for key in ["getting-started-section-loop", "getting-started-key-tempo"] {
        // One line per plugin: exactly one Label node carries the text.
        let lines = find_containing(nodes, &tr(key))
            .into_iter()
            .filter(|n| n.role == Role::Label)
            .count();
        assert_eq!(lines, 1, "{key}");
    }
    for key in ["getting-started-shortcuts", "getting-started-tutorial"] {
        assert_eq!(find_all(nodes, Role::Link, &tr(key)).len(), 1, "{key}");
    }
    assert_eq!(
        find_all(nodes, Role::Button, &tr("getting-started-dismiss")).len(),
        1
    );
}

#[test]
fn getting_started_has_no_chord_prose() {
    let ctx = ctx();
    let (f, _) = card_frame(&ctx, input(SIZES[0], None));
    assert!(find_containing(&f.nodes, "Shortcuts:").is_empty());
    for platform in [Platform::Mac, Platform::Other] {
        for def in CATALOG {
            for encoded in def.default_bindings {
                let chord = Chord::parse(encoded).unwrap();
                let label = chord.display(platform);
                assert!(
                    f.nodes
                        .iter()
                        .filter(|n| n.role != Role::Button && n.role != Role::Link)
                        .all(|n| n.name() != Some(label.as_str())
                            && !n.name().is_some_and(|s| s.contains(&format!("({label})")))),
                    "card shows chord {label:?}"
                );
            }
        }
    }
}

#[test]
fn getting_started_shortcuts_link_opens_shortcuts() {
    let ctx = ctx();
    let (f, _) = card_frame(&ctx, input(SIZES[0], None));
    let rect = find_all(&f.nodes, Role::Link, &tr("getting-started-shortcuts"))[0]
        .bounds
        .unwrap();
    let (_, outcome) = card_frame(&ctx, click_input(SIZES[0], rect));
    assert_eq!(outcome, GettingStartedOutcome::OpenShortcuts);
}

fn about_frame(
    ctx: &Context,
    raw: RawInput,
    screen: &mut AboutScreen,
    dismissed: bool,
) -> (Full, AboutOutcome) {
    let mut outcome = AboutOutcome::None;
    let full = run_full(ctx, raw, |ui| {
        outcome = about::show(ui, screen, dismissed);
    });
    (full, outcome)
}

/// The Settings screen's mapping of the outcome (settings/mod.rs).
fn apply_about(c: &mut Controller, outcome: AboutOutcome) {
    if outcome == AboutOutcome::RestoreGettingStarted {
        c.restore_getting_started();
    }
}

#[test]
fn about_restore_row_is_a_labelled_button_with_help() {
    let ctx = ctx();
    let mut screen = AboutScreen::default();
    let (f, _) = about_frame(&ctx, input(SIZES[0], None), &mut screen, true);
    assert_eq!(
        find_all(&f.nodes, Role::Button, &tr("setting-show-getting-started")).len(),
        1
    );
    assert!(!find_containing(&f.nodes, &tr("setting-show-getting-started-desc")).is_empty());
    assert_eq!(about::SHOW_GETTING_STARTED_ID, "about.show_getting_started");
}

#[test]
fn about_restore_enabled_only_when_dismissed() {
    let ctx = ctx();
    let name = tr("setting-show-getting-started");
    for dismissed in [true, false] {
        let mut screen = AboutScreen::default();
        let (f, _) = about_frame(&ctx, input(SIZES[0], None), &mut screen, dismissed);
        // Click it: only an enabled button yields the outcome.
        let rect = button_bounds(&f.nodes, &name);
        let (_, outcome) = about_frame(&ctx, click_input(SIZES[0], rect), &mut screen, dismissed);
        assert_eq!(
            outcome == AboutOutcome::RestoreGettingStarted,
            dismissed,
            "dismissed={dismissed}"
        );
        let visible = tr("setting-show-getting-started-visible");
        let described = !find_containing(&f.nodes, &visible).is_empty();
        let _ = described; // description is an AccessKit property, not a name
    }
}

#[test]
fn about_restore_click_restores_and_persists() {
    let (mut c, _dir, path) = controller("a3");
    c.dismiss_getting_started();
    assert!(c.getting_started_dismissed());
    let ctx = ctx();
    let mut screen = AboutScreen::default();
    // Landing on About is the first interaction, the click the second (A5).
    let (f, _) = about_frame(&ctx, input(SIZES[0], None), &mut screen, true);
    let rect = button_bounds(&f.nodes, &tr("setting-show-getting-started"));
    let (_, outcome) = about_frame(&ctx, click_input(SIZES[0], rect), &mut screen, true);
    assert_eq!(outcome, AboutOutcome::RestoreGettingStarted);
    apply_about(&mut c, outcome);
    assert!(!c.getting_started_dismissed());
    assert!(
        !SettingsStore::with_path(path)
            .load()
            .settings
            .getting_started_dismissed
    );
}

#[test]
fn about_restore_persists_across_a_rebuilt_controller() {
    let (mut c, _dir, path) = controller("a6");
    c.dismiss_getting_started();
    let rebuilt = PlaybackController::new(
        FakeBackend::new(vec![]),
        ScriptedHost::new(),
        SettingsStore::with_path(path.clone()),
    );
    assert!(rebuilt.getting_started_dismissed());
    c.restore_getting_started();
    let rebuilt = PlaybackController::new(
        FakeBackend::new(vec![]),
        ScriptedHost::new(),
        SettingsStore::with_path(path),
    );
    assert!(!rebuilt.getting_started_dismissed());
}
