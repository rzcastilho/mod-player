// SPDX-License-Identifier: MIT OR Apache-2.0

//! The app shell's left-rail navigation (contracts/ui-surface.md "Main
//! window"): five sections — Library, Search, Now Playing, Plugins,
//! Settings — selectable by pointer or `Ctrl/Cmd+1..5`. Drawing the five
//! nav buttons in this fixed order also fixes their Tab focus order to
//! match (egui's default focus order follows widget-creation order within
//! a frame). `app.rs` owns the `CentralPanel` that renders whichever
//! section is selected; this module only owns the rail and the Plugins
//! placeholder, which has no dedicated screen yet (Constitution Principle
//! II). The Library placeholder wiring point that used to live here is
//! dropped (004-search-and-library-browse, T026) — `app.rs` renders that
//! section's content directly until `library_view` (US2, T063/T065)
//! replaces it.
//!
//! `Section::Search` is new (004-search-and-library-browse,
//! contracts/ui-surface.md §1): `Ctrl/Cmd+F` or `/` from anywhere jumps to
//! it and asks the search box (`search_view.rs`, US1, T038/T039) to take
//! focus, via `focus_search_requested`.

use egui::{Id, Order, Panel, Sense, Stroke, Ui, vec2};
use modplayer_account::LaunchStep;
use modplayer_core::{tr, tr_args};

use crate::theme;
use crate::theme::controls;

/// 011-plugin-ui-contributions, contracts/ui-panels.md L2: a floated
/// plugin panel is a plain `Order::Middle` window — the same layer the
/// notification area's own `Area` uses — so it never floats above a
/// critical notification by construction. `plugin_panels.rs`'s floated
/// `Window`s are built with this constant rather than a bare
/// `egui::Order::Middle` literal, so the ordering *policy* (which layer
/// a plugin surface may use) stays declared in one place, next to the
/// section it only ever appears in.
///
/// Known caveat (2026-09-20 manual walk, recorded in this feature's
/// Manual Scenario Log): `app.rs` currently draws the notification
/// `Area` *before* `CentralPanel`/`now_playing::show`, so within-layer
/// insertion order still lets a freshly (re)drawn floated panel paint
/// over a notification this same frame. Fixing that requires reordering
/// `app.rs`'s own draw calls, which is out of this phase's file scope;
/// tracked for a follow-up rather than silently left undocumented.
pub const PLUGIN_FLOATED_WINDOW_ORDER: Order = Order::Middle;

/// 018-window-sizing-and-responsive-dock, contract D1: the narrow-window
/// dock overlay (`plugin_panels::show_overlay`) is a plain `Order::Middle`
/// `egui::Area` too — the same layer level as a floated panel window
/// ([`PLUGIN_FLOATED_WINDOW_ORDER`]), since it stands in for the very same
/// docked panels a wider window would show in a `Panel::right` column, not
/// a transient popup.
pub const PLUGIN_DOCK_OVERLAY_ORDER: Order = Order::Middle;

/// The five navigable sections (contracts/ui-surface.md §1), in the fixed
/// left-rail display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Library,
    Search,
    NowPlaying,
    Plugins,
    Settings,
}

/// Every section with its Fluent label key, in nav-rail display order.
/// `nav-search` lands in `locales/en-US/app.ftl` with the rest of Search
/// (US1, T040); until then it renders as the raw key (`tr`'s missing-key
/// fallback).
const SECTIONS: [(Section, &str); 5] = [
    (Section::Library, "nav-library"),
    (Section::Search, "nav-search"),
    (Section::NowPlaying, "nav-now-playing"),
    (Section::Plugins, "nav-plugins"),
    (Section::Settings, "nav-settings"),
];

/// The shell's own UI state: which section is currently selected, and
/// whether a global shortcut just asked the search box to take focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shell {
    pub section: Section,
    /// Set by `actions::invoke`'s `FocusSearch` arm (007, contracts/
    /// ui-actions.md §3; formerly `Shell::handle_shortcuts`'s own
    /// `Ctrl/Cmd+F`/`/` handling) — `search_view::show` consumes and
    /// clears it to request keyboard focus on the search box the same
    /// frame.
    pub focus_search_requested: bool,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            section: Section::Library,
            focus_search_requested: false,
        }
    }
}

