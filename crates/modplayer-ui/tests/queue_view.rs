// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T072 (US2): the Queue panel (contracts/ui-surface.md §2) — rows render
//! in effective order, and the keyboard-operable Move up/Move down/Play
//! next/Remove actions mutate the controller's queue.
//!
//! 021-transport-bar-and-panel-layout (contracts/queue-row.md Q1-Q11):
//! rows now render through `rows::queue_row` — the 3-column geometry
//! (artwork, title-over-artist, right-aligned position), the current
//! row's ▶ glyph + leading `nav_indicator` accent bar (never the 016
//! selected fill or a visible "Now playing:" prefix), and the four quiet
//! actions painted on every row regardless of hover.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui::accesskit::{NodeId, Role};
use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{PlaybackController, tr, tr_args};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::artwork::ArtworkCache;
use modplayer_ui::theme::controls as theme_controls;
use modplayer_ui::theme::tokens::LIGHT;
use modplayer_ui::widgets::initials::initials;

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-queue-view-{label}-{}-{unique}",
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

fn fake_device() -> FakeDevice {
    FakeDevice {
        id: DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        name: "Speakers".to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        is_default: true,
    }
}

fn track(id: &str) -> TrackRef {
    TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap_or_else(|_| unreachable!()),
        id,
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    )
}

/// A track with a distinct title/artist (unlike [`track`], whose title is
/// just the id) — for the geometry/placeholder tests, where the painted
/// title and artist strings must be told apart.
fn track_full(id: &str, title: &str, artist: &str) -> TrackRef {
    TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap_or_else(|_| unreachable!()),
        title,
        vec![artist.to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    )
}

/// A controller over a confirmed device, ready for a queue to be built on
/// (mirrors `modplayer-core`'s own `controller_streaming.rs` fixture).
fn ready_controller(label: &str) -> (PlaybackController<FakeBackend, ScriptedHost>, TempDir) {
    let (store, dir) = fresh_store(label);
    let devices = vec![fake_device()];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), ScriptedHost::new(), store);
    controller.launch();
    controller.confirm_device(
        DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        BufferPreset::Balanced,
    );
    (controller, dir)
}

/// `queue_view::show` renders through `theme::mono_text`/`theme::
/// section_label`, which only exist once the token `Style` is installed
/// (mirrors `now_playing.rs`/`markers.rs`'s own identically-named helper).
fn fresh_ctx() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    }
}

