// SPDX-License-Identifier: MIT OR Apache-2.0

//! `AudioSettings`: the persisted subset of settings (data-model.md §5.1)
//! plus the wire format it round-trips through `settings.toml`
//! (contracts/settings-file.md).
//!
//! The domain type (`AudioSettings`) is built from engine value types,
//! which already clamp every numeric field on construction; the wire type
//! (`RawSettings`) mirrors the TOML schema verbatim (plain strings/numbers)
//! so `serde`/`toml` can read a file a user hand-edited, however malformed,
//! without failing the whole parse. `settings/store.rs` turns one into the
//! other and reports which fields (if any) fell back to a default.

use modplayer_engine::{BufferPreset, CeilingDb, DeviceId, SafeVolume, Theme, VolumePercent};
use serde::{Deserialize, Serialize};

/// Current on-disk schema version (contracts/settings-file.md).
pub const SCHEMA_VERSION: u32 = 1;

/// The persisted audio + appearance settings (DM-20 subset).
#[derive(Debug, Clone, PartialEq)]
pub struct AudioSettings {
    pub output_device: Option<DeviceId>,
    pub device_confirmed: bool,
    pub buffer_preset: BufferPreset,
    pub limiter_ceiling_db: CeilingDb,
    pub safe_volume: SafeVolume,
    pub master_volume: VolumePercent,
    pub theme: Theme,
    pub schema_version: u32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            output_device: None,
            device_confirmed: false,
            buffer_preset: BufferPreset::default(),
            limiter_ceiling_db: CeilingDb::default(),
            safe_volume: SafeVolume::default(),
            master_volume: VolumePercent::new(80),
            theme: Theme::default(),
            schema_version: SCHEMA_VERSION,
        }
    }
}

/// A field that fell back to its default because the file held an
/// unrecognised enum string (contracts/settings-file.md: "Unknown enum
/// string").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidField {
    BufferPreset,
    Theme,
}

impl InvalidField {
    /// The TOML key this field corresponds to, for the warning message.
    pub fn field_name(self) -> &'static str {
        match self {
            InvalidField::BufferPreset => "audio.buffer_preset",
            InvalidField::Theme => "appearance.theme",
        }
    }
}

/// The literal `settings.toml` shape: every field is the raw wire type
/// (string/bool/number), so a malformed enum or out-of-range number
/// deserializes successfully and is validated afterwards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSettings {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub audio: RawAudio,
    #[serde(default)]
    pub appearance: RawAppearance,
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for RawSettings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            audio: RawAudio::default(),
            appearance: RawAppearance::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawAudio {
    #[serde(default)]
    pub output_device: Option<String>,
    #[serde(default)]
    pub device_confirmed: bool,
    #[serde(default = "default_buffer_preset")]
    pub buffer_preset: String,
    #[serde(default = "default_ceiling")]
    pub limiter_ceiling_db: f64,
    #[serde(default = "default_master_volume")]
    pub master_volume: i64,
    #[serde(default)]
    pub safe_volume: RawSafeVolume,
}

impl Default for RawAudio {
    fn default() -> Self {
        Self {
            output_device: None,
            device_confirmed: false,
            buffer_preset: default_buffer_preset(),
            limiter_ceiling_db: default_ceiling(),
            master_volume: default_master_volume(),
            safe_volume: RawSafeVolume::default(),
        }
    }
}

fn default_buffer_preset() -> String {
    "balanced".to_string()
}

fn default_ceiling() -> f64 {
    -1.0
}

