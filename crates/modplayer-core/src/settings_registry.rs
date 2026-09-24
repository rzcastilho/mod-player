// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Settings registry and search (data-model.md §5.4,
//! contracts/ui-surface.md "Settings"): the eleven categories in fixed
//! display order, and the `SettingDescriptor`s for this slice's working
//! settings. `search(query)` is a case-insensitive substring match over
//! each descriptor's resolved title + description; categories with only
//! placeholder content (everything but Audio/Appearance/Language/Developer
//! this phase) simply have no entries in `DESCRIPTORS`, so they
//! automatically contribute nothing to search (SC-008).

use crate::i18n::tr;
use crate::plugins::{PluginId, PluginSettingsView};

/// The eleven Settings categories, in fixed display order
/// (contracts/ui-surface.md, data-model.md §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
    Account,
    Audio,
    Playback,
    Controls,
    Plugins,
    Offline,
    Appearance,
    Language,
    Developer,
    PrivacyDiagnostics,
    About,
}

impl SettingsCategory {
    /// Every category, in fixed order — the category list's iteration
    /// source.
    pub const ALL: [SettingsCategory; 11] = [
        SettingsCategory::Account,
        SettingsCategory::Audio,
        SettingsCategory::Playback,
        SettingsCategory::Controls,
        SettingsCategory::Plugins,
        SettingsCategory::Offline,
        SettingsCategory::Appearance,
        SettingsCategory::Language,
        SettingsCategory::Developer,
        SettingsCategory::PrivacyDiagnostics,
        SettingsCategory::About,
    ];

    /// The Fluent key for this category's display name
    /// (`settings-cat-account` … `settings-cat-about`).
    pub fn label_key(self) -> &'static str {
        match self {
            SettingsCategory::Account => "settings-cat-account",
            SettingsCategory::Audio => "settings-cat-audio",
            SettingsCategory::Playback => "settings-cat-playback",
            SettingsCategory::Controls => "settings-cat-controls",
            SettingsCategory::Plugins => "settings-cat-plugins",
            SettingsCategory::Offline => "settings-cat-offline",
            SettingsCategory::Appearance => "settings-cat-appearance",
            SettingsCategory::Language => "settings-cat-language",
            SettingsCategory::Developer => "settings-cat-developer",
            SettingsCategory::PrivacyDiagnostics => "settings-cat-privacy-diagnostics",
            SettingsCategory::About => "settings-cat-about",
        }
    }
}

/// One searchable setting (data-model.md §5.4).
#[derive(Debug, Clone, Copy)]
pub struct SettingDescriptor {
    pub category: SettingsCategory,
    /// Stable id, e.g. `"audio.limiter_ceiling"` (contracts/ui-surface.md).
    pub id: &'static str,
    pub title_key: &'static str,
    pub description_key: &'static str,
}

/// The working settings' descriptors (contracts/ui-surface.md "Working
/// settings"), in the order they appear on their category screen. Every
/// other category (Plugins, Offline, Privacy & diagnostics) renders
/// placeholder content only and so has no entries here — Controls gained
/// its one descriptor in 007-keyboard-actions-and-shortcuts
/// (contracts/ui-actions.md §4: the shortcut map itself is one
/// `SettingDescriptor`, not one per action). About is
/// descriptor-free by design even though it has real content
/// (`about-product`/`about-version`/… are read-only, not searchable
/// settings, plan.md "Project Structure"). Playback's screen itself lands
/// with US1 (T062); the `device_name` descriptor is registered now so it
/// is searchable from the start (003-streaming-playback-and-queue
/// contracts/transport-and-queue.md §5).
pub const DESCRIPTORS: &[SettingDescriptor] = &[
    SettingDescriptor {
        category: SettingsCategory::Account,
        id: "account.recheck_subscription",
        title_key: "account-recheck",
        description_key: "account-recheck-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Account,
        id: "account.sign_out",
        title_key: "account-sign-out",
        description_key: "account-sign-out-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Audio,
        id: "audio.output_device",
        title_key: "setting-output-device",
        description_key: "setting-output-device-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Audio,
        id: "audio.buffer_preset",
        title_key: "setting-buffer-preset",
        description_key: "setting-buffer-preset-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Audio,
        id: "audio.limiter_ceiling",
        title_key: "setting-limiter-ceiling",
        description_key: "setting-limiter-ceiling-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Audio,
        id: "audio.safe_volume_enabled",
        title_key: "setting-safe-volume",
        description_key: "setting-safe-volume-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Audio,
        id: "audio.safe_volume_cap",
        title_key: "setting-safe-volume-cap",
        description_key: "setting-safe-volume-cap-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Audio,
        id: "audio.test_output_device",
        title_key: "setting-test-output-device",
        description_key: "setting-test-output-device-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Playback,
        id: "playback.device_name",
        title_key: "setting-device-name",
        description_key: "setting-device-name-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Playback,
        id: "markers.nudge_step_ms",
        title_key: "setting-nudge-step",
        description_key: "setting-nudge-step-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Controls,
        id: "controls.keybindings",
        title_key: "setting-keybindings",
        description_key: "setting-keybindings-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Appearance,
        id: "appearance.theme",
        title_key: "setting-theme",
        description_key: "setting-theme-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Appearance,
        id: "appearance.high_contrast",
        title_key: "setting-high-contrast",
        description_key: "setting-high-contrast-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Language,
        id: "language.locale",
        title_key: "setting-locale",
        description_key: "setting-locale-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Developer,
        id: "developer.buffer_frames",
        title_key: "setting-buffer-frames",
        description_key: "setting-buffer-frames-desc",
    },
    SettingDescriptor {
        category: SettingsCategory::Developer,
        id: "developer.raise_notification",
        title_key: "setting-raise-notification",
        description_key: "setting-raise-notification-desc",
    },
];

