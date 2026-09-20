// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings search tests (contracts/ui-surface.md, SC-008):
//! `modplayer_core::settings_registry::search` is a case-insensitive
//! substring match over each descriptor's resolved title + description,
//! comfortably under the 50 ms per-keystroke budget (target <1 ms).

use std::time::Instant;

use modplayer_core::settings_registry::search;

#[test]
fn ceiling_query_finds_the_limiter_setting() {
    let results = search("ceiling");
    assert!(
        results.iter().any(|d| d.id == "audio.limiter_ceiling"),
        "\"ceiling\" should surface audio.limiter_ceiling"
    );
}

#[test]
fn ceiling_query_is_case_insensitive() {
    let results = search("CEIL");
    assert!(
        results.iter().any(|d| d.id == "audio.limiter_ceiling"),
        "\"CEIL\" should surface audio.limiter_ceiling regardless of case"
    );
}

#[test]
fn unmatched_query_returns_nothing() {
    assert!(search("zzz").is_empty());
}

#[test]
fn search_completes_well_under_the_fifty_millisecond_budget() {
    // A single per-keystroke search, timed the way it actually runs (one
    // call per keystroke), must land comfortably inside the 50 ms budget
    // (SC-008, target <1 ms) — even a debug build, cold cache included.
    let start = Instant::now();
    let _ = search("ceiling");
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 50,
        "one search took {elapsed:?}, expected well under the 50ms per-keystroke budget"
    );
}