fn default_master_volume() -> i64 {
    80
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSafeVolume {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_cap")]
    pub cap: i64,
}

impl Default for RawSafeVolume {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            cap: default_cap(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_cap() -> i64 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawAppearance {
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for RawAppearance {
    fn default() -> Self {
        Self {
            theme: default_theme(),
        }
    }
}

fn default_theme() -> String {
    "system".to_string()
}

impl RawSettings {
    /// Serialize `settings` to its wire form.
    pub fn from_settings(settings: &AudioSettings) -> Self {
        Self {
            schema_version: settings.schema_version,
            audio: RawAudio {
                output_device: settings
                    .output_device
                    .as_ref()
                    .map(|id| id.as_str().to_string()),
                device_confirmed: settings.device_confirmed,
                buffer_preset: match settings.buffer_preset {
                    BufferPreset::Performance => "performance",
                    BufferPreset::Balanced => "balanced",
                    BufferPreset::Safe => "safe",
                }
                .to_string(),
                limiter_ceiling_db: f64::from(settings.limiter_ceiling_db.db()),
                master_volume: i64::from(settings.master_volume.value()),
                safe_volume: RawSafeVolume {
                    enabled: settings.safe_volume.enabled,
                    cap: i64::from(settings.safe_volume.cap.value()),
                },
            },
            appearance: RawAppearance {
                theme: match settings.theme {
                    Theme::System => "system",
                    Theme::Light => "light",
                    Theme::Dark => "dark",
                }
                .to_string(),
            },
        }
    }

    /// Validate and clamp into the domain type. Out-of-range numbers are
    /// clamped silently (the newtype constructors do this); an
    /// unrecognised enum string falls back to its field default and is
    /// reported in the returned list (contracts/settings-file.md).
    pub fn into_settings(self) -> (AudioSettings, Vec<InvalidField>) {
        let mut invalid = Vec::new();

        let buffer_preset = match self.audio.buffer_preset.as_str() {
            "performance" => BufferPreset::Performance,
            "balanced" => BufferPreset::Balanced,
            "safe" => BufferPreset::Safe,
            _ => {
                invalid.push(InvalidField::BufferPreset);
                BufferPreset::default()
            }
        };

        let theme = match self.appearance.theme.as_str() {
            "system" => Theme::System,
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            _ => {
                invalid.push(InvalidField::Theme);
                Theme::default()
            }
        };

        let output_device = self.audio.output_device.and_then(DeviceId::new);

        let settings = AudioSettings {
            output_device,
            device_confirmed: self.audio.device_confirmed,
            buffer_preset,
            limiter_ceiling_db: CeilingDb::from_f64(self.audio.limiter_ceiling_db),
            safe_volume: SafeVolume {
                enabled: self.audio.safe_volume.enabled,
                cap: VolumePercent::from_i64(self.audio.safe_volume.cap),
            },
            master_volume: VolumePercent::from_i64(self.audio.master_volume),
            theme,
            schema_version: self.schema_version,
        };

        (settings, invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_match_contract() {
        let settings = AudioSettings::default();
        assert_eq!(settings.output_device, None);
        assert!(!settings.device_confirmed);
        assert_eq!(settings.buffer_preset, BufferPreset::Balanced);
        assert_eq!(settings.limiter_ceiling_db.db(), -1.0);
        assert_eq!(
            settings.safe_volume,
            SafeVolume {
                enabled: true,
                cap: VolumePercent::new(50)
            }
        );
        assert_eq!(settings.master_volume.value(), 80);
        assert_eq!(settings.theme, Theme::System);
        assert_eq!(settings.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn round_trips_through_raw() {
        let settings = AudioSettings::default();
        let raw = RawSettings::from_settings(&settings);
        let (round_tripped, invalid) = raw.into_settings();
        assert!(invalid.is_empty());
        assert_eq!(round_tripped, settings);
    }

    #[test]
    fn out_of_range_numbers_clamp_silently() {
        let mut raw = RawSettings::default();
        raw.audio.limiter_ceiling_db = 3.0;
        raw.audio.master_volume = 250;
        raw.audio.safe_volume.cap = -5;
        let (settings, invalid) = raw.into_settings();
        assert!(invalid.is_empty());
        assert_eq!(settings.limiter_ceiling_db.db(), -0.1);
        assert_eq!(settings.master_volume.value(), 100);
        assert_eq!(settings.safe_volume.cap.value(), 0);
    }

    #[test]
    fn unknown_enum_strings_fall_back_and_are_reported() {
        let mut raw = RawSettings::default();
        raw.audio.buffer_preset = "turbo".to_string();
        raw.appearance.theme = "midnight".to_string();
        let (settings, invalid) = raw.into_settings();
        assert_eq!(settings.buffer_preset, BufferPreset::default());
        assert_eq!(settings.theme, Theme::default());
        assert_eq!(
            invalid,
            vec![InvalidField::BufferPreset, InvalidField::Theme]
        );
    }
}
