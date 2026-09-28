// SPDX-License-Identifier: MIT OR Apache-2.0

//! The literal scan (contracts/literal-scan.md, FR-018/FR-018a): walks
//! `src/**` (this crate) and `../modplayer/src/**` (the binary crate) for
//! colour constructors/constants, font-size literals, and panel-level
//! `ui.separator()` calls, per S1-S4. Exclusions are exactly S3: the token
//! module (`src/theme/**`), test code (`tests/` directories and
//! `#[cfg(test)]` blocks), and `use` statements / doc comments.
//!
//! S5 baseline: contracts/literal-scan.md quotes **17 hits across 9
//! files**, verified 2026-09-22. Running this scanner against this
//! worktree at the start of Phase 2 (also 2026-09-22, before Phases 4-7
//! land their call-site conversions per S2/S3 exactly as written) instead
//! measured **13 hits across 6 files**:
//! `plugin_overlays.rs` (3), `markers.rs` (4), `plugins_view.rs` (3),
//! `widgets/peak_meter.rs` (1), `widgets/initials.rs` (1),
//! `waveform/paint.rs` (1) — matching the call sites data-model.md §6 and
//! tasks.md T057-T064 name. The discrepancy from the contract's stated 17
//! is a stale estimate in the contract doc, not a scanner gap
//! (spot-checked with a plain-text grep for every S2 pattern across both
//! crates' `src/`, `theme/`/`tests/` excluded).
//!
//! **Phase 5 (US3, this session)** converted `markers.rs`'s cue-slot digit
//! and `waveform/paint.rs`'s placeholder label to `theme::mono_font_id()`,
//! dropping the count to **11 hits across 5 files** (`markers.rs` now 3,
//! `waveform/paint.rs` now 0). `EXPECTED_BASELINE_HITS` below tracks the
//! current count, updated alongside each recorded conversion — not a bug,
//! the expected, incrementally-shrinking failure for
//! `no_colour_or_font_literals_outside_theme` until T066 (US5) re-runs it
//! expecting **0**.

use std::fs;
use std::path::{Path, PathBuf};

/// S5: the measured starting count in this worktree (see module doc for
/// why this is 13, not the contract's stated 17). Update this only
/// alongside a recorded, deliberate call-site conversion (never to
/// silently accept a new hit).
///
/// **Phase 5 (US3, T037/T043):** `markers.rs`'s cue-slot digit
/// (`FontId::monospace(9.0)`) and `waveform/paint.rs`'s
/// "analysis unavailable" label (`FontId::proportional((h*0.35).max(10.0))`)
/// both now route through `theme::mono_font_id()` — two font-size literals
/// converted, 13 → 11.
///
/// **Phase 7 (US5, T057-T066, this session):** every remaining named
/// offender converted — `plugins_view.rs`'s three health-dot colours,
/// `widgets/peak_meter.rs`'s over-ceiling colour, `widgets/initials.rs`'s
/// avatar font size, `markers.rs`'s three focus/slot-digit whites,
/// `plugin_overlays.rs`'s glyph-texture tint and its now-token-routed
/// `secondary_font_id()` label font — 11 → **0**, the literal scan's true
/// zero-state (contracts/literal-scan.md).
const EXPECTED_BASELINE_HITS: usize = 0;

#[derive(Debug)]
struct Hit {
    path: PathBuf,
    line: usize,
    text: String,
    matched: &'static str,
}

impl std::fmt::Display for Hit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: {} ({}) — move this into crates/modplayer-ui/src/theme/ and reference a role (FR-018)",
            self.path.display(),
            self.line,
            self.text.trim(),
            self.matched,
        )
    }
}

/// S1: the two source roots this scan walks.
fn scan_roots() -> Vec<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    vec![
        manifest_dir.join("src"),
        manifest_dir.join("..").join("modplayer").join("src"),
    ]
}

