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

use std::collections::BTreeMap;

use modplayer_engine::{BufferPreset, CeilingDb, DeviceId, SafeVolume, Theme, VolumePercent};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::actions::{Chord, HostAction, KeymapOverrides};
use crate::plugins::FocusPolicy;

/// 011-plugin-ui-contributions (FR-005): a panel's chosen placement.
/// `Docked` is the default a panel opens in every session (the persisted
/// value is only ever written once the user floats/re-docks it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelPlacement {
    #[default]
    Docked,
    Floated,
}

impl PanelPlacement {
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            PanelPlacement::Docked => "docked",
            PanelPlacement::Floated => "floated",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "docked" => Some(PanelPlacement::Docked),
            "floated" => Some(PanelPlacement::Floated),
            _ => None,
        }
    }
}

/// 011-plugin-ui-contributions (FR-005/FR-006, data-model.md §5.1): one
/// `[plugin_panels."<identifier>/<panel-id>"]` entry — placement, a
/// floated geometry (only meaningful while `Floated`; `None` before the
/// user has ever moved/resized it), and the persisted `disabled` flag
/// (FR-006, independent of the session-only `closed` state).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PanelPersisted {
    pub placement: PanelPlacement,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub w: Option<f32>,
    pub h: Option<f32>,
    pub disabled: bool,
}

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
    /// `[markers] nudge_step_ms` (006, FR-027, data-model.md §5): how far a
    /// focused marker's arrow-key nudge moves it, in milliseconds (`Shift`
    /// multiplies by 10). Clamped `1..=1000` silently — no `InvalidField`
    /// variant, matching `master_volume`/`safe_volume.cap`'s precedent.
    pub nudge_step_ms: u16,
    /// `[keybindings]` shadow state (007, data-model.md §2, §5): only the
    /// actions whose effective bindings differ from the shipped catalog
    /// default. Compared in `PartialEq` like every other field, so a
    /// round-trip test catches a regression here too.
    pub keybinding_overrides: KeymapOverrides,
    /// `[transport] focus_policy` (010-transport-focus, FR-012,
    /// data-model.md §2.2): the user's global transport-focus assignment
    /// policy. The holder and pending queue are session-only and never
    /// persisted (FR-012) — only this enum lives here.
    pub focus_policy: FocusPolicy,
    /// `[plugin_panels]` (011-plugin-ui-contributions, FR-005/FR-006):
    /// keyed `"<plugin-identifier>/<panel-id>"`. An entry for a panel not
    /// currently registered is retained dormant (never dropped on load —
    /// mirrors 007 FR-013's keybinding-override convention).
    pub plugin_panels: BTreeMap<String, PanelPersisted>,
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
            nudge_step_ms: DEFAULT_NUDGE_STEP_MS,
            keybinding_overrides: KeymapOverrides::default(),
            focus_policy: FocusPolicy::default(),
            plugin_panels: BTreeMap::new(),
            schema_version: SCHEMA_VERSION,
        }
    }
}

/// `[markers] nudge_step_ms`'s default (data-model.md §5).
const DEFAULT_NUDGE_STEP_MS: u16 = 10;

/// Clamp a raw (possibly out-of-range, possibly negative) TOML integer
/// into `1..=1000` (data-model.md §5 "clamped silently").
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn clamp_nudge_step_ms(raw: i64) -> u16 {
    raw.clamp(1, 1_000) as u16
}

/// A field that fell back to its default because the file held an
/// unrecognised enum string (contracts/settings-file.md: "Unknown enum
/// string").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidField {
    BufferPreset,
    Theme,
    /// `[playback] device_name` was longer than 64 characters after trim
    /// (contracts/transport-and-queue.md §5).
    DeviceName,
    /// `[transport] focus_policy` held an unrecognised string
    /// (010-transport-focus, research R5) — falls back to
    /// `FocusPolicy::default()` (`AutoOnInteraction`).
    FocusPolicy,
    /// 011-plugin-ui-contributions (FR-005): a `[plugin_panels.*]` entry
    /// held an unrecognised `placement` string — that one entry is
    /// dropped (never the whole table).
    PluginPanel(String),
}

