// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! 029-keyboard-shortcut-reference, contracts/shortcut-sheet.md S6–S18:
//! the read-only shortcut sheet window and its printable export. Driven
//! headlessly against a bare `egui::Context` and a real
//! `ActionRegistry`, like `tests/controls.rs`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use egui::accesskit::{Role, TreeUpdate};
use egui::{Context, Event, Key, Modifiers, Pos2, RawInput, Rect, vec2};
use modplayer_core::actions::{
    ActionCategory, ActionRegistry, Chord, HostAction, KeymapOverrides, Platform,
};
use modplayer_core::{tr, tr_args};
use modplayer_ui::key_label::KeyLabel;
use modplayer_ui::shortcut_groups::{self, RowGroup, ShortcutGroup, ShortcutRow, snapshot};
use modplayer_ui::shortcut_sheet::{
    self, SheetOutcome, ShortcutSheetState, export_printable, printable_html, write_printable,
};

fn input(width: f32, events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0))),
        events,
        ..Default::default()
    }
}

fn fresh_ctx() -> Context {
    let ctx = Context::default();
    // egui defaults `os()` to the build target; pin it so labels match
    // `PLATFORM` on every host.
    ctx.set_os(egui::os::OperatingSystem::Nix);
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx.enable_accesskit();
    ctx
}

fn registry() -> ActionRegistry {
    ActionRegistry::new(KeymapOverrides::default())
}

/// Run a few warm-up frames (a new `Window` sizes itself invisibly first),
/// returning the last frame's tree.
fn settle(
    ctx: &Context,
    state: &mut ShortcutSheetState,
    registry: &ActionRegistry,
    width: f32,
) -> TreeUpdate {
    let mut last = frame(ctx, state, registry, width, vec![]).1;
    for _ in 0..3 {
        last = frame(ctx, state, registry, width, vec![]).1;
    }
    last
}

/// One frame of the sheet; returns the outcome and the AccessKit tree.
fn frame(
    ctx: &Context,
    state: &mut ShortcutSheetState,
    registry: &ActionRegistry,
    width: f32,
    events: Vec<Event>,
) -> (SheetOutcome, TreeUpdate) {
    let mut outcome = SheetOutcome::None;
    let mut output = ctx.run_ui(input(width, events), |ui| {
        outcome = shortcut_sheet::show(ui.ctx(), state, registry);
    });
    let update = output.platform_output.accesskit_update.take().unwrap();
    output.drop_without_applying_deltas();
    (outcome, update)
}

fn labels(update: &TreeUpdate) -> Vec<String> {
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().or(node.value()).map(str::to_string))
        .collect()
}

fn open_state() -> ShortcutSheetState {
    ShortcutSheetState {
        open: true,
        ..ShortcutSheetState::default()
    }
}

fn spoken(row: &ShortcutRow, platform: Platform) -> String {
    if row.bindings.is_empty() {
        tr("shortcuts-not-bound")
    } else {
        row.bindings
            .iter()
            .map(|&c| KeyLabel::new(c, platform).spoken)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn row_name(row: &ShortcutRow, platform: Platform) -> String {
    tr_args(
        "controls-row",
        &[
            ("action", row.label.clone()),
            ("bindings", spoken(row, platform)),
        ],
    )
}

/// The OS `fresh_ctx` pins (non-mac).
const PLATFORM: Platform = Platform::Other;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-shortcut-sheet-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// -- Window (S6–S13) ------------------------------------------------------

/// S6: one fixed-id window; non-modal (a button elsewhere still gets the
/// click); opening again is idempotent.
#[test]
fn sheet_single_instance_non_modal() {
    let ctx = fresh_ctx();
    let reg = registry();
    let mut state = open_state();
    for _ in 0..3 {
        state.open();
        settle(&ctx, &mut state, &reg, 1400.0);
    }
    let windows = ctx.memory(|m| {
        m.areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|layer| layer.id == shortcut_sheet::window_id())
            .count()
    });
    assert_eq!(windows, 1);
    assert!(state.open);

    // Non-modal: a button in a far corner outside the window still
    // receives a click while the sheet is open.
    let target = Pos2::new(1360.0, 870.0);
    let mut clicked = false;
    let mut corner_frame = |events: Vec<Event>, state: &mut ShortcutSheetState| {
        let mut output = ctx.run_ui(input(1400.0, events), |ui| {
            shortcut_sheet::show(ui.ctx(), state, &reg);
            egui::Area::new(egui::Id::new("corner"))
                .fixed_pos(Pos2::new(1340.0, 860.0))
                .show(ui.ctx(), |ui| {
                    clicked |= ui.button("corner").clicked();
                });
        });
        output.platform_output.accesskit_update.take();
        output.drop_without_applying_deltas();
    };
    corner_frame(vec![], &mut state);
    corner_frame(vec![Event::PointerMoved(target)], &mut state);
    for pressed in [true, false] {
        corner_frame(
            vec![Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }],
            &mut state,
        );
    }
    assert!(clicked, "the sheet must not block input elsewhere");
}