/// S3.1: the token module is the sanctioned exception — never scanned.
/// S3.2: any `tests/` directory is test code — never scanned (this scan
/// itself lives under one).
fn is_excluded_dir_component(component: &std::ffi::OsStr) -> bool {
    component == "theme" || component == "tests"
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name()
                && is_excluded_dir_component(name)
            {
                continue;
            }
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// S2 colour: constructors, named constants (`TRANSPARENT` excluded — it
/// names no colour), `Rgba`/`Hsva`/`HsvaGamma`, and hex/byte-triple
/// literals.
fn colour_hit(line: &str) -> Option<&'static str> {
    const CTOR_PREFIXES: &[&str] = &[
        "Color32::from_rgb", // also matches from_rgb_additive, from_rgba_*
        "Color32::from_gray",
        "Color32::from_black_alpha",
        "Color32::from_white_alpha",
        "Color32::from_additive_luminance",
        "Rgba::from_",
        "Hsva::new",
        "HsvaGamma",
    ];
    for needle in CTOR_PREFIXES {
        if line.contains(needle) {
            return Some(needle);
        }
    }

    const NAMED_CONSTANTS: &[&str] = &[
        "Color32::WHITE",
        "Color32::BLACK",
        "Color32::RED",
        "Color32::GREEN",
        "Color32::BLUE",
        "Color32::YELLOW",
        "Color32::GRAY",
        "Color32::LIGHT_",
        "Color32::DARK_",
        "Color32::KHAKI",
        "Color32::GOLD",
        "Color32::BROWN",
        "Color32::ORANGE",
        "Color32::PLACEHOLDER",
        "Color32::DEBUG_COLOR",
    ];
    for needle in NAMED_CONSTANTS {
        if line.contains(needle) {
            return Some(needle);
        }
    }

    if contains_hex_colour_literal(line) {
        return Some("#rrggbb(aa) literal");
    }
    if contains_byte_triple_literal(line) {
        return Some("0xNN, 0xNN, 0xNN triple");
    }
    None
}

/// A `#` immediately followed by 6 or 8 hex digits (a `#rrggbb`/`#rrggbbaa`
/// string), not preceded by another hex digit (so `##ffffff` still counts
/// once, and this never matches inside a longer hex run).
fn contains_hex_colour_literal(line: &str) -> bool {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'#' {
            continue;
        }
        let rest = &line[i + 1..];
        let hex_len = rest.chars().take_while(|c| c.is_ascii_hexdigit()).count();
        if hex_len == 6 || hex_len == 8 {
            // Reject boundary hex digit right after (would be a longer run).
            let next_is_hex = rest
                .chars()
                .nth(hex_len)
                .is_some_and(|c| c.is_ascii_hexdigit());
            if !next_is_hex {
                return true;
            }
        }
    }
    false
}

/// `0xNN, 0xNN, 0xNN` — three consecutive 2-digit hex byte literals,
/// comma-separated (an inline colour triple such as `(0xC2, 0x51, 0x9A)`).
fn contains_byte_triple_literal(line: &str) -> bool {
    let mut positions: Vec<usize> = Vec::new();
    let mut search_from = 0usize;
    while let Some(rel) = line[search_from..].find("0x") {
        let abs = search_from + rel;
        let rest = &line[abs + 2..];
        let hex_len = rest.chars().take_while(|c| c.is_ascii_hexdigit()).count();
        if hex_len == 2 {
            positions.push(abs);
        }
        search_from = abs + 2;
    }
    if positions.len() < 3 {
        return false;
    }
    for window in positions.windows(3) {
        let between_ok = window.windows(2).all(|pair| {
            let (a_end, b_start) = (pair[0] + 4, pair[1]);
            b_start > a_end && line[a_end..b_start].trim().starts_with(',')
        });
        if between_ok {
            return true;
        }
    }
    false
}

