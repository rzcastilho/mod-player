// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T009 (005-now-playing-waveform): mechanical enforcement of Constitution
//! V's raw-sample boundary (contracts/decoded-store.md §4). `DecodedStore::
//! read_frames` is the only raw-sample accessor; this scans every
//! `crates/*/src/**/*.rs` and `crates/*/tests/**/*.rs` outside the
//! allow-listed source/engine crates for the literal call `read_frames(`
//! and fails the build if one is found. Must pass trivially before any
//! caller exists (this phase) and stay green through every later phase —
//! a failure here is a stop-the-line signal, never something to work
//! around.

use std::fs;
use std::path::{Path, PathBuf};

/// Crates allowed to call `DecodedStore::read_frames` (contracts/
/// decoded-store.md §4): the trait crate itself, its synthetic/scripted
/// test doubles, the receiver (writer thread + RT feed), and the engine.
const ALLOWED_CRATES: &[&str] = &[
    "modplayer-audio-source",
    "modplayer-audio-source-synthetic",
    "modplayer-audio-source-connect",
    "modplayer-engine",
];

/// This guard test's own file: its doc comments and string literals
/// legitimately discuss `read_frames(` without ever calling it, so it is
/// exempted from its own scan by name.
const SELF_FILE_NAME: &str = "decoded_store_boundary.rs";

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn read_frames_confined_to_allowed_crates() {
    // Built from two literals rather than one, so this test's own source
    // text never contains the contiguous needle it searches for.
    let needle = concat!("read_frames", "(");

    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| panic!("CARGO_MANIFEST_DIR has no parent"));
    assert!(
        crates_dir.ends_with("crates"),
        "expected the workspace's `crates/` directory, got {}",
        crates_dir.display()
    );

    let mut violations = Vec::new();
    let crate_dirs = fs::read_dir(crates_dir)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", crates_dir.display()));
    for entry in crate_dirs.flatten() {
        let crate_path = entry.path();
        if !crate_path.is_dir() {
            continue;
        }
        let crate_name = crate_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if ALLOWED_CRATES.contains(&crate_name) {
            continue;
        }

        for subdir in ["src", "tests"] {
            let dir = crate_path.join(subdir);
            if !dir.exists() {
                continue;
            }
            let mut files = Vec::new();
            collect_rs_files(&dir, &mut files);
            for file in files {
                if file.file_name().and_then(|n| n.to_str()) == Some(SELF_FILE_NAME) {
                    continue;
                }
                let contents = fs::read_to_string(&file)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", file.display()));
                if contents.contains(needle) {
                    violations.push(file.display().to_string());
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "raw `DecodedStore::read_frames` callers found outside {ALLOWED_CRATES:?} \
         (Constitution V, contracts/decoded-store.md §4):\n{}",
        violations.join("\n")
    );
}
