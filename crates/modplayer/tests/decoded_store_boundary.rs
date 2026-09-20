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

/// T093 (008-effect-chain-and-built-in-nodes): mechanical enforcement of
/// Constitution V's "no audio ever leaves the engine" rule for the new
/// `modplayer-effects`/`modplayer-engine` surface (FR-013, SC-009): no
/// public item of either crate may write to a file or socket, or hand a
/// raw sample slice to a caller beyond `Processor::render`'s device
/// buffer parameter. A small, reviewed allow-list covers the RT-internal
/// bus/spectrum plumbing that must stay `pub` across the effects/engine
/// crate boundary but is never called from `modplayer-core`,
/// `modplayer-ui` or `modplayer` — any *new* sample-slice-returning
/// public item must be added here deliberately, never slip in silently.
mod no_sample_sink {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// `(crate name, path relative to its `src/`, fn name)` for every
    /// `pub fn` already reviewed and known to return a raw sample slice.
    /// Each exists only to move frames between `Processor::render`'s own
    /// pipeline stages inside the engine; none is ever called from
    /// `modplayer-core`, `modplayer-ui` or `modplayer`.
    const ALLOWED_SLICE_RETURNS: &[(&str, &str, &str)] = &[
        // `ChainRt`'s ping-pong bus: written by `fill_from_source`,
        // consumed by `Processor::render` itself, contracts/
        // engine-effect-chain.md §3.
        ("modplayer-effects", "rt/chain.rs", "input_buffer_mut"),
        ("modplayer-effects", "rt/chain.rs", "process"),
        // The FFT magnitude bands are quantised into `RtShared::
        // spectrum_bits` immediately after this call; never handed
        // outward as samples (research R9).
        ("modplayer-effects", "spectrum.rs", "bands"),
    ];

    /// Literal needles with no legitimate reason to appear anywhere in
    /// this real-time DSP surface (Constitution I forbids I/O under
    /// `render` outright; Constitution V forbids any sample sink).
    const IO_NEEDLES: &[&str] = &[
        "std::fs::File",
        "fs::File",
        "std::net::TcpStream",
        "net::TcpStream",
        "io::Write",
    ];

    /// Crates whose `src/` this test scans.
    const SCANNED_CRATES: &[&str] = &["modplayer-effects", "modplayer-engine"];

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

    /// Extracts `(fn_name, signature_text)` for every `pub fn` in
    /// `content`, where `signature_text` runs from `fn` up to (but
    /// excluding) the first `{` or `;` that follows — enough to see a
    /// return type even when the parameter list wraps onto extra lines.
    fn pub_fn_signatures(content: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut rest = content;
        while let Some(idx) = rest.find("pub fn ") {
            let tail = &rest[idx + "pub fn ".len()..];
            let name_end = tail
                .find(|c: char| c == '(' || c == '<' || c.is_whitespace())
                .unwrap_or(tail.len());
            let name = tail[..name_end].to_string();
            let sig_end = tail.find(['{', ';']).unwrap_or(tail.len());
            out.push((name, tail[..sig_end].to_string()));
            rest = &tail[sig_end.min(tail.len())..];
        }
        out
    }