/// S7: the sheet lists the same groups and rows as the controls page.
#[test]
fn sheet_matches_controls_groups_and_rows() {
    let ctx = fresh_ctx();
    let reg = registry();
    let mut state = open_state();
    let update = settle(&ctx, &mut state, &reg, 1400.0);
    let seen = labels(&update);
    let groups = snapshot(&reg);
    assert!(!groups.is_empty());
    for group in &groups {
        assert!(
            seen.contains(&group.heading),
            "missing heading {}",
            group.heading
        );
        for row in &group.rows {
            let name = row_name(row, PLATFORM);
            assert!(seen.contains(&name), "missing row {name:?}");
        }
    }
}

/// S8: no editing controls anywhere in the sheet.
#[test]
fn sheet_has_no_editing_controls() {
    let ctx = fresh_ctx();
    let reg = registry();
    let mut state = open_state();
    let update = settle(&ctx, &mut state, &reg, 1400.0);
    let forbidden = [
        tr("controls-add-binding"),
        tr("controls-reset-all"),
        tr("controls-confirm"),
    ];
    for label in labels(&update) {
        assert!(!forbidden.contains(&label), "editing control {label:?}");
        assert!(!label.starts_with("Remove binding"), "{label:?}");
        assert!(!label.starts_with("Reset "), "{label:?}");
    }
    assert!(
        update
            .nodes
            .iter()
            .all(|(_, node)| node.role() != Role::TextInput),
        "no filter or capture input"
    );
}

/// S9: two columns when the sheet is wide, one when narrow.
#[test]
fn sheet_two_columns_when_wide() {
    let heading_xs = |width: f32| -> Vec<i64> {
        let ctx = fresh_ctx();
        let reg = registry();
        let mut state = open_state();
        let update = settle(&ctx, &mut state, &reg, width);
        let headings: Vec<String> = snapshot(&reg).iter().map(|g| g.heading.clone()).collect();
        update
            .nodes
            .iter()
            .filter(|(_, n)| n.label().is_some_and(|l| headings.iter().any(|h| h == l)))
            .filter(|(_, n)| n.role() != Role::Window)
            .filter_map(|(_, n)| n.bounds().map(|b| b.x0.round() as i64))
            .collect()
    };
    let mut wide = heading_xs(1600.0);
    wide.sort_unstable();
    wide.dedup();
    assert_eq!(wide.len(), 2, "two column origins when wide: {wide:?}");
    let mut narrow = heading_xs(600.0);
    narrow.sort_unstable();
    narrow.dedup();
    assert_eq!(narrow.len(), 1, "one column origin when narrow: {narrow:?}");
}

/// S10 / SC-004: a registry change shows on the next frame.
#[test]
fn sheet_reflects_binding_change_next_frame() {
    let ctx = fresh_ctx();
    let mut reg = registry();
    let mut state = open_state();
    settle(&ctx, &mut state, &reg, 1400.0);

    let action = HostAction::TogglePlayPause;
    let before: ShortcutRow = snapshot(&reg)
        .into_iter()
        .flat_map(|g| g.rows)
        .find(|r| r.id == action.into())
        .unwrap();
    let extra = Chord::parse("Primary+Shift+Y").unwrap();
    reg.add_binding(action, extra).unwrap();
    let after: ShortcutRow = snapshot(&reg)
        .into_iter()
        .flat_map(|g| g.rows)
        .find(|r| r.id == action.into())
        .unwrap();
    assert_ne!(row_name(&before, PLATFORM), row_name(&after, PLATFORM));

    let (_, update) = frame(&ctx, &mut state, &reg, 1400.0, vec![]);
    let seen = labels(&update);
    assert!(seen.contains(&row_name(&after, PLATFORM)));
    assert!(!seen.contains(&row_name(&before, PLATFORM)));

    reg.reset_all();
    let (_, update) = frame(&ctx, &mut state, &reg, 1400.0, vec![]);
    assert!(labels(&update).contains(&row_name(&before, PLATFORM)));
}

