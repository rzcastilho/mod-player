// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Connectivity` (data-model.md §3.6, research R5): the single derived
//! "online"/"offline" fact `SearchSession` and `SyncScheduler` both read.
//! Computed the same way `PlaybackController::is_online` already computes
//! it inline (Foundational phase T019) — this type gives that fact a name
//! so `library`'s state doesn't need the controller to ask it.

use modplayer_audio_source::SourceHealth;

/// Whether the service is currently reachable (research R5): the source
/// health is `Ok` and the device is registered. `Transient`/`Unavailable`
/// health, or not-yet-registered, both read as `Offline`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connectivity {
    Online,
    Offline,
}

impl Connectivity {
    /// Derive the fact from the source's own health and registration state
    /// (research R5).
    ///
    /// ```
    /// use modplayer_audio_source::SourceHealth;
    /// use modplayer_core::library::Connectivity;
    ///
    /// assert_eq!(
    ///     Connectivity::derive(&SourceHealth::Ok, true),
    ///     Connectivity::Online
    /// );
    /// assert_eq!(
    ///     Connectivity::derive(&SourceHealth::Ok, false),
    ///     Connectivity::Offline
    /// );
    /// ```
    pub fn derive(health: &SourceHealth, registered: bool) -> Self {
        if registered && matches!(health, SourceHealth::Ok) {
            Connectivity::Online
        } else {
            Connectivity::Offline
        }
    }

    pub fn is_online(self) -> bool {
        matches!(self, Connectivity::Online)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_and_registered_is_online() {
        assert_eq!(
            Connectivity::derive(&SourceHealth::Ok, true),
            Connectivity::Online
        );
    }

    #[test]
    fn ok_but_not_registered_is_offline() {
        assert_eq!(
            Connectivity::derive(&SourceHealth::Ok, false),
            Connectivity::Offline
        );
    }

    #[test]
    fn transient_health_is_offline_even_when_registered() {
        assert_eq!(
            Connectivity::derive(
                &SourceHealth::Transient {
                    since: std::time::Instant::now(),
                    next_retry_in: std::time::Duration::from_secs(1),
                },
                true
            ),
            Connectivity::Offline
        );
    }
}