impl InvalidField {
    /// The TOML key this field corresponds to, for the warning message.
    pub fn field_name(self) -> &'static str {
        match self {
            InvalidField::BufferPreset => "audio.buffer_preset",
            InvalidField::Theme => "appearance.theme",
            InvalidField::DeviceName => "playback.device_name",
            InvalidField::FocusPolicy => "transport.focus_policy",
            InvalidField::PluginPanel(_) => "plugin_panels",
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
    #[serde(default)]
    pub markers: RawMarkers,
    /// `[transport]` (010-transport-focus, research R5): an optional
    /// table so older files (with no such section) load the default
    /// policy.
    #[serde(default)]
    pub transport: RawTransport,
    /// `[keybindings]` (007, contracts/keymap-settings.md): action id ->
    /// arbitrary TOML value, so one malformed entry's *shape* (not an
    /// array, or an array with a non-string) never fails the whole file
    /// — only that entry is dropped, in `into_settings`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keybindings: BTreeMap<String, toml::Value>,
    /// `[plugin_panels]` (011-plugin-ui-contributions, FR-005/FR-006):
    /// `"<identifier>/<panel-id>"` -> its persisted placement/geometry/
    /// disabled flag. An unparseable `placement` string drops only that
    /// one entry (`InvalidField::PluginPanel`), never the whole table.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plugin_panels: BTreeMap<String, RawPanel>,
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
            markers: RawMarkers::default(),
            transport: RawTransport::default(),
            keybindings: BTreeMap::new(),
            plugin_panels: BTreeMap::new(),
        }
    }
}

/// One `[plugin_panels."<key>"]` entry's wire shape (011-plugin-ui-
/// contributions, data-model.md §5.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawPanel {
    #[serde(default = "default_panel_placement")]
    pub placement: String,
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
    #[serde(default)]
    pub w: Option<f32>,
    #[serde(default)]
    pub h: Option<f32>,
    #[serde(default)]
    pub disabled: bool,
}

fn default_panel_placement() -> String {
    PanelPlacement::default().wire_name().to_string()
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

/// The `[markers]` section (006, data-model.md §5): an optional table so
/// older files (with no such section) load the default step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMarkers {
    #[serde(default = "default_nudge_step_ms")]
    pub nudge_step_ms: i64,
}

impl Default for RawMarkers {
    fn default() -> Self {
        Self {
            nudge_step_ms: default_nudge_step_ms(),
        }
    }
}

fn default_nudge_step_ms() -> i64 {
    i64::from(DEFAULT_NUDGE_STEP_MS)
}

/// The `[transport]` section (010-transport-focus, research R5,
/// data-model.md §2.2): the user's global focus policy. The holder and
/// pending queue are session-only and never written here (FR-012).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTransport {
    #[serde(default = "default_focus_policy")]
    pub focus_policy: String,
}

impl Default for RawTransport {
    fn default() -> Self {
        Self {
            focus_policy: default_focus_policy(),
        }
    }
}

