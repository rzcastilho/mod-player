// SPDX-License-Identifier: MIT OR Apache-2.0

//! Device-resolution policy (data-model.md §6.2, contracts/output-backend.md):
//! pure functions deciding which device becomes active at launch (or a
//! reconnect), matching a preferred device by stable id then by name. No
//! I/O, no backend calls — the controller drives these with whatever
//! `OutputBackend::devices()` returned.

use modplayer_audio_io::OutputDeviceInfo;
use modplayer_engine::DeviceId;

/// Find `preferred` in `devices`: first by stable id, then by display name
/// (a device can be replugged into a different port/driver instance and
/// change id while keeping its name; FR-004).
pub fn find_preferred<'a>(
    devices: &'a [OutputDeviceInfo],
    preferred: &DeviceId,
) -> Option<&'a OutputDeviceInfo> {
    devices
        .iter()
        .find(|d| &d.id == preferred)
        .or_else(|| devices.iter().find(|d| d.name == preferred.as_str()))
}

/// The system's default output device, falling back to the first listed
/// device if none is marked default (defensive: every real backend reports
/// exactly one default whenever `devices` is non-empty).
pub fn default_device(devices: &[OutputDeviceInfo]) -> Option<&OutputDeviceInfo> {
    devices
        .iter()
        .find(|d| d.is_default)
        .or_else(|| devices.first())
}

/// The outcome of resolving which device should be active
/// (data-model.md §6.2).
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceResolution {
    /// A device to open a stream on. `is_fallback` is `true` only for the
    /// "confirmed device is no longer present" case — not for the normal
    /// not-yet-confirmed preview, which previews the system default without
    /// being a fallback from anything.
    Active {
        device: OutputDeviceInfo,
        is_fallback: bool,
    },
    /// No output-capable device exists at all.
    NoDevice,
}

/// A warning to raise alongside a `DeviceResolution`, if any.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceWarning {
    /// The confirmed/preferred device is gone; fell back to the default
    /// (019-notification-presentation, data-model.md §3). `device_name` is
    /// a resolved human display name (never the raw id) — `None` means no
    /// name could be resolved and the caller substitutes a generic phrase.
    /// `device_id` carries the raw id for `detail` (FR-013); `fallback_name`
    /// is the fallback device's own name.
    MissingPreferred {
        device_name: Option<String>,
        device_id: Option<DeviceId>,
        fallback_name: String,
    },
    /// No output-capable devices are available at all.
    NoDevices,
}