/// Case-insensitive substring search over each descriptor's resolved title +
/// description (SC-008: well under the 50 ms per-keystroke budget, target
/// <1 ms). An empty query matches nothing — there is no "browse all" mode
/// via search, only the category list.
pub fn search(query: &str) -> Vec<&'static SettingDescriptor> {
    if query.is_empty() {
        return Vec::new();
    }
    let query = query.to_lowercase();
    DESCRIPTORS
        .iter()
        .filter(|descriptor| {
            tr(descriptor.title_key).to_lowercase().contains(&query)
                || tr(descriptor.description_key)
                    .to_lowercase()
                    .contains(&query)
        })
        .collect()
}

/// One search hit inside a plugin's own settings page (US4 T100, research
/// R13, contracts/overlays-settings-notify.md S6): the dynamic complement
/// to [`DESCRIPTORS`]/[`search`] — plugin fields cannot join that
/// `&'static` list (their label/description are plugin-declared strings,
/// resolved at registration, not `Fluent` keys), so they get their own
/// search path that the Settings screen's own search box (`modplayer-ui`)
/// concatenates with `search`'s own results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSettingHit {
    pub plugin: PluginId,
    /// This field's own id within its page — `settings/plugins.rs`'s
    /// "open this field" action's target.
    pub field_id: String,
    /// "Plugins › <plugin name> › <field label>" (S6), ready to render as
    /// a search result row exactly like [`search`]'s own `"{category} ›
    /// {title}"` results.
    pub path: String,
}

/// S6: case-insensitive substring match over every visible plugin settings
/// page's field label + description (mirrors [`search`]'s own matching
/// rule) — `views` is the caller's already-built [`PluginSettingsView`]
/// list (`PlaybackController::plugin_settings_views()`), so this function
/// itself touches no registry or controller state. An empty query matches
/// nothing, exactly like [`search`].
#[must_use]
pub fn search_plugin_settings(query: &str, views: &[PluginSettingsView]) -> Vec<PluginSettingHit> {
    if query.is_empty() {
        return Vec::new();
    }
    let query = query.to_lowercase();
    let mut hits = Vec::new();
    for view in views {
        for field in &view.page.fields {
            let label_matches = field.label.to_lowercase().contains(&query);
            let description_matches = field
                .description
                .as_deref()
                .is_some_and(|d| d.to_lowercase().contains(&query));
            if label_matches || description_matches {
                hits.push(PluginSettingHit {
                    plugin: view.plugin,
                    field_id: field.id.as_str().to_string(),
                    path: format!(
                        "{} › {} › {}",
                        tr(SettingsCategory::Plugins.label_key()),
                        view.name,
                        field.label
                    ),
                });
            }
        }
    }
    hits
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn categories_are_in_the_contract_fixed_order() {
        assert_eq!(
            SettingsCategory::ALL.map(|c| c.label_key()),
            [
                "settings-cat-account",
                "settings-cat-audio",
                "settings-cat-playback",
                "settings-cat-controls",
                "settings-cat-plugins",
                "settings-cat-offline",
                "settings-cat-appearance",
                "settings-cat-language",
                "settings-cat-developer",
                "settings-cat-privacy-diagnostics",
                "settings-cat-about",
            ]
        );
    }

    #[test]
    fn every_descriptor_id_is_unique() {
        let mut ids: Vec<&str> = DESCRIPTORS.iter().map(|d| d.id).collect();
        let count_before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count_before, "duplicate descriptor id");
    }

    #[test]
    fn placeholder_only_categories_contribute_no_descriptors() {
        // 007-keyboard-actions-and-shortcuts, T069: Controls dropped from
        // this list — it gained the `controls.keybindings` descriptor
        // above once Settings › Controls became a real screen.
        for category in [
            SettingsCategory::Plugins,
            SettingsCategory::Offline,
            SettingsCategory::PrivacyDiagnostics,
            SettingsCategory::About,
        ] {
            assert!(
                !DESCRIPTORS.iter().any(|d| d.category == category),
                "{category:?} should contribute no descriptors"
            );
        }
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(search("").is_empty());
    }

    /// S6: a plugin settings field's own label is searchable, and the hit
    /// carries the "Plugins › <plugin name> › <label>" path.
    #[test]
    fn search_plugin_settings_finds_field_by_label() {
        use crate::plugins::ui::settings::SettingsPage;
        use modplayer_capability_gateway::ui::{FieldKind, SettingsField, UiId};

        let view = PluginSettingsView {
            plugin: PluginId(0),
            name: "UI settings fixture".to_string(),
            page: SettingsPage {
                fields: vec![SettingsField {
                    id: UiId::parse("shift").unwrap_or_else(|| unreachable!()),
                    kind: FieldKind::Number {
                        min: -12.0,
                        max: 12.0,
                        step: 1.0,
                    },
                    label: "Semitone shift".to_string(),
                    description: None,
                    default: serde_json::json!(0.0),
                }],
                values: std::collections::BTreeMap::new(),
            },
        };
        let hits = search_plugin_settings("semitone", &[view]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].field_id, "shift");
        assert_eq!(
            hits[0].path,
            "Plugins › UI settings fixture › Semitone shift"
        );
    }
}
