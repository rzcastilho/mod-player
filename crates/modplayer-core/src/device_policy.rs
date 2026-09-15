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
    /// The confirmed/preferred device is gone; fell back to the default.
    MissingPreferred { device_name: String },
    /// No output-capable devices are available at all.
    NoDevices,
}

/// Resolve which device should become active, per data-model.md §6.2's
/// state machine:
///
/// - zero devices → `NoDevice` + `NoDevices`
/// - confirmed && present (id, then name) → `Active(preferred)`, no warning
/// - confirmed && absent → `Active(default, is_fallback)` + `MissingPreferred`
/// - not confirmed → `Active(default)` (preview; not a fallback), no warning
pub fn resolve(
    devices: &[OutputDeviceInfo],
    preferred: Option<&DeviceId>,
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
        // *missing* device (FR-014), not the fallback. Only its id is
        // persisted (contracts/settings-file.md), so the id is the best
        // name available until the settings schema also stores the name.
        return match default_device(devices) {
            Some(default) => (
                DeviceResolution::Active {
                    device: default.clone(),
                    is_fallback: true,
                },
                Some(DeviceWarning::MissingPreferred {
                    device_name: preferred
                        .map(|id| id.as_str().to_string())
                        .unwrap_or_else(|| default.name.clone()),
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
        let (resolution, warning) = resolve(&[], None, false);
        assert_eq!(resolution, DeviceResolution::NoDevice);
        assert_eq!(warning, Some(DeviceWarning::NoDevices));
    }

    #[test]
    fn not_confirmed_previews_default_without_warning() {
        let devices = [device("a", "A", false), device("b", "B", true)];
        let (resolution, warning) = resolve(&devices, None, false);
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
        let (resolution, warning) = resolve(&devices, Some(&preferred), true);
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
        let (resolution, _) = resolve(&devices, Some(&by_name), true);
        assert_eq!(
            resolution,
            DeviceResolution::Active {
                device: devices[0].clone(),
                is_fallback: false
            }
        );
        // A genuinely unrelated preferred id/name falls back with a warning.
        let (resolution, warning) = resolve(&devices, Some(&preferred), true);
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
        let (resolution, warning) = resolve(&devices, Some(&preferred), true);
        assert_eq!(
            resolution,
            DeviceResolution::Active {
                device: devices[0].clone(),
                is_fallback: true
            }
        );
        // FR-014: the warning names the *missing* device, not the fallback.
        assert_eq!(
            warning,
            Some(DeviceWarning::MissingPreferred {
                device_name: "missing".to_string()
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