impl Shell {
    /// Draw the left-rail nav buttons in fixed order, updating `section` on
    /// click.
    ///
    /// 014-design-tokens-and-type-scale (US2, T031): the rail's own
    /// entries are this shell's only "heading"-shaped text (no section's
    /// content draws a screen heading of its own in this slice — Library/
    /// Search/Settings/Plugins open straight into content, and Now
    /// Playing's `display` title lives in `now_playing.rs`, T022) — styled
    /// through `theme::section_label` like every other panel/group header
    /// (data-model.md §6), never a hand-uppercased string. The accessible
    /// name is pinned back to the exact, un-uppercased `tr(key)` (mirrors
    /// `settings::controls::section_heading`, `markers::panel`'s T030)
    /// rather than left to derive from the now-uppercased painted text —
    /// FR-019 forbids a behaviour change, and a screen reader's own
    /// announcement of a nav button is behaviour.
    ///
    /// 020-shell-navigation-and-gates (US3, FR-005, FR-006, contracts/
    /// shell-chrome.md C5-C8): each item renders through
    /// `widgets::controls::nav_item` — a leading-edge `accent` indicator,
    /// never a filled `selectable_label` background — which already pins
    /// the exact accessible name and `set_selected` itself, replacing this
    /// function's own former accesskit override.
    pub fn nav_rail(&mut self, ui: &mut Ui) {
        for (section, key) in SECTIONS {
            let label = tr(key);
            let response = crate::widgets::controls::nav_item(ui, self.section == section, &label);
            if response.clicked() {
                self.section = section;
            }
        }
    }
}

/// One of the three launch-gate steps the step indicator shows (020-shell-
/// navigation-and-gates, data-model.md §1, contracts/shell-chrome.md).
/// `position()` (1..=3) and `label_key()` are fixed, display-order data;
/// `from_launch_step` is the only place a `LaunchStep` maps to one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStep {
    Welcome,
    SignIn,
    AudioOutputCheck,
}

/// The step indicator's fixed total (Clarification 4): never derived from
/// tier or any other runtime condition.
pub const GATE_STEP_TOTAL: u8 = 3;

impl GateStep {
    /// Display order (contracts/shell-chrome.md).
    pub const ALL: [GateStep; 3] = [
        GateStep::Welcome,
        GateStep::SignIn,
        GateStep::AudioOutputCheck,
    ];

    /// `1..=3`, fixed by variant — never derived from a retry counter or
    /// sub-view (FR-003).
    ///
    /// ```
    /// # use modplayer_ui::shell::GateStep;
    /// assert_eq!(GateStep::Welcome.position(), 1);
    /// assert_eq!(GateStep::AudioOutputCheck.position(), 3);
    /// ```
    pub const fn position(self) -> u8 {
        match self {
            GateStep::Welcome => 1,
            GateStep::SignIn => 2,
            GateStep::AudioOutputCheck => 3,
        }
    }

    /// This step's Fluent label key (locales/en-US/app.ftl).
    pub const fn label_key(self) -> &'static str {
        match self {
            GateStep::Welcome => "gate-step-welcome",
            GateStep::SignIn => "gate-step-sign-in",
            GateStep::AudioOutputCheck => "gate-step-audio-output-check",
        }
    }

    /// The 1:1 mapping from `modplayer_account::LaunchStep` (`Main` has no
    /// gate step, data-model.md §1).
    ///
    /// ```
    /// # use modplayer_ui::shell::GateStep;
    /// # use modplayer_account::LaunchStep;
    /// assert_eq!(
    ///     GateStep::from_launch_step(LaunchStep::SignIn),
    ///     Some(GateStep::SignIn)
    /// );
    /// assert_eq!(GateStep::from_launch_step(LaunchStep::Main), None);
    /// ```
    pub const fn from_launch_step(step: LaunchStep) -> Option<GateStep> {
        match step {
            LaunchStep::Welcome => Some(GateStep::Welcome),
            LaunchStep::SignIn => Some(GateStep::SignIn),
            LaunchStep::DeviceCheck => Some(GateStep::AudioOutputCheck),
            LaunchStep::Main => None,
        }
    }

    /// This step's paint state relative to `current` (research.md R2):
    /// purely a position comparison, so a retry/sub-view — which never
    /// changes `current` — never changes any item's state either.
    ///
    /// ```
    /// # use modplayer_ui::shell::{GateStep, StepState};
    /// assert_eq!(GateStep::Welcome.state_relative_to(GateStep::SignIn), StepState::Complete);
    /// assert_eq!(GateStep::SignIn.state_relative_to(GateStep::SignIn), StepState::Current);
    /// assert_eq!(GateStep::AudioOutputCheck.state_relative_to(GateStep::SignIn), StepState::Upcoming);
    /// ```
    pub fn state_relative_to(self, current: GateStep) -> StepState {
        match self.position().cmp(&current.position()) {
            std::cmp::Ordering::Less => StepState::Complete,
            std::cmp::Ordering::Equal => StepState::Current,
            std::cmp::Ordering::Greater => StepState::Upcoming,
        }
    }
}