#[derive(Debug, Clone)]
struct AccessNode {
    id: NodeId,
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

/// One `queue_view::show` frame over `input`: the focused node id (if
/// any), every AccessKit node, and every painted `Shape::Text`'s
/// `(text, anchor pos)` pair — everything the tests below need, gathered
/// once per frame (mirrors `responsive_dock.rs`'s own `run_frame`/
/// `run_now_playing_frame_with_elisions`).
struct Frame {
    focus: Option<NodeId>,
    nodes: Vec<AccessNode>,
    texts: Vec<(String, Pos2)>,
    shapes: Vec<egui::Shape>,
}

fn run(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    input: RawInput,
) -> Frame {
    let mut output = ctx.run_ui(input, |ui| {
        modplayer_ui::queue_view::show(ui, controller, artwork);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    let focus = update.focus;
    let nodes = update
        .nodes
        .iter()
        .map(|(id, node)| AccessNode {
            id: *id,
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
        .collect();
    let texts = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(t) => Some((t.galley.job.text.clone(), t.pos)),
            _ => None,
        })
        .collect();
    let shapes = output
        .shapes
        .iter()
        .map(|clipped| clipped.shape.clone())
        .collect();
    output.drop_without_applying_deltas();
    Frame {
        focus: Some(focus),
        nodes,
        texts,
        shapes,
    }
}

/// Every non-empty accessible text (`value`+`label`) `queue_view::show`
/// renders this frame.
fn rendered_texts(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
) -> Vec<String> {
    run(ctx, controller, artwork, default_input())
        .nodes
        .into_iter()
        .flat_map(|n| [n.label, n.value])
        .flatten()
        .filter(|text| !text.is_empty())
        .collect()
}

/// The bounds of the `nth` (top-to-bottom) `Role::Button` node whose label
/// equals `label` — rows repeat the same action label once per row, so the
/// row order (current first, per `effective_order`) disambiguates which
/// one a click lands on.
fn nth_button_bounds(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    label: &str,
    nth: usize,
) -> Rect {
    let frame = run(ctx, controller, artwork, default_input());
    let mut matches: Vec<Rect> = frame
        .nodes
        .iter()
        .filter(|n| n.role == Role::Button && n.label.as_deref() == Some(label))
        .filter_map(|n| n.bounds)
        .collect();
    matches.sort_by(|a, b| {
        a.top()
            .partial_cmp(&b.top())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    *matches.get(nth).unwrap_or_else(|| {
        panic!(
            "expected at least {} `{label}` button(s), found {}",
            nth + 1,
            matches.len()
        )
    })
}

/// Press then release the primary button at `pos`, in two separate frames
/// (a single frame is not guaranteed to register `clicked()`).
fn click_at(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    pos: Pos2,
) {
    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    run(ctx, controller, artwork, press);

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    run(ctx, controller, artwork, release);
}

fn tab_input() -> RawInput {
    let mut input = default_input();
    input.events.push(Event::Key {
        key: Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    });
    input
}

fn enter_input() -> RawInput {
    let mut input = default_input();
    input.events.push(Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    });
    input
}

/// Repeatedly presses Tab (one frame each) until the focused node's bounds
/// match `target` (within a pixel), or panics after a generous bound.
/// Bounds, not accessible name, disambiguate which row's action currently
/// has focus — every row repeats the same four action labels (contract
/// Q10).
fn tab_until_bounds(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    target: Rect,
) {
    for _ in 0..60 {
        let frame = run(ctx, controller, artwork, tab_input());
        if let Some(focus) = frame.focus
            && let Some(node) = frame.nodes.iter().find(|n| n.id == focus)
            && let Some(b) = node.bounds
            && (b.min - target.min).length() < 1.0
            && (b.max - target.max).length() < 1.0
        {
            return;
        }
    }
    panic!("Tab never reached the target button ({target:?}) within 60 presses");
}

#[test]
fn rows_render_in_effective_order() {
    let (mut controller, _dir) = ready_controller("rows-order");
    controller.queue_replace(vec![track("a"), track("b"), track("c")]);
    controller.queue_play_next_track(track("x"));

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let texts = rendered_texts(&ctx, &mut controller, &mut artwork);

    // Title/artist are now painted plainly (contract Q1), not through the
    // old `queue-row` fluent template — but each row's `Role::ListItem`
    // accessible name (contract Q11) still carries them.
    for id in ["a", "x", "b", "c"] {
        assert!(
            texts.iter().any(|t| t.contains(id)),
            "expected row {id:?} to be represented in the rendered text, got {texts:?}"
        );
    }
    assert!(
        texts.contains(&tr("queue-badge-play-next")),
        "expected the play-next badge on \"x\", got {texts:?}"
    );
    assert!(
        !texts.contains(&tr("queue-badge-unavailable")),
        "nothing is marked unavailable yet, got {texts:?}"
    );
}

#[test]
fn empty_queue_shows_the_empty_state() {
    let (mut controller, _dir) = ready_controller("empty");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let texts = rendered_texts(&ctx, &mut controller, &mut artwork);
    assert!(texts.contains(&tr("queue-empty")));
}

/// T-Q1 (contracts/queue-row.md): leading → trailing, every row has an
/// artwork placeholder, a title above its artist, and a right-aligned
/// mono position figure.
#[test]
fn row_geometry_is_artwork_then_title_over_artist_then_position() {
    let (mut controller, _dir) = ready_controller("geometry");
    controller.queue_replace(vec![track_full("a", "Song A", "Artist A")]);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let frame = run(&ctx, &mut controller, &mut artwork, default_input());

    // No `artwork_url` -> the initials placeholder (T-Q2's own case, also
    // exercised here since it's this row's only artwork state).
    let initials_text = initials("Song A").text();
    assert!(
        frame.texts.iter().any(|(t, _)| t == &initials_text),
        "expected the initials placeholder {initials_text:?}, got {:?}",
        frame.texts
    );

    let title_pos = frame
        .texts
        .iter()
        .find(|(t, _)| t == "Song A")
        .unwrap_or_else(|| panic!("no painted title, got {:?}", frame.texts))
        .1;
    let artist_pos = frame
        .texts
        .iter()
        .find(|(t, _)| t == "Artist A")
        .unwrap_or_else(|| panic!("no painted artist, got {:?}", frame.texts))
        .1;
    assert!(
        title_pos.y < artist_pos.y,
        "the title must sit above the artist: title {title_pos:?}, artist {artist_pos:?}"
    );

    let position_pos = frame
        .texts
        .iter()
        .find(|(t, _)| t == "1")
        .unwrap_or_else(|| panic!("no painted position figure, got {:?}", frame.texts))
        .1;
    assert!(
        position_pos.x > title_pos.x && position_pos.x > artist_pos.x,
        "the position column must trail the text column: position {position_pos:?}, title {title_pos:?}"
    );
}

/// T-Q2: a row with no `artwork_url` shows the initials placeholder
/// derived from `artwork_name` (album, else title — here, the title,
/// since `track_full` sets no album).
#[test]
fn missing_artwork_shows_the_initials_placeholder() {
    let (mut controller, _dir) = ready_controller("missing-artwork");
    controller.queue_replace(vec![track_full("a", "Dancing Queen", "Abba")]);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let frame = run(&ctx, &mut controller, &mut artwork, default_input());

    let expected = initials("Dancing Queen").text();
    assert!(
        frame.texts.iter().any(|(t, _)| t == &expected),
        "expected the initials placeholder {expected:?}, got {:?}",
        frame.texts
    );
}

/// T-Q3 (FR-017, NFR-7.4): at 960 px width with +40% pseudo-localisation,
/// no action label is elided, and no two interactive rects overlap.
#[test]
fn actions_are_not_elided_or_overlapping_under_pseudo_localization() {
    let (mut controller, _dir) = ready_controller("pseudo-localization");
    controller.queue_replace(vec![track("a"), track("b"), track("c")]);
    controller.queue_play_next_track(track("x"));

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();

    modplayer_core::i18n::with_pseudo_expansion(40, || {
        let action_labels = [
            tr("queue-move-up"),
            tr("queue-move-down"),
            tr("queue-play-next"),
            tr("queue-remove"),
        ];
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(960.0, 640.0))),
            ..Default::default()
        };
        let frame = run(&ctx, &mut controller, &mut artwork, input);

        let elided: Vec<String> = frame
            .shapes
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::Text(t) if t.galley.elided => Some(t.galley.job.text.clone()),
                _ => None,
            })
            .collect();
        for text in &elided {
            assert!(
                !action_labels.contains(text),
                "an action label was elided at 960px + 40% pseudo-localisation: {text:?}"
            );
        }

        let buttons: Vec<Rect> = frame
            .nodes
            .iter()
            .filter(|n| n.role == Role::Button)
            .filter_map(|n| n.bounds)
            .collect();
        for i in 0..buttons.len() {
            for j in (i + 1)..buttons.len() {
                let overlap = buttons[i].intersect(buttons[j]);
                assert!(
                    overlap.width() <= 0.5 || overlap.height() <= 0.5,
                    "interactive rects overlap: {:?} vs {:?}",
                    buttons[i],
                    buttons[j]
                );
            }
        }
    });
}