/// Resolve a missing preferred device's display name (019-notification-
/// presentation, data-model.md §4, contract C5): `saved_name` (trimmed,
/// non-empty) wins; else, when `id`'s own string is in `name:<name>` form,
/// the trimmed, non-empty remainder; else `None` (the caller substitutes a
/// generic phrase, e.g. `tr("notification-device-unknown")`). Never returns
/// any other substring of the raw id — in particular, an id that is not in
/// `name:` form never contributes to the result.
///
/// # Examples
///
/// ```
/// use modplayer_core::device_policy::display_name_for_saved;
/// use modplayer_engine::DeviceId;
///
/// let id = DeviceId::new("coreaudio:AppleUSBAudioEngine:1234").unwrap();
/// assert_eq!(
///     display_name_for_saved(Some(&id), Some("Scarlett 2i2")),
///     Some("Scarlett 2i2".to_string())
/// );
///
/// let legacy = DeviceId::new("name:Old Interface").unwrap();
/// assert_eq!(
///     display_name_for_saved(Some(&legacy), None),
///     Some("Old Interface".to_string())
/// );
///
/// assert_eq!(display_name_for_saved(Some(&id), None), None);
/// ```
pub fn display_name_for_saved(id: Option<&DeviceId>, saved_name: Option<&str>) -> Option<String> {
    if let Some(saved) = saved_name {
        let trimmed = saved.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(id) = id
        && let Some(remainder) = id.as_str().strip_prefix("name:")
    {
        let trimmed = remainder.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Resolve which device should become active, per data-model.md §6.2's
/// state machine:
///
/// - zero devices → `NoDevice` + `NoDevices`
/// - confirmed && present (id, then name) → `Active(preferred)`, no warning
/// - confirmed && absent → `Active(default, is_fallback)` + `MissingPreferred`
/// - not confirmed → `Active(default)` (preview; not a fallback), no warning
///
/// `saved_name` (019-notification-presentation, data-model.md §3) is the
/// persisted `[audio] output_device_name`, threaded into
/// `MissingPreferred.device_name` via [`display_name_for_saved`] so the
/// warning can name the missing device without ever falling back to its
/// raw id.
pub fn resolve(
    devices: &[OutputDeviceInfo],
    preferred: Option<&DeviceId>,
    saved_name: Option<&str>,
    confirmed: bool,
) -> (DeviceResolution, Option<DeviceWarning>) {
    if devices.is_empty() {
        return (DeviceResolution::NoDevice, Some(DeviceWarning::NoDevices));
    }

    if confirmed {
        if let Some(found) = preferred.and_then(|id| find_preferred(devices, id)) {
            return (
                DeviceResolution::Active {
                    device: found.clone(),
                    is_fallback: false,
                },
                None,
            );
        }
        // Confirmed but the preferred device (or no preference at all) is
        // absent: fall back to the default with a warning that names the
        // *missing* device (FR-014, FR-011) by resolved display name, not
        // its raw id — the raw id survives only in `device_id` for the
        // caller's `detail` (FR-013).
        return match default_device(devices) {
            Some(default) => (
                DeviceResolution::Active {
                    device: default.clone(),
                    is_fallback: true,
                },
                Some(DeviceWarning::MissingPreferred {
                    device_name: display_name_for_saved(preferred, saved_name),
                    device_id: preferred.cloned(),
                    fallback_name: default.name.clone(),
                }),
            ),
            None => (DeviceResolution::NoDevice, Some(DeviceWarning::NoDevices)),
        };
    }

    // Not confirmed yet: preview the system default (first-launch / Device
    // Check flow). Not a fallback — there is nothing to have fallen back from.
    match default_device(devices) {
        Some(default) => (
            DeviceResolution::Active {
                device: default.clone(),
                is_fallback: false,
            },
            None,
        ),
        None => (DeviceResolution::NoDevice, Some(DeviceWarning::NoDevices)),
    }
}

/// The outcome of losing the active stream's device mid-session (US3
/// acceptance 1, 3; data-model.md §6.2: "Active(X) --DeviceLost(X)-->
/// Active(system default, is_fallback) ... clock continues" / "--
/// DeviceLost(X), no default--> NoDevice, transport Paused, Critical").
/// `devices` passed in is expected to already exclude the lost device (the
/// backend removes it from its list before emitting the event).
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceLostOutcome {
    /// Fall back to the system default and keep playing; `lost_device_name`
    /// is carried through for the Critical "device lost" notification.
    FellBackTo {
        device: OutputDeviceInfo,
        lost_device_name: String,
    },
    /// No output device remains at all: transport must pause, position
    /// retained (the caller does this — this module has no I/O), and a
    /// Critical notification raised.
    NoDeviceRemains,
}

/// Decide what happens when the active device disappears (T072). Pure:
/// the caller (controller.rs) performs the actual stream rebuild/transport
/// change and raises the notification.
pub fn on_device_lost(devices: &[OutputDeviceInfo], lost_device_name: String) -> DeviceLostOutcome {
    match default_device(devices) {
        Some(default) => DeviceLostOutcome::FellBackTo {
            device: default.clone(),
            lost_device_name,
        },
        None => DeviceLostOutcome::NoDeviceRemains,
    }
}

/// Whether `preferred` has reappeared in `devices` (US3 acceptance 4): the
/// app stays on the fallback device regardless — this is a pure query the
/// caller uses only to decide whether to raise an Info notification, never
/// to switch the active stream back (data-model.md §6.2: "Active(fallback)
/// --DeviceListChanged (X reappears)--> unchanged; Info \"X available
/// again\"; preferred stays X").
pub fn reappeared<'a>(
    devices: &'a [OutputDeviceInfo],
    preferred: &DeviceId,
) -> Option<&'a OutputDeviceInfo> {
    find_preferred(devices, preferred)
}

#[cfg(test)]
mod tests {
    use super::*;
    use modplayer_engine::{FrameCount, SampleRate};

    fn device(id: &str, name: &str, is_default: bool) -> OutputDeviceInfo {
        OutputDeviceInfo {
            id: DeviceId::new(id).unwrap_or_else(|| unreachable!("test id is non-empty")),
            name: name.to_string(),
            is_default,
            default_rate: SampleRate::new(44_100),
            channels: 2,
            buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        }
    }

    #[test]
    fn zero_devices_is_no_device_with_warning() {
        let (resolution, warning) = resolve(&[], None, None, false);
        assert_eq!(resolution, DeviceResolution::NoDevice);
        assert_eq!(warning, Some(DeviceWarning::NoDevices));
    }

    #[test]
    fn not_confirmed_previews_default_without_warning() {
        let devices = [device("a", "A", false), device("b", "B", true)];
        let (resolution, warning) = resolve(&devices, None, None, false);
        assert_eq!(
            resolution,
            DeviceResolution::Active {
                device: devices[1].clone(),
                is_fallback: false
            }
        );
        assert_eq!(warning, None);
    }

    #[test]
    fn confirmed_and_present_resolves_active_no_warning() {
        let devices = [device("a", "A", true), device("b", "B", false)];
        let preferred = DeviceId::new("b").unwrap_or_else(|| unreachable!());
        let (resolution, warning) = resolve(&devices, Some(&preferred), None, true);
        assert_eq!(
            resolution,
            DeviceResolution::Active {
                device: devices[1].clone(),
                is_fallback: false
            }
        );
        assert_eq!(warning, None);
    }

    #[test]
    fn confirmed_matches_by_name_when_id_changed() {
        let devices = [device("new-id", "Speakers", true)];
        let preferred = DeviceId::new("old-id").unwrap_or_else(|| unreachable!());
        // Simulate a name-based match by using the same name as `preferred`'s id.
        let by_name = DeviceId::new("Speakers").unwrap_or_else(|| unreachable!());
        let (resolution, _) = resolve(&devices, Some(&by_name), None, true);
        assert_eq!(
            resolution,
            DeviceResolution::Active {
                device: devices[0].clone(),
                is_fallback: false
            }
        );
        // A genuinely unrelated preferred id/name falls back with a warning.
        let (resolution, warning) = resolve(&devices, Some(&preferred), None, true);
        assert_eq!(
            resolution,
            DeviceResolution::Active {
                device: devices[0].clone(),
                is_fallback: true
            }
        );
        assert!(matches!(
            warning,
            Some(DeviceWarning::MissingPreferred { .. })
        ));
    }

    #[test]
    fn confirmed_and_absent_falls_back_to_default_with_warning() {
        let devices = [device("a", "A", true)];
        let preferred = DeviceId::new("missing").unwrap_or_else(|| unreachable!());
        let (resolution, warning) = resolve(&devices, Some(&preferred), None, true);
        assert_eq!(
            resolution,
            DeviceResolution::Active {
                device: devices[0].clone(),
                is_fallback: true
            }
        );
        // FR-014: the warning names the *missing* device by resolved display
        // name, never the raw id — "missing" isn't a `name:`-form id and no
        // `saved_name` was given, so `device_name` is `None` (the caller
        // substitutes a generic phrase); the raw id survives only in
        // `device_id`, for `detail`.
        assert_eq!(
            warning,
            Some(DeviceWarning::MissingPreferred {
                device_name: None,
                device_id: Some(preferred),
                fallback_name: "A".to_string(),
            })
        );
    }

    /// 019-notification-presentation (data-model.md §4, contract C5): a
    /// `saved_name` wins over a `name:`-form id, which wins over `None`.
    #[test]
    fn missing_preferred_prefers_saved_name_then_legacy_id_form_then_none() {
        let devices = [device("a", "A", true)];

        let plain_id = DeviceId::new("coreaudio:xyz").unwrap_or_else(|| unreachable!());
        let (_, warning) = resolve(&devices, Some(&plain_id), Some("My Interface"), true);
        assert_eq!(
            warning,
            Some(DeviceWarning::MissingPreferred {
                device_name: Some("My Interface".to_string()),
                device_id: Some(plain_id.clone()),
                fallback_name: "A".to_string(),
            })
        );

        let legacy_id = DeviceId::new("name:Old Interface").unwrap_or_else(|| unreachable!());
        let (_, warning) = resolve(&devices, Some(&legacy_id), None, true);
        assert_eq!(
            warning,
            Some(DeviceWarning::MissingPreferred {
                device_name: Some("Old Interface".to_string()),
                device_id: Some(legacy_id),
                fallback_name: "A".to_string(),
            })
        );

        let (_, warning) = resolve(&devices, Some(&plain_id), None, true);
        assert_eq!(
            warning,
            Some(DeviceWarning::MissingPreferred {
                device_name: None,
                device_id: Some(plain_id),
                fallback_name: "A".to_string(),
            })
        );
    }

    #[test]
    fn device_lost_falls_back_to_default_when_one_remains() {
        // The lost device is already absent from `devices` (the backend
        // removes it before emitting `DeviceLost`).
        let devices = [device("b", "B", true)];
        let outcome = on_device_lost(&devices, "A".to_string());
        assert_eq!(
            outcome,
            DeviceLostOutcome::FellBackTo {
                device: devices[0].clone(),
                lost_device_name: "A".to_string(),
            }
        );
    }

    #[test]
    fn device_lost_with_none_remaining_is_no_device_remains() {
        let outcome = on_device_lost(&[], "A".to_string());
        assert_eq!(outcome, DeviceLostOutcome::NoDeviceRemains);
    }

    #[test]
    fn reappeared_finds_preferred_by_id_or_name() {
        let devices = [device("a", "A", true)];
        let preferred = DeviceId::new("a").unwrap_or_else(|| unreachable!());
        assert_eq!(reappeared(&devices, &preferred), Some(&devices[0]));

        let absent = DeviceId::new("missing").unwrap_or_else(|| unreachable!());
        assert_eq!(reappeared(&devices, &absent), None);
    }
}
