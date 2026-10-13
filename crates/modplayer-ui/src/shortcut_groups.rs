// SPDX-License-Identifier: MIT OR Apache-2.0

//! The grouped, display-ready view of every action and its bindings,
//! shared by Settings › Controls and the shortcut sheet
//! (029-keyboard-shortcut-reference, data-model §3, contracts C2/C4).
//!
//! Moved out of `settings/controls.rs` (formerly `RowSnapshot`): a
//! [`ShortcutGroup`] list is snapshotted from the registry before any
//! widget can mutate it, laid out with [`column_break`], and each row's
//! content is painted by [`paint_name`] / [`paint_bindings`].

use egui::{Frame, Label, Margin, Response, RichText, Sense, Ui};
use modplayer_core::actions::{
    ActionCategory, ActionId, ActionKind, ActionRegistry, Chord, Platform, RowCategory, RowLabel,
};
use modplayer_core::{tr, tr_args};

use crate::key_label::KeyLabel;

/// Logical width at and above which groups are laid out in two columns.
pub const TWO_COLUMN_MIN_WIDTH: f32 = 720.0;

/// Which heading group a row falls under (011-plugin-ui-contributions,
/// contracts/action-registry-plugins.md G15): a host category, or a
/// named owning plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowGroup {
    Host(ActionCategory),
    Plugin(String),
}

/// One action's data, snapshotted from the registry. Widened from the
/// 007 `HostAction`-only shape so a plugin's own actions render through
/// the same row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutRow {
    pub id: ActionId,
    pub label: String,
    pub kind: ActionKind,
    pub enabled: bool,
    pub bindings: Vec<Chord>,
    /// The subset of `bindings` currently flagged as conflicting.
    pub conflicts: Vec<Chord>,
    pub group: RowGroup,
}

/// A heading and its rows, in registry order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutGroup {
    pub group: RowGroup,
    /// The resolved heading text.
    pub heading: String,
    pub rows: Vec<ShortcutRow>,
}

impl ShortcutGroup {
    /// The text a filter compares against: the category label for a host
    /// group, the plugin's display name for a plugin group.
    pub fn match_name(&self) -> String {
        match &self.group {
            RowGroup::Host(_) => self.heading.clone(),
            RowGroup::Plugin(name) => name.clone(),
        }
    }
}

/// Snapshot every row of `registry`: host groups in catalog
/// ([`ActionCategory::ALL`]) order, then plugin groups in the registry's
/// own order (already sorted by plugin display name). Empty groups are
/// dropped.
///
/// ```
/// use modplayer_core::actions::{ActionRegistry, KeymapOverrides};
/// use modplayer_ui::shortcut_groups::snapshot;
///
/// let registry = ActionRegistry::new(KeymapOverrides::default());
/// let groups = snapshot(&registry);
/// assert!(!groups.is_empty());
/// assert!(groups.iter().all(|g| !g.rows.is_empty()));
/// ```
pub fn snapshot(registry: &ActionRegistry) -> Vec<ShortcutGroup> {
    let rows: Vec<ShortcutRow> = registry
        .rows()
        .map(|row| {
            let label = match row.label {
                RowLabel::Fluent(key) => tr(key),
                RowLabel::Literal(s) => s,
            };
            let group = match row.category {
                RowCategory::Host(category) => RowGroup::Host(category),
                RowCategory::Plugin { name } => RowGroup::Plugin(name),
            };
            ShortcutRow {
                id: row.id,
                label,
                kind: row.kind,
                enabled: row.enabled,
                bindings: row.bindings.to_vec(),
                conflicts: row.conflicts,
                group,
            }
        })
        .collect();

    let mut groups = Vec::new();
    for category in ActionCategory::ALL {
        let group_rows: Vec<ShortcutRow> = rows
            .iter()
            .filter(|row| row.group == RowGroup::Host(category))
            .cloned()
            .collect();
        if !group_rows.is_empty() {
            groups.push(ShortcutGroup {
                group: RowGroup::Host(category),
                heading: tr(category.label_key()),
                rows: group_rows,
            });
        }
    }

    let mut seen: Vec<&str> = Vec::new();
    for row in &rows {
        let RowGroup::Plugin(name) = &row.group else {
            continue;
        };
        if seen.contains(&name.as_str()) {
            continue;
        }
        seen.push(name.as_str());
        let group_rows: Vec<ShortcutRow> = rows
            .iter()
            .filter(|r| matches!(&r.group, RowGroup::Plugin(n) if n == name))
            .cloned()
            .collect();
        groups.push(ShortcutGroup {
            group: RowGroup::Plugin(name.clone()),
            heading: tr_args("controls-plugin-group", &[("plugin", name.clone())]),
            rows: group_rows,
        });
    }
    groups
}

