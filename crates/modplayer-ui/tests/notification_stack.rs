// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! 019-notification-presentation, Phase 3/US1 (contract S1, research
//! R1/R2, FR-001/FR-002, SC-001): the `shell-notifications` `Area` anchors
//! `Align2::RIGHT_BOTTOM` at `(-8, -8)`, and the collapsed stack never
//! intersects the library tab strip/first row, the Settings category
//! list, the plugins table's column headers, or the nav rail, at
//! 960×640 and 1200×820. Cards here are the current, plain (pre-cap,
//! pre-styling) ones — the cap (US2) and severity styling (US3) land in
//! later phases of this same feature.
//!
//! Phase 4/US2 (contract S2/S3, research R3/R4, FR-003–FR-006, SC-002)
//! extends this file with the collapsed 3-card cap, the "{N} more"/"Show
//! fewer" control, and the expanded stack's 60% max-height scroll cap.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use egui::accesskit::Role;
use egui::{
    Align2, Area, CentralPanel, Context, Event, Id, Modifiers, Panel, PointerButton, Pos2,
    RawInput, Rect, vec2,
};
use modplayer_account::{AccountService, FakeAuthorizationService, FakeClock};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{
    Availability, LibraryItem, LibraryPage, LibrarySet, TrackId, TrackRef,
};
use modplayer_audio_source_synthetic::scripted::HydratedReply;
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_core::settings::SettingsStore;
use modplayer_core::{
    NotificationAction, NotificationCenter, PlaybackController, Severity, tr, tr_args,
};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_secure_store::MemorySecureStore;
use modplayer_ui::artwork::ArtworkCache;
use modplayer_ui::library_view::{self, LibraryViewState};
use modplayer_ui::settings::{self, SettingsScreen};
use modplayer_ui::shell::Shell;
use modplayer_ui::{notifications, plugins_view, theme};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-notification-stack-{label}-{}-{unique}",
            std::process::id(),
        ));
        let _ = std::fs::create_dir_all(&dir);
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fresh_store(label: &str) -> (SettingsStore, TempDir) {
    let dir = TempDir::new(label);
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    (store, dir)
}

fn fresh_ctx() -> Context {
    let ctx = Context::default();
    theme::apply_tokens(&ctx);
    ctx
}

fn sized_input(w: f32, h: f32) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(w, h))),
        ..Default::default()
    }
}

// -- Library fixture (mirrors `tests/library_view.rs`'s own helpers,
// trimmed to the one Saved Tracks row this file needs) -----------------

fn fake_device() -> FakeDevice {
    FakeDevice {
        id: DeviceId::new("dev-1").unwrap(),
        name: "Speakers".to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        is_default: true,
    }
}

fn active_library_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
    TempDir,
) {
    let (store, dir) = fresh_store(label);
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![fake_device()];
    let mut controller = PlaybackController::new(FakeBackend::new(devices), host, store);
    controller.launch();
    controller.confirm_device(DeviceId::new("dev-1").unwrap(), BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.tick();
    (controller, handle, dir)
}

/// Syncs exactly one Saved Tracks row ("Track A — Artist") and empties the
/// other three sets, so the Library screen's first list row is visible on
/// the default tab.
fn sync_one_saved_track(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    handle: &ScriptedHostHandle,
) {
    let track_ref = TrackRef::new(
        TrackId::new("spotify:track:a").unwrap(),
        "Track A",
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    );
    handle.script_hydrate(HydratedReply {
        tracks: vec![track_ref.clone()],
        ..Default::default()
    });
    handle.script_library(
        LibrarySet::SavedTracks,
        vec![Ok(LibraryPage {
            set: LibrarySet::SavedTracks,
            items: vec![LibraryItem::Track {
                track: track_ref,
                added_at: None,
            }],
            next_page: None,
            sync_token: None,
        })],
    );
    for set in [
        LibrarySet::SavedAlbums,
        LibrarySet::FollowedArtists,
        LibrarySet::Playlists,
    ] {
        handle.script_library(
            set,
            vec![Ok(LibraryPage {
                set,
                items: vec![],
                next_page: None,
                sync_token: None,
            })],
        );
    }
    controller.library_retry_sync();
    for _ in 0..12 {
        controller.tick();
    }
}

// -- Plugins fixture (mirrors `tests/plugins_view.rs`'s own
// `fixture_controller` — discovery only, never spawned) ----------------

static PLUGIN_ENV_LOCK: Mutex<()> = Mutex::new(());

fn plugin_fixture_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TempDir,
    TempDir,
    TempDir,
) {
    let (store, dir) = fresh_store(label);
    let plugin_state_dir = TempDir::new(&format!("{label}-plugin-state"));
    let track_state_dir = TempDir::new(&format!("{label}-track-state"));
    let controller = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: narrowly scopes each mutation to the one synchronous
        // read `PlaybackController::new` makes of it, serialized against
        // every other test in this binary via the lock above.
        unsafe {
            std::env::set_var("MODPLAYER_PLUGIN_FIXTURES", "1");
            std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", plugin_state_dir.path());
            std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path());
        }
        let controller =
            PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_FIXTURES");
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
            std::env::remove_var("MODPLAYER_TRACK_STATE_DIR");
        }
        controller
    };
    (controller, dir, plugin_state_dir, track_state_dir)
}

