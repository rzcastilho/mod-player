// SPDX-License-Identifier: MIT OR Apache-2.0

//! Whole-package acceptance for the bundled Section Loop plugin
//! (012-section-loop-plugin, Phase 6, contracts/section-loop-plugin.md
//! §1 "P1"-"P4"): discovery/enablement, exact manifest permissions,
//! that `main.luau` calls only the public schema surface (SC-008),
//! byte-identical licence copies, and full `@key` string coverage.
//!
//! 013-key-and-tempo-plugin (research R8): `bundled::packages()` now
//! also ships Key & Tempo alongside Section Loop — every test below
//! still looks Section Loop's own record up by identifier (never by
//! position or an exact package count), so it is unaffected; the
//! sibling `bundled_key_tempo.rs` covers the same acceptance shape for
//! the second package.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_capability_gateway::api::RequestKind;
use modplayer_capability_gateway::manifest;
use modplayer_core::plugins::PluginHost;
use modplayer_core::plugins::bundled;

const IDENTIFIER: &str = "org.modplayer.section-loop";

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-bundled-section-loop-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = std::fs::create_dir_all(&dir);
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `MODPLAYER_PLUGIN_STATE_DIR` is process-global (mirrors
/// `plugins_manifest_discovery.rs`'s own lock).
static PLUGIN_STATE_ENV_LOCK: Mutex<()> = Mutex::new(());

fn discover(fixtures_enabled: bool) -> (PluginHost, TempDir) {
    let dir = TempDir::new();
    let host = {
        let _guard = PLUGIN_STATE_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: narrowly scopes the mutation to the one synchronous
        // read `PluginStatePaths::resolve()` makes inside `discover()`,
        // serialized against every other test in this binary via the
        // lock above.
        unsafe { std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", dir.path()) };
        let host = PluginHost::discover(fixtures_enabled);
        unsafe { std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR") };
        host
    };
    (host, dir)
}

/// P1: the bundled package is always discovered, `fixture = false`, and
/// enabled by default with no fixtures involved (FR-001, FR-002).
#[test]
fn package_discovered_enabled_by_default() {
    let (host, _dir) = discover(false);
    let record = host
        .records()
        .iter()
        .find(|r| r.identifier.as_str() == IDENTIFIER)
        .unwrap_or_else(|| unreachable!("Section Loop must always be discovered"));
    assert!(
        record.enabled,
        "the bundled Section Loop package must be enabled by default"
    );
    assert!(
        record.manifest.is_ok(),
        "Section Loop's manifest must parse and validate: {:?}",
        record.manifest.as_ref().err()
    );
}

/// P2: exactly the 7 required + 1 optional permissions the contract
/// names, each with a non-empty justification.
#[test]
fn manifest_permissions_exact() {
    let package = bundled::packages()
        .into_iter()
        .find(|p| p.identifier == IDENTIFIER)
        .unwrap_or_else(|| unreachable!("Section Loop must be a bundled package"));
    let m = manifest::parse_and_validate(package.manifest_toml, true)
        .unwrap_or_else(|e| unreachable!("Section Loop manifest must validate: {e}"));

    assert_eq!(m.api.major, 1);
    assert_eq!(m.api.min_minor, 3);
    assert_eq!(m.license, "MIT OR Apache-2.0");

    let mut required: Vec<&str> = m.required.iter().map(|p| p.permission.name()).collect();
    required.sort_unstable();
    let mut expected_required = [
        "playback.observe",
        "transport.control",
        "markers.read",
        "markers.write",
        "ui.panel",
        "ui.overlay",
        "ui.shortcuts",
    ];
    expected_required.sort_unstable();
    assert_eq!(required, expected_required);

    let optional: Vec<&str> = m.optional.iter().map(|p| p.permission.name()).collect();
    assert_eq!(optional, vec!["analysis.read"]);

    for entry in m.required.iter().chain(m.optional.iter()) {
        assert!(
            !entry.justification.trim().is_empty(),
            "{} must carry a non-empty justification",
            entry.permission.name()
        );
    }
}

/// P3/SC-008: every `api.<ns>.<method>(` identifier `main.luau` calls is
/// a `(namespace, method)` pair the generated schema actually serves —
/// plus the runtime locals `api.on`, `api.ready`, `api.log.*`,
/// `api.capabilities`, which carry no schema entry.
#[test]
fn script_calls_only_schema_requests() {
    let package = bundled::packages()
        .into_iter()
        .find(|p| p.identifier == IDENTIFIER)
        .unwrap_or_else(|| unreachable!("Section Loop must be a bundled package"));

    let schema_pairs: Vec<(&str, &str)> = RequestKind::ALL.iter().map(|r| r.lua_path()).collect();
    let runtime_locals = ["on", "ready", "log", "capabilities"];

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

/// Minimal hand-rolled scanner for `api.<ident>.<ident>(` (std only, no
/// regex crate dependency — plan.md's "regex-free string scanning").
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
        // Skip whitespace before the call parenthesis (none used in this
        // script, but tolerate it).
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

/// P1: the package's licence copies are byte-identical to the workspace
/// root's.
#[test]
fn license_files_match_workspace() {
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

fn workspace_root() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `crates/modplayer-core`; the workspace root
    // is two levels up.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| unreachable!("CARGO_MANIFEST_DIR must have two parents"))
        .to_path_buf()
}

/// FR-018: every `"@key"` the script references in a widget label, a
/// `set_status`/`update_widget` text argument, an action label, or an
/// overlay `text =`/`label =` field resolves to an entry in the
/// manifest's `[strings.en-US]` table.
#[test]
fn strings_cover_every_at_key() {
    let package = bundled::packages()
        .into_iter()
        .find(|p| p.identifier == IDENTIFIER)
        .unwrap_or_else(|| unreachable!("Section Loop must be a bundled package"));
    let m = manifest::parse_and_validate(package.manifest_toml, true)
        .unwrap_or_else(|e| unreachable!("Section Loop manifest must validate: {e}"));
    let table = m
        .strings
        .get("en-US")
        .unwrap_or_else(|| unreachable!("[strings.en-US] must exist"));

    let mut missing = Vec::new();
    for key in find_at_keys(package.entry) {
        if !table.contains_key(key) {
            missing.push(key);
        }
    }
    assert!(
        missing.is_empty(),
        "main.luau references @key(s) missing from [strings.en-US]: {missing:?}"
    );
    assert!(!table.is_empty(), "[strings.en-US] must not be empty");
}

/// Scans quoted Lua string literals of the shape `"@ident"` /
/// `'@ident'` — every `@key` reference in `main.luau` is a bare literal
/// like `"@status_no_region"`, never built by concatenation (per the
/// contract, per-slot keys are literal too: `"@status_cue_owned_" .. n`
/// is a *prefix* concatenation, so this scan also special-cases that one
/// known pattern by checking the numbered variants directly).
fn find_at_keys(source: &str) -> Vec<&str> {
    // These four are literal *prefixes* the script concatenates with a
    // slot number at runtime (`"@status_cue_owned_" .. n`), never a
    // complete key on their own — the per-slot expansion below supplies
    // the real keys.
    const CONCAT_PREFIXES: [&str; 4] = [
        "status_cue_owned_",
        "cue_label_",
        "action_set_cue_",
        "action_jump_cue_",
    ];
    let mut keys = Vec::new();
    for quote in ['"', '\''] {
        let mut i = 0usize;
        while let Some(rel) = source[i..].find(quote) {
            let start = i + rel + 1;
            if let Some(end_rel) = source[start..].find(quote) {
                let end = start + end_rel;
                let literal = &source[start..end];
                if let Some(key) = literal.strip_prefix('@')
                    && !key.is_empty()
                    && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    && !CONCAT_PREFIXES.contains(&key)
                {
                    keys.push(key);
                }
                i = end + 1;
            } else {
                break;
            }
        }
    }
    // The one concatenated family this script builds at runtime
    // (`"@status_cue_owned_" .. n`, `n` in 1..=8) is not a bare literal,
    // so it is added explicitly here — the manifest declares all eight.
    if source.contains("\"@status_cue_owned_\"") {
        for n in 1..=8 {
            keys.push(match n {
                1 => "status_cue_owned_1",
                2 => "status_cue_owned_2",
                3 => "status_cue_owned_3",
                4 => "status_cue_owned_4",
                5 => "status_cue_owned_5",
                6 => "status_cue_owned_6",
                7 => "status_cue_owned_7",
                8 => "status_cue_owned_8",
                _ => unreachable!(),
            });
        }
    }
    if source.contains("\"@cue_label_\"") {
        for n in 1..=8 {
            keys.push(match n {
                1 => "cue_label_1",
                2 => "cue_label_2",
                3 => "cue_label_3",
                4 => "cue_label_4",
                5 => "cue_label_5",
                6 => "cue_label_6",
                7 => "cue_label_7",
                8 => "cue_label_8",
                _ => unreachable!(),
            });
        }
    }
    if source.contains("\"@action_set_cue_\"") {
        for n in 1..=8 {
            keys.push(match n {
                1 => "action_set_cue_1",
                2 => "action_set_cue_2",
                3 => "action_set_cue_3",
                4 => "action_set_cue_4",
                5 => "action_set_cue_5",
                6 => "action_set_cue_6",
                7 => "action_set_cue_7",
                8 => "action_set_cue_8",
                _ => unreachable!(),
            });
        }
    }
    if source.contains("\"@action_jump_cue_\"") {
        for n in 1..=8 {
            keys.push(match n {
                1 => "action_jump_cue_1",
                2 => "action_jump_cue_2",
                3 => "action_jump_cue_3",
                4 => "action_jump_cue_4",
                5 => "action_jump_cue_5",
                6 => "action_jump_cue_6",
                7 => "action_jump_cue_7",
                8 => "action_jump_cue_8",
                _ => unreachable!(),
            });
        }
    }
    keys
}
