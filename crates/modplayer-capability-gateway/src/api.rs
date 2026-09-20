// SPDX-License-Identifier: MIT OR Apache-2.0

//! The generated API surface: `Permission`, `RequestKind`, `EventKind`,
//! `ApiVersion` (from `api/v1.toml` via `build.rs`, Constitution IX,
//! research R6) plus [`HOST_CAPABILITIES`].

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

/// The operable permission names plus `"timers"` (data-model.md §1.1):
/// what `ready_ack.capabilities` and the runtime's dispatcher advertise
/// as actually implemented this slice (FR-015).
pub const HOST_CAPABILITIES: &[&str] = &[
    "playback.observe",
    "transport.control",
    "queue.write",
    "markers.read",
    "markers.write",
    "audio.effects",
    "audio.meter",
    "state.plugin",
    "state.track",
    "timers",
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn permission_catalog_has_25_entries() {
        assert_eq!(Permission::ALL.len(), 25);
    }

    #[test]
    fn exactly_nine_permissions_are_operable() {
        assert_eq!(Permission::ALL.iter().filter(|p| p.operable()).count(), 9);
    }

    #[test]
    fn permission_name_round_trips_through_parse() {
        for p in Permission::ALL {
            assert_eq!(Permission::parse(p.name()), Some(p));
        }
    }

    #[test]
    fn every_request_kind_has_a_lua_path() {
        for r in RequestKind::ALL {
            let (ns, method) = r.lua_path();
            assert!(!method.is_empty(), "{ns}.{method}");
        }
    }

    #[test]
    fn host_capabilities_are_all_operable_permissions_plus_timers() {
        let operable = Permission::ALL.iter().filter(|p| p.operable()).count();
        assert_eq!(HOST_CAPABILITIES.len(), operable + 1);
        assert!(HOST_CAPABILITIES.contains(&"timers"));
    }

    #[test]
    fn api_version_is_one_dot_one() {
        // 010-transport-focus (research R6, Constitution IX): the minor
        // bump that gates the FocusGranted/FocusRevoked events.
        assert_eq!(API_VERSION.major, 1);
        assert_eq!(API_VERSION.minor, 1);
    }
}