// -- Settings fixture ----------------------------------------------------

fn fresh_account(dir: &TempDir) -> AccountService {
    AccountService::new(
        std::sync::Arc::new(MemorySecureStore::new()),
        std::sync::Arc::new(FakeAuthorizationService::new()),
        std::sync::Arc::new(FakeClock::default()),
        dir.path().to_path_buf(),
    )
}

// -- AccessKit node collection (mirrors `tests/responsive_dock.rs`'s own
// `AccessNode`/`run_frame`) ---------------------------------------------

#[derive(Debug, Clone)]
struct AccessNode {
    role: Role,
    label: Option<String>,
    value: Option<String>,
    bounds: Option<Rect>,
}

impl AccessNode {
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

fn run_frame(
    ctx: &Context,
    input: RawInput,
    mut render: impl FnMut(&mut egui::Ui),
) -> Vec<AccessNode> {
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
        .map(|(_, node)| AccessNode {
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

/// Every rect of a node whose accessible name is in `names` (nav rail,
/// tab strip, settings category list, plugin table headers — all decided
/// by name alone, since their exact `Role` differs across widgets but
/// their Fluent-resolved label doesn't).
fn rects_named(nodes: &[AccessNode], names: &[String]) -> Vec<Rect> {
    nodes
        .iter()
        .filter(|n| {
            n.accessible_name()
                .is_some_and(|name| names.contains(&name.to_string()))
        })
        .filter_map(|n| n.bounds)
        .collect()
}

fn nav_rail_names() -> Vec<String> {
    [
        "nav-library",
        "nav-search",
        "nav-now-playing",
        "nav-plugins",
        "nav-settings",
    ]
    .into_iter()
    .map(tr)
    .collect()
}

fn library_tab_names() -> Vec<String> {
    [
        "library-tab-saved-tracks",
        "library-tab-saved-albums",
        "library-tab-followed-artists",
        "library-tab-playlists",
        "library-tab-recently-played",
    ]
    .into_iter()
    .map(tr)
    .collect()
}

fn plugin_header_names() -> Vec<String> {
    [
        "plugins-col-name",
        "plugins-col-version",
        "plugins-col-source",
        "plugins-col-enabled",
        "plugins-col-health",
        "plugins-col-permissions",
        "plugins-col-cpu",
        "plugins-col-memory",
    ]
    .into_iter()
    .map(tr)
    .collect()
}

fn settings_category_names() -> Vec<String> {
    modplayer_core::settings_registry::SettingsCategory::ALL
        .into_iter()
        .map(|c| tr(c.label_key()))
        .collect()
}

/// The library screen (nav rail + tab strip + Saved Tracks' one row) at
/// `w`×`h`: nav rail + tab-strip rects, plus every `Role::ListItem` rect
/// (the library's first, and here only, list row).
fn library_protected_rects(
    ctx: &Context,
    w: f32,
    h: f32,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
) -> Vec<Rect> {
    let mut artwork = ArtworkCache::new();
    let mut state = LibraryViewState::default();
    let mut shell = Shell::default();
    let nodes = run_frame(ctx, sized_input(w, h), |ui| {
        Panel::left(Id::new("shell-nav-rail")).show(ui, |ui| {
            shell.nav_rail(ui);
        });
        CentralPanel::default().show(ui, |ui| {
            let _ = library_view::show(ui, controller, &mut artwork, &mut state);
        });
    });
    let mut names = nav_rail_names();
    names.extend(library_tab_names());
    let mut rects = rects_named(&nodes, &names);
    rects.extend(
        nodes
            .iter()
            .filter(|n| n.role == Role::ListItem)
            .filter_map(|n| n.bounds),
    );
    rects
}

/// The Settings screen (nav rail + category list) at `w`×`h`.
fn settings_protected_rects(
    ctx: &Context,
    w: f32,
    h: f32,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    account: &mut AccountService,
    screen: &mut SettingsScreen,
) -> Vec<Rect> {
    let mut shell = Shell::default();
    let nodes = run_frame(ctx, sized_input(w, h), |ui| {
        Panel::left(Id::new("shell-nav-rail")).show(ui, |ui| {
            shell.nav_rail(ui);
        });
        CentralPanel::default().show(ui, |ui| {
            let _ = settings::show(ui, controller, account, screen);
        });
    });
    let mut names = nav_rail_names();
    names.extend(settings_category_names());
    rects_named(&nodes, &names)
}

/// The Plugins screen (nav rail + column headers) at `w`×`h`.
fn plugins_protected_rects(
    ctx: &Context,
    w: f32,
    h: f32,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
) -> Vec<Rect> {
    let mut shell = Shell::default();
    let nodes = run_frame(ctx, sized_input(w, h), |ui| {
        Panel::left(Id::new("shell-nav-rail")).show(ui, |ui| {
            shell.nav_rail(ui);
        });
        CentralPanel::default().show(ui, |ui| {
            plugins_view::show(ui, controller);
        });
    });
    let mut names = nav_rail_names();
    names.extend(plugin_header_names());
    rects_named(&nodes, &names)
}

fn draw_stack(
    ui: &mut egui::Ui,
    center: &NotificationCenter,
    state: &mut notifications::StackState,
) {
    Area::new(Id::new("shell-notifications"))
        .anchor(Align2::RIGHT_BOTTOM, vec2(-8.0, -8.0))
        .show(ui.ctx(), |ui| notifications::show(ui, center, state));
}

/// Cap on the extra settle frames [`settle_stack`] renders past this
/// suite's original fixed "two-frame settle" convention (T023, US4):
/// pre-US4 cards were a single row each, so an expanded multi-card stack
/// always fit the 60%-height cap (contract S3) without ever engaging real
/// scrolling — auto-sizing to content converged in one extra frame. US4's
/// taller, multi-row cards (message + optional Show-more/Details rows +
/// an actions row) can push a 4+-card expanded stack past that cap, and
/// *actual* scrolling's offset/clip-rect memory converges over a few more
/// frames than plain auto-shrink-to-content did. Generous, but bounded so
/// a genuine non-convergence fails loudly instead of looping forever.
const MAX_SETTLE_FRAMES: usize = 12;

/// Render `render` (redrawing the `shell-notifications` `Area`) repeatedly
/// until its measured rect stops moving between consecutive frames, or
/// panic past [`MAX_SETTLE_FRAMES`] — generalizes this suite's original
/// hard-coded "two-frame settle" to content whose settle time isn't a
/// fixed constant (see `MAX_SETTLE_FRAMES`'s own doc comment for why).
fn settle_stack(
    ctx: &Context,
    input: impl Fn() -> RawInput,
    mut render: impl FnMut(&mut egui::Ui),
) {
    let mut previous: Option<Rect> = None;
    for _ in 0..MAX_SETTLE_FRAMES {
        let output = ctx.run_ui(input(), |ui| render(ui));
        output.drop_without_applying_deltas();
        let rect = ctx.memory(|m| m.area_rect(Id::new("shell-notifications")));
        if let (Some(prev), Some(cur)) = (previous, rect) {
            let moved = (prev.min - cur.min).length() + (prev.max - cur.max).length();
            if moved < 0.05 {
                return;
            }
        }
        previous = rect;
    }
    panic!(
        "shell-notifications area did not settle within {MAX_SETTLE_FRAMES} frames \
         (last measured rect {previous:?})"
    );
}

/// The collapsed stack's own rect at `w`×`h`, settled via [`settle_stack`]
/// (research R1/R2).
fn stack_rect(w: f32, h: f32, center: &NotificationCenter) -> Rect {
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut state = notifications::StackState::default();
    settle_stack(
        &ctx,
        || sized_input(w, h),
        |ui| {
            draw_stack(ui, center, &mut state);
        },
    );
    ctx.memory(|m| m.area_rect(Id::new("shell-notifications")))
        .expect("the notification area must record its rect after rendering")
}

/// A worst-case notification load (research R2, T023): three Critical
/// cards, each with two actions, a "Details" toggle, a "Show more" toggle
/// (forced by a 40-character device name that overruns the 2-row cap at
/// 360 px card width) — the largest a single card can get, per contract
/// S4's full layout (accent bar, icon+severity+attribution row, 2-row
/// message + Show more, Details payload, two actions + Dismiss). No
/// production raise site combines actions with `detail` (contract C4), so
/// this fixture uses the test-only `raise_with_actions_and_detail` to
/// reach that worst case anyway.
fn worst_case_center() -> NotificationCenter {
    let mut center = NotificationCenter::new();
    let device_name: String = "A".repeat(40);
    for _ in 0..3 {
        center.raise_with_actions_and_detail(
            Severity::Critical,
            "device-lost",
            vec![
                ("device", device_name.clone()),
                ("fallback", "the system default output".to_string()),
            ],
            vec![NotificationAction::SignIn, NotificationAction::RetrySource],
            "coreaudio:worst-case-detail-id".to_string(),
        );
    }
    center
}

fn assert_no_intersection(stack: Rect, protected: &[Rect], label: &str) {
    assert!(
        !protected.is_empty(),
        "{label}: no protected rects were found to check against"
    );
    for r in protected {
        let overlap = stack.intersect(*r);
        let overlapping = overlap.width() > 0.0 && overlap.height() > 0.0;
        assert!(
            !overlapping,
            "{label}: notification stack {stack:?} intersects protected rect {r:?} (overlap {overlap:?})"
        );
    }
}

// -- S1 placement ---------------------------------------------------------

/// Contract S1: the `shell-notifications` `Area` anchors
/// `Align2::RIGHT_BOTTOM` at `(-8, -8)` — its rect's bottom-right corner
/// sits 8 logical px in from the window's own bottom-right corner, at
/// both sizes this feature's geometry test uses.
#[test]
fn notification_area_anchors_bottom_right_eight_px_offset() {
    let center = worst_case_center();
    for (w, h) in [(960.0_f32, 640.0_f32), (1200.0_f32, 820.0_f32)] {
        let rect = stack_rect(w, h, &center);
        let eps = 0.5;
        assert!(
            (rect.right() - (w - 8.0)).abs() < eps,
            "at {w}x{h}: right edge {} must sit 8px from the window's right edge ({w})",
            rect.right()
        );
        assert!(
            (rect.bottom() - (h - 8.0)).abs() < eps,
            "at {w}x{h}: bottom edge {} must sit 8px from the window's bottom edge ({h})",
            rect.bottom()
        );
    }
}

// -- SC-001 zero intersection ---------------------------------------------

/// SC-001: at 960×640 and 1200×820, the collapsed notification stack
/// never intersects the library tab strip, the library's first list row,
/// the Settings category list, the plugins table's column headers, or
/// the nav rail.
#[test]
fn collapsed_stack_never_covers_protected_regions() {
    let center = worst_case_center();

    for (w, h) in [(960.0_f32, 640.0_f32), (1200.0_f32, 820.0_f32)] {
        let stack = stack_rect(w, h, &center);

        let library_ctx = fresh_ctx();
        library_ctx.enable_accesskit();
        let (mut lib_controller, handle, _dir) =
            active_library_controller(&format!("library-{w}x{h}"));
        sync_one_saved_track(&mut lib_controller, &handle);
        let library_rects = library_protected_rects(&library_ctx, w, h, &mut lib_controller);
        assert_no_intersection(stack, &library_rects, &format!("library screen at {w}x{h}"));

        let settings_ctx = fresh_ctx();
        settings_ctx.enable_accesskit();
        let (mut settings_controller, dir) = {
            let (store, dir) = fresh_store(&format!("settings-{w}x{h}"));
            (
                PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store),
                dir,
            )
        };
        let mut account = fresh_account(&dir);
        let mut screen = SettingsScreen::new(&settings_controller);
        let settings_rects = settings_protected_rects(
            &settings_ctx,
            w,
            h,
            &mut settings_controller,
            &mut account,
            &mut screen,
        );
        assert_no_intersection(
            stack,
            &settings_rects,
            &format!("settings screen at {w}x{h}"),
        );

        let plugins_ctx = fresh_ctx();
        plugins_ctx.enable_accesskit();
        let (mut plugin_controller, _dir, _psd, _tsd) =
            plugin_fixture_controller(&format!("plugins-{w}x{h}"));
        let plugin_rects = plugins_protected_rects(&plugins_ctx, w, h, &mut plugin_controller);
        assert_no_intersection(stack, &plugin_rects, &format!("plugins screen at {w}x{h}"));
    }
}

// -- SC-002 cap / overflow / expand (Phase 4/US2) -------------------------

/// `n` Info notifications, newest first — a stand-in "worst case" load for
/// this phase's cap/overflow behaviour (severity doesn't matter here; US3
/// gives it a colour/icon later).
fn notifications(n: usize) -> NotificationCenter {
    let mut center = NotificationCenter::new();
    for _ in 0..n {
        center.raise(Severity::Info, "sample-notification-info");
    }
    center
}

fn draw_stack_with_state(
    ui: &mut egui::Ui,
    center: &NotificationCenter,
    state: &mut notifications::StackState,
) -> notifications::NotificationInteraction {
    Area::new(Id::new("shell-notifications"))
        .anchor(Align2::RIGHT_BOTTOM, vec2(-8.0, -8.0))
        .show(ui.ctx(), |ui| notifications::show(ui, center, state))
        .inner
}

/// Renders the stack and returns its accesskit nodes, settled via
/// [`settle_stack`] first (an `Area` needs a prior frame's measured size
/// to anchor itself, and US4's taller cards can need a few more than the
/// two frames pre-US4 content did — see `MAX_SETTLE_FRAMES`'s own doc
/// comment — so a stale-bounds read can't hand back a click target the
/// real layout has already moved away from).
fn nodes_for(
    ctx: &Context,
    w: f32,
    h: f32,
    center: &NotificationCenter,
    state: &mut notifications::StackState,
) -> Vec<AccessNode> {
    settle_stack(
        ctx,
        || sized_input(w, h),
        |ui| {
            draw_stack_with_state(ui, center, state);
        },
    );
    run_frame(ctx, sized_input(w, h), |ui| {
        draw_stack_with_state(ui, center, state);
    })
}

fn button_named<'a>(nodes: &'a [AccessNode], name: &str) -> Option<&'a AccessNode> {
    nodes
        .iter()
        .find(|n| n.role == Role::Button && n.accessible_name() == Some(name))
}

fn dismiss_button_count(nodes: &[AccessNode]) -> usize {
    let dismiss = tr("notification-dismiss");
    nodes
        .iter()
        .filter(|n| n.role == Role::Button && n.accessible_name() == Some(dismiss.as_str()))
        .count()
}

fn more_label(count: usize) -> String {
    tr_args("notification-more", &[("count", count.to_string())])
}

/// Press then release the primary button at `pos`, in two separate frames
/// (mirrors `tests/queue_view.rs`'s own proven press/release pattern — a
/// single frame is not guaranteed to register `clicked()`).
fn click(
    ctx: &Context,
    w: f32,
    h: f32,
    pos: Pos2,
    center: &NotificationCenter,
    state: &mut notifications::StackState,
) {
    let mut press = sized_input(w, h);
    press.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| {
        draw_stack_with_state(ui, center, state);
    });
    output.drop_without_applying_deltas();

