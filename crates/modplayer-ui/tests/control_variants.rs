// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! T012 (015-control-variants, Phase 2 Foundational): the module-level
//! integration suite for button variants and the switch
//! (contracts/control-variants.md), written **before** the values and
//! widgets it pins (T013-T025) so the red -> green transition is real
//! (Constitution VIII). This file grows further named tests in later
//! phases (US1/US5); Phase 2 adds exactly the rules that do not depend on
//! any call-site conversion: B3, B4, S4, L1, A5/F6.

use std::fs;
use std::path::PathBuf;

use egui::{Color32, Context, Event, Modifiers, PointerButton, RawInput, Rect, Stroke};
use modplayer_ui::theme::controls::{self, Variant};
use modplayer_ui::theme::tokens::{self, DARK, LIGHT, Roles, divider_color_for};
use modplayer_ui::widgets::controls::{SwitchKind, button, switch};

/// A variant's `(fill, outline.color, outline.width, label)` tuple
/// (contract B3).
fn variant_triple(roles: &Roles, variant: Variant) -> (Color32, Color32, f32, Color32) {
    let paint = controls::variant_paint(roles, variant);
    (
        paint.fill,
        paint.outline.color,
        paint.outline.width,
        paint.label,
    )
}

/// **B3**: the four variants are four *pairwise distinct*
/// `(fill, outline, label)` triples, in both themes.
#[test]
fn four_variants_are_four_distinct_triples() {
    for roles in [&LIGHT, &DARK] {
        let variants = [
            Variant::Primary,
            Variant::Default,
            Variant::Quiet,
            Variant::Destructive,
        ];
        let triples: Vec<_> = variants.iter().map(|&v| variant_triple(roles, v)).collect();
        for i in 0..triples.len() {
            for j in (i + 1)..triples.len() {
                assert_ne!(
                    triples[i], triples[j],
                    "{:?} and {:?} share a paint triple: {:?}",
                    variants[i], variants[j], triples[i]
                );
            }
        }
    }
}

/// **B4**: every colour a variant paints is one of the ten 014 roles, a
/// data-model §3 derived value (`divider_color_for`, the `Default`
/// outline's colour), or `TRANSPARENT`; every stroke width is a
/// `theme::controls` constant (`DEFAULT_OUTLINE_WIDTH`/
/// `DESTRUCTIVE_OUTLINE_WIDTH`, both `1.0`, or `0.0` for `Stroke::NONE`).
#[test]
fn variant_colours_come_from_roles() {
    for roles in [&LIGHT, &DARK] {
        let role_colours = [
            roles.text_primary,
            roles.text_secondary,
            roles.text_disabled,
            roles.surface_base,
            roles.surface_raised,
            roles.accent,
            roles.text_on_accent,
            roles.positive,
            roles.warning,
            roles.danger,
            divider_color_for(roles),
        ];

        for variant in [
            Variant::Primary,
            Variant::Default,
            Variant::Quiet,
            Variant::Destructive,
        ] {
            let paint = controls::variant_paint(roles, variant);

            assert!(
                paint.fill == Color32::TRANSPARENT || role_colours.contains(&paint.fill),
                "{variant:?}.fill {:?} is neither TRANSPARENT nor a 014 role",
                paint.fill
            );
            assert!(
                paint.outline == Stroke::NONE || role_colours.contains(&paint.outline.color),
                "{variant:?}.outline.color {:?} is neither NONE nor a 014 role",
                paint.outline.color
            );
            assert!(
                paint.outline.width == 0.0
                    || paint.outline.width == controls::DEFAULT_OUTLINE_WIDTH
                    || paint.outline.width == controls::DESTRUCTIVE_OUTLINE_WIDTH,
                "{variant:?}.outline.width {} is not a theme::controls constant",
                paint.outline.width
            );
            assert!(
                role_colours.contains(&paint.label),
                "{variant:?}.label {:?} is not a 014 role",
                paint.label
            );
        }
    }
}

/// **S4** (data-model.md §5 Validation V8): a switch is not mistakable
/// for a button in any state. The switch's `(track fill, outline, thumb)`
/// colour triple *can* numerically coincide with a variant's (the `On`
/// state and `Primary` are both `accent`/`Stroke::NONE`/`text_on_accent`
/// in this token table — a legitimate coincidence, not a bug, the same
/// way 014 allows `pressed_fill` to coincide with `divider`), but its
/// `radius::full` corner is used by **no** variant (every variant renders
/// at the shared `radius::SM` the `Style` construction site installs) —
/// so the full `(fill, outline, label_or_thumb, corner_radius)` signature
/// is always distinct.
#[test]
fn a_switch_is_not_any_button_variant() {
    let switch_radius = tokens::radius::full(controls::switch_metrics().track.y);
    let button_radius = tokens::radius::SM;
    assert_ne!(
        switch_radius, button_radius,
        "the switch's pill radius must differ from every button's radius::SM"
    );

    for roles in [&LIGHT, &DARK] {
        for on in [false, true] {
            let (track_fill, track_outline) = controls::switch_track(roles, on);
            let thumb = controls::switch_thumb(roles, on);
            let switch_signature = (
                track_fill,
                track_outline.color,
                track_outline.width,
                thumb,
                switch_radius,
            );

            for variant in [
                Variant::Primary,
                Variant::Default,
                Variant::Quiet,
                Variant::Destructive,
            ] {
                let (fill, outline_color, outline_width, label) = variant_triple(roles, variant);
                let variant_signature = (fill, outline_color, outline_width, label, button_radius);
                assert_ne!(
                    switch_signature, variant_signature,
                    "switch(on={on}) matches {variant:?}'s full signature: {variant_signature:?}"
                );
            }
        }
    }
}