/// T-Q5/T-Q6 (contract Q5/Q6, FR-013/FR-014): the current row paints a ▶
/// glyph and a `nav_indicator`-stroke line the row's full height — never
/// the 016 selected fill (`roles.accent`) — and no other row does either.
#[test]
fn current_row_is_marked_by_glyph_and_accent_bar_not_fill() {
    let (mut controller, _dir) = ready_controller("current-marker");
    controller.queue_replace(vec![track("a"), track("b")]);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    // Pin the theme so the painted stroke colour can be compared against a
    // known `Roles` constant below (mirrors `shell_navigation.rs`'s own
    // nav-indicator test).
    ctx.set_theme(egui::ThemePreference::from(egui::Theme::Light));
    let mut artwork = ArtworkCache::new();
    let frame = run(&ctx, &mut controller, &mut artwork, default_input());

    let glyph = tr("queue-playing-glyph");
    let glyph_count = frame.texts.iter().filter(|(t, _)| t == &glyph).count();
    assert_eq!(
        glyph_count, 1,
        "exactly one row (the current one) must paint the {glyph:?} glyph, got {} in {:?}",
        glyph_count, frame.texts
    );

    let roles = LIGHT;
    let indicator = theme_controls::nav_indicator(&roles);
    let indicator_lines = frame
        .shapes
        .iter()
        .filter(|shape| {
            matches!(shape, egui::Shape::LineSegment { stroke, .. } if stroke.color == indicator.color)
        })
        .count();
    assert_eq!(
        indicator_lines, 1,
        "exactly one nav-indicator-coloured line must be painted (the current row's)"
    );

    let accent_fill = frame
        .shapes
        .iter()
        .any(|shape| matches!(shape, egui::Shape::Rect(r) if r.fill == roles.accent));
    assert!(
        !accent_fill,
        "the current row must never use the 016 selected (`accent`) fill (contract Q6)"
    );
}

