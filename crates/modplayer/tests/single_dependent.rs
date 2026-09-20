// SPDX-License-Identifier: MIT OR Apache-2.0

//! T046: `crates/modplayer-audio-source-connect` is the only crate
//! depending on `librespot-*`; `crates/modplayer` is its only dependent
//! (Constitution IV, plan.md's dependency graph). Guards this with
//! `cargo metadata` rather than eyeballing every `Cargo.toml` by hand.
//!
//! T010 (005-now-playing-waveform, research R16) extends this guard to the
//! two new dependencies: `symphonia` (receiver only — direct decode for
//! exact `SeekedTo::actual_ts`/packet `ts`, research R3) and
//! `thread-priority` (receiver's decode-ahead thread and `modplayer-core`'s
//! analysis thread only, research R9).

use std::process::Command;

const RECEIVER_CRATE: &str = "modplayer-audio-source-connect";
const SYMPHONIA_CRATE: &str = "symphonia";
const THREAD_PRIORITY_CRATE: &str = "thread-priority";

fn cargo_metadata_packages() -> Vec<serde_json::Value> {
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
    metadata["packages"]
        .as_array()
        .unwrap_or_else(|| panic!("metadata JSON has no `packages` array"))
        .clone()
}

/// Every workspace member (other than `dep_name` itself, were it a member)
/// whose manifest declares a direct dependency on `dep_name`.
fn dependents_of(packages: &[serde_json::Value], dep_name: &str) -> Vec<String> {
    let mut dependents: Vec<String> = packages
        .iter()
        .filter(|package| package["name"].as_str() != Some(dep_name))
        .filter(|package| {
            package["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|dep| dep["name"].as_str() == Some(dep_name))
        })
        .map(|package| package["name"].as_str().unwrap_or_default().to_string())
        .collect();
    dependents.sort();
    dependents
}

#[test]
fn receiver_crate_has_single_dependent() {
    let packages = cargo_metadata_packages();
    assert_eq!(
        dependents_of(&packages, RECEIVER_CRATE),
        vec!["modplayer".to_string()],
        "only the `modplayer` binary may depend on `{RECEIVER_CRATE}` (Constitution IV)"
    );
}

#[test]
fn symphonia_is_confined_to_the_receiver_crate() {
    let packages = cargo_metadata_packages();
    assert_eq!(
        dependents_of(&packages, SYMPHONIA_CRATE),
        vec![RECEIVER_CRATE.to_string()],
        "only `{RECEIVER_CRATE}` may depend on `{SYMPHONIA_CRATE}` directly (005-now-playing-waveform, research R16)"
    );
}

#[test]
fn thread_priority_is_confined_to_core_and_receiver() {
    let packages = cargo_metadata_packages();
    assert_eq!(
        dependents_of(&packages, THREAD_PRIORITY_CRATE),
        vec![RECEIVER_CRATE.to_string(), "modplayer-core".to_string()],
        "only `modplayer-core` and `{RECEIVER_CRATE}` may depend on `{THREAD_PRIORITY_CRATE}` (005-now-playing-waveform, research R9/R16)"
    );
}
