// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `search_view::apply_row_action` — the acting-list rule wired end to end
//! through a real `PlaybackController` (contracts/library-and-search-
//! core.md §3, FR-005/006/007): a search Tracks-group row's Play now uses
//! the whole loaded list cursored at the clicked track, Play next/Add to
//! queue act on just that one track, and the three placeholder actions
//! raise `coming-soon` and record no new `SourceCommand`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui::accesskit::Role;
use egui::epaint::ClippedShape;
use egui::{Color32, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{AlbumId, AlbumRef, Availability, SourceCommand, TrackId, TrackRef};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{PlaybackController, Severity};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::artwork::ArtworkCache;
use modplayer_ui::rows::{self, RowAction, RowEntity, RowEvent, list_row};
use modplayer_ui::search_view::apply_row_action;
use modplayer_ui::theme::controls as theme_controls;
use modplayer_ui::theme::tokens;
use modplayer_ui::widgets::skeleton::{ROW_HEIGHT, WIDE_ROW_HEIGHT};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-rows-{label}-{}-{unique}",
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

fn track(id: &str) -> TrackRef {
    TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap(),
        format!("Title {id}"),
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    )
}

fn active_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    modplayer_audio_source_synthetic::ScriptedHostHandle,
    TempDir,
) {
    let dir = TempDir::new(label);
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![FakeDevice {
        id: DeviceId::new("dev-1").unwrap(),
        name: "Speakers".to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        is_default: true,
    }];
    let mut controller = PlaybackController::new(FakeBackend::new(devices), host, store);
    controller.launch();
    controller.confirm_device(DeviceId::new("dev-1").unwrap(), BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.tick();
    (controller, handle, dir)
}

#[test]
fn play_now_on_a_track_row_uses_the_whole_loaded_list_cursored_at_it() {
    let (mut controller, _handle, _dir) = active_controller("play-now");
    let loaded = vec![track("a"), track("b"), track("c")];

    apply_row_action(
        &mut controller,
        &RowEntity::Track(loaded[1].clone()),
        &loaded,
        RowAction::PlayNow,
    );

    assert_eq!(
        controller.queue().current().map(|i| i.track.id.as_str()),
        Some("spotify:track:b"),
        "playback must start on the clicked track"
    );
    let order: Vec<_> = controller
        .queue()
        .effective_order()
        .iter()
        .map(|i| i.track.id.as_str().to_string())
        .collect();
    assert_eq!(
        order,
        vec!["spotify:track:b", "spotify:track:c"],
        "the acting list is the whole loaded list, cursored at the clicked track"
    );
}

#[test]
fn play_next_on_a_track_row_queues_only_that_track() {
    let (mut controller, _handle, _dir) = active_controller("play-next");
    let loaded = vec![track("a"), track("b"), track("c")];
    controller.queue_replace(vec![track("z")]);

    apply_row_action(
        &mut controller,
        &RowEntity::Track(loaded[0].clone()),
        &loaded,
        RowAction::PlayNext,
    );

    let play_next: Vec<_> = controller
        .queue()
        .play_next_items()
        .map(|i| i.track.id.as_str().to_string())
        .collect();
    assert_eq!(
        play_next,
        vec!["spotify:track:a"],
        "Play next on a track row queues only that one track, not the whole loaded list"
    );
}

#[test]
fn add_to_queue_on_a_track_row_appends_only_that_track() {
    let (mut controller, _handle, _dir) = active_controller("add-to-queue");
    let loaded = vec![track("a"), track("b")];
    controller.queue_replace(vec![track("z")]);

    apply_row_action(
        &mut controller,
        &RowEntity::Track(loaded[1].clone()),
        &loaded,
        RowAction::AddToQueue,
    );

    let order: Vec<_> = controller
        .queue()
        .effective_order()
        .iter()
        .map(|i| i.track.id.as_str().to_string())
        .collect();
    assert_eq!(order, vec!["spotify:track:z", "spotify:track:b"]);
}

#[test]
fn placeholders_raise_coming_soon_and_record_no_new_command() {
    let (mut controller, handle, _dir) = active_controller("placeholders");
    let loaded = vec![track("a")];

    for action in [
        RowAction::AddToPlaylist,
        RowAction::SaveToLibrary,
        RowAction::PinForOffline,
    ] {
        let commands_before = handle.record_commands().len();

        apply_row_action(
            &mut controller,
            &RowEntity::Track(loaded[0].clone()),
            &loaded,
            action,
        );

        let notification = controller
            .notifications()
            .visible()
            .find(|n| n.message_key == "coming-soon");
        assert!(
            notification.is_some(),
            "{action:?} must raise the coming-soon notification"
        );
        assert_eq!(notification.map(|n| n.severity), Some(Severity::Info));

        let commands_after = handle.record_commands();
        assert_eq!(
            commands_after.len(),
            commands_before,
            "{action:?} must record no new SourceCommand: {commands_after:?}"
        );
        assert!(
            !commands_after
                .iter()
                .any(|c| matches!(c, SourceCommand::LoadProgram(_))),
            "{action:?} must never load a program"
        );

        controller.notifications_mut().dismiss_by_key("coming-soon");
    }
}

#[test]
fn all_six_actions_are_represented_in_the_fixed_menu_order() {
    assert_eq!(
        RowAction::ORDER,
        [
            RowAction::PlayNow,
            RowAction::PlayNext,
            RowAction::AddToQueue,
            RowAction::AddToPlaylist,
            RowAction::SaveToLibrary,
            RowAction::PinForOffline,
        ]
    );
}

// -- list_row selection/paint (contracts/list-row.md S1/S7/S12, A1-A3/A5) --

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    }
}

