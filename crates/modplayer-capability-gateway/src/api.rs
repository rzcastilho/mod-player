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
    "ui.panel",
    "ui.overlay",
    "ui.shortcuts",
    "ui.settings",
    "ui.notify",
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
    fn exactly_fourteen_permissions_are_operable() {
        // 011-plugin-ui-contributions: the five `ui.*` permissions join
        // 009's original nine (contracts/plugin-api-v1.2.md §1).
        assert_eq!(Permission::ALL.iter().filter(|p| p.operable()).count(), 14);
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
    fn api_version_is_one_dot_three() {
        // 012-section-loop-plugin (research R1, Constitution IX): the
        // minor bump that adds `markers.set_loop_endpoint`/
        // `markers.set_loop_repeat`.
        assert_eq!(API_VERSION.major, 1);
        assert_eq!(API_VERSION.minor, 3);
    }
}
