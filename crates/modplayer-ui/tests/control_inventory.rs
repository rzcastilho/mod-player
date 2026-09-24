// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! T033 (015-control-variants, Phase 4 User Story 2): the source-level
//! inventory pinning contract rules S5/S6 (SC-010) — every persistent
//! boolean control in the FR-008 acceptance set and the FR-008a app-wide
//! set (data-model.md §9.3) renders the switch, and every FR-008b
//! one-of-N selection control stays exactly as it was. Written before
//! T034-T042's call-site conversions so the red -> green transition is
//! real (Constitution VIII): before those tasks, none of the acceptance
//! sites call `switch(`, and this suite fails loudly rather than
//! vacuously.

use std::fs;
use std::path::PathBuf;

fn src_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn read(rel: &str) -> String {
    let path = src_root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {rel}: {e}"))
}

/// The 0-based line index of the first line in `contents` containing
/// `needle`, panicking with a useful message if there is none.
fn line_index(contents: &str, needle: &str, context: &str) -> usize {
    contents
        .lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("{context}: no line containing {needle:?}"))
}

/// The `[start, end)` line-index range within `radius` lines either side
/// of `center` (clamped to the file's own bounds) — a call site's
/// opening `switch(`/`tr(...)` line and its named-key literal line can
/// fall on either side of each other depending on argument order.
fn window_range(total_lines: usize, center: usize, radius: usize) -> (usize, usize) {
    let start = center.saturating_sub(radius);
    let end = (center + radius + 1).min(total_lines);
    (start, end)
}

/// Every `SwitchKind::Checkbox`/`SwitchKind::Toggle` mention within
/// `radius` lines of `center`.
fn switch_kinds_in_window(contents: &str, center: usize, radius: usize) -> Vec<&str> {
    let lines: Vec<&str> = contents.lines().collect();
    let (start, end) = window_range(lines.len(), center, radius);
    lines[start..end]
        .iter()
        .flat_map(|line| {
            let mut kinds = Vec::new();
            if line.contains("SwitchKind::Checkbox") {
                kinds.push("Checkbox");
            }
            if line.contains("SwitchKind::Toggle") {
                kinds.push("Toggle");
            }
            kinds
        })
        .collect()
}

fn window_contains(contents: &str, center: usize, radius: usize, needle: &str) -> bool {
    let lines: Vec<&str> = contents.lines().collect();
    let (start, end) = window_range(lines.len(), center, radius);
    lines[start..end].iter().any(|line| line.contains(needle))
}

/// **S5**: every control in data-model.md §9.3's FR-008 acceptance set and
/// FR-008a app-wide set renders `switch(...)` with the accessible-role-
/// preserving `SwitchKind` data-model.md §8 names for it, and no
/// `toggle_value`, boolean `Checkbox` or boolean `selectable_label`
/// construct survives anywhere in `src/**` (every one of those constructs
/// this feature touches was a named acceptance-set site — none is left
/// over once the conversion is complete).
#[test]
fn every_boolean_control_is_a_switch() {
    // (file, an anchor line unique to the site, expected SwitchKind,
    // how many lines after the anchor `switch(...)` may appear in).
    let sites: &[(&str, &str, &str)] = &[
        // FR-008 acceptance set.
        ("now_playing.rs", "\"queue-toggle\"", "Toggle"),
        ("now_playing.rs", "\"effects-toggle\"", "Toggle"),
        ("now_playing.rs", "\"transport-toggle\"", "Toggle"),
        ("plugins_view.rs", "\"plugins-enable-toggle\"", "Checkbox"),
        ("effects_view.rs", "\"effects-bypass\"", "Toggle"),
        // FR-008a app-wide boolean set.
        ("effects_view.rs", "\"effects-param-formant\"", "Toggle"),
        ("effects_view.rs", "\"effects-param-mute\"", "Toggle"),
        ("effects_view.rs", "\"effects-param-mono-sum\"", "Toggle"),
        (
            "effects_view.rs",
            "\"effects-param-phase-invert\"",
            "Toggle",
        ),
        (
            "effects_view.rs",
            "\"effects-param-channel-swap\"",
            "Toggle",
        ),
        ("queue_view.rs", "\"queue-shuffle\"", "Toggle"),
        (
            "markers.rs",
            "let enabled = row.armed || row.armable;",
            "Checkbox",
        ),
        ("settings/audio.rs", "\"setting-safe-volume\"", "Checkbox"),
    ];

    let mut cache = std::collections::HashMap::new();
    for (file, ..) in sites {
        cache.entry(*file).or_insert_with(|| read(file));
    }
    // Two more files whose boolean site has no fixed `tr(...)` key literal
    // (the label is the plugin/field's own runtime string) — anchored on
    // the enclosing match arm / function instead.
    cache
        .entry("settings/plugins.rs")
        .or_insert_with(|| read("settings/plugins.rs"));
    cache
        .entry("plugin_panels.rs")
        .or_insert_with(|| read("plugin_panels.rs"));

    const RADIUS: usize = 6;

    for (file, anchor, expected_kind) in sites {
        let contents = &cache[*file];
        let idx = line_index(contents, anchor, file);
        assert!(
            window_contains(contents, idx, RADIUS, "switch("),
            "{file}: expected a switch(...) call within {RADIUS} lines of {anchor:?}"
        );
        let kinds = switch_kinds_in_window(contents, idx, RADIUS);
        assert!(
            kinds.contains(expected_kind),
            "{file}: expected SwitchKind::{expected_kind} near {anchor:?}, found {kinds:?}"
        );
        assert!(
            !window_contains(contents, idx, RADIUS, "toggle_value(")
                && !window_contains(contents, idx, RADIUS, "Checkbox::new(")
                && !window_contains(contents, idx, RADIUS, ".checkbox("),
            "{file}: a legacy toggle_value/Checkbox construct survives near {anchor:?}"
        );
    }

    // settings/plugins.rs `show_boolean`: the field label is a runtime
    // string, not a fixed `tr(...)` key, so it is anchored on its own
    // function signature.
    let plugins_settings = &cache["settings/plugins.rs"];
    let idx = line_index(plugins_settings, "fn show_boolean", "settings/plugins.rs");
    assert!(
        window_contains(plugins_settings, idx, 12, "switch("),
        "settings/plugins.rs: expected show_boolean to call switch(...)"
    );
    assert!(
        switch_kinds_in_window(plugins_settings, idx, 12).contains(&"Checkbox"),
        "settings/plugins.rs: expected show_boolean's switch to be SwitchKind::Checkbox"
    );

    // plugin_panels.rs `WidgetKind::Toggle` match arm: same reasoning —
    // the plugin-contributed label is a runtime string.
    let plugin_panels = &cache["plugin_panels.rs"];
    let idx = line_index(plugin_panels, "WidgetKind::Toggle =>", "plugin_panels.rs");
    assert!(
        window_contains(plugin_panels, idx, 6, "switch("),
        "plugin_panels.rs: expected the WidgetKind::Toggle arm to call switch(...)"
    );
    assert!(
        switch_kinds_in_window(plugin_panels, idx, 6).contains(&"Checkbox"),
        "plugin_panels.rs: expected the WidgetKind::Toggle arm's switch to be SwitchKind::Checkbox"
    );

    // No legacy boolean-control construct survives anywhere in src/** —
    // every one this feature owns was a named acceptance-set site above.
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
    let mut all = Vec::new();
    walk(src_root(), &mut all);

    for path in all {
        let contents = fs::read_to_string(&path).unwrap_or_default();
        for needle in ["toggle_value(", "Checkbox::new(", ".checkbox("] {
            assert!(
                !contents.contains(needle),
                "{}: unexpected legacy boolean construct {needle:?} outside the S5 acceptance set",
                path.display()
            );
        }
    }
}

