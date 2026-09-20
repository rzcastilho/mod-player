// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Manifest parsing/validation tests (Constitution VIII, contracts/
//! manifest.md §5).

use modplayer_capability_gateway::api::Permission;
use modplayer_capability_gateway::manifest::{self, ManifestError};

const VALID: &str = r#"
identifier = "org.modplayer.fixture.wellbehaved"
name = "Well-behaved fixture"
description = "Exercises every operable capability."
version = "1.0.0"
api = "1.0"
author = "ModPlayer project"
license = "MIT OR Apache-2.0"
source = "bundled"
entry = "main.luau"

[[permissions.required]]
permission = "playback.observe"
justification = "Follows the playhead."

[[permissions.optional]]
permission = "audio.meter"
justification = "Shows a level readout."

network_hosts = []
"#;

#[test]
fn valid_manifest_parses() {
    let manifest = manifest::parse_and_validate(VALID, true).expect("valid");
    assert_eq!(
        manifest.identifier.as_str(),
        "org.modplayer.fixture.wellbehaved"
    );
    assert_eq!(manifest.required.len(), 1);
    assert_eq!(manifest.optional.len(), 1);
}

#[test]
fn rejects_unknown_permission() {
    let toml = VALID.replace("playback.observe", "teleport.everywhere");
    let err = manifest::parse_and_validate(&toml, true).unwrap_err();
    assert!(matches!(err, ManifestError::UnknownPermission { .. }));
}

#[test]
fn network_rules() {
    let required_network = VALID.replacen(
        "[[permissions.required]]\npermission = \"playback.observe\"",
        "[[permissions.required]]\npermission = \"network\"",
        1,
    );
    assert_eq!(
        manifest::parse_and_validate(&required_network, true).unwrap_err(),
        ManifestError::NetworkMustBeOptional
    );

    let optional_network_no_hosts = VALID.replacen(
        "[[permissions.optional]]\npermission = \"audio.meter\"",
        "[[permissions.optional]]\npermission = \"network\"",
        1,
    );
    assert_eq!(
        manifest::parse_and_validate(&optional_network_no_hosts, true).unwrap_err(),
        ManifestError::NetworkWithoutHosts
    );
}

#[test]
fn duplicate_permission() {
    let toml = format!(
        "{VALID}\n[[permissions.optional]]\npermission = \"playback.observe\"\njustification = \"x\"\n"
    );
    let err = manifest::parse_and_validate(&toml, true).unwrap_err();
    assert_eq!(
        err,
        ManifestError::DuplicatePermission(Permission::PlaybackObserve)
    );
}

#[test]
fn missing_field_names_the_field() {
    let toml = VALID.replace("name = \"Well-behaved fixture\"\n", "");
    let err = manifest::parse_and_validate(&toml, true).unwrap_err();
    assert_eq!(err, ManifestError::MissingField("name"));
}

#[test]
fn entry_missing_when_not_present() {
    let err = manifest::parse_and_validate(VALID, false).unwrap_err();
    assert_eq!(err, ManifestError::EntryMissing("main.luau".to_string()));
}

#[test]
fn malformed_identifier_cases() {
    use modplayer_capability_gateway::manifest::PluginIdentifier;
    for bad in ["foo", "Foo.bar", "a..b", &"a".repeat(129)] {
        assert_eq!(PluginIdentifier::parse(bad), None, "{bad}");
    }
    assert!(PluginIdentifier::parse("org.modplayer.ok").is_some());
}

mod proptests {
    use super::*;
    use modplayer_capability_gateway::manifest::Version;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn round_trip(major in 0u32..1000, minor in 0u32..1000, patch in 0u32..1000) {
            let s = format!("{major}.{minor}.{patch}");
            let v = Version::parse(&s).expect("parses");
            prop_assert_eq!(v.to_string(), s);
        }

        #[test]
        fn rejects_unknown_permission_strings(s in "[a-z]{1,10}\\.[a-z]{1,10}") {
            if Permission::parse(&s).is_none() {
                let toml = VALID.replace("playback.observe", &s);
                let err = manifest::parse_and_validate(&toml, true).unwrap_err();
                let is_unknown_permission = matches!(err, ManifestError::UnknownPermission { .. });
                prop_assert!(is_unknown_permission);
            }
        }
    }
}