/// A row that survived the filter, with per-binding key-match flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilteredRow<'a> {
    pub row: &'a ShortcutRow,
    /// Parallel to `row.bindings`: true when that binding's key alias
    /// equals the query (drawn outlined, contracts C16).
    pub key_hits: Vec<bool>,
}

/// Apply the controls filter (contracts C14/C15). `query` is trimmed and
/// lower-cased here. A row stays when the group heading or the action
/// label contains the query (substring), or when a binding's
/// [`KeyLabel`] alias equals it exactly. Groups left empty are dropped.
/// An empty query keeps everything with no key hits.
///
/// ```
/// use modplayer_core::actions::{ActionRegistry, KeymapOverrides, Platform};
/// use modplayer_ui::shortcut_groups::{filter_groups, snapshot};
///
/// let registry = ActionRegistry::new(KeymapOverrides::default());
/// let groups = snapshot(&registry);
/// assert!(filter_groups(&groups, "", Platform::Other).len() == groups.len());
/// assert!(filter_groups(&groups, "zzzz-no-such", Platform::Other).is_empty());
/// ```
pub fn filter_groups<'a>(
    groups: &'a [ShortcutGroup],
    query: &str,
    platform: Platform,
) -> Vec<(&'a ShortcutGroup, Vec<FilteredRow<'a>>)> {
    let query = query.trim().to_lowercase();
    groups
        .iter()
        .filter_map(|group| {
            let heading_matches = group.match_name().to_lowercase().contains(&query);
            let rows: Vec<FilteredRow<'a>> = group
                .rows
                .iter()
                .filter_map(|row| {
                    let key_hits: Vec<bool> = row
                        .bindings
                        .iter()
                        .map(|&chord| KeyLabel::new(chord, platform).matches_key(&query))
                        .collect();
                    let keep = heading_matches
                        || row.label.to_lowercase().contains(&query)
                        || key_hits.iter().any(|&hit| hit);
                    keep.then_some(FilteredRow { row, key_hits })
                })
                .collect();
            (!rows.is_empty()).then_some((group, rows))
        })
        .collect()
}

/// Whether `available_width` is wide enough for two columns.
///
/// ```
/// use modplayer_ui::shortcut_groups::use_two_columns;
/// assert!(use_two_columns(720.0));
/// assert!(!use_two_columns(719.9));
/// ```
pub fn use_two_columns(available_width: f32) -> bool {
    available_width >= TWO_COLUMN_MIN_WIDTH
}

/// Where to split groups into two columns. `weights` is `1 + rows` per
/// group; the result `k ∈ 0..=weights.len()` puts the first `k` groups in
/// the left column, minimising the weight imbalance. Ties pick the larger
/// `k`, so the left column fills first and is never empty while there is
/// a group (a lone filtered group sits on the left, not the right).
///
/// ```
/// use modplayer_ui::shortcut_groups::column_break;
/// assert_eq!(column_break(&[]), 0);
/// assert_eq!(column_break(&[5]), 1);
/// assert_eq!(column_break(&[4, 4, 4, 4]), 2);
/// assert_eq!(column_break(&[10, 1, 1]), 1);
/// ```
pub fn column_break(weights: &[usize]) -> usize {
    let total: usize = weights.iter().sum();
    let mut best_k = 0;
    let mut best_imbalance = total;
    let mut left = 0;
    for (i, weight) in weights.iter().enumerate() {
        left += weight;
        let imbalance = left.abs_diff(total - left);
        if imbalance <= best_imbalance {
            best_imbalance = imbalance;
            best_k = i + 1;
        }
    }
    best_k
}

/// The `Platform` whose key labels `ctx` should show.
pub fn platform_of(ctx: &egui::Context) -> Platform {
    if ctx.os().is_mac() {
        Platform::Mac
    } else {
        Platform::Other
    }
}

/// Paint a row's name, with "(inactive)" and de-emphasis when the action
/// is disabled (contracts/action-registry-plugins.md G15).
pub fn paint_name(ui: &mut Ui, row: &ShortcutRow) -> Response {
    paint_name_with_sense(ui, row, Sense::hover())
}

/// [`paint_name`] with an explicit `sense` — the controls page makes the
/// name its row's focusable element (contracts C11).
pub fn paint_name_with_sense(ui: &mut Ui, row: &ShortcutRow, sense: Sense) -> Response {
    let label_text = if row.enabled {
        row.label.clone()
    } else {
        format!("{} {}", row.label, tr("controls-inactive"))
    };
    let text = RichText::new(label_text);
    let text = if row.enabled {
        text
    } else {
        text.color(ui.visuals().weak_text_color())
    };
    ui.add(Label::new(text).sense(sense))
}