/// T-Q7: no row paints the old "Now playing:" prefix text — the
/// `queue-current` fluent key is gone (contract Q7).
#[test]
fn no_row_paints_the_old_now_playing_prefix() {
    let (mut controller, _dir) = ready_controller("no-prefix");
    controller.queue_replace(vec![track("a"), track("b")]);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let frame = run(&ctx, &mut controller, &mut artwork, default_input());

    assert!(
        frame.texts.iter().all(|(t, _)| t != "Now playing:"),
        "found the old visible \"Now playing:\" prefix: {:?}",
        frame.texts
    );
}

/// T-Q8: with the pointer away from every row, every row's four actions
/// are still painted (quiet, but never hover-revealed) — contract Q8.
#[test]
fn actions_paint_on_every_row_without_hover() {
    let (mut controller, _dir) = ready_controller("always-visible-actions");
    controller.queue_replace(vec![track("a"), track("b")]);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    // `default_input()` carries no pointer position at all this frame.
    let frame = run(&ctx, &mut controller, &mut artwork, default_input());

    for (key, expected_count) in [
        ("queue-move-up", 2),
        ("queue-move-down", 2),
        ("queue-remove", 2),
        // Play next is absent on the current row (contract Q9).
        ("queue-play-next", 1),
    ] {
        let label = tr(key);
        let count = frame
            .nodes
            .iter()
            .filter(|n| n.role == Role::Button && n.label.as_deref() == Some(label.as_str()))
            .count();
        assert_eq!(
            count, expected_count,
            "expected {expected_count} `{key}` button(s) painted without hover, found {count}"
        );
    }
}

/// T-Q9: Remove on the current row advances the controller's queue
/// (unchanged pre-021 behaviour), each action applying to its own row's
/// uid.
#[test]
fn remove_button_on_the_current_row_advances_the_controllers_queue() {
    let (mut controller, _dir) = ready_controller("remove-current");
    controller.queue_replace(vec![track("a"), track("b")]);
    assert_eq!(
        controller
            .queue()
            .current()
            .map(|i| i.track.id.as_str().to_string()),
        Some("spotify:track:a".to_string())
    );

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    // Rows render current-first, so the topmost "Remove" button belongs to
    // "a" (the current item).
    let bounds = nth_button_bounds(&ctx, &mut controller, &mut artwork, &tr("queue-remove"), 0);
    click_at(&ctx, &mut controller, &mut artwork, bounds.center());

    assert_eq!(
        controller
            .queue()
            .current()
            .map(|i| i.track.id.as_str().to_string()),
        Some("spotify:track:b".to_string()),
        "clicking Remove on the current row's button must advance past it"
    );
}