/// A gate step's paint state relative to the current step (research.md R2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Complete,
    Current,
    Upcoming,
}

/// The per-frame shell decision (020-shell-navigation-and-gates, data-
/// model.md §2, contracts/shell-chrome.md): whether the nav rail exists
/// this frame, and which gate step (if any) is current. Computed once per
/// frame by [`Chrome::for_frame`] and never stored — a stored flag could go
/// stale the frame a sign-out or a Settings-triggered Device Check preview
/// starts (Clarification 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chrome {
    pub rail: bool,
    pub gate: Option<GateStep>,
}

impl Chrome {
    /// research.md R1's table, exhaustively: a gate step (`Welcome`/
    /// `SignIn`/`DeviceCheck`) always hides the rail; `Main` shows it unless
    /// `device_check_open` (the Settings "Test output device" preview,
    /// FR-001a) hides it too. `rail ⇒ gate.is_none()` by construction.
    ///
    /// ```
    /// # use modplayer_ui::shell::{Chrome, GateStep};
    /// # use modplayer_account::LaunchStep;
    /// let gate = Chrome::for_frame(LaunchStep::SignIn, false);
    /// assert_eq!(gate, Chrome { rail: false, gate: Some(GateStep::SignIn) });
    ///
    /// let main = Chrome::for_frame(LaunchStep::Main, false);
    /// assert_eq!(main, Chrome { rail: true, gate: None });
    ///
    /// let preview = Chrome::for_frame(LaunchStep::Main, true);
    /// assert_eq!(preview, Chrome { rail: false, gate: None });
    /// ```
    pub const fn for_frame(step: LaunchStep, device_check_open: bool) -> Chrome {
        match GateStep::from_launch_step(step) {
            Some(gate) => Chrome {
                rail: false,
                gate: Some(gate),
            },
            None => Chrome {
                rail: !device_check_open,
                gate: None,
            },
        }
    }

    /// Whether `actions::dispatch` may run this frame (FR-001b): identical
    /// to `rail`, named separately so the dispatcher gate and the rail's
    /// own visibility can never drift from each other by construction.
    pub const fn navigation_enabled(self) -> bool {
        self.rail
    }
}

/// Draw this frame's chrome (contracts/shell-chrome.md, normative frame
/// order): the top step indicator iff `chrome.gate.is_some()`, the left nav
/// rail iff `chrome.rail`. Neither `Panel` is added at all when its
/// condition is false, so egui never reserves its space (FR-004) and the
/// central content spans the full window.
pub fn show_chrome(ui: &mut Ui, chrome: Chrome, shell: &mut Shell) {
    if let Some(gate) = chrome.gate {
        Panel::top(Id::new("gate-step-indicator")).show(ui, |ui| {
            show_step_indicator(ui, gate);
        });
    }
    if chrome.rail {
        Panel::left(Id::new("shell-nav-rail")).show(ui, |ui| {
            shell.nav_rail(ui);
        });
    }
}

