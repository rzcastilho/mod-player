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
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Current on-disk schema version (contracts/settings-file.md).
pub const SCHEMA_VERSION: u32 = 1;

/// Device-scoped record of the user's first-launch disclosure
/// acknowledgement (002-first-launch-and-sign-in data-model.md §1.1,
/// DM-27). `None` (or an absent `[disclosure]` section on disk) means
/// never acknowledged; sign-out and revocation never touch it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisclosureAcknowledgement {
    /// Equals `modplayer_account::DISCLOSURE_BUNDLE_VERSION` at the time
    /// of acknowledgement. The welcome screen re-shows whenever this
    /// stops matching the current bundle version.
    pub acknowledged_version: u32,
    /// Set once per acknowledgement; informational only.
    pub acknowledged_at: OffsetDateTime,
}

impl DisclosureAcknowledgement {
    /// Build an acknowledgement of `version`, timestamped now (UTC). The
    /// welcome screen's acknowledge action (`modplayer-ui::welcome`) is the
    /// only caller — kept here rather than in `modplayer-ui` so that crate
    /// does not need its own `time` dependency just to stamp one field.
    pub fn now(version: u32) -> Self {
        Self {
            acknowledged_version: version,
            acknowledged_at: OffsetDateTime::now_utc(),
        }
    }
}

/// The persisted audio + appearance settings (DM-20 subset), plus the
/// device-scoped disclosure acknowledgement (DM-27).
#[derive(Debug, Clone, PartialEq)]
pub struct AudioSettings {
    pub output_device: Option<DeviceId>,
    pub device_confirmed: bool,
    pub buffer_preset: BufferPreset,
    pub limiter_ceiling_db: CeilingDb,
    pub safe_volume: SafeVolume,
    pub master_volume: VolumePercent,
    pub theme: Theme,
    pub disclosure: Option<DisclosureAcknowledgement>,
    /// `[playback] device_name` (FR-001); `None` = use the default name.
    pub device_name: Option<DeviceName>,
    /// `[playback] connect_device_id` (research R8); `None` until first
    /// generated.
    pub connect_device_id: Option<String>,
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
            disclosure: None,
            device_name: None,
            connect_device_id: None,
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
    /// `[playback] device_name` was longer than 64 characters after trim
    /// (contracts/transport-and-queue.md §5).
    DeviceName,
}

impl InvalidField {
    /// The TOML key this field corresponds to, for the warning message.
    pub fn field_name(self) -> &'static str {
        match self {
            InvalidField::BufferPreset => "audio.buffer_preset",
            InvalidField::Theme => "appearance.theme",
            InvalidField::DeviceName => "playback.device_name",
        }
    }
}

/// A user-chosen Connect device name (data-model.md §4, FR-001): trimmed,
/// 1-64 characters. `DeviceName::parse` is the single validating
/// constructor; `None` (from an empty/absent input) means "use the default
/// name" (`"ModPlayer on <hostname>"` / `"ModPlayer"`), computed by the
/// caller (`modplayer-audio-source-connect`, which knows the hostname).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceName(String);

/// `DeviceName::parse` rejected its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceNameError {
    /// Longer than 64 characters after trimming.
    TooLong,
}

impl DeviceName {
    /// The maximum length, in characters, after trimming.
    pub const MAX_LEN: usize = 64;

    /// Trim `input`; empty trims to `Ok(None)` (restore the default);
    /// longer than [`Self::MAX_LEN`] characters is `Err(TooLong)`.
    pub fn parse(input: &str) -> Result<Option<Self>, DeviceNameError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if trimmed.chars().count() > Self::MAX_LEN {
            return Err(DeviceNameError::TooLong);
        }
        Ok(Some(Self(trimmed.to_string())))
    }

    /// The validated, trimmed name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Generate a fresh 32-hex-character Connect device id (data-model.md §4),
/// used the first time a device id is needed; persisted afterward so other
/// controllers see a stable device across launches (research R8).
pub fn generate_connect_device_id() -> String {
    let mut bytes = [0u8; 16];
    // A failed read (vanishingly rare) leaves `bytes` zeroed — still a
    // valid-shaped (if predictable) id rather than a panic; the caller can
    // regenerate on the next launch if it observes an all-zero id.
    let _ = getrandom::fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn is_valid_connect_device_id(id: &str) -> bool {
    id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit())
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
    #[serde(default)]
    pub disclosure: RawDisclosure,
    #[serde(default)]
    pub playback: RawPlayback,
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
            disclosure: RawDisclosure::default(),
            playback: RawPlayback::default(),
        }
    }
}

