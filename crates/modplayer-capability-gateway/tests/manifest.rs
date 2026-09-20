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

// -- 011-plugin-ui-contributions: icon/glyphs/default_locale/strings ------

#[test]
fn icon_glyphs_locale_and_strings_round_trip() {
    // Inserted right after `entry = ...` — VALID already opens
    // `[[permissions.required]]` further down, and TOML bare keys after a
    // table header belong to that table, not the document root.
    let toml = VALID.replace(
        "entry = \"main.luau\"\n",
        "entry = \"main.luau\"\nicon = \"icon.png\"\ndefault_locale = \"en-US\"\n\n[glyphs]\nchord = \"glyphs/chord.png\"\n\n[strings.en-US]\ntempo = \"Tempo\"\n",
    );
    let manifest = manifest::parse_and_validate(&toml, true).expect("valid");
    assert_eq!(manifest.icon.as_deref(), Some("icon.png"));
    assert_eq!(manifest.default_locale, "en-US");
    assert_eq!(
        manifest.glyphs.get("chord").map(String::as_str),
        Some("glyphs/chord.png")
    );
    assert_eq!(
        manifest
            .strings
            .get("en-US")
            .and_then(|table| table.get("tempo"))
            .map(String::as_str),
        Some("Tempo")
    );
}

#[test]
fn missing_icon_glyphs_locale_and_strings_default_sensibly() {
    let manifest = manifest::parse_and_validate(VALID, true).expect("valid");
    assert_eq!(manifest.icon, None);
    assert_eq!(manifest.default_locale, "en-US");
    assert!(manifest.glyphs.is_empty());
    assert!(manifest.strings.is_empty());
}

#[test]
fn malformed_glyph_key_is_malformed_field() {
    let toml = format!("{VALID}\n[glyphs]\nBadKey = \"glyphs/bad.png\"\n");
    let err = manifest::parse_and_validate(&toml, true).unwrap_err();
    assert_eq!(
        err,
        ManifestError::MalformedField {
            field: "glyphs",
            detail: "glyph keys must match [a-z][a-z0-9_]{0,31} and number at most 32".to_string(),
        }
    );
}

#[test]
fn over_limit_glyph_count_is_malformed_field() {
    let mut glyphs_table = String::from("[glyphs]\n");
    for i in 0..33 {
        glyphs_table.push_str(&format!("g{i} = \"glyphs/g{i}.png\"\n"));
    }
    let toml = format!("{VALID}\n{glyphs_table}");
    let err = manifest::parse_and_validate(&toml, true).unwrap_err();
    assert!(matches!(
        err,
        ManifestError::MalformedField {
            field: "glyphs",
            ..
        }
    ));
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