fn default_focus_policy() -> String {
    FocusPolicy::default().wire_name().to_string()
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
            markers: RawMarkers {
                nudge_step_ms: i64::from(settings.nudge_step_ms),
            },
            transport: RawTransport {
                focus_policy: settings.focus_policy.wire_name().to_string(),
            },
            keybindings: {
                // 011-plugin-ui-contributions (contracts/action-registry-
                // plugins.md K1): plugin overrides and dormant entries
                // share the same `[keybindings]` table as host overrides,
                // keyed by their own (non-`host.`) id string.
                let mut map: BTreeMap<String, toml::Value> = settings
                    .keybinding_overrides
                    .iter()
                    .map(|(action, chords)| (action.id().to_string(), encode_chords(chords)))
                    .collect();
                for (id, chords) in settings.keybinding_overrides.plugin_iter() {
                    map.insert(id.id(), encode_chords(chords));
                }
                for (id, chords) in settings.keybinding_overrides.dormant_iter() {
                    map.insert(id.clone(), encode_chords(chords));
                }
                map
            },
            plugin_panels: settings
                .plugin_panels
                .iter()
                .map(|(key, panel)| {
                    (
                        key.clone(),
                        RawPanel {
                            placement: panel.placement.wire_name().to_string(),
                            x: panel.x,
                            y: panel.y,
                            w: panel.w,
                            h: panel.h,
                            disabled: panel.disabled,
                        },
                    )
                })
                .collect(),
        }
    }

    /// Validate and clamp into the domain type. Out-of-range numbers are
    /// clamped silently (the newtype constructors do this); an
    /// unrecognised enum string falls back to its field default and is
    /// reported in the returned list (contracts/settings-file.md). The
    /// third element lists every `[keybindings]` entry dropped in
    /// isolation (contracts/keymap-settings.md): an unknown action id, a
    /// value that isn't an array of strings, or a chord string that
    /// fails `Chord::parse` drops that whole entry, never a partial
    /// binding list; duplicate chords within one entry are deduplicated
    /// silently (first occurrence kept), with no warning.
    pub fn into_settings(self) -> (AudioSettings, Vec<InvalidField>, Vec<String>) {
        let mut invalid = Vec::new();
        let mut dropped_keybindings = Vec::new();
        let mut keybinding_overrides = KeymapOverrides::default();
        for (id, value) in &self.keybindings {
            if id.starts_with("host.") {
                match decode_keybinding_entry(value) {
                    Some(chords) => match HostAction::parse(id) {
                        Some(action) => keybinding_overrides.set(action, chords),
                        None => dropped_keybindings.push(id.clone()),
                    },
                    None => dropped_keybindings.push(id.clone()),
                }
            } else {
                // 011-plugin-ui-contributions (FR-010a, contracts/
                // action-registry-plugins.md K1/K2): a non-`host.`-
                // namespaced id whose value decodes is retained dormant,
                // re-serialised verbatim, and never raises `keybindings-
                // invalid-entries` — even when it is not (or not yet) a
                // real, registered plugin action id. An undecodable value
                // is silently dropped: only `host.`-namespaced unknowns/
                // unparseable values are reported (K2).
                if let Some(chords) = decode_keybinding_entry(value) {
                    keybinding_overrides.set_dormant(id.clone(), chords);
                }
            }
        }

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

        let focus_policy = match FocusPolicy::parse(&self.transport.focus_policy) {
            Some(policy) => policy,
            None => {
                invalid.push(InvalidField::FocusPolicy);
                FocusPolicy::default()
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

        // 011-plugin-ui-contributions (FR-005): an entry whose `placement`
        // fails to parse drops that one entry (with a warning), never the
        // whole `[plugin_panels]` table — mirrors `[keybindings]`'s own
        // per-entry isolation above, but reported through `invalid`
        // (`InvalidField`) rather than a separate dropped-list, since
        // there is exactly one way a `[plugin_panels]` entry goes wrong.
        let mut plugin_panels = BTreeMap::new();
        for (key, raw) in &self.plugin_panels {
            match PanelPlacement::parse(&raw.placement) {
                Some(placement) => {
                    plugin_panels.insert(
                        key.clone(),
                        PanelPersisted {
                            placement,
                            x: raw.x,
                            y: raw.y,
                            w: raw.w,
                            h: raw.h,
                            disabled: raw.disabled,
                        },
                    );
                }
                None => invalid.push(InvalidField::PluginPanel(key.clone())),
            }
        }

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
            nudge_step_ms: clamp_nudge_step_ms(self.markers.nudge_step_ms),
            keybinding_overrides,
            focus_policy,
            plugin_panels,
            schema_version: self.schema_version,
        };

        (settings, invalid, dropped_keybindings)
    }
}

/// The wire shape of one `[keybindings]` entry's value: an array of
/// [`Chord::encode`] strings (007, contracts/keymap-settings.md), shared
/// by host overrides, plugin overrides and dormant entries alike
/// (011-plugin-ui-contributions, contracts/action-registry-plugins.md
/// K1).
fn encode_chords(chords: &[Chord]) -> toml::Value {
    toml::Value::Array(
        chords
            .iter()
            .map(|c| toml::Value::String(c.encode()))
            .collect(),
    )
}

