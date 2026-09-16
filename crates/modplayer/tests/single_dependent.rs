// SPDX-License-Identifier: MIT OR Apache-2.0

//! T046: `crates/modplayer-audio-source-connect` is the only crate
//! depending on `librespot-*`; `crates/modplayer` is its only dependent
//! (Constitution IV, plan.md's dependency graph). Guards this with
//! `cargo metadata` rather than eyeballing every `Cargo.toml` by hand.

use std::process::Command;

const RECEIVER_CRATE: &str = "modplayer-audio-source-connect";

#[test]
fn receiver_crate_has_single_dependent() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .unwrap_or_else(|e| panic!("`cargo metadata` failed to run: {e}"));
    assert!(
        output.status.success(),
        "`cargo metadata` exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("`cargo metadata` produced invalid JSON: {e}"));
    let packages = metadata["packages"]
        .as_array()
        .unwrap_or_else(|| panic!("metadata JSON has no `packages` array"));

    let mut dependents: Vec<String> = Vec::new();
    for package in packages {
        let name = package["name"].as_str().unwrap_or_default();
        if name == RECEIVER_CRATE {
            continue;
        }
        let depends_on_receiver = package["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|dep| dep["name"].as_str() == Some(RECEIVER_CRATE));
        if depends_on_receiver {
            dependents.push(name.to_string());
        }
    }

    assert_eq!(
        dependents,
        vec!["modplayer".to_string()],
        "only the `modplayer` binary may depend on `{RECEIVER_CRATE}` (Constitution IV)"
    );
}
