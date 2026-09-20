// SPDX-License-Identifier: MIT OR Apache-2.0

//! Whole-package acceptance for the bundled Key & Tempo plugin
//! (013-key-and-tempo-plugin, Phase 2 Foundational, contracts/
//! key-tempo-plugin.md §1 "P1"-"P3"): manifest validity, byte-identical
//! licence copies, and that `main.luau` calls only the public schema
//! surface (SC-008).

use std::path::{Path, PathBuf};

use modplayer_capability_gateway::api::RequestKind;
use modplayer_capability_gateway::manifest;
use modplayer_core::plugins::bundled;

const IDENTIFIER: &str = "org.modplayer.key-tempo";

/// P1/P2 (contracts/key-tempo-plugin.md §1): the manifest validates,
/// declares API 1.4, exactly the five required permissions (no
/// optional), and the package is `packages()`'s second element.
#[test]
fn key_tempo_package_is_valid() {
    let packages = bundled::packages();
    assert_eq!(
        packages.len(),
        2,
        "the bundled catalogue must hold exactly Section Loop and Key & Tempo"
    );
    let package = packages
        .into_iter()
        .find(|p| p.identifier == IDENTIFIER)
        .unwrap_or_else(|| unreachable!("Key & Tempo must be a bundled package"));
    assert!(!package.fixture, "Key & Tempo is a real bundled package");

    let m = manifest::parse_and_validate(package.manifest_toml, true)
        .unwrap_or_else(|e| unreachable!("Key & Tempo manifest must validate: {e}"));
    assert_eq!(m.identifier.as_str(), IDENTIFIER);
    assert_eq!(m.api.major, 1);
    assert_eq!(m.api.min_minor, 4);
    assert_eq!(m.license, "MIT OR Apache-2.0");
    assert_eq!(m.source, "bundled");

    let mut required: Vec<&str> = m.required.iter().map(|p| p.permission.name()).collect();
    required.sort_unstable();
    let mut expected_required = [
        "audio.effects",
        "ui.panel",
        "ui.shortcuts",
        "state.track",
        "playback.observe",
    ];
    expected_required.sort_unstable();
    assert_eq!(required, expected_required);
    assert!(
        m.optional.is_empty(),
        "Key & Tempo declares no optional permission"
    );
    for entry in &m.required {
        assert!(
            !entry.justification.trim().is_empty(),
            "{} must carry a non-empty justification",
            entry.permission.name()
        );
    }

    let bundled_packages = bundled::packages();
    assert_eq!(
        bundled_packages[1].identifier, IDENTIFIER,
        "packages()[1] must be Key & Tempo (declaration order, R8)"
    );
}

/// P1: the package's licence copies are byte-identical to the workspace
/// root's.
#[test]
fn key_tempo_licences_match_root() {
    let root = workspace_root();
    let package_dir = root.join("plugins/bundled").join(IDENTIFIER);

    for name in ["LICENSE-MIT", "LICENSE-APACHE"] {
        let root_bytes = std::fs::read(root.join(name))
            .unwrap_or_else(|e| unreachable!("root {name} must exist: {e}"));
        let package_bytes = std::fs::read(package_dir.join(name))
            .unwrap_or_else(|e| unreachable!("package {name} must exist: {e}"));
        assert_eq!(
            root_bytes, package_bytes,
            "plugins/bundled/{IDENTIFIER}/{name} must be byte-identical to the workspace root copy"
        );
    }
}

/// P3/SC-008: every `api.<ns>.<method>(` identifier `main.luau` calls is
/// a `(namespace, method)` pair the generated schema actually serves —
/// plus the runtime locals `api.on`, `api.ready`, `api.log.*` — and the
/// script never calls `api.debug_probe`.
#[test]
fn key_tempo_uses_only_public_api() {
    let package = bundled::packages()
        .into_iter()
        .find(|p| p.identifier == IDENTIFIER)
        .unwrap_or_else(|| unreachable!("Key & Tempo must be a bundled package"));

    assert!(
        !package.entry.contains("debug_probe"),
        "the shipped plugin must never reference api.debug_probe (research R11)"
    );

    let schema_pairs: Vec<(&str, &str)> = RequestKind::ALL.iter().map(|r| r.lua_path()).collect();
    let runtime_locals = ["on", "ready", "log"];

    let mut found_any = false;
    for call in find_api_calls(package.entry) {
        found_any = true;
        if runtime_locals.contains(&call.namespace) {
            continue;
        }
        assert!(
            schema_pairs
                .iter()
                .any(|(ns, method)| *ns == call.namespace && *method == call.method),
            "main.luau calls api.{}.{}(...) which is not in the generated schema",
            call.namespace,
            call.method
        );
    }
    assert!(
        found_any,
        "the scan must find at least one api.<ns>.<method>( call to be meaningful"
    );
}

struct ApiCall<'a> {
    namespace: &'a str,
    method: &'a str,
}

/// Minimal hand-rolled scanner for `api.<ident>.<ident>(` — mirrors
/// `bundled_section_loop.rs`'s own `find_api_calls`.
fn find_api_calls(source: &str) -> Vec<ApiCall<'_>> {
    let bytes = source.as_bytes();
    let mut calls = Vec::new();
    let mut i = 0usize;
    while let Some(rel) = source[i..].find("api.") {
        let start = i + rel;
        let mut cursor = start + 4; // past "api."
        let ns_start = cursor;
        while cursor < bytes.len() && is_ident_byte(bytes[cursor]) {
            cursor += 1;
        }
        let namespace = &source[ns_start..cursor];
        if namespace.is_empty() || cursor >= bytes.len() || bytes[cursor] != b'.' {
            i = start + 4;
            continue;
        }
        cursor += 1; // past '.'
        let method_start = cursor;
        while cursor < bytes.len() && is_ident_byte(bytes[cursor]) {
            cursor += 1;
        }
        let method = &source[method_start..cursor];
        let mut paren = cursor;
        while paren < bytes.len() && (bytes[paren] as char).is_whitespace() {
            paren += 1;
        }
        if !method.is_empty() && paren < bytes.len() && bytes[paren] == b'(' {
            calls.push(ApiCall { namespace, method });
        }
        i = start + 4;
    }
    calls
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn workspace_root() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `crates/modplayer-core`; the workspace root
    // is two levels up.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| unreachable!("CARGO_MANIFEST_DIR must have two parents"))
        .to_path_buf()
}