fn track_entity(id: &str) -> RowEntity {
    RowEntity::Track(track(id))
}

/// A Track row exercising every text run contract A3/A4 name: title,
/// secondary line, duration, an explicit "E" badge, and an availability
/// reason (region-locked) — the same widest case `design_token_contrast.rs`
/// samples for A4.
fn widest_track_entity(id: &str) -> RowEntity {
    let mut track = TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap(),
        format!("Title {id}"),
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::UnavailableRegion,
    );
    track.explicit = true;
    RowEntity::Track(track)
}

/// The first accesskit node with `role`/`name`'s bounds, as an `egui::Rect`
/// (mirrors `library_view.rs` tests' own tutorial-button-bounds pattern).
fn find_node_rect(mut output: egui::FullOutput, role: Role, name: &str) -> Rect {
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();
    let bounds = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == role && n.label() == Some(name))
        .and_then(|(_, n)| n.bounds())
        .unwrap_or_else(|| panic!("expected a {role:?} node named {name:?}"));
    Rect::from_min_max(
        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
    )
}

/// The row's own background fill (contract A1/A2, data-model.md §7): the
/// `Shape::Rect` `list_row` paints into its reserved shape index — a
/// zero-stroke rect at (approximately) `row_rect`, distinct from a focus
/// ring (`interaction_states.rs`'s `ring_shape_rect`, which instead matches
/// on stroke width).
fn row_fill(shapes: &[ClippedShape], row_rect: Rect) -> Option<Color32> {
    shapes.iter().find_map(|clipped| match &clipped.shape {
        Shape::Rect(r)
            if r.stroke.width == 0.0
                && (r.rect.min - row_rect.min).length() < 0.5
                && (r.rect.max - row_rect.max).length() < 0.5 =>
        {
            Some(r.fill)
        }
        _ => None,
    })
}

/// Every colour any text run in `shapes` resolved to (data-model.md §7):
/// `RichText::color`/`.weak()` both bake into the `Galley`'s own
/// `LayoutJob` at layout time (egui's `WidgetText::get_text_color`) — a
/// `.weak()` run resolves to `visuals.weak_text_color()`, an explicit
/// `.color(c)` run resolves to exactly `c`.
fn text_colors(shapes: &[ClippedShape]) -> HashSet<Color32> {
    shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Text(t) => Some(t.galley.job.sections.iter().map(|s| s.format.color)),
            _ => None,
        })
        .flatten()
        .collect()
}

/// **S1** (FR-006): a synthetic primary click yields exactly
/// `Some(RowEvent::Select)` — no `Action`, no `Open`.
#[test]
fn primary_click_reports_select_and_nothing_else() {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut cache = ArtworkCache::new();
    let entity = track_entity("row-select");
    let name = rows::accessible_name(&entity);

    let discover = ctx.run_ui(default_input(), |ui| {
        let _ = list_row(ui, &mut cache, &entity, false);
    });
    let rect = find_node_rect(discover, Role::ListItem, &name);
    let center = rect.center();

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: center,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    ctx.run_ui(press, |ui| {
        let _ = list_row(ui, &mut cache, &entity, false);
    })
    .drop_without_applying_deltas();

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: center,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    let mut event = None;
    let output = ctx.run_ui(release, |ui| {
        event = list_row(ui, &mut cache, &entity, false);
    });
    output.drop_without_applying_deltas();

    assert_eq!(
        event,
        Some(RowEvent::Select),
        "a plain primary click must report exactly Select"
    );
}