/// **L1**: `widgets/controls.rs` writes no colour, alpha or stroke-width
/// literal of its own — every value it uses comes from `theme::controls`
/// by name. A targeted source check (distinct from 014's crate-wide
/// `design_token_literals.rs`, which this file does not duplicate):
/// no `Color32` constructor/named constant (`TRANSPARENT` excepted, as it
/// names no colour), no `Stroke::new(`/`Stroke::NONE` literal construction
/// and no `FontId` constructor appear in the widget module — every one of
/// those types reaches this file already built, from `theme::controls`.
#[test]
fn widget_module_uses_only_token_values() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/widgets/controls.rs");
    let contents = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    let mut in_test_mod = false;
    let mut depth = 0i32;
    let mut pending_cfg_test = false;
    let mut hits = Vec::new();

    for (idx, line) in contents.lines().enumerate() {
        let trimmed = line.trim();

        if in_test_mod {
            depth += line.matches('{').count() as i32;
            depth -= line.matches('}').count() as i32;
            if depth <= 0 {
                in_test_mod = false;
                depth = 0;
            }
            continue;
        }
        if trimmed.contains("#[cfg(test)]") {
            pending_cfg_test = true;
            continue;
        }
        if pending_cfg_test {
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            pending_cfg_test = false;
            if trimmed.contains("mod ") {
                in_test_mod = true;
                depth = line.matches('{').count() as i32 - line.matches('}').count() as i32;
                if depth <= 0 {
                    in_test_mod = false;
                    depth = 0;
                }
                continue;
            }
        }
        if trimmed.starts_with("use ") || trimmed.starts_with("///") || trimmed.starts_with("//!") {
            continue;
        }

        const FORBIDDEN: &[&str] = &[
            "Color32::from_rgb",
            "Color32::from_gray",
            "Color32::from_black_alpha",
            "Color32::from_white_alpha",
            "Color32::WHITE",
            "Color32::BLACK",
            "Color32::RED",
            "Color32::GREEN",
            "Color32::BLUE",
            "Stroke::new(",
            "Stroke::NONE",
            "FontId::new(",
            "FontId::proportional(",
            "FontId::monospace(",
        ];
        for needle in FORBIDDEN {
            if line.contains(needle) {
                hits.push(format!(
                    "{}:{}: {} ({needle})",
                    path.display(),
                    idx + 1,
                    trimmed
                ));
            }
        }
    }

    assert!(
        hits.is_empty(),
        "widgets/controls.rs must consume theme::controls values by name, found:\n{}",
        hits.join("\n")
    );
}

/// **A5, F6, FR-020**: a disabled control shows no hover, no pressed and
/// holds no focus — `Response::hovered()`/`is_pointer_button_down_on()`
/// are `false` for a disabled widget by construction (egui), and a
/// disabled widget cannot claim `Memory::focused()`.
#[test]
fn disabled_controls_show_no_feedback() {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);

    // Pass 1 (enabled): establish the rect, exactly as a real disabled
    // control still occupies space from a prior pass.
    let mut rect = Rect::NOTHING;
    let output = ctx.run_ui(RawInput::default(), |ui| {
        rect = button(ui, Variant::Default, "Test").rect;
    });
    output.drop_without_applying_deltas();

    // Pass 2 (disabled): the pointer hovers and holds the primary button
    // down over that same rect.
    let pointer_pos = rect.center();
    let raw_input = RawInput {
        events: vec![
            Event::PointerMoved(pointer_pos),
            Event::PointerButton {
                pos: pointer_pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        ..Default::default()
    };

    let mut hovered = true;
    let mut pressed = true;
    let mut on = false;
    let mut switch_hovered = true;
    let output = ctx.run_ui(raw_input, |ui| {
        ui.add_enabled_ui(false, |ui| {
            let button_resp = button(ui, Variant::Default, "Test");
            hovered = button_resp.hovered();
            pressed = button_resp.is_pointer_button_down_on();

            let switch_resp = switch(ui, SwitchKind::Checkbox, &mut on, "Test switch");
            switch_hovered = switch_resp.hovered();
        });
    });
    output.drop_without_applying_deltas();

    assert!(!hovered, "a disabled button must not report hover");
    assert!(!pressed, "a disabled button must not report pressed");
    assert!(!switch_hovered, "a disabled switch must not report hover");

    let focused = ctx.memory(|m| m.focused());
    assert!(
        focused.is_none(),
        "a disabled control must not hold focus, found {focused:?}"
    );
}