    #[test]
    fn effects_crate_exposes_no_sample_sink() {
        let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or_else(|| panic!("CARGO_MANIFEST_DIR has no parent"));

        let mut io_violations = Vec::new();
        let mut slice_violations = Vec::new();

        for crate_name in SCANNED_CRATES {
            let src_dir = crates_dir.join(crate_name).join("src");
            let mut files = Vec::new();
            collect_rs_files(&src_dir, &mut files);
            for file in files {
                let contents = fs::read_to_string(&file)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", file.display()));
                let rel_path = file
                    .strip_prefix(&src_dir)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");

                for needle in IO_NEEDLES {
                    if contents.contains(needle) {
                        io_violations
                            .push(format!("{crate_name}/src/{rel_path}: found `{needle}`"));
                    }
                }

                for (name, sig) in pub_fn_signatures(&contents) {
                    let returns_slice = sig.contains("-> &[f32]")
                        || sig.contains("-> &mut [f32]")
                        || sig.contains("-> Vec<f32>");
                    if !returns_slice {
                        continue;
                    }
                    let allowed = ALLOWED_SLICE_RETURNS.iter().any(
                        |&(allowed_crate, allowed_path, allowed_name)| {
                            allowed_crate == *crate_name
                                && allowed_path == rel_path
                                && allowed_name == name
                        },
                    );
                    if !allowed {
                        slice_violations.push(format!(
                            "{crate_name}/src/{rel_path}: `pub fn {name}` returns a raw sample \
                             slice and is not on the reviewed allow-list"
                        ));
                    }
                }
            }
        }

        assert!(
            io_violations.is_empty(),
            "file/socket I/O found in real-time DSP surface (Constitution I/V):\n{}",
            io_violations.join("\n")
        );
        assert!(
            slice_violations.is_empty(),
            "unreviewed sample-slice-returning public item(s) (Constitution V, FR-013, \
             SC-009) — add a justified entry to ALLOWED_SLICE_RETURNS or make it non-`pub`:\n{}",
            slice_violations.join("\n")
        );
    }
}

/// T064 (009-plugin-runtime-and-permissions, plan.md § Project Structure,
/// Constitution V): neither new crate exposes sample data. Unlike
/// `no_sample_sink` above, no allow-list is needed — nothing in either
/// crate has a legitimate reason to return a raw sample slice, so the
/// list starts (and should stay) empty.
mod plugin_no_sample_sink {
    use std::fs;
    use std::path::{Path, PathBuf};

    const SCANNED_CRATES: &[&str] = &["modplayer-capability-gateway", "modplayer-plugin-runtime"];

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

    /// Extracts `(fn_name, signature_text)` for every `pub fn` in
    /// `content` (mirrors `no_sample_sink::pub_fn_signatures`).
    fn pub_fn_signatures(content: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut rest = content;
        while let Some(idx) = rest.find("pub fn ") {
            let tail = &rest[idx + "pub fn ".len()..];
            let name_end = tail
                .find(|c: char| c == '(' || c == '<' || c.is_whitespace())
                .unwrap_or(tail.len());
            let name = tail[..name_end].to_string();
            let sig_end = tail.find(['{', ';']).unwrap_or(tail.len());
            out.push((name, tail[..sig_end].to_string()));
            rest = &tail[sig_end.min(tail.len())..];
        }
        out
    }

    #[test]
    fn plugin_crates_expose_no_sample_sink() {
        let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or_else(|| panic!("CARGO_MANIFEST_DIR has no parent"));

        let mut violations = Vec::new();

        for crate_name in SCANNED_CRATES {
            let src_dir = crates_dir.join(crate_name).join("src");
            let mut files = Vec::new();
            collect_rs_files(&src_dir, &mut files);
            for file in files {
                let contents = fs::read_to_string(&file)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", file.display()));
                let rel_path = file
                    .strip_prefix(&src_dir)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");

                for (name, sig) in pub_fn_signatures(&contents) {
                    let returns_slice = sig.contains("-> &[f32]")
                        || sig.contains("-> &mut [f32]")
                        || sig.contains("-> Vec<f32>")
                        || sig.contains("-> Box<[f32]>");
                    if returns_slice {
                        violations.push(format!(
                            "{crate_name}/src/{rel_path}: `pub fn {name}` returns a raw sample \
                             slice (Constitution V) — no capability request/event may carry \
                             sample data"
                        ));
                    }
                }
            }
        }

        assert!(
            violations.is_empty(),
            "sample-data-exposing public item(s) found in the plugin runtime/gateway crates \
             (Constitution V, plan.md § Constitution Check):\n{}",
            violations.join("\n")
        );
    }
}