/// S2 font-size: `FontId` constructors, a `.size` field assignment, and a
/// `const`/`let` binding named `*FONT_SIZE*`/`*TEXT_SIZE*`.
fn font_size_hit(line: &str) -> Option<&'static str> {
    const CTORS: &[&str] = &[
        "FontId::new(",
        "FontId::proportional(",
        "FontId::monospace(",
    ];
    for needle in CTORS {
        if line.contains(needle) {
            return Some(needle);
        }
    }

    let trimmed = line.trim_start();
    if let Some(dot) = trimmed.find(".size") {
        let after = &trimmed[dot + ".size".len()..].trim_start();
        if after.starts_with('=') && !after.starts_with("==") {
            return Some(".size = <literal>");
        }
    }

    if (trimmed.starts_with("const ") || trimmed.starts_with("let "))
        && (trimmed.contains("FONT_SIZE") || trimmed.contains("TEXT_SIZE"))
    {
        return Some("const/let *FONT_SIZE*/*TEXT_SIZE*");
    }
    None
}

/// U3 separator: `ui.separator()` / `Separator::default()` under
/// `crates/modplayer-ui/src/**` only (S2's "Separator" bullet is scoped to
/// this crate's src, not the binary crate).
fn separator_hit(line: &str) -> Option<&'static str> {
    if line.contains("ui.separator()") {
        return Some("ui.separator()");
    }
    if line.contains("Separator::default()") {
        return Some("Separator::default()");
    }
    None
}

/// S3.3: `use` statements and doc comments name no colour/size — skip them.
fn is_excluded_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("use ")
        || trimmed.starts_with("use\t")
        || trimmed == "use"
        || trimmed.starts_with("///")
        || trimmed.starts_with("//!")
}

/// S3.2: lines inside a `#[cfg(test)] mod … { … }` block are test code.
/// Tracks brace depth from the `mod` line that immediately follows a
/// `#[cfg(test)]` attribute (skipping blank/attribute lines in between)
/// until the block closes.
struct CfgTestTracker {
    pending_cfg_test: bool,
    in_test_mod: bool,
    depth: i32,
}

impl CfgTestTracker {
    fn new() -> Self {
        Self {
            pending_cfg_test: false,
            in_test_mod: false,
            depth: 0,
        }
    }

    /// Returns true if `line` is inside a `#[cfg(test)]` module and should
    /// be skipped. Must be called once per line, in order.
    fn advance(&mut self, line: &str) -> bool {
        let trimmed = line.trim();

        if self.in_test_mod {
            self.depth += line.matches('{').count() as i32;
            self.depth -= line.matches('}').count() as i32;
            let inside = true;
            if self.depth <= 0 {
                self.in_test_mod = false;
                self.depth = 0;
            }
            return inside;
        }

        if trimmed.contains("#[cfg(test)]") {
            self.pending_cfg_test = true;
            return false;
        }

        if self.pending_cfg_test {
            if trimmed.is_empty() || trimmed.starts_with('#') {
                // Stay pending across blank lines / stacked attributes.
                return false;
            }
            self.pending_cfg_test = false;
            if trimmed.contains("mod ") {
                self.in_test_mod = true;
                self.depth = line.matches('{').count() as i32 - line.matches('}').count() as i32;
                if self.depth <= 0 {
                    self.in_test_mod = false;
                    self.depth = 0;
                }
                return true;
            }
        }

        false
    }
}

fn scan_file(path: &Path, hits: &mut Vec<Hit>) {
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };
    let mut tracker = CfgTestTracker::new();
    for (idx, line) in contents.lines().enumerate() {
        if tracker.advance(line) {
            continue;
        }
        if is_excluded_line(line) {
            continue;
        }
        if let Some(matched) = colour_hit(line) {
            hits.push(Hit {
                path: path.to_path_buf(),
                line: idx + 1,
                text: line.to_string(),
                matched,
            });
        }
        if let Some(matched) = font_size_hit(line) {
            hits.push(Hit {
                path: path.to_path_buf(),
                line: idx + 1,
                text: line.to_string(),
                matched,
            });
        }
    }
}

fn scan_all_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in scan_roots() {
        collect_rs_files(&root, &mut files);
    }
    files
}