/// **S7** (FR-029, research R3): a synthetic click at the "…" button's own
/// centre never reports `Select` — pins egui's "in tie, pick last =
/// topmost" hit-test rule (the button is created after the row's own
/// `interact` call, so it wins the hit test at that exact point).
#[test]
fn clicking_the_actions_button_never_reports_select() {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut cache = ArtworkCache::new();
    let entity = track_entity("row-menu-click");
    let name = rows::accessible_name(&entity);
    let button_label = modplayer_core::tr_args("row-actions", &[("name", name.clone())]);

    let discover = ctx.run_ui(default_input(), |ui| {
        let _ = list_row(ui, &mut cache, &entity, false);
    });
    let rect = find_node_rect(discover, Role::Button, &button_label);
    let center = rect.center();

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: center,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    ctx.run_ui(press, |ui| {
        let _ = list_row(ui, &mut cache, &entity, false);
    })
    .drop_without_applying_deltas();

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: center,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    let mut event = None;
    let output = ctx.run_ui(release, |ui| {
        event = list_row(ui, &mut cache, &entity, false);
    });
    output.drop_without_applying_deltas();

    assert_ne!(
        event,
        Some(RowEvent::Select),
        "a click on the \"…\" button's own centre must never select the row"
    );
}

/// **S12** (FR-030, Clarification 5): a click then Enter activates the row
/// (the click also took keyboard focus); once focus moves elsewhere, the
/// same Enter key no longer does — Enter follows focus, not the earlier
/// selection.
#[test]
fn click_then_enter_activates_but_not_once_focus_moves_away() {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut cache = ArtworkCache::new();
    let entity = track_entity("row-focus");
    let name = rows::accessible_name(&entity);

    let discover = ctx.run_ui(default_input(), |ui| {
        let _ = list_row(ui, &mut cache, &entity, false);
    });
    let rect = find_node_rect(discover, Role::ListItem, &name);
    let center = rect.center();

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: center,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    ctx.run_ui(press, |ui| {
        let _ = list_row(ui, &mut cache, &entity, false);
    })
    .drop_without_applying_deltas();

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: center,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    let mut select_event = None;
    ctx.run_ui(release, |ui| {
        select_event = list_row(ui, &mut cache, &entity, false);
    })
    .drop_without_applying_deltas();
    assert_eq!(select_event, Some(RowEvent::Select));

    let mut enter_input = default_input();
    enter_input.events.push(Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    let mut enter_event = None;
    ctx.run_ui(enter_input.clone(), |ui| {
        enter_event = list_row(ui, &mut cache, &entity, false);
    })
    .drop_without_applying_deltas();
    assert_eq!(
        enter_event,
        Some(RowEvent::Action(RowAction::PlayNow)),
        "Enter right after the click, while the row still holds focus, must activate it"
    );

    // Move focus to an unrelated id — no widget needs to exist for this,
    // `Memory::request_focus` alone is enough (mirrors
    // `interaction_states.rs`'s `no_ring_without_a_visible_focus`).
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("rows-test-focus-thief")));

    let mut enter_event_after = None;
    ctx.run_ui(enter_input, |ui| {
        enter_event_after = list_row(ui, &mut cache, &entity, false);
    })
    .drop_without_applying_deltas();
    assert_ne!(
        enter_event_after,
        Some(RowEvent::Action(RowAction::PlayNow)),
        "once focus moved away, the same Enter must not activate the row"
    );
}