#[test]
fn move_down_button_reorders_the_controllers_queue() {
    let (mut controller, _dir) = ready_controller("move-down");
    controller.queue_replace(vec![track("a"), track("b"), track("c")]);
    let order_before: Vec<_> = controller
        .queue()
        .effective_order()
        .iter()
        .map(|i| i.track.id.as_str().to_string())
        .collect();
    assert_eq!(
        order_before,
        vec!["spotify:track:a", "spotify:track:b", "spotify:track:c"]
    );

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    // Rows render current ("a") first, then "b", then "c" — the second
    // (index 1) "Move down" button belongs to "b".
    let bounds = nth_button_bounds(
        &ctx,
        &mut controller,
        &mut artwork,
        &tr("queue-move-down"),
        1,
    );
    click_at(&ctx, &mut controller, &mut artwork, bounds.center());

    let order_after: Vec<_> = controller
        .queue()
        .effective_order()
        .iter()
        .map(|i| i.track.id.as_str().to_string())
        .collect();
    assert_eq!(
        order_after,
        vec!["spotify:track:a", "spotify:track:c", "spotify:track:b"],
        "\"b\" must move one slot later in the effective order"
    );
}

#[test]
fn shuffle_and_repeat_header_controls_mutate_the_controllers_queue() {
    let (mut controller, _dir) = ready_controller("header");
    controller.queue_replace(vec![track("a"), track("b"), track("c")]);
    assert!(!controller.queue().shuffle_enabled());
    assert_eq!(
        controller.queue().repeat(),
        modplayer_audio_source::Repeat::Off
    );

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let shuffle_bounds =
        nth_button_bounds(&ctx, &mut controller, &mut artwork, &tr("queue-shuffle"), 0);
    click_at(&ctx, &mut controller, &mut artwork, shuffle_bounds.center());
    assert!(
        controller.queue().shuffle_enabled(),
        "clicking the shuffle toggle must turn shuffle on"
    );

    let repeat_bounds = nth_button_bounds(
        &ctx,
        &mut controller,
        &mut artwork,
        &tr("queue-repeat-off"),
        0,
    );
    click_at(&ctx, &mut controller, &mut artwork, repeat_bounds.center());
    assert_eq!(
        controller.queue().repeat(),
        modplayer_audio_source::Repeat::One,
        "clicking the repeat-cycle button must advance Off -> One"
    );
}

/// T-Q10: Tab reaches a row's Remove action, and Enter on the focused
/// button removes that row (egui's generic focused-widget Space/Enter
/// activation — `Context::interact` — needs no bespoke handling here).
#[test]
fn tab_reaches_remove_and_enter_activates_it() {
    let (mut controller, _dir) = ready_controller("tab-enter-remove");
    controller.queue_replace(vec![track("a"), track("b")]);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();

    // The second (index 1) "Remove" button belongs to "b" (rows render
    // current-first).
    let target = nth_button_bounds(&ctx, &mut controller, &mut artwork, &tr("queue-remove"), 1);
    tab_until_bounds(&ctx, &mut controller, &mut artwork, target);

    run(&ctx, &mut controller, &mut artwork, enter_input());

    let order_after: Vec<_> = controller
        .queue()
        .effective_order()
        .iter()
        .map(|i| i.track.id.as_str().to_string())
        .collect();
    assert_eq!(
        order_after,
        vec!["spotify:track:a"],
        "Enter on the focused Remove button must remove \"b\""
    );
}

