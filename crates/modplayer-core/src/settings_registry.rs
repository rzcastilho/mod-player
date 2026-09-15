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
/// other category (Account, Playback, Controls, Plugins, Offline, Privacy &
/// diagnostics, About) renders placeholder content only and so has no
/// entries here.
pub const DESCRIPTORS: &[SettingDescriptor] = &[
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
        category: SettingsCategory::Appearance,
        id: "appearance.theme",
        title_key: "setting-theme",
        description_key: "setting-theme-desc",
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

#[cfg(test)]
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
        for category in [
            SettingsCategory::Account,
            SettingsCategory::Playback,
            SettingsCategory::Controls,
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
}