fn esc() -> Vec<Event> {
    vec![Event::Key {
        key: Key::Escape,
        physical_key: Some(Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]
}

fn tab() -> Vec<Event> {
    vec![Event::Key {
        key: Key::Tab,
        physical_key: Some(Key::Tab),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]
}

/// S11: Esc closes only while focus is inside the sheet.
#[test]
fn sheet_esc_closes_when_focused_only() {
    let ctx = fresh_ctx();
    let reg = registry();

    // Focus elsewhere (nothing focused): Esc does nothing.
    let mut state = open_state();
    settle(&ctx, &mut state, &reg, 1400.0);
    frame(&ctx, &mut state, &reg, 1400.0, esc());
    assert!(
        state.open,
        "Esc without focus in the sheet must not close it"
    );

    // Tab into the sheet, then Esc closes it.
    frame(&ctx, &mut state, &reg, 1400.0, tab());
    frame(&ctx, &mut state, &reg, 1400.0, vec![]);
    let focused_in_sheet = ctx
        .memory(|m| m.focused())
        .and_then(|id| ctx.read_response(id))
        .is_some_and(|r| r.layer_id.id == shortcut_sheet::window_id());
    assert!(focused_in_sheet, "Tab should land focus inside the sheet");
    frame(&ctx, &mut state, &reg, 1400.0, esc());
    assert!(!state.open, "Esc with focus inside closes the sheet");
}

fn click(pos: Pos2) -> [Vec<Event>; 3] {
    let button = |pressed| Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    [
        vec![Event::PointerMoved(pos)],
        vec![button(true)],
        vec![button(false)],
    ]
}

/// S11: clicking the sheet's body (no widget there to focus) counts as
/// focus inside, so Esc closes it; a later click outside the sheet does
/// not (found in the T051 manual walk: Tab from the app lands in the nav
/// rail, so a mouse user had no way to make Esc work).
#[test]
fn sheet_esc_closes_after_clicking_its_body() {
    let ctx = fresh_ctx();
    let reg = registry();
    let mut state = open_state();
    settle(&ctx, &mut state, &reg, 1400.0);
    let rect = ctx
        .memory(|m| m.area_rect(shortcut_sheet::window_id()))
        .unwrap();
    let outside = Pos2::new(rect.right() + 20.0, rect.center().y);
    assert!(outside.x < 1400.0, "sanity: room right of the sheet");

    for events in click(outside) {
        frame(&ctx, &mut state, &reg, 1400.0, events);
    }
    frame(&ctx, &mut state, &reg, 1400.0, esc());
    assert!(state.open, "Esc after clicking outside must not close it");

    for events in click(rect.center()) {
        frame(&ctx, &mut state, &reg, 1400.0, events);
    }
    frame(&ctx, &mut state, &reg, 1400.0, vec![]);
    frame(&ctx, &mut state, &reg, 1400.0, esc());
    assert!(!state.open, "Esc after clicking the sheet body closes it");

    // Reopened, the earlier click is forgotten.
    state.open = true;
    settle(&ctx, &mut state, &reg, 1400.0);
    frame(&ctx, &mut state, &reg, 1400.0, esc());
    assert!(state.open, "a reopened sheet needs a fresh click or focus");
}

/// S12: a fresh state starts closed.
#[test]
fn sheet_starts_closed() {
    assert!(!ShortcutSheetState::default().open);
    let ctx = fresh_ctx();
    let mut state = ShortcutSheetState::default();
    let (outcome, update) = frame(&ctx, &mut state, &registry(), 1400.0, vec![]);
    assert_eq!(outcome, SheetOutcome::None);
    assert!(!labels(&update).contains(&tr("shortcuts-sheet-title")));
}

// -- Printable version (S14–S18) -----------------------------------------

fn group_with(label: &str, group_name: &str) -> ShortcutGroup {
    ShortcutGroup {
        group: RowGroup::Plugin(group_name.to_string()),
        heading: group_name.to_string(),
        rows: vec![ShortcutRow {
            id: HostAction::TogglePlayPause.into(),
            label: label.to_string(),
            kind: modplayer_core::actions::ActionKind::Trigger,
            enabled: true,
            bindings: vec![],
            conflicts: vec![],
            group: RowGroup::Plugin(group_name.to_string()),
        }],
    }
}

/// S14: the export writes the fixed file name and returns a file URL.
#[test]
fn printable_writes_file_and_returns_url() {
    let dir = TempDir::new();
    let groups = snapshot(&registry());
    let mut state = open_state();
    let outcome = export_printable(&mut state, &groups, PLATFORM, &dir.0);
    let SheetOutcome::OpenPrintable(url) = outcome else {
        panic!("expected a URL, got {outcome:?}");
    };
    assert!(url.starts_with("file://"));
    assert!(url.ends_with("/modplayer-shortcuts.html"));
    let written = std::fs::read_to_string(dir.0.join("modplayer-shortcuts.html")).unwrap();
    assert!(written.contains("<h1>"));
    assert!(state.export_error.is_none());
    // Overwrites rather than failing.
    assert!(matches!(
        export_printable(&mut state, &groups, PLATFORM, &dir.0),
        SheetOutcome::OpenPrintable(_)
    ));
    assert!(write_printable(&dir.0, "x").is_ok());
}

/// S15: self-contained structure.
#[test]
fn printable_html_structure() {
    let groups = snapshot(&registry());
    let html = printable_html(&groups, PLATFORM, "en-US", "2026-10-09 12:00");
    assert!(html.contains("<html lang=\"en-US\">"));
    assert!(html.contains("<style>") && html.contains("@media print"));
    assert!(!html.contains("<script"));
    assert!(!html.contains("src="));
    assert!(!html.contains("href="));
    assert_eq!(html.matches("<h2>").count(), groups.len());
    assert_eq!(html.matches("<table>").count(), groups.len());
    assert!(html.contains("2026-10-09 12:00"));
    for group in &groups {
        for row in &group.rows {
            assert!(html.contains(&row.label) || html.contains(&row.label.replace('&', "&amp;")));
        }
    }
    assert!(html.contains(&tr("shortcuts-not-bound")));
}

/// S16: plugin-supplied labels are escaped.
#[test]
fn printable_html_escapes_plugin_labels() {
    let groups = vec![group_with("<script>x</script>", "a&b\"")];
    let html = printable_html(&groups, PLATFORM, "en-US", "now");
    assert!(!html.contains("<script"));
    assert!(html.contains("&lt;script&gt;x&lt;/script&gt;"));
    assert!(html.contains("a&amp;b&quot;"));
}

/// S18: a write failure sets the inline error, opens no URL, and the
/// sheet shows the message while staying open.
#[test]
fn printable_write_failure_shows_inline_error() {
    let dir = TempDir::new();
    let missing = dir.0.join("does-not-exist");
    let groups = snapshot(&registry());
    let mut state = open_state();
    let outcome = export_printable(&mut state, &groups, PLATFORM, &missing);
    assert_eq!(outcome, SheetOutcome::None);
    assert!(state.export_error.is_some());
    assert!(state.open);

    let ctx = fresh_ctx();
    let reg = registry();
    let update = settle(&ctx, &mut state, &reg, 1400.0);
    assert!(labels(&update).contains(&tr("shortcuts-printable-error")));
    assert!(state.open);
}

#[test]
fn unused_helpers_are_exercised() {
    // Keeps the shared imports honest when a contract test is trimmed.
    let _ = (
        ActionCategory::ALL.len(),
        shortcut_groups::TWO_COLUMN_MIN_WIDTH,
    );
}

/// 029 T049 (NFR-7.1): with every string padded by 40 %, no two row
/// elements in the same column overlap vertically and everything stays
/// inside the screen, wide (two columns) and narrow (one column).
#[test]
fn sheet_fits_under_40_percent_pseudo_expansion() {
    modplayer_core::i18n::with_pseudo_expansion(40, || {
        for width in [1600.0_f32, 600.0] {
            let ctx = fresh_ctx();
            let reg = registry();
            let mut state = open_state();
            let update = settle(&ctx, &mut state, &reg, width);
            let row_nodes: Vec<_> = snapshot(&reg)
                .iter()
                .flat_map(|g| g.rows.iter().map(|r| row_name(r, PLATFORM)))
                .collect();
            let mut rects: Vec<egui::accesskit::Rect> = update
                .nodes
                .iter()
                .filter(|(_, n)| n.label().is_some_and(|l| row_nodes.iter().any(|r| r == l)))
                .filter_map(|(_, n)| n.bounds())
                .collect();
            assert!(!rects.is_empty(), "no row elements found at {width}");
            for r in &rects {
                assert!(
                    r.x0 >= 0.0 && r.x1 <= f64::from(width) + 1.0,
                    "row leaves the screen at {width}: {r:?}"
                );
            }
            rects.sort_by(|a, b| a.y0.total_cmp(&b.y0));
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    let x_overlap = a.x0 < b.x1 - 1.0 && b.x0 < a.x1 - 1.0;
                    let y_overlap = a.y0 < b.y1 - 1.0 && b.y0 < a.y1 - 1.0;
                    assert!(
                        !(x_overlap && y_overlap),
                        "rows overlap at {width}: {a:?} / {b:?}"
                    );
                }
            }
        }
    });
}