/// **S6**: every FR-008b one-of-N selection control (data-model.md §9.3)
/// is left exactly as it was — still a `selectable_label`/`ComboBox`
/// option, never a `switch(...)` call. `library_view.rs`'s tab strip is
/// the one deliberate exception: 016-list-row-and-panel-components'
/// FR-013/contracts/tab-strip.md T1 restyles it from a filled
/// `selectable_label` into the dedicated `widgets::controls::tab`
/// underline widget — still not a boolean `switch(...)`, so `no_
/// library_tab_became_a_switch` below covers it instead of this list.
#[test]
fn no_selection_control_became_a_switch() {
    // (file, an anchor line unique to the site).
    let sites: &[(&str, &str)] = &[
        ("shell.rs", "self.section == section"),
        ("settings/mod.rs", "screen.category == category"),
        ("settings/mod.rs", "tr(descriptor.category.label_key())"),
        ("settings/mod.rs", "hit.path.clone()"),
        ("settings/audio.rs", "is_selected, device.name.clone()"),
        ("settings/plugins.rs", "view.name.clone()).clicked()"),
        ("settings/plugins.rs", "is_selected, option.label.clone()"),
        ("plugin_panels.rs", "is_selected, &item.label"),
        (
            "settings/language.rs",
            "ui.selectable_label(true, tr(\"language-english\"))",
        ),
    ];

    let mut cache = std::collections::HashMap::new();
    for (file, ..) in sites {
        cache.entry(*file).or_insert_with(|| read(file));
    }

    const RADIUS: usize = 5;

    for (file, anchor) in sites {
        let contents = &cache[*file];
        let idx = line_index(contents, anchor, file);
        assert!(
            window_contains(contents, idx, RADIUS, "selectable_label"),
            "{file}: expected {anchor:?} to still be a selectable_label (unconverted, FR-008b)"
        );
        assert!(
            !window_contains(contents, idx, RADIUS, "switch("),
            "{file}: {anchor:?} must not have become a switch(...) call (FR-008b)"
        );
    }
}

/// **S6 exception** (016-list-row-and-panel-components, FR-013,
/// contracts/tab-strip.md T1): the Library tab strip is a one-of-N
/// selection control that intentionally left `selectable_label` behind —
/// for a dedicated navigation-underline widget, not a boolean `switch`.
/// Pins the same negative half of S6 (never `switch(...)`) plus the
/// positive fact that replaces it (`widgets::controls::tab`).
#[test]
fn no_library_tab_became_a_switch() {
    let contents = read("library_view.rs");
    let idx = line_index(&contents, "state.tab == tab", "library_view.rs");
    const RADIUS: usize = 5;
    assert!(
        window_contains(&contents, idx, RADIUS, "tab_widget("),
        "library_view.rs: expected \"state.tab == tab\" to route through the \
         tab widget (016 FR-013)"
    );
    assert!(
        !window_contains(&contents, idx, RADIUS, "switch("),
        "library_view.rs: the tab strip must not have become a switch(...) call (FR-008b)"
    );
}