/// The step indicator's content (research.md R2): each of the three items
/// painted Complete/Current/Upcoming (tokens only, no literal — contract
/// C10), joined by divider-coloured connectors, plus one non-interactive
/// `ProgressIndicator` AccessKit node for the whole row (contract C4). The
/// three item labels are plain, non-focusable `Label`s, so Tab order into
/// the gate content underneath is unchanged.
fn show_step_indicator(ui: &mut Ui, current: GateStep) {
    let roles = theme::roles(ui.visuals());
    let n = current.position();
    let progress_label = tr_args(
        "gate-step-progress",
        &[
            ("current", n.to_string()),
            ("total", GATE_STEP_TOTAL.to_string()),
            ("label", tr(current.label_key())),
        ],
    );

    let dot = theme::space::SM;
    let row = ui.horizontal(|ui| {
        for (i, step) in GateStep::ALL.into_iter().enumerate() {
            if i > 0 {
                let (rect, _) = ui.allocate_exact_size(vec2(theme::space::LG, dot), Sense::hover());
                ui.painter().hline(
                    rect.x_range(),
                    rect.center().y,
                    Stroke::new(
                        controls::DEFAULT_OUTLINE_WIDTH,
                        theme::tokens::divider_color_for(roles),
                    ),
                );
            }
            let state = step.state_relative_to(current);
            let (rect, _) = ui.allocate_exact_size(vec2(dot, dot), Sense::hover());
            match state {
                StepState::Complete => {
                    ui.painter()
                        .circle_filled(rect.center(), dot / 2.0, roles.accent);
                }
                StepState::Current => {
                    ui.painter().circle_stroke(
                        rect.center(),
                        dot / 2.0,
                        controls::focus_ring(roles),
                    );
                }
                StepState::Upcoming => {
                    ui.painter().circle_stroke(
                        rect.center(),
                        dot / 2.0,
                        Stroke::new(
                            controls::DEFAULT_OUTLINE_WIDTH,
                            theme::tokens::divider_color_for(roles),
                        ),
                    );
                }
            }
            let text_color = if matches!(state, StepState::Current) {
                roles.text_primary
            } else {
                roles.text_secondary
            };
            ui.label(egui::RichText::new(tr(step.label_key())).color(text_color));
        }
    });

    // One non-interactive AccessKit node for the whole indicator (contract
    // C4): built directly, not via `Response::widget_info`, so no
    // `Action::Focus`/`Action::Click` is ever added — the indicator
    // contributes no focusable node.
    ui.ctx().accesskit_node_builder(row.response.id, |b| {
        b.set_role(egui::accesskit::Role::ProgressIndicator);
        b.set_label(progress_label);
        b.set_numeric_value(f64::from(n));
        b.set_max_numeric_value(f64::from(GATE_STEP_TOTAL));
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use egui::accesskit::Role;
    use egui::{Context, RawInput, Ui};
    use modplayer_core::{NotificationCenter, Severity};

    #[test]
    fn default_section_is_library() {
        assert_eq!(Shell::default().section, Section::Library);
    }

    // -- T002 (US1, contract C1): GateStep/StepState/Chrome::for_frame ----

    #[test]
    fn gate_step_position_and_label_key() {
        assert_eq!(GateStep::Welcome.position(), 1);
        assert_eq!(GateStep::SignIn.position(), 2);
        assert_eq!(GateStep::AudioOutputCheck.position(), 3);
        assert_eq!(GateStep::Welcome.label_key(), "gate-step-welcome");
        assert_eq!(GateStep::SignIn.label_key(), "gate-step-sign-in");
        assert_eq!(
            GateStep::AudioOutputCheck.label_key(),
            "gate-step-audio-output-check"
        );
        assert_eq!(GateStep::ALL.len(), 3);
        assert_eq!(GATE_STEP_TOTAL, 3);
    }

    #[test]
    fn gate_step_from_launch_step_is_one_to_one() {
        assert_eq!(
            GateStep::from_launch_step(LaunchStep::Welcome),
            Some(GateStep::Welcome)
        );
        assert_eq!(
            GateStep::from_launch_step(LaunchStep::SignIn),
            Some(GateStep::SignIn)
        );
        assert_eq!(
            GateStep::from_launch_step(LaunchStep::DeviceCheck),
            Some(GateStep::AudioOutputCheck)
        );
        assert_eq!(GateStep::from_launch_step(LaunchStep::Main), None);
    }

    #[test]
    fn state_relative_to_is_a_pure_position_comparison() {
        for current in GateStep::ALL {
            for step in GateStep::ALL {
                let expected = match step.position().cmp(&current.position()) {
                    std::cmp::Ordering::Less => StepState::Complete,
                    std::cmp::Ordering::Equal => StepState::Current,
                    std::cmp::Ordering::Greater => StepState::Upcoming,
                };
                assert_eq!(step.state_relative_to(current), expected);
            }
        }
    }

    /// Contract C1: `Chrome::for_frame` matches research.md R1's table for
    /// all 4 `LaunchStep` values × 2 `device_check_open` values,
    /// exhaustively; `rail ⇒ gate.is_none()` holds for every case too.
    #[test]
    fn chrome_for_frame_matches_the_table_exhaustively() {
        let cases = [
            (LaunchStep::Welcome, false, false, Some(GateStep::Welcome)),
            (LaunchStep::Welcome, true, false, Some(GateStep::Welcome)),
            (LaunchStep::SignIn, false, false, Some(GateStep::SignIn)),
            (LaunchStep::SignIn, true, false, Some(GateStep::SignIn)),
            (
                LaunchStep::DeviceCheck,
                false,
                false,
                Some(GateStep::AudioOutputCheck),
            ),
            (
                LaunchStep::DeviceCheck,
                true,
                false,
                Some(GateStep::AudioOutputCheck),
            ),
            (LaunchStep::Main, true, false, None),
            (LaunchStep::Main, false, true, None),
        ];
        for (step, device_check_open, expected_rail, expected_gate) in cases {
            let chrome = Chrome::for_frame(step, device_check_open);
            assert_eq!(
                chrome.rail, expected_rail,
                "step={step:?} device_check_open={device_check_open}"
            );
            assert_eq!(
                chrome.gate, expected_gate,
                "step={step:?} device_check_open={device_check_open}"
            );
            assert_eq!(chrome.navigation_enabled(), chrome.rail);
            assert!(
                !chrome.rail || chrome.gate.is_none(),
                "rail must imply no gate: {chrome:?}"
            );
        }
    }

    /// Every shell/nav/notification widget exposes an explicit accessible
    /// name (research.md R9's accessibility caveat, FR-022): render the nav
    /// rail, the placeholders, and one notification of each severity, then
    /// walk the AccessKit tree `egui` produces and assert every clickable
    /// (`Role::Button`) node — every nav button, every Dismiss button — has
    /// a non-empty label.
    #[test]
    fn every_shell_and_notification_widget_has_an_accessible_name() {
        let ctx = Context::default();
        theme::apply_tokens(&ctx);
        ctx.enable_accesskit();

        let mut shell = Shell::default();
        let mut center = NotificationCenter::new();
        center.raise(Severity::Critical, "sample-notification-critical");
        center.raise(Severity::Warning, "sample-notification-warning");
        center.raise(Severity::Info, "sample-notification-info");

        let mut stack_state = crate::notifications::StackState::default();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            shell.nav_rail(ui);
            crate::notifications::show(ui, &center, &mut stack_state);
        });

        let Some(update) = output.platform_output.accesskit_update.take() else {
            panic!("accesskit_update should be populated once enabled");
        };
        // The font atlas texture delta from this synthetic frame is never
        // painted by anything; tell egui we're intentionally discarding it
        // rather than triggering its debug-mode "unapplied deltas" panic.
        output.drop_without_applying_deltas();

        let mut buttons_checked = 0;
        for (_, node) in &update.nodes {
            if node.role() == Role::Button {
                buttons_checked += 1;
                assert!(
                    node.label().is_some_and(|label| !label.is_empty()),
                    "a Button-role node has no accessible name"
                );
            }
        }
        // 5 nav buttons + 3 Dismiss buttons (one per notification raised above).
        assert_eq!(buttons_checked, 8);
    }

    /// Run `render` in a fresh headless context with AccessKit enabled,
    /// and assert every `Button`/`ProgressIndicator`-role node it produces
    /// has a non-empty accessible name (research.md R9, FR-023;
    /// Constitution X). A fresh `Context` per call avoids widget-id reuse
    /// across the different screen configurations this test exercises.
    fn accessible_widget_count(render: impl FnMut(&mut Ui)) -> usize {
        let ctx = Context::default();
        // 014-design-tokens-and-type-scale (US2): the token `Style`'s
        // `Name("display")`/`Name("section")` text styles must be
        // installed before rendering any of the screens this helper
        // exercises (welcome, sign-in, settings › account/about), exactly
        // as `App::new`/`App::update` do.
        theme::apply_tokens(&ctx);
        ctx.enable_accesskit();
        let mut output = ctx.run_ui(RawInput::default(), render);
        let Some(update) = output.platform_output.accesskit_update.take() else {
            panic!("accesskit_update should be populated once enabled");
        };
        output.drop_without_applying_deltas();

        let mut checked = 0;
        for (_, node) in &update.nodes {
            if node.role() == Role::Button || node.role() == Role::ProgressIndicator {
                checked += 1;
                assert!(
                    node.label().is_some_and(|label| !label.is_empty()),
                    "a {:?}-role node has no accessible name",
                    node.role()
                );
            }
        }
        checked
    }

    fn fresh_account_service() -> modplayer_account::AccountService {
        use modplayer_account::{AuthorizationService, Clock, FakeAuthorizationService, FakeClock};
        use modplayer_secure_store::{MemorySecureStore, SecureStore};
        use std::sync::Arc;

        modplayer_account::AccountService::new(
            Arc::new(MemorySecureStore::new()) as Arc<dyn SecureStore>,
            Arc::new(FakeAuthorizationService::new()) as Arc<dyn AuthorizationService>,
            Arc::new(FakeClock::default()) as Arc<dyn Clock>,
            std::env::temp_dir().join(format!(
                "modplayer-ui-shell-a11y-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or_default()
            )),
        )
    }

    /// Drive a real sign-in to `Active`/Premium via a real loopback
    /// listener and `FakeAuthorizationService`, exactly like
    /// `modplayer-account`'s own US2 integration tests — needed to reach
    /// Settings › Account's signed-in view, which only renders once
    /// `state()` is `Active`/`Expired`.
    fn active_account_service() -> modplayer_account::AccountService {
        use modplayer_account::auth_service::REDIRECT_PATH;
        use modplayer_account::fake_auth::ScriptedCall;
        use modplayer_account::{
            AccountEvent, AuthorizationService, Clock, FakeAuthorizationService, FakeClock,
            Profile, Tier, TokenSet,
        };
        use modplayer_secure_store::{MemorySecureStore, SecureStore};
        use std::io::Write;
        use std::net::TcpStream;
        use std::sync::Arc;
        use std::time::Duration;

        let auth = FakeAuthorizationService::new();
        auth.push_exchange_code(ScriptedCall::ok(TokenSet {
            access_token: "access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_in: Duration::from_secs(3600),
            scope: "streaming".to_string(),
        }));
        auth.push_fetch_profile(ScriptedCall::ok(Profile {
            id: "user-1".to_string(),
            display_name: Some("Alex".to_string()),
            tier: Tier::Premium,
        }));

        let mut service = modplayer_account::AccountService::new(
            Arc::new(MemorySecureStore::new()) as Arc<dyn SecureStore>,
            Arc::new(auth) as Arc<dyn AuthorizationService>,
            Arc::new(FakeClock::default()) as Arc<dyn Clock>,
            std::env::temp_dir().join(format!(
                "modplayer-ui-shell-a11y-active-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or_default()
            )),
        );

        let events = service.start_sign_in();
        let Some(AccountEvent::BrowserUrlReady(url)) = events.into_iter().next() else {
            panic!("expected BrowserUrlReady");
        };
        let query = url.split('?').nth(1).expect("query string");
        let mut port = None;
        let mut state = None;
        for pair in query.split('&') {
            let (k, v) = pair.split_once('=').expect("key=value");
            match k {
                "port" => port = Some(v.to_string()),
                "state" => state = Some(v.to_string()),
                _ => {}
            }
        }
        let (port, state) = (port.expect("port"), state.expect("state"));

        let addr = format!("127.0.0.1:{port}");
        let mut connected = false;
        for _ in 0..50 {
            if let Ok(mut stream) = TcpStream::connect(&addr) {
                let _ = stream.write_all(
                    format!("GET {REDIRECT_PATH}?code=auth-code&state={state} HTTP/1.1\r\n\r\n")
                        .as_bytes(),
                );
                connected = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(connected, "could not connect to loopback listener");

        for _ in 0..200 {
            service.tick();
            if matches!(service.state(), modplayer_account::SessionState::Active) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            matches!(service.state(), modplayer_account::SessionState::Active),
            "test setup: expected Active, got {:?}",
            service.state()
        );
        service
    }

    /// Every account/disclosure widget exposes an explicit accessible name
    /// (contracts/ui-surface.md; FR-023; Constitution X): the sign-in
    /// step's SignedOut/Authorizing/tier-result sub-views, Settings ›
    /// Account's signed-in/signed-out views and sign-out confirmation
    /// modal, Settings › About's main/privacy sub-views, and the Welcome
    /// (first-launch disclosure) screen — T106's final accessibility
    /// sweep across every US1-US4 surface.
    #[test]
    fn every_account_widget_has_an_accessible_name() {
        use crate::settings::about::{self, AboutScreen};
        use crate::sign_in::{self, SignInScreen, TierResult};

        // SignedOut, no note: one "Sign in" button.
        let mut signed_out = fresh_account_service();
        let mut signed_out_screen = SignInScreen::default();
        assert_eq!(
            accessible_widget_count(|ui| {
                sign_in::show(ui, &mut signed_out, &mut signed_out_screen);
            }),
            1
        );

        // Authorizing: "Open the browser again" + "Cancel".
        let mut authorizing = fresh_account_service();
        authorizing.start_sign_in();
        let mut authorizing_screen = SignInScreen::default();
        assert_eq!(
            accessible_widget_count(|ui| {
                sign_in::show(ui, &mut authorizing, &mut authorizing_screen);
            }),
            2
        );

        // Tier result: Free — "Open upgrade page" + "Continue".
        let mut tier_service = fresh_account_service();
        let mut free_screen = SignInScreen::default();
        free_screen.set_tier_result(TierResult::Free);
        assert_eq!(
            accessible_widget_count(|ui| {
                sign_in::show(ui, &mut tier_service, &mut free_screen);
            }),
            2
        );

        // Tier result: Unknown — "Retry" + "Continue".
        let mut unknown_screen = SignInScreen::default();
        unknown_screen.set_tier_result(TierResult::Unknown);
        assert_eq!(
            accessible_widget_count(|ui| {
                sign_in::show(ui, &mut tier_service, &mut unknown_screen);
            }),
            2
        );

        // Settings > About, main sub-view: "Privacy notice" + "Disclosure".
        let mut about_screen = AboutScreen::default();
        assert_eq!(
            accessible_widget_count(|ui| about::show(ui, &mut about_screen)),
            2
        );

        // The Privacy sub-view's "Back" button — the exact same function
        // About's own Privacy branch calls.
        assert_eq!(
            accessible_widget_count(|ui| {
                let _ = crate::privacy_notice::show(ui);
            }),
            1
        );

        // Settings > Account, signed-in view: "Re-check subscription" +
        // "Sign out" (US3, T085/T086 — the sign-out confirmation modal
        // itself only renders once "Sign out" is clicked, so it adds no
        // widgets to this closed-state count).
        let mut active = active_account_service();
        assert_eq!(
            accessible_widget_count(|ui| {
                let _ = crate::settings::account::show(ui, &mut active);
            }),
            2
        );

        // Settings > Account, signed-out view (US3, T087): "Sign in".
        let mut signed_out_account = fresh_account_service();
        assert_eq!(
            accessible_widget_count(|ui| {
                let _ = crate::settings::account::show(ui, &mut signed_out_account);
            }),
            1
        );

        // Settings > Account, sign-out confirmation modal, open (US3,
        // T085): "Cancel" + destructive "Sign out".
        let mut modal_account = fresh_account_service();
        assert_eq!(
            accessible_widget_count(|ui| {
                let _ =
                    crate::settings::account::show_sign_out_modal_for_test(ui, &mut modal_account);
            }),
            2
        );

        // Welcome, disclosure sub-view (US1, T039/T041): "Privacy notice"
        // + "I understand, continue" + "Decline".
        let mut welcome_controller = fresh_playback_controller();
        let mut welcome_screen = crate::welcome::WelcomeScreen::default();
        assert_eq!(
            accessible_widget_count(|ui| {
                welcome_screen.show(ui, &mut welcome_controller);
            }),
            3
        );
    }

    /// A `PlaybackController` over a silent `FakeBackend`, pointed at a
    /// fresh temp `settings.toml` — enough to draw `WelcomeScreen` without
    /// touching real audio devices (mirrors `settings/developer.rs`'s own
    /// test fixture).
    fn fresh_playback_controller() -> modplayer_core::PlaybackController<
        modplayer_audio_io::FakeBackend,
        modplayer_audio_source_synthetic::SyntheticHost,
    > {
        use modplayer_core::SettingsStore;
        use std::sync::atomic::{AtomicU64, Ordering};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-shell-a11y-welcome-test-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = std::fs::create_dir_all(&dir);
        modplayer_core::PlaybackController::new(
            modplayer_audio_io::FakeBackend::new(vec![]),
            modplayer_audio_source_synthetic::SyntheticHost::new(44_100),
            SettingsStore::with_path(dir.join("settings.toml")),
        )
    }
}
