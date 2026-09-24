// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! T044 (015-control-variants, Phase 5 User Story 3): the integration
//! suite for hover/pressed/focus (contracts/interaction-states.md),
//! written **before** the call-site wiring it pins (T045-T048) so the
//! red -> green transition is real (Constitution VIII). I3, I7 and F4 are
//! pinned purely from Phase 2's foundational values/widgets and pass as
//! soon as this file exists; I6, F5 and F3 exercise `row_frame` and
//! `paint_focus_ring` directly and stay green through Phase 5's wiring —
//! they pin the mechanism, not any one call site, since the mechanism is
//! entirely Phase 2's (design note 8).

use std::fs;
use std::path::PathBuf;

use egui::epaint::ClippedShape;
use egui::{Color32, Context, Id, RawInput, Rect, Shape};
use modplayer_ui::theme::controls::{self, Variant};
use modplayer_ui::theme::tokens::{self, DARK, LIGHT};
use modplayer_ui::widgets::controls::{paint_focus_ring, row_frame};

/// **I3** (contract, FR-011 normative): for every variant x theme,
/// resting, hover and pressed are three pairwise-distinct composited
/// fills, exactly the composition `button()` performs (data-model.md §3
/// "Composition rule"). The ordering of the overlay alphas is the
/// contract, not the two percentages.
#[test]
fn three_states_are_three_increasing_fills() {
    for roles in [&LIGHT, &DARK] {
        for variant in [
            Variant::Primary,
            Variant::Default,
            Variant::Quiet,
            Variant::Destructive,
        ] {
            let paint = controls::variant_paint(roles, variant);
            let resting = if paint.fill == Color32::TRANSPARENT {
                roles.surface_base
            } else {
                paint.fill
            };
            let hover = resting.blend(controls::hover_fill(roles));
            let pressed = resting.blend(controls::pressed_fill(roles));

            assert_ne!(
                resting, hover,
                "{variant:?}: resting and hover must be distinct fills"
            );
            assert_ne!(
                hover, pressed,
                "{variant:?}: hover and pressed must be distinct fills"
            );
            assert_ne!(
                resting, pressed,
                "{variant:?}: resting and pressed must be distinct fills"
            );
        }
    }

    // The ordering itself, not the two numbers (I3's normative clause).
    const { assert!(controls::HOVER_ALPHA > 0.0) };
    const { assert!(controls::PRESSED_ALPHA > controls::HOVER_ALPHA) };
}

/// **I6** (research R5, FR-018): `row_frame` costs zero layout — the rect
/// it hands back to its caller is exactly the rect the same content would
/// occupy drawn directly, because the reserve-then-set fill never
/// allocates space of its own.
#[test]
fn row_frame_adds_no_layout() {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);

    let mut direct_rect = Rect::NOTHING;
    let output = ctx.run_ui(RawInput::default(), |ui| {
        let response = ui.horizontal(|ui| {
            ui.label("Row content");
        });
        direct_rect = response.response.rect;
    });
    output.drop_without_applying_deltas();

    let mut wrapped_rect = Rect::NOTHING;
    let output = ctx.run_ui(RawInput::default(), |ui| {
        let (rect, _inner) = row_frame(ui, Id::new("interaction-states-row-frame"), |ui| {
            ui.horizontal(|ui| {
                ui.label("Row content");
            });
        });
        wrapped_rect = rect;
    });
    output.drop_without_applying_deltas();

    assert_eq!(
        direct_rect, wrapped_rect,
        "row_frame must add no layout: the wrapped rect must equal the unwrapped one"
    );
}

/// **I7**: host controls (`button`/`switch`/`row_frame`, all of
/// `widgets/controls.rs`) resolve interaction state from the `Response`'s
/// own fields, never `Response::widget_state()` — which folds a merely
/// focused widget into `Active` (research R2) — so a source scan (not a
/// runtime check: there is nothing to observe once the call is simply
/// absent) is the correct pin, the same shape as L1's literal scan.
#[test]
fn host_controls_do_not_use_widget_state() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/widgets/controls.rs");
    let contents = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    let mut hits = Vec::new();
    for (idx, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("///") || trimmed.starts_with("//!") || trimmed.starts_with("//") {
            // Comments are allowed to *name* widget_state() while
            // explaining why it is avoided (see the doc comments above
            // `button`/`switch`) — only actual code is forbidden.
            continue;
        }
        let code_part = line.split("//").next().unwrap_or(line);
        if code_part.contains(".widget_state()") {
            hits.push(format!("{}:{}: {}", path.display(), idx + 1, trimmed));
        }
    }

    assert!(
        hits.is_empty(),
        "widgets/controls.rs must never call Response::widget_state() (I7), found:\n{}",
        hits.join("\n")
    );
}