/// Decode one `[keybindings]` entry's value into a deduplicated chord
/// list, or `None` if its shape is wrong (not an array, or an array
/// holding a non-string) or any element fails [`Chord::parse`] — in
/// which case the whole entry is dropped, never a partial list
/// (contracts/keymap-settings.md).
fn decode_keybinding_entry(value: &toml::Value) -> Option<Vec<Chord>> {
    let array = value.as_array()?;
    let mut chords = Vec::with_capacity(array.len());
    for item in array {
        let raw = item.as_str()?;
        let chord = Chord::parse(raw).ok()?;
        if !chords.contains(&chord) {
            chords.push(chord);
        }
    }
    Some(chords)
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
        let (round_tripped, invalid, dropped) = raw.into_settings();
        assert!(invalid.is_empty());
        assert!(dropped.is_empty());
        assert_eq!(round_tripped, settings);
    }

    #[test]
    fn out_of_range_numbers_clamp_silently() {
        let mut raw = RawSettings::default();
        raw.audio.limiter_ceiling_db = 3.0;
        raw.audio.master_volume = 250;
        raw.audio.safe_volume.cap = -5;
        let (settings, invalid, dropped) = raw.into_settings();
        assert!(invalid.is_empty());
        assert!(dropped.is_empty());
        assert_eq!(settings.limiter_ceiling_db.db(), -0.1);
        assert_eq!(settings.master_volume.value(), 100);
        assert_eq!(settings.safe_volume.cap.value(), 0);
    }

    #[test]
    fn unknown_enum_strings_fall_back_and_are_reported() {
        let mut raw = RawSettings::default();
        raw.audio.buffer_preset = "turbo".to_string();
        raw.appearance.theme = "midnight".to_string();
        let (settings, invalid, dropped) = raw.into_settings();
        assert!(dropped.is_empty());
        assert_eq!(settings.buffer_preset, BufferPreset::default());
        assert_eq!(settings.theme, Theme::default());
        assert_eq!(
            invalid,
            vec![InvalidField::BufferPreset, InvalidField::Theme]
        );
    }

    #[test]
    fn absent_disclosure_section_means_never_acknowledged() {
        let (settings, invalid, dropped) = RawSettings::default().into_settings();
        assert!(invalid.is_empty());
        assert!(dropped.is_empty());
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
        let (round_tripped, invalid, dropped) = raw.into_settings();
        assert!(invalid.is_empty());
        assert!(dropped.is_empty());
        assert_eq!(round_tripped, settings);
    }

    #[test]
    fn acknowledged_version_zero_is_never_acknowledged_even_with_a_timestamp() {
        let mut raw = RawSettings::default();
        raw.disclosure.acknowledged_version = 0;
        raw.disclosure.acknowledged_at = Some("2026-09-15T13:00:00Z".to_string());
        let (settings, _invalid, _dropped) = raw.into_settings();
        assert_eq!(settings.disclosure, None);
    }

    #[test]
    fn plugin_panels_round_trip() {
        let mut settings = AudioSettings::default();
        settings.plugin_panels.insert(
            "org.modplayer.fixture.ui-panel/main".to_string(),
            PanelPersisted {
                placement: PanelPlacement::Floated,
                x: Some(12.0),
                y: Some(34.0),
                w: Some(320.0),
                h: Some(240.0),
                disabled: true,
            },
        );
        let raw = RawSettings::from_settings(&settings);
        let (round_tripped, invalid, dropped) = raw.into_settings();
        assert!(invalid.is_empty());
        assert!(dropped.is_empty());
        assert_eq!(round_tripped, settings);
    }

    #[test]
    fn plugin_panels_bad_placement_dropped() {
        let mut raw = RawSettings::default();
        raw.plugin_panels.insert(
            "org.modplayer.fixture.ui-panel/main".to_string(),
            RawPanel {
                placement: "sideways".to_string(),
                x: None,
                y: None,
                w: None,
                h: None,
                disabled: false,
            },
        );
        let (settings, invalid, dropped) = raw.into_settings();
        assert!(dropped.is_empty());
        assert!(settings.plugin_panels.is_empty());
        assert_eq!(
            invalid,
            vec![InvalidField::PluginPanel(
                "org.modplayer.fixture.ui-panel/main".to_string()
            )]
        );
    }

    #[test]
    fn unparseable_acknowledged_at_still_counts_as_acknowledged() {
        let mut raw = RawSettings::default();
        raw.disclosure.acknowledged_version = 1;
        raw.disclosure.acknowledged_at = Some("not a timestamp".to_string());
        let (settings, _invalid, _dropped) = raw.into_settings();
        assert_eq!(
            settings.disclosure,
            Some(DisclosureAcknowledgement {
                acknowledged_version: 1,
                acknowledged_at: OffsetDateTime::UNIX_EPOCH,
            })
        );
    }
}