    let mut release = sized_input(w, h);
    release.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| {
        draw_stack_with_state(ui, center, state);
    });
    output.drop_without_applying_deltas();
}

/// SC-002 (first half): 3 non-dismissed notifications render with no
/// overflow control at all — the cap only ever kicks in past 3.
#[test]
fn three_notifications_render_with_no_overflow_control() {
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let center = notifications(3);
    let mut state = notifications::StackState::default();
    let (w, h) = (960.0_f32, 640.0_f32);

    let nodes = nodes_for(&ctx, w, h, &center, &mut state);
    assert_eq!(
        dismiss_button_count(&nodes),
        3,
        "all 3 notifications must render"
    );
    assert!(
        button_named(&nodes, &more_label(1)).is_none(),
        "no overflow control below the cap"
    );
    assert!(
        button_named(&nodes, &tr("notification-show-fewer")).is_none(),
        "no `Show fewer` control when nothing is hidden"
    );
}

/// SC-002 (second half): a 4th notification is capped behind "1 more";
/// activating it reveals all 4 and offers "Show fewer"; activating that
/// collapses back to 3 + the control; dismissing down to 3 while expanded
/// makes the control disappear and the stack collapse on its own (FR-006),
/// with nothing lost.
#[test]
fn fourth_notification_is_capped_and_the_overflow_control_expands_and_collapses() {
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut center = notifications(4);
    let mut state = notifications::StackState::default();
    let (w, h) = (960.0_f32, 640.0_f32);

    // Collapsed: 3 cards + "1 more".
    let nodes = nodes_for(&ctx, w, h, &center, &mut state);
    assert_eq!(
        dismiss_button_count(&nodes),
        3,
        "collapsed stack shows exactly 3 cards"
    );
    let label = more_label(1);
    let more =
        button_named(&nodes, &label).unwrap_or_else(|| panic!("expected a `{label}` control"));
    let more_pos = more
        .bounds
        .expect("the overflow control must have bounds")
        .center();

    // Activate it: all 4 cards, relabelled "Show fewer", no more "1 more".
    click(&ctx, w, h, more_pos, &center, &mut state);
    let nodes = nodes_for(&ctx, w, h, &center, &mut state);
    assert_eq!(
        dismiss_button_count(&nodes),
        4,
        "expanded stack shows every card"
    );
    assert!(
        button_named(&nodes, &label).is_none(),
        "the overflow control is gone once expanded"
    );
    let fewer = button_named(&nodes, &tr("notification-show-fewer"))
        .expect("expanded stack must offer `Show fewer`");
    let fewer_pos = fewer
        .bounds
        .expect("`Show fewer` must have bounds")
        .center();

    // Activate "Show fewer": back to 3 cards + the control.
    click(&ctx, w, h, fewer_pos, &center, &mut state);
    let nodes = nodes_for(&ctx, w, h, &center, &mut state);
    assert_eq!(
        dismiss_button_count(&nodes),
        3,
        "collapsing returns to exactly 3 cards"
    );
    assert!(
        button_named(&nodes, &label).is_some(),
        "collapsing brings the overflow control back"
    );

    // Expand again, then dismiss down to 3 while expanded: the button
    // disappears, the stack collapses on its own, and nothing is lost.
    let more = button_named(&nodes, &label).expect("the overflow control must be present");
    let more_pos = more.bounds.unwrap().center();
    click(&ctx, w, h, more_pos, &center, &mut state);

    let newest_id = center
        .visible()
        .next()
        .map(|n| n.id)
        .expect("at least one visible notification");
    center.dismiss(newest_id);

    let nodes = nodes_for(&ctx, w, h, &center, &mut state);
    assert_eq!(
        dismiss_button_count(&nodes),
        3,
        "no notification is lost dropping to the cap"
    );
    assert!(
        button_named(&nodes, &tr("notification-show-fewer")).is_none(),
        "`Show fewer` disappears once overflow is 0"
    );
    assert!(
        button_named(&nodes, &more_label(1)).is_none(),
        "no overflow control once every remaining card fits"
    );
}