/// The `[playback]` section (contracts/transport-and-queue.md §5): an
/// optional table so older files (with no such section) load unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawPlayback {
    #[serde(default)]
    pub device_name: Option<String>,
    #[serde(default)]
    pub connect_device_id: Option<String>,
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

/// The `[disclosure]` section (002-first-launch-and-sign-in
/// contracts/account-session.md). `acknowledged_version = 0`/absent means
/// never acknowledged; `acknowledged_at` is only meaningful once
/// `acknowledged_version >= 1`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawDisclosure {
    #[serde(default)]
    pub acknowledged_version: u32,
    #[serde(default)]
    pub acknowledged_at: Option<String>,
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
            disclosure: match settings.disclosure {
                Some(ack) => RawDisclosure {
                    acknowledged_version: ack.acknowledged_version,
                    // A timestamp that refuses to format is a
                    // programming error (`OffsetDateTime`'s own range is
                    // always representable in RFC 3339), not a reason to
                    // fail the whole settings save; informational field
                    // only (data-model.md §1.1).
                    acknowledged_at: ack.acknowledged_at.format(&Rfc3339).ok(),
                },
                None => RawDisclosure::default(),
            },
            playback: RawPlayback {
                device_name: settings
                    .device_name
                    .as_ref()
                    .map(|name| name.as_str().to_string()),
                connect_device_id: settings.connect_device_id.clone(),
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

        // `acknowledged_version == 0` means never acknowledged
        // (data-model.md §1.1); an unparseable/absent timestamp on an
        // otherwise-acknowledged record still counts as acknowledged
        // (the timestamp is informational only) and falls back to the
        // Unix epoch rather than losing the acknowledgement.
        let disclosure =
            (self.disclosure.acknowledged_version > 0).then(|| DisclosureAcknowledgement {
                acknowledged_version: self.disclosure.acknowledged_version,
                acknowledged_at: self
                    .disclosure
                    .acknowledged_at
                    .as_deref()
                    .and_then(|raw| OffsetDateTime::parse(raw, &Rfc3339).ok())
                    .unwrap_or(OffsetDateTime::UNIX_EPOCH),
            });

        // Absent -> `None` (default name); present-but-too-long -> `None`
        // plus a warning; present-and-valid -> `Some` (contracts/
        // transport-and-queue.md §5).
        let device_name = match self.playback.device_name.as_deref() {
            None => None,
            Some(raw) => match DeviceName::parse(raw) {
                Ok(name) => name,
                Err(DeviceNameError::TooLong) => {
                    invalid.push(InvalidField::DeviceName);
                    None
                }
            },
        };

        // An invalid-shaped id (not present, or not 32 hex chars) is
        // silently dropped; the caller regenerates and persists a fresh
        // one on next use (contracts/transport-and-queue.md §5) — not
        // itself a reported `InvalidField`, matching `output_device`'s
        // silent-drop-on-empty precedent above.
        let connect_device_id = self
            .playback
            .connect_device_id
            .filter(|id| is_valid_connect_device_id(id));

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
            disclosure,
            device_name,
            connect_device_id,
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

    #[test]
    fn absent_disclosure_section_means_never_acknowledged() {
        let (settings, invalid) = RawSettings::default().into_settings();
        assert!(invalid.is_empty());
        assert_eq!(settings.disclosure, None);
    }

    #[test]
    fn acknowledged_disclosure_round_trips() {
        let settings = AudioSettings {
            disclosure: Some(DisclosureAcknowledgement {
                acknowledged_version: 1,
                acknowledged_at: OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(3600),
            }),
            ..AudioSettings::default()
        };
        let raw = RawSettings::from_settings(&settings);
        let (round_tripped, invalid) = raw.into_settings();
        assert!(invalid.is_empty());
        assert_eq!(round_tripped, settings);
    }

    #[test]
    fn acknowledged_version_zero_is_never_acknowledged_even_with_a_timestamp() {
        let mut raw = RawSettings::default();
        raw.disclosure.acknowledged_version = 0;
        raw.disclosure.acknowledged_at = Some("2026-09-15T13:00:00Z".to_string());
        let (settings, _invalid) = raw.into_settings();
        assert_eq!(settings.disclosure, None);
    }

    #[test]
    fn unparseable_acknowledged_at_still_counts_as_acknowledged() {
        let mut raw = RawSettings::default();
        raw.disclosure.acknowledged_version = 1;
        raw.disclosure.acknowledged_at = Some("not a timestamp".to_string());
        let (settings, _invalid) = raw.into_settings();
        assert_eq!(
            settings.disclosure,
            Some(DisclosureAcknowledgement {
                acknowledged_version: 1,
                acknowledged_at: OffsetDateTime::UNIX_EPOCH,
            })
        );
    }
}