/// T1: zero colour/font-size literals outside `theme/`. **Expected to fail
/// with exactly `EXPECTED_BASELINE_HITS` hits until Phases 4-7 convert
/// every call site (S5) — a shrinking, recorded baseline, not a bug.**
#[test]
fn no_colour_or_font_literals_outside_theme() {
    let mut hits = Vec::new();
    for file in scan_all_files() {
        scan_file(&file, &mut hits);
    }

    if hits.len() != EXPECTED_BASELINE_HITS {
        let report: Vec<String> = hits.iter().map(Hit::to_string).collect();
        panic!(
            "expected exactly {} literal hit(s) (S5 baseline), found {}:\n{}",
            EXPECTED_BASELINE_HITS,
            hits.len(),
            report.join("\n"),
        );
    }
}

/// U3: no `ui.separator()` / `Separator::default()` anywhere under
/// `crates/modplayer-ui/src/**` (panel-to-panel separation uses spacing
/// tokens instead — research R20).
#[test]
fn no_separator_between_panels() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs_files(&manifest_dir.join("src"), &mut files);

    let mut hits = Vec::new();
    for file in files {
        let Ok(contents) = fs::read_to_string(&file) else {
            continue;
        };
        let mut tracker = CfgTestTracker::new();
        for (idx, line) in contents.lines().enumerate() {
            if tracker.advance(line) {
                continue;
            }
            if is_excluded_line(line) {
                continue;
            }
            if let Some(matched) = separator_hit(line) {
                hits.push(Hit {
                    path: file.clone(),
                    line: idx + 1,
                    text: line.to_string(),
                    matched,
                });
            }
        }
    }

    if !hits.is_empty() {
        let report: Vec<String> = hits.iter().map(Hit::to_string).collect();
        panic!(
            "expected zero ui.separator() sites, found {}:\n{}",
            hits.len(),
            report.join("\n"),
        );
    }
}

/// T015 (US3, Phase 5): `notifications.rs`'s new severity-colour/accent-bar
/// code (`severity_color`, the `Frame`/`painter.rect_filled` call site)
/// needs no scan carve-out — `scan_roots`/`collect_rs_files` already walk
/// every `.rs` file directly under this crate's `src/` (only `theme/` and
/// `tests/` are excluded, S3.1/S3.2), so `notifications.rs` was already
/// covered before this phase touched it. This pins that fact: if a future
/// change ever moved the module under an excluded directory, this test —
/// not a silently-widened `EXPECTED_BASELINE_HITS` — is what would catch
/// it.
#[test]
fn notifications_module_is_covered_by_the_literal_scan() {
    let files = scan_all_files();
    let covered = files.iter().any(|p| {
        p.file_name().and_then(|n| n.to_str()) == Some("notifications.rs")
            && p.parent()
                .and_then(|d| d.file_name())
                .and_then(|n| n.to_str())
                == Some("src")
    });
    assert!(
        covered,
        "expected crates/modplayer-ui/src/notifications.rs among the scanned files"
    );
}

/// S6 self-check: a broken walk (wrong path, both roots resolving to the
/// same directory, an early return swallowing every entry) must never
/// yield a vacuous pass.
#[test]
fn scan_actually_reaches_both_crates() {
    let files = scan_all_files();
    assert!(
        files.len() > 40,
        "expected > 40 .rs files across both crates, found {} — scan roots are broken",
        files.len()
    );

    let has_ui_crate_file = files
        .iter()
        .any(|p| p.components().any(|c| c.as_os_str() == "app.rs"));
    assert!(
        has_ui_crate_file,
        "expected to find crates/modplayer-ui/src/app.rs — modplayer-ui root is broken"
    );

    let has_binary_crate_file = files
        .iter()
        .any(|p| p.components().any(|c| c.as_os_str() == "main.rs"));
    assert!(
        has_binary_crate_file,
        "expected to find crates/modplayer/src/main.rs — modplayer root is broken"
    );
}