/// T-Q11 (contract Q11): the current row's `Role::ListItem` label equals
/// the resolved `queue-row-name-current` key; a non-current row's equals
/// `queue-row-name`.
#[test]
fn list_item_label_reflects_current_state() {
    let (mut controller, _dir) = ready_controller("list-item-label");
    controller.queue_replace(vec![
        track_full("a", "Song A", "Artist A"),
        track_full("b", "Song B", "Artist B"),
    ]);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let frame = run(&ctx, &mut controller, &mut artwork, default_input());

    let list_items: Vec<&AccessNode> = frame
        .nodes
        .iter()
        .filter(|n| n.role == Role::ListItem)
        .collect();
    assert_eq!(list_items.len(), 2, "expected one ListItem per row");

    // Fluent wraps each interpolated segment in bidi-isolate marks, so the
    // expected value is resolved through `tr_args` itself rather than a
    // hand-written literal (mirrors `accessibility.rs`'s own
    // `marker-glyph` comment).
    let expected_current = tr_args(
        "queue-row-name-current",
        &[
            ("title", "Song A".to_string()),
            ("artist", "Artist A".to_string()),
            ("has_artist", "yes".to_string()),
        ],
    );
    let expected_other = tr_args(
        "queue-row-name",
        &[
            ("title", "Song B".to_string()),
            ("artist", "Artist B".to_string()),
            ("has_artist", "yes".to_string()),
        ],
    );

    let current_name = list_items
        .iter()
        .find_map(|n| n.accessible_name())
        .filter(|name| name.starts_with("Now playing"));
    assert_eq!(
        current_name,
        Some(expected_current.as_str()),
        "the current row's ListItem label must resolve queue-row-name-current"
    );
    assert!(
        list_items
            .iter()
            .any(|n| n.accessible_name() == Some(expected_other.as_str())),
        "the non-current row's ListItem label must resolve queue-row-name: {:?}",
        list_items
            .iter()
            .map(|n| n.accessible_name())
            .collect::<Vec<_>>()
    );
}

/// T-Q12: a single-item queue where the one row is current renders the
/// mark correctly (no off-by-one against a second row that doesn't
/// exist).
#[test]
fn single_item_current_only_queue_renders_the_mark() {
    let (mut controller, _dir) = ready_controller("single-item");
    controller.queue_replace(vec![track_full("a", "Solo", "Only Artist")]);

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let frame = run(&ctx, &mut controller, &mut artwork, default_input());

    assert!(
        frame
            .texts
            .iter()
            .any(|(t, _)| t == &tr("queue-playing-glyph")),
        "the lone row is current and must paint the glyph"
    );
    let list_item = frame
        .nodes
        .iter()
        .find(|n| n.role == Role::ListItem)
        .unwrap_or_else(|| panic!("expected one ListItem, got {:?}", frame.nodes));
    let expected = tr_args(
        "queue-row-name-current",
        &[
            ("title", "Solo".to_string()),
            ("artist", "Only Artist".to_string()),
            ("has_artist", "yes".to_string()),
        ],
    );
    assert_eq!(list_item.accessible_name(), Some(expected.as_str()));
    // Play next is absent on the current (and here, only) row.
    assert!(
        frame
            .nodes
            .iter()
            .filter(|n| n.role == Role::Button
                && n.label.as_deref() == Some(tr("queue-play-next").as_str()))
            .count()
            == 0,
        "a single current row must not show Play next"
    );
}

/// C8 (016-list-row-and-panel-components, contracts/panel-card.md): the
/// Queue panel gains a header reading "Queue", through the shared card.
#[test]
fn panel_body_draws_no_heading_of_its_own() {
    // 021-transport-bar-and-panel-layout (contract C1, C4): the card's
    // chrome — including its `Role::Heading` — moved out of `queue_view::
    // show` (now body-only) into `now_playing::show`, which owns it via
    // `widgets::controls::collapsible_panel_card` (covered by that
    // widget's own tests plus `tests/accessibility.rs`'s
    // `queue_panel_header_exposes_its_exact_accessible_name`).
    let (mut controller, _dir) = ready_controller("header-title");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();

    let frame = run(&ctx, &mut controller, &mut artwork, default_input());

    assert!(
        !frame.nodes.iter().any(|n| n.role == Role::Heading),
        "queue_view::show is body-only now; the heading belongs to the caller's card"
    );
    assert_eq!(tr("queue-panel-title"), "Queue");
}