/// Every `.rs` file directly under `src/`, recursing into subdirectories —
/// the same walk `control_variants.rs::walk_src_rs_files` uses, duplicated
/// locally so this file has no cross-file dependency on another test
/// binary.
fn walk_src_rs_files() -> Vec<PathBuf> {
    fn walk(dir: PathBuf, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(&dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut out,
    );
    out
}

/// **F4** (research R3): `paint_focus_ring` is called exactly once in all
/// of `src/**` — its own definition in `widgets/controls.rs` plus the one
/// call site, the last statement of `App::ui` in `app.rs`. No view file
/// paints a ring.
#[test]
fn focus_ring_is_painted_once_app_wide() {
    let mut call_sites = Vec::new();
    let mut total = 0usize;

    for path in walk_src_rs_files() {
        let contents = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        for (idx, line) in contents.lines().enumerate() {
            if line.contains("paint_focus_ring(") {
                total += 1;
                if !line.trim_start().starts_with("pub fn paint_focus_ring(") {
                    call_sites.push((path.clone(), idx + 1));
                }
            }
        }
    }

    // One definition + one call site.
    assert_eq!(
        total, 2,
        "expected exactly 2 occurrences of paint_focus_ring( (1 definition + 1 call site) in src/**, found {total}"
    );
    assert_eq!(
        call_sites.len(),
        1,
        "expected exactly one paint_focus_ring(...) call site, found {}: {call_sites:?}",
        call_sites.len()
    );
    let (call_path, _) = &call_sites[0];
    assert_eq!(
        call_path.file_name().and_then(|n| n.to_str()),
        Some("app.rs"),
        "the one paint_focus_ring(...) call site must be in app.rs, found {}",
        call_path.display()
    );
}

/// A ring-stroked rect in `shapes`: a `Shape::Rect` whose stroke width
/// equals `FOCUS_RING_WIDTH` — `paint_focus_ring`'s own signature, distinct
/// from any fill-only shape a button/switch/row paints.
fn ring_shape_rect(shapes: &[ClippedShape]) -> Option<Rect> {
    shapes.iter().find_map(|clipped| match &clipped.shape {
        Shape::Rect(r) if (r.stroke.width - controls::FOCUS_RING_WIDTH).abs() < f32::EPSILON => {
            Some(r.rect)
        }
        _ => None,
    })
}

/// **F5**: no ring is drawn when nothing is focused, or when the focused
/// id was never interacted with this pass or last (`read_response`
/// returns `None`) — the "ghost" focus a scrolled-away row would leave
/// behind.
#[test]
fn no_ring_without_a_visible_focus() {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);

    // Case 1: nothing focused at all.
    let output = ctx.run_ui(RawInput::default(), |ui| {
        paint_focus_ring(ui.ctx());
    });
    assert!(
        ring_shape_rect(&output.shapes).is_none(),
        "no widget is focused; paint_focus_ring must paint nothing"
    );
    output.drop_without_applying_deltas();

    // Case 2: memory holds a focus id for a widget that has never been
    // drawn (this pass or last) — `read_response` returns `None`.
    let ghost_id = Id::new("interaction-states-ghost-focus");
    ctx.memory_mut(|m| m.request_focus(ghost_id));
    let output = ctx.run_ui(RawInput::default(), |ui| {
        paint_focus_ring(ui.ctx());
    });
    assert!(
        ring_shape_rect(&output.shapes).is_none(),
        "the focused id was never interacted with (F5); paint_focus_ring must paint nothing"
    );
    output.drop_without_applying_deltas();
}

/// **F3**: ring and selection stay separable *structurally*, not
/// chromatically. The selection fill stays interior and unchanged from
/// 014 (`Visuals::selection`); the ring is painted with `StrokeKind::
/// Outside` on `rect.expand(FOCUS_RING_GAP)` (F2), so a real focused,
/// visible control's ring rect is strictly larger than the control's own
/// rect on every side — the ring can never sit inside, on top of, or
/// replace the selection fill it might be layered over.
#[test]
fn ring_and_selection_do_not_overlap() {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let style = modplayer_ui::theme::style::build_style(theme, false);
        let roles = tokens::for_dark_mode(matches!(theme, egui::Theme::Dark));
        assert_eq!(
            style.visuals.selection.bg_fill, roles.accent,
            "the selection fill must stay interior/unchanged from 014 (F3)"
        );
    }

    // The mechanism (F2): pinned at the source so a later edit cannot
    // quietly move the ring inward.
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/widgets/controls.rs");
    let contents = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    assert!(
        contents.contains("StrokeKind::Outside"),
        "paint_focus_ring must stroke the ring with StrokeKind::Outside (F2/F3)"
    );

    // The behaviour: a real focused, visible control's ring sits strictly
    // outside its own rect, expanded by exactly FOCUS_RING_GAP.
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);

    let mut rect = Rect::NOTHING;
    let mut id = Id::NULL;
    let output = ctx.run_ui(RawInput::default(), |ui| {
        let response = ui.button("Focus me");
        rect = response.rect;
        id = response.id;
    });
    output.drop_without_applying_deltas();

    ctx.memory_mut(|m| m.request_focus(id));

    let output = ctx.run_ui(RawInput::default(), |ui| {
        let response = ui.button("Focus me");
        assert_eq!(
            response.id, id,
            "the button's id must be stable across passes"
        );
        paint_focus_ring(ui.ctx());
    });

    let expanded = rect.expand(controls::FOCUS_RING_GAP);
    let ring_rect = ring_shape_rect(&output.shapes)
        .unwrap_or_else(|| panic!("expected a ring shape once the button holds focus"));
    assert!(
        (ring_rect.min - expanded.min).length() < 0.5
            && (ring_rect.max - expanded.max).length() < 0.5,
        "ring rect {ring_rect:?} must equal the control's rect expanded by FOCUS_RING_GAP {expanded:?}"
    );
    assert!(
        ring_rect.min.x < rect.min.x
            && ring_rect.min.y < rect.min.y
            && ring_rect.max.x > rect.max.x
            && ring_rect.max.y > rect.max.y,
        "ring rect {ring_rect:?} must lie strictly outside the control's own rect {rect:?}"
    );

    output.drop_without_applying_deltas();
}