/// **A1/A2** (FR-008): a selected row's fill is plain `accent` at rest;
/// hover/pressed blend `hover_fill`/`pressed_fill` *over* that base — both
/// themes, both distinct from plain `accent`.
#[test]
fn selected_row_fills_accent_and_hover_pressed_blend_over_it() {
    for dark in [false, true] {
        let ctx = Context::default();
        ctx.set_theme(if dark {
            egui::ThemePreference::Dark
        } else {
            egui::ThemePreference::Light
        });
        ctx.enable_accesskit();
        let roles = *tokens::for_dark_mode(dark);
        let mut cache = ArtworkCache::new();
        let entity = track_entity(if dark {
            "row-fill-dark"
        } else {
            "row-fill-light"
        });
        let name = rows::accessible_name(&entity);

        let discover = ctx.run_ui(default_input(), |ui| {
            let _ = list_row(ui, &mut cache, &entity, false);
        });
        let rect = find_node_rect(discover, Role::ListItem, &name);

        let resting = ctx.run_ui(default_input(), |ui| {
            let _ = list_row(ui, &mut cache, &entity, true);
        });
        let resting_fill =
            row_fill(&resting.shapes, rect).expect("a selected row must always paint a fill");
        resting.drop_without_applying_deltas();
        assert_eq!(
            resting_fill, roles.accent,
            "dark={dark}: selected, unhovered/unpressed fill must be plain accent"
        );

        let hover_input = RawInput {
            events: vec![Event::PointerMoved(rect.center())],
            ..default_input()
        };
        let hovered = ctx.run_ui(hover_input, |ui| {
            let _ = list_row(ui, &mut cache, &entity, true);
        });
        let hovered_fill =
            row_fill(&hovered.shapes, rect).expect("a hovered selected row must paint a fill");
        hovered.drop_without_applying_deltas();
        let expected_hover = roles.accent.blend(theme_controls::hover_fill(&roles));
        assert_eq!(
            hovered_fill, expected_hover,
            "dark={dark}: hover must blend over accent"
        );
        assert_ne!(
            hovered_fill, roles.accent,
            "dark={dark}: hover fill must differ from plain accent"
        );

        let press_input = RawInput {
            events: vec![
                Event::PointerMoved(rect.center()),
                Event::PointerButton {
                    pos: rect.center(),
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            ..default_input()
        };
        let pressed = ctx.run_ui(press_input, |ui| {
            let _ = list_row(ui, &mut cache, &entity, true);
        });
        let pressed_fill =
            row_fill(&pressed.shapes, rect).expect("a pressed selected row must paint a fill");
        pressed.drop_without_applying_deltas();
        let expected_pressed = roles.accent.blend(theme_controls::pressed_fill(&roles));
        assert_eq!(
            pressed_fill, expected_pressed,
            "dark={dark}: pressed must blend over accent"
        );
        assert_ne!(
            pressed_fill, roles.accent,
            "dark={dark}: pressed fill must differ from plain accent"
        );
        assert_ne!(
            pressed_fill, hovered_fill,
            "dark={dark}: pressed and hover fills must be distinct (FR-011's ordering)"
        );
    }
}

/// **A3** (FR-031): every text run on a selected row — title, secondary
/// line, duration, "E" badge, availability reason — resolves to
/// `text_on_accent`, never the unselected `.weak()` colour, both themes.
#[test]
fn selected_row_every_text_run_is_text_on_accent_never_weak() {
    for dark in [false, true] {
        let ctx = Context::default();
        let theme = if dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        ctx.set_theme(egui::ThemePreference::from(theme));
        let weak_text_color = ctx.style_of(theme).visuals.weak_text_color();
        let roles = *tokens::for_dark_mode(dark);
        let mut cache = ArtworkCache::new();
        let entity = widest_track_entity(if dark { "row-a3-dark" } else { "row-a3-light" });

        let unselected = ctx.run_ui(default_input(), |ui| {
            let _ = list_row(ui, &mut cache, &entity, false);
        });
        let unselected_colors = text_colors(&unselected.shapes);
        unselected.drop_without_applying_deltas();
        assert!(
            unselected_colors.contains(&weak_text_color),
            "dark={dark}: unselected must still carry the weak run colour: {unselected_colors:?}"
        );
        assert!(
            !unselected_colors.contains(&roles.text_on_accent),
            "dark={dark}: unselected must never use text_on_accent: {unselected_colors:?}"
        );

        let selected = ctx.run_ui(default_input(), |ui| {
            let _ = list_row(ui, &mut cache, &entity, true);
        });
        let selected_colors = text_colors(&selected.shapes);
        selected.drop_without_applying_deltas();
        assert!(
            !selected_colors.contains(&weak_text_color),
            "dark={dark}: no selected-row text run may stay `.weak()`: {selected_colors:?}"
        );
        assert!(
            selected_colors.contains(&roles.text_on_accent),
            "dark={dark}: selected-row text must include text_on_accent: {selected_colors:?}"
        );
    }
}

/// **A5** (FR-011, FR-025): a selected row's accesskit node gets
/// `Toggled`... no — `selected` state via `set_selected(true)`, while
/// `Role::ListItem` and [`rows::accessible_name`] stay exactly as before.
#[test]
fn selected_row_exposes_accesskit_selected_state() {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut cache = ArtworkCache::new();
    let entity = track_entity("row-a5");
    let name = rows::accessible_name(&entity);

    let mut output = ctx.run_ui(default_input(), |ui| {
        let _ = list_row(ui, &mut cache, &entity, true);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    let node = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::ListItem && n.label() == Some(name.as_str()))
        .map(|(_, n)| n)
        .unwrap_or_else(|| panic!("expected a ListItem node named {name:?}: {update:?}"));
    assert_eq!(
        node.is_selected(),
        Some(true),
        "a selected row's node must report selected"
    );
}

// -- Three-column grid (contract L5b/L6/L7, FR-001-005) ---------------------

/// **L5b** (FR-002): no accesskit node whose label is the secondary detail
/// line ends with the row's formatted duration — the duration moved to its
/// own trailing column.
#[test]
fn l5b_no_secondary_line_node_ends_with_the_duration() {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut cache = ArtworkCache::new();
    let mut t = track("row-l5b");
    t.duration_ms = 65_000; // "1:05"
    t.album = Some("Album X".to_string());
    let entity = RowEntity::Track(t.clone());

    let mut output = ctx.run_ui(default_input(), |ui| {
        let _ = list_row(ui, &mut cache, &entity, false);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    let duration = rows::format_duration(t.duration_ms);
    let artists = t.artists.join(", ");
    let secondary_line_labels: Vec<&str> = update
        .nodes
        .iter()
        .filter_map(|(_, n)| n.label())
        .filter(|label| label.contains(&artists))
        .collect();
    assert!(
        !secondary_line_labels.is_empty(),
        "expected a secondary-line node containing the artists: {update:?}"
    );
    for label in secondary_line_labels {
        assert!(
            !label.ends_with(&duration),
            "secondary line {label:?} must not end with the duration {duration:?}"
        );
    }
}

/// **L6** (FR-004): a title far longer than any plausible column still
/// truncates — the row's own height stays exactly `ROW_HEIGHT`/
/// `WIDE_ROW_HEIGHT`, never growing to wrap it.
#[test]
fn l6_a_long_title_truncates_without_growing_the_row_height() {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut cache = ArtworkCache::new();
    let long_title = "X".repeat(500);

    let mut t = track("row-l6");
    t.title = long_title.clone();
    let track_entity = RowEntity::Track(t);
    let name = rows::accessible_name(&track_entity);
    let output = ctx.run_ui(default_input(), |ui| {
        let _ = list_row(ui, &mut cache, &track_entity, false);
    });
    let rect = find_node_rect(output, Role::ListItem, &name);
    assert!(
        (rect.height() - ROW_HEIGHT).abs() < 0.5,
        "a long title must not grow a track row past ROW_HEIGHT: {}",
        rect.height()
    );

    let long_album = AlbumRef {
        id: AlbumId::new("spotify:album:row-l6").unwrap(),
        name: long_title,
        artists: vec!["Artist".to_string()],
        artwork_url: None,
        release_date: None,
        track_count: 1,
    };
    let album_entity = RowEntity::Album(long_album);
    let album_name = rows::accessible_name(&album_entity);
    let output = ctx.run_ui(default_input(), |ui| {
        let _ = list_row(ui, &mut cache, &album_entity, false);
    });
    let rect = find_node_rect(output, Role::ListItem, &album_name);
    assert!(
        (rect.height() - WIDE_ROW_HEIGHT).abs() < 0.5,
        "a long title must not grow a wide row past WIDE_ROW_HEIGHT: {}",
        rect.height()
    );
}

/// **L7** (FR-005): the full-row hover fill survives the narrower text
/// column — the "…" button's rect stays inside the row's own rect.
#[test]
fn l7_actions_button_rect_is_contained_by_the_row_rect() {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut cache = ArtworkCache::new();
    let entity = track_entity("row-l7");
    let name = rows::accessible_name(&entity);
    let button_label = modplayer_core::tr_args("row-actions", &[("name", name.clone())]);

    let mut output = ctx.run_ui(default_input(), |ui| {
        let _ = list_row(ui, &mut cache, &entity, false);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    let bounds_of = |role: Role, label: &str| -> Rect {
        let b = update
            .nodes
            .iter()
            .find(|(_, n)| n.role() == role && n.label() == Some(label))
            .and_then(|(_, n)| n.bounds())
            .unwrap_or_else(|| panic!("expected a {role:?} node named {label:?}"));
        Rect::from_min_max(
            Pos2::new(b.x0 as f32, b.y0 as f32),
            Pos2::new(b.x1 as f32, b.y1 as f32),
        )
    };

    let row_rect = bounds_of(Role::ListItem, &name);
    let button_rect = bounds_of(Role::Button, &button_label);
    assert!(
        row_rect.contains_rect(button_rect),
        "the \"…\" button rect {button_rect:?} must be contained by the row rect {row_rect:?}"
    );
}

// ---------------------------------------------------------------------
// 025 US3 — row actions stay with their row, quietly, by keyboard
// (contracts/row-actions-menu.md RM1-RM8)
// ---------------------------------------------------------------------

fn row_input(width: f32, mut events: Vec<Event>) -> RawInput {
    // egui 0.36 takes `InputState::modifiers` from `ModifiersChanged`.
    let modifiers = events
        .iter()
        .find_map(|e| match e {
            Event::Key { modifiers, .. } => Some(*modifiers),
            _ => None,
        })
        .unwrap_or(Modifiers::NONE);
    events.insert(0, Event::ModifiersChanged(modifiers));
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 600.0))),
        events,
        ..Default::default()
    }
}

fn key_event(key: egui::Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn click_events(pos: Pos2) -> [Vec<Event>; 2] {
    let at = |pressed| {
        vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }]
    };
    [at(true), at(false)]
}

/// Labelled rects of every accesskit node in `output`, plus the label of
/// the node holding focus.
struct Nodes {
    nodes: Vec<(Role, String, Rect)>,
    focused: Option<(Role, String)>,
}

fn read_nodes(mut output: egui::FullOutput) -> Nodes {
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();
    let nodes = update
        .nodes
        .iter()
        .filter_map(|(_, n)| {
            let b = n.bounds()?;
            Some((
                n.role(),
                n.label().unwrap_or_default().to_string(),
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                ),
            ))
        })
        .collect();
    let focused = update
        .nodes
        .iter()
        .find(|(id, _)| *id == update.focus)
        .map(|(_, n)| (n.role(), n.label().unwrap_or_default().to_string()));
    Nodes { nodes, focused }
}

impl Nodes {
    fn rect(&self, role: Role, label: &str) -> Rect {
        self.nodes
            .iter()
            .find(|(r, l, _)| *r == role && l == label)
            .map(|(_, _, rect)| *rect)
            .unwrap_or_else(|| panic!("expected a {role:?} node named {label:?}"))
    }

    fn menu_items(&self) -> Vec<&(Role, String, Rect)> {
        let mut items: Vec<_> = self
            .nodes
            .iter()
            .filter(|(r, _, _)| *r == Role::MenuItem)
            .collect();
        // Accesskit node order is not layout order: sort top to bottom.
        items.sort_by(|a, b| a.2.min.y.total_cmp(&b.2.min.y));
        items
    }
}

struct RowHarness {
    ctx: Context,
    cache: ArtworkCache,
    entity: RowEntity,
    name: String,
    opener: String,
    width: f32,
}

impl RowHarness {
    fn new(entity: RowEntity, width: f32) -> Self {
        let ctx = Context::default();
        ctx.enable_accesskit();
        let name = rows::accessible_name(&entity);
        let opener = modplayer_core::tr_args("row-actions", &[("name", name.clone())]);
        Self {
            ctx,
            cache: ArtworkCache::new(),
            entity,
            name,
            opener,
            width,
        }
    }

    /// Run one frame with `events`, then one empty settling frame (egui
    /// popups size themselves a frame after opening). Returns the row event
    /// of the first frame and the accesskit nodes of the settled one.
    fn frame(&mut self, events: Vec<Event>) -> (Option<RowEvent>, Nodes) {
        let mut event = None;
        self.ctx
            .run_ui(row_input(self.width, events), |ui| {
                event = list_row(ui, &mut self.cache, &self.entity, false);
            })
            .drop_without_applying_deltas();
        let output = self.ctx.run_ui(row_input(self.width, vec![]), |ui| {
            let _ = list_row(ui, &mut self.cache, &self.entity, false);
        });
        (event, read_nodes(output))
    }

    /// Click the row body (selects + focuses the row).
    fn focus_row(&mut self) {
        let (_, nodes) = self.frame(vec![]);
        let rect = nodes.rect(Role::ListItem, &self.name);
        let [down, up] = click_events(rect.center());
        self.frame(down);
        self.frame(up);
    }

    /// Focus the row and open its menu with Shift+F10.
    fn open_via_shift_f10(&mut self) -> Nodes {
        self.focus_row();
        self.frame(vec![key_event(egui::Key::F10, Modifiers::SHIFT)])
            .1
    }

    fn focused_item_index(nodes: &Nodes) -> Option<usize> {
        let (role, label) = nodes.focused.clone()?;
        if role != Role::MenuItem {
            return None;
        }
        nodes.menu_items().iter().position(|(_, l, _)| *l == label)
    }
}

fn other_kinds() -> Vec<RowEntity> {
    use modplayer_audio_source::{ArtistId, ArtistRef, PlaylistId, PlaylistRef};
    vec![
        widest_track_entity("rm2-track"),
        RowEntity::Artist(ArtistRef {
            id: ArtistId::new("spotify:artist:rm2").unwrap(),
            name: "An Artist With A Very Long Name Indeed, Over And Over Again".into(),
            artwork_url: None,
        }),
        RowEntity::Playlist(PlaylistRef {
            id: PlaylistId::new("spotify:playlist:rm2").unwrap(),
            name: "A Playlist With A Very Long Name Indeed, Over And Over Again".into(),
            owner_name: "Somebody With A Long Owner Name".into(),
            editable: true,
            artwork_url: None,
            track_count: 12,
            revision: None,
        }),
    ]
}

/// **RM1** (FR-009): at rest the opener paints no fill and no outline.
#[test]
fn rm1_opener_is_quiet_at_rest_and_always_visible() {
    let mut h = RowHarness::new(track_entity("rm1"), 800.0);
    h.frame(vec![]);
    let mut output = h.ctx.run_ui(row_input(800.0, vec![]), |ui| {
        let _ = list_row(ui, &mut h.cache, &h.entity, false);
    });
    let update = output.platform_output.accesskit_update.take().unwrap();
    let opener_rect = read_opener_rect(&update, &h.opener);
    let visible_fill = output.shapes.iter().any(|s| match &s.shape {
        Shape::Rect(r) => {
            r.rect.expand(0.5).contains_rect(opener_rect)
                && opener_rect.expand(0.5).contains_rect(r.rect)
                && (r.fill != Color32::TRANSPARENT || r.stroke.width > 0.0)
        }
        _ => false,
    });
    output.drop_without_applying_deltas();
    assert!(
        !visible_fill,
        "Quiet opener must have no resting fill/outline"
    );
    assert!(opener_rect.width() > 0.0);
}

fn read_opener_rect(update: &egui::accesskit::TreeUpdate, label: &str) -> Rect {
    let b = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::Button && n.label() == Some(label))
        .and_then(|(_, n)| n.bounds())
        .expect("opener node");
    Rect::from_min_max(
        Pos2::new(b.x0 as f32, b.y0 as f32),
        Pos2::new(b.x1 as f32, b.y1 as f32),
    )
}