/// S3/R4: the expanded stack's card list is capped at 60% of the window's
/// height (via a `ScrollArea`), not the unbounded height 17 extra cards
/// would otherwise need.
#[test]
fn expanded_stack_stays_within_the_sixty_percent_scroll_cap() {
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let center = notifications(20);
    let mut state = notifications::StackState::default();
    let (w, h) = (960.0_f32, 640.0_f32);

    let nodes = nodes_for(&ctx, w, h, &center, &mut state);
    let label = more_label(17);
    let more =
        button_named(&nodes, &label).unwrap_or_else(|| panic!("expected a `{label}` control"));
    let more_pos = more
        .bounds
        .expect("the overflow control must have bounds")
        .center();
    click(&ctx, w, h, more_pos, &center, &mut state);

    // Settle via `settle_stack` (matches `stack_rect`'s own convention
    // above) before reading the Area's measured rect.
    settle_stack(
        &ctx,
        || sized_input(w, h),
        |ui| {
            draw_stack_with_state(ui, &center, &mut state);
        },
    );
    let rect = ctx
        .memory(|m| m.area_rect(Id::new("shell-notifications")))
        .expect("the notification area must record its rect after rendering");

    // The scroll area itself is capped at 60% of the window height; allow
    // headroom above that for the "Show fewer" button and frame margins
    // that sit outside the scroll area (contract S3).
    let max_expected = 0.6 * h + 80.0;
    assert!(
        rect.height() <= max_expected,
        "expanded stack height {} exceeds the 60% scroll cap + control headroom ({max_expected})",
        rect.height()
    );
}