/// Paint one binding chip: a bordered label showing
/// [`KeyLabel::visible`] only, whose accessible name is
/// `controls-binding-chip { $binding = spoken }` (contracts C8).
pub fn paint_chip(ui: &mut Ui, key_label: &KeyLabel, highlighted: bool) -> Response {
    let name = tr_args(
        "controls-binding-chip",
        &[("binding", key_label.spoken.clone())],
    );
    let stroke = if highlighted {
        crate::theme::controls::focus_ring(crate::theme::roles(ui.visuals()))
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke
    };
    let inner = Frame::new()
        .stroke(stroke)
        .corner_radius(3)
        .inner_margin(Margin::symmetric(6, 0))
        .show(ui, |ui| ui.label(key_label.visible.clone()));
    let label_id = inner.inner.id;
    ui.ctx().accesskit_node_builder(label_id, |b| {
        b.set_label(name.clone());
    });
    inner.response
}

/// Paint a row's bindings: one chip per binding ([`KeyLabel`]), a `⚠`
/// note for each `(chord, message)` in `conflict_notes`, and whatever
/// `chip_extra` adds after the chip (the controls page's `×`), or
/// "Not bound" when there are none. Chips sit directly in the caller's
/// layout, so a wrapping row wraps them whole, never eliding text.
pub fn paint_bindings(
    ui: &mut Ui,
    row: &ShortcutRow,
    platform: Platform,
    conflict_notes: &[(Chord, String)],
    key_hits: &[bool],
    chip_extra: &mut dyn FnMut(&mut Ui, Chord),
) {
    if row.bindings.is_empty() {
        ui.label(tr("shortcuts-not-bound"));
        return;
    }
    for (i, &chord) in row.bindings.iter().enumerate() {
        let key_label = KeyLabel::new(chord, platform);
        let hit = key_hits.get(i).copied().unwrap_or(false);
        paint_chip(ui, &key_label, hit);
        if let Some((_, message)) = conflict_notes.iter().find(|(c, _)| *c == chord) {
            ui.label(format!("\u{26a0} {message}"));
        }
        chip_extra(ui, chord);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modplayer_core::actions::KeymapOverrides;
    use proptest::prelude::*;

    fn imbalance(weights: &[usize], k: usize) -> usize {
        let left: usize = weights[..k].iter().sum();
        let total: usize = weights.iter().sum();
        left.abs_diff(total - left)
    }

    #[test]
    fn column_break_edge_cases() {
        assert_eq!(column_break(&[]), 0);
        assert_eq!(column_break(&[5]), 1);
        assert_eq!(column_break(&[3, 3]), 1);
        assert_eq!(column_break(&[1, 1, 1, 1, 10]), 4);
    }

    #[test]
    fn two_column_threshold_is_inclusive() {
        assert!(use_two_columns(TWO_COLUMN_MIN_WIDTH));
        assert!(!use_two_columns(TWO_COLUMN_MIN_WIDTH - 0.1));
    }

    #[test]
    fn snapshot_follows_catalog_order_and_drops_nothing() {
        let registry = ActionRegistry::new(KeymapOverrides::default());
        let groups = snapshot(&registry);
        let categories: Vec<ActionCategory> = groups
            .iter()
            .filter_map(|g| match g.group {
                RowGroup::Host(c) => Some(c),
                RowGroup::Plugin(_) => None,
            })
            .collect();
        assert_eq!(categories, ActionCategory::ALL.to_vec());
        let rows: usize = groups.iter().map(|g| g.rows.len()).sum();
        assert_eq!(rows, registry.rows().count());
        for group in &groups {
            assert_eq!(
                group.heading,
                tr(match group.group {
                    RowGroup::Host(c) => c.label_key(),
                    RowGroup::Plugin(_) => unreachable!(),
                })
            );
        }
    }

    proptest! {
        /// C4: the chosen boundary is optimal, and the largest optimal one
        /// (so the left column is never empty while there is a group).
        #[test]
        fn column_break_is_optimal_boundary(weights in proptest::collection::vec(1usize..40, 0..20)) {
            let k = column_break(&weights);
            prop_assert!(k <= weights.len());
            let best = imbalance(&weights, k);
            for other in 0..=weights.len() {
                prop_assert!(imbalance(&weights, other) >= best);
            }
            for larger in k + 1..=weights.len() {
                prop_assert!(imbalance(&weights, larger) > best);
            }
            prop_assert!(weights.is_empty() || k >= 1);
        }
    }
}