/// **RM2** (FR-007, SC-003): the opener sits inside its row and inside the
/// viewport at every width and text scale, for every row kind.
#[test]
fn rm2_opener_stays_inside_row_and_viewport_at_every_width_and_scale() {
    let mut entities = vec![track_entity("rm2-plain")];
    entities.extend(other_kinds());
    for entity in entities {
        for width in [400.0_f32, 480.0, 560.0, 960.0 - 72.0, 1400.0] {
            for scale in [1.0_f32, 2.0] {
                let mut h = RowHarness::new(entity.clone(), width);
                h.ctx.set_zoom_factor(1.0);
                h.ctx.all_styles_mut(|s| {
                    for font in s.text_styles.values_mut() {
                        font.size *= scale;
                    }
                });
                h.frame(vec![]);
                let (_, nodes) = h.frame(vec![]);
                let row = nodes.rect(Role::ListItem, &h.name);
                let opener = nodes.rect(Role::Button, &h.opener);
                let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 600.0));
                assert!(
                    row.expand(0.5).contains_rect(opener),
                    "{} @ {width}px x{scale}: opener {opener:?} outside row {row:?}",
                    h.name
                );
                assert!(
                    viewport.expand(0.5).contains_rect(opener),
                    "{} @ {width}px x{scale}: opener {opener:?} outside viewport",
                    h.name
                );
            }
        }
    }
}

/// **RM3** (FR-007): row height does not depend on width.
#[test]
fn rm3_row_height_is_width_independent() {
    for entity in other_kinds() {
        let heights: Vec<f32> = [400.0_f32, 560.0, 1400.0]
            .iter()
            .map(|w| {
                let mut h = RowHarness::new(entity.clone(), *w);
                h.frame(vec![]);
                let (_, nodes) = h.frame(vec![]);
                nodes.rect(Role::ListItem, &h.name).height()
            })
            .collect();
        assert!(
            heights.iter().all(|h| (h - heights[0]).abs() < 0.5),
            "{heights:?}"
        );
    }
}

/// **RM4** (FR-007) + **RM5**: Shift+F10 on a focused row opens the menu
/// anchored to that row's opener.
#[test]
fn rm4_rm5_shift_f10_opens_menu_anchored_to_the_row() {
    let mut h = RowHarness::new(track_entity("rm4"), 800.0);
    let nodes = h.open_via_shift_f10();
    let items = nodes.menu_items();
    assert_eq!(items.len(), 6, "Shift+F10 must open the six-item menu");
    let row = nodes.rect(Role::ListItem, &h.name);
    let opener = nodes.rect(Role::Button, &h.opener);
    let first = items[0].2;
    assert!(
        (first.min.y - row.min.y).abs() <= row.height() + 1.0,
        "menu {first:?} must sit next to its row {row:?}"
    );
    assert!(
        (first.min.x - opener.max.x).abs() <= opener.width() + 200.0
            && first.min.x >= opener.min.x - 200.0,
        "menu {first:?} must be adjacent to opener {opener:?}"
    );
}

/// **RM5**: secondary click and Enter on the focused opener open the menu.
#[test]
fn rm5_secondary_click_and_opener_enter_open_the_menu() {
    let mut h = RowHarness::new(track_entity("rm5"), 800.0);
    let (_, nodes) = h.frame(vec![]);
    let pos = nodes.rect(Role::ListItem, &h.name).center();
    let secondary = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Secondary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    h.frame(vec![secondary(true)]);
    let nodes = h.frame(vec![secondary(false)]).1;
    assert_eq!(
        nodes.menu_items().len(),
        6,
        "secondary click opens the menu"
    );

    // Enter on a focused opener: Tab from the row reaches it.
    let mut h = RowHarness::new(track_entity("rm5b"), 800.0);
    h.focus_row();
    let (_, nodes) = h.frame(vec![key_event(egui::Key::Tab, Modifiers::NONE)]);
    assert_eq!(nodes.focused, Some((Role::Button, h.opener.clone())));
    let nodes = h
        .frame(vec![key_event(egui::Key::Enter, Modifiers::NONE)])
        .1;
    assert_eq!(nodes.menu_items().len(), 6, "Enter on the opener opens it");
}

/// **RM6**: ↓↑ wrap, Home/End jump, Enter activates, Escape closes.
#[test]
fn rm6_arrow_wrap_home_end_activate_and_escape() {
    let mut h = RowHarness::new(track_entity("rm6"), 800.0);
    let nodes = h.open_via_shift_f10();
    assert_eq!(RowHarness::focused_item_index(&nodes), Some(0));

    let press = |h: &mut RowHarness, key| h.frame(vec![key_event(key, Modifiers::NONE)]);
    let (_, n) = press(&mut h, egui::Key::ArrowUp);
    assert_eq!(RowHarness::focused_item_index(&n), Some(5), "0 wraps to 5");
    let (_, n) = press(&mut h, egui::Key::ArrowDown);
    assert_eq!(RowHarness::focused_item_index(&n), Some(0), "5 wraps to 0");
    let (_, n) = press(&mut h, egui::Key::ArrowDown);
    assert_eq!(RowHarness::focused_item_index(&n), Some(1));
    let (_, n) = press(&mut h, egui::Key::End);
    assert_eq!(RowHarness::focused_item_index(&n), Some(5));
    let (_, n) = press(&mut h, egui::Key::Home);
    assert_eq!(RowHarness::focused_item_index(&n), Some(0));

    let (event, _) = press(&mut h, egui::Key::Enter);
    assert_eq!(
        event,
        Some(RowEvent::Action(RowAction::PlayNow)),
        "Enter activates the focused first item"
    );
}

/// **RM6/RM7**: Escape closes without action and focus returns to the row.
#[test]
fn rm6_rm7_escape_closes_without_action_and_focus_returns_to_row() {
    let mut h = RowHarness::new(track_entity("rm7"), 800.0);
    h.open_via_shift_f10();
    let (event, _) = h.frame(vec![key_event(egui::Key::Escape, Modifiers::NONE)]);
    assert_eq!(event, None, "Escape must not act");
    let (_, nodes) = h.frame(vec![]);
    assert!(nodes.menu_items().is_empty(), "menu closed");
    assert_eq!(
        nodes.focused,
        Some((Role::ListItem, h.name.clone())),
        "focus returns to the row"
    );
}

/// **RM7**: after activating an item, focus returns to the row.
#[test]
fn rm7_activate_returns_focus_to_the_row() {
    let mut h = RowHarness::new(track_entity("rm7b"), 800.0);
    h.open_via_shift_f10();
    h.frame(vec![key_event(egui::Key::Enter, Modifiers::NONE)]);
    let (_, nodes) = h.frame(vec![]);
    assert_eq!(nodes.focused, Some((Role::ListItem, h.name.clone())));
}

/// **RM8**: an unavailable (greyed) row still offers all six actions.
#[test]
fn rm8_greyed_rows_keep_all_six_actions() {
    let mut h = RowHarness::new(widest_track_entity("rm8"), 800.0);
    let nodes = h.open_via_shift_f10();
    assert_eq!(nodes.menu_items().len(), 6);
}
