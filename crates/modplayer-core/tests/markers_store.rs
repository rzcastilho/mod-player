// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `markers::store` tests (006, data-model.md §4, contracts/
//! marker-service.md §3): the atomic write/crash-mid-write safety
//! (SC-009/SC-014), the read-rule table (missing / unreadable / newer
//! schema / oversize / field repairs), the writer's failure report, and
//! two Constitution VIII proptests (arbitrary-state round trip; the
//! track-id hex encoding).

use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_audio_source::TrackId;
use modplayer_core::markers::store::{
    LoadWarning, PersistJob, StoreEvent, TrackStatePaths, decode_track_id, encode, encode_track_id,
    load, spawn_writer,
};
use modplayer_core::markers::{
    CueSlot, MarkerId, MarkerKind, PaletteIndex, RegionId, RepeatCount, TrackMarkers,
};
use modplayer_core::plugins::PluginIdTable;
use proptest::prelude::*;

const RATE: u32 = 44_100;
const LEN: u64 = RATE as u64 * 180;

fn temp_paths(tag: &str) -> TrackStatePaths {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "modplayer-markers-store-test-{tag}-{}-{}",
        std::process::id(),
        unique
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    TrackStatePaths::with_dir(dir)
}

fn track(tag: &str) -> TrackId {
    TrackId::new(format!("spotify:track:markers-store-{tag}")).unwrap_or_else(|_| unreachable!())
}

// -- round trip (Constitution VIII) -----------------------------------------

/// A bounded op applied to a `TrackMarkers` (indices are taken modulo the
/// current marker/region count at *apply* time, so the same sequence
/// stays meaningful regardless of what earlier ops did — mirrors
/// `queue_proptest.rs`'s pattern).
#[derive(Debug, Clone)]
enum Op {
    AddPoint(u64),
    NewRegion,
    SetLoopA(u64),
    SetLoopB(u64),
    SetCue(u8, u64),
    MoveExisting(usize, u64),
    Rename(usize, String),
    Recolor(usize, u8),
    Delete(usize),
    SetCrossfade(usize, u8),
    SetRepeat(usize, Option<u16>),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0u64..LEN).prop_map(Op::AddPoint),
        Just(Op::NewRegion),
        (0u64..LEN).prop_map(Op::SetLoopA),
        (0u64..LEN).prop_map(Op::SetLoopB),
        (1u8..=8, 0u64..LEN).prop_map(|(slot, pos)| Op::SetCue(slot, pos)),
        (0usize..80, 0u64..LEN).prop_map(|(i, pos)| Op::MoveExisting(i, pos)),
        (0usize..80, "[a-zA-Z0-9 ]{0,16}").prop_map(|(i, name)| Op::Rename(i, name)),
        (0usize..80, 0u8..10).prop_map(|(i, c)| Op::Recolor(i, c)),
        (0usize..80).prop_map(Op::Delete),
        (0usize..16, 0u8..60).prop_map(|(i, ms)| Op::SetCrossfade(i, ms)),
        (0usize..16, prop::option::of(1u16..1_200)).prop_map(|(i, n)| Op::SetRepeat(i, n)),
    ]
}

fn nth_marker_id(state: &TrackMarkers, idx: usize) -> Option<MarkerId> {
    let markers = state.markers();
    if markers.is_empty() {
        return None;
    }
    Some(markers[idx % markers.len()].id)
}

fn nth_region_id(state: &TrackMarkers, idx: usize) -> Option<RegionId> {
    let regions = state.regions();
    if regions.is_empty() {
        return None;
    }
    Some(regions[idx % regions.len()].id)
}

fn apply(state: &mut TrackMarkers, op: Op) {
    match op {
        Op::AddPoint(pos) => {
            let _ = state.add_point(pos);
        }
        Op::NewRegion => {
            let _ = state.new_loop_region();
        }
        Op::SetLoopA(pos) => {
            let _ = state.set_loop_a(pos);
        }
        Op::SetLoopB(pos) => {
            let _ = state.set_loop_b(pos);
        }
        Op::SetCue(slot, pos) => {
            if let Some(slot) = CueSlot::new(slot) {
                let _ = state.set_cue(slot, pos);
            }
        }
        Op::MoveExisting(idx, pos) => {
            if let Some(id) = nth_marker_id(state, idx) {
                let _ = state.move_marker(id, pos);
            }
        }
        Op::Rename(idx, name) => {
            if let Some(id) = nth_marker_id(state, idx) {
                let _ = state.rename(id, &name);
            }
        }
        Op::Recolor(idx, color) => {
            if let Some(id) = nth_marker_id(state, idx) {
                let _ = state.recolor(id, PaletteIndex::new(color));
            }
        }
        Op::Delete(idx) => {
            if let Some(id) = nth_marker_id(state, idx) {
                let _ = state.delete(id);
            }
        }
        Op::SetCrossfade(idx, ms) => {
            if let Some(id) = nth_region_id(state, idx) {
                let _ = state.set_crossfade_ms(id, ms);
            }
        }
        Op::SetRepeat(idx, times) => {
            if let Some(id) = nth_region_id(state, idx) {
                let repeat = match times {
                    None => RepeatCount::Infinite,
                    Some(n) => RepeatCount::Times(n),
                };
                let _ = state.set_repeat(id, repeat);
            }
        }
    }
}

type MarkerSnapshot = (MarkerId, MarkerKind, u64, String, PaletteIndex, bool, bool);
type RegionSnapshot = (
    RegionId,
    Option<MarkerId>,
    Option<MarkerId>,
    u8,
    RepeatCount,
);

/// Everything `encode`/`load` round-trip (data-model.md §4) — deliberately
/// excludes `armed`/`wraps` (never serialized, data-model.md §1.4) and
/// `clamped` (session-only, data-model.md §1.3; a marker loaded from a
/// file whose position needed no clamping is never flagged).
#[allow(clippy::type_complexity)]
fn snapshot(
    state: &TrackMarkers,
) -> (
    TrackId,
    u32,
    u64,
    Option<RegionId>,
    Vec<MarkerSnapshot>,
    Vec<RegionSnapshot>,
) {
    (
        state.track().clone(),
        state.sample_rate(),
        state.len_frames(),
        state.current_region(),
        state
            .markers()
            .iter()
            .map(|m| {
                (
                    m.id,
                    m.kind,
                    m.position,
                    m.name.clone(),
                    m.color,
                    m.transient,
                    m.visible,
                )
            })
            .collect(),
        state
            .regions()
            .iter()
            .map(|r| (r.id, r.a, r.b, r.crossfade_ms, r.repeat))
            .collect(),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// `armed`/`wraps`/`clamped` excluded (see `snapshot`'s doc comment) —
    /// every other field of an arbitrary op-sequence-built state survives
    /// `encode` -> disk -> `load` unchanged (SC-009's persistence contract,
    /// data-model.md §4).
    #[test]
    fn round_trip_is_identity_for_any_state(ops in prop::collection::vec(op_strategy(), 0..=80)) {
        let id = track("round-trip");
        let mut state = TrackMarkers::new(id.clone(), RATE, LEN);
        for op in ops {
            apply(&mut state, op);
        }
        let before = snapshot(&state);

        let paths = temp_paths("round-trip");
        std::fs::write(paths.file_for(&id), encode(&state, &PluginIdTable::new())).expect("write state");
        let outcome = load(&paths, &id, RATE, LEN, &mut PluginIdTable::new());

        prop_assert_eq!(outcome.warning, None);
        prop_assert_eq!(before, snapshot(&outcome.state));
    }

    /// research R10: reversible, case-free (the default macOS/Windows
    /// filesystems are case-insensitive; hex sidesteps aliasing that a
    /// case-preserving encoding of the base62 id would risk).
    #[test]
    fn track_id_hex_encoding_round_trips_and_is_case_free(s in "[a-zA-Z0-9:_-]{1,64}") {
        let id = TrackId::new(s).unwrap_or_else(|_| unreachable!());
        let hex = encode_track_id(&id);
        prop_assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        prop_assert_eq!(decode_track_id(&hex), Some(id.clone()));
        prop_assert_eq!(decode_track_id(&hex.to_uppercase()), Some(id));
    }
}

// -- crash safety / read rules (contracts/marker-service.md §3, §6) --------

/// SC-009/SC-014: a `.tmp` left behind by an interrupted write (no
/// `rename` ever happened) is never consulted — the previous, fully
/// written file loads exactly as before.
#[test]
fn crash_mid_write_keeps_previous_file() {
    let paths = temp_paths("crash-mid-write");
    let id = track("crash");
    let mut state = TrackMarkers::new(id.clone(), RATE, LEN);
    state.add_point(1_000).unwrap_or_else(|_| unreachable!());
    let path = paths.file_for(&id);
    std::fs::write(&path, encode(&state, &PluginIdTable::new())).expect("write previous good file");
    std::fs::write(path.with_extension("json.tmp"), b"garbage-mid-write")
        .expect("write leftover tmp");

    let outcome = load(&paths, &id, RATE, LEN, &mut PluginIdTable::new());

    assert_eq!(outcome.warning, None);
    assert_eq!(
        outcome.state.count(),
        1,
        "the previous file, not the tmp, was read"
    );
}

#[test]
fn missing_file_loads_empty_without_warning() {
    let paths = temp_paths("missing");
    let id = track("missing");

    let outcome = load(&paths, &id, RATE, LEN, &mut PluginIdTable::new());

    assert_eq!(outcome.warning, None);
    assert_eq!(outcome.state.count(), 0);
    assert!(outcome.rewrite_allowed);
}

#[test]
fn unparseable_loads_empty_with_unreadable() {
    let paths = temp_paths("unparseable");
    let id = track("unparseable");
    std::fs::write(paths.file_for(&id), b"{ not json").expect("write garbage");

    let outcome = load(&paths, &id, RATE, LEN, &mut PluginIdTable::new());

    assert_eq!(outcome.warning, Some(LoadWarning::Unreadable));
    assert_eq!(outcome.state.count(), 0);
    assert!(
        outcome.rewrite_allowed,
        "unreadable may be rewritten immediately"
    );
}

#[test]
fn oversize_file_is_unreadable() {
    let paths = temp_paths("oversize");
    let id = track("oversize");
    // A valid-JSON but > 64 KiB file: padding inside a `name` field keeps
    // it parseable were the size check absent, isolating the size rule.
    let padding = "x".repeat(70 * 1024);
    let body = format!(
        r#"{{"schema_version":1,"track_id":"{id}","sample_rate":44100,"len_frames":100,
        "markers":[{{"id":1,"kind":"point","position":0,"name":"{padding}","color":0,
        "owner":"host","transient":false,"visible":true}}],"regions":[]}}"#,
        id = id.as_str(),
        padding = padding,
    );
    std::fs::write(paths.file_for(&id), body).expect("write oversize file");

    let outcome = load(&paths, &id, RATE, LEN, &mut PluginIdTable::new());

    assert_eq!(outcome.warning, Some(LoadWarning::Unreadable));
    assert_eq!(outcome.state.count(), 0);
}

#[test]
fn newer_schema_loads_empty_and_is_not_rewritten_until_mutation() {
    let paths = temp_paths("newer-schema");
    let id = track("newer-schema");
    std::fs::write(
        paths.file_for(&id),
        format!(
            r#"{{"schema_version":99,"track_id":"{}","sample_rate":44100,"len_frames":100}}"#,
            id.as_str()
        ),
    )
    .expect("write newer-schema file");

    let outcome = load(&paths, &id, RATE, LEN, &mut PluginIdTable::new());

    assert_eq!(outcome.warning, Some(LoadWarning::NewerSchema));
    assert_eq!(outcome.state.count(), 0);
    assert!(
        !outcome.rewrite_allowed,
        "not rewritten until the caller's own dirty-gated flush sees a real mutation"
    );
    assert!(
        !outcome.state.is_dirty(),
        "a freshly loaded empty state is not itself unsaved, so a dirty-gated flush stays quiet"
    );
}

/// data-model.md §4: unknown top-level/marker keys are ignored; out-of-
/// range `color`/`crossfade_ms`/`repeat`/`position` are clamped rather
/// than rejected.
#[test]
fn unknown_keys_ignored_and_out_of_range_clamped() {
    let paths = temp_paths("unknown-keys");
    let id = track("unknown-keys");
    let len_frames = 1_000u64;
    let body = format!(
        r#"{{
            "schema_version": 1,
            "track_id": "{id}",
            "sample_rate": 44100,
            "len_frames": {len_frames},
            "next_marker_id": 5,
            "next_region_id": 2,
            "current_region": null,
            "totally_unknown_top_level_field": 42,
            "markers": [
                {{"id": 1, "kind": "point", "position": 999999999, "name": "n",
                  "color": 250, "owner": "host", "transient": false, "visible": true,
                  "unknown_marker_field": "x"}}
            ],
            "regions": [
                {{"id": 1, "a": null, "b": null, "crossfade_ms": 250, "repeat": 99999,
                  "unknown_region_field": true}}
            ]
        }}"#,
        id = id.as_str(),
        len_frames = len_frames,
    );
    std::fs::write(paths.file_for(&id), body).expect("write");

    let outcome = load(&paths, &id, RATE, len_frames, &mut PluginIdTable::new());

    assert_eq!(
        outcome.warning, None,
        "unknown keys alone are not a parse failure"
    );
    let marker = outcome
        .state
        .markers()
        .first()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(marker.position, len_frames, "clamped to the current length");
    assert!(
        marker.clamped,
        "the saved position exceeded the current length"
    );
    assert_eq!(marker.color.get(), 7, "color clamped into 0..=7");

    let region = outcome
        .state
        .regions()
        .first()
        .unwrap_or_else(|| unreachable!());
    assert_eq!(region.crossfade_ms, 50, "crossfade_ms clamped into 0..=50");
    assert_eq!(
        region.repeat,
        RepeatCount::Times(1_000),
        "repeat clamped into 1..=1000"
    );
}

/// data-model.md §4: a region marker whose `region` doesn't exist becomes
/// a plain point; a region whose `a`/`b` doesn't name a marker of the
/// matching kind has that side repaired to `None` rather than the whole
/// file being rejected.
#[test]
fn dangling_region_refs_are_repaired() {
    let paths = temp_paths("dangling-refs");
    let id = track("dangling-refs");
    let body = format!(
        r#"{{
            "schema_version": 1,
            "track_id": "{id}",
            "sample_rate": 44100,
            "len_frames": 4410000,
            "next_marker_id": 10,
            "next_region_id": 5,
            "current_region": 1,
            "markers": [
                {{"id": 1, "kind": "region_start", "region": 99, "position": 1000,
                  "name": "", "color": 0, "owner": "host", "transient": false, "visible": true}},
                {{"id": 2, "kind": "region_start", "region": 1, "position": 2000,
                  "name": "", "color": 0, "owner": "host", "transient": false, "visible": true}},
                {{"id": 3, "kind": "region_end", "region": 1, "position": 3000,
                  "name": "", "color": 0, "owner": "host", "transient": false, "visible": true}}
            ],
            "regions": [
                {{"id": 1, "a": 2, "b": 999, "crossfade_ms": 5, "repeat": null}}
            ]
        }}"#,
        id = id.as_str(),
    );
    std::fs::write(paths.file_for(&id), body).expect("write");

    let outcome = load(&paths, &id, RATE, LEN, &mut PluginIdTable::new());

    assert_eq!(outcome.warning, None);
    // Ids are crate-internal, so markers are found by the positions they
    // were seeded at rather than by reconstructing a raw `MarkerId`.
    let orphan = outcome
        .state
        .markers()
        .iter()
        .find(|m| m.position == 1_000)
        .unwrap_or_else(|| unreachable!());
    assert_eq!(
        orphan.kind,
        MarkerKind::Point,
        "orphaned region ref repaired to a point"
    );

    let region_start = outcome
        .state
        .markers()
        .iter()
        .find(|m| m.position == 2_000)
        .unwrap_or_else(|| unreachable!());
    let region_id = match region_start.kind {
        MarkerKind::RegionStart { region } => region,
        other => unreachable!("expected RegionStart, got {other:?}"),
    };

    let region = outcome
        .state
        .region(region_id)
        .unwrap_or_else(|| unreachable!());
    assert_eq!(region.a, Some(region_start.id), "a valid ref is kept");
    assert_eq!(region.b, None, "a dangling ref is repaired to None");
}

/// contracts/marker-service.md §3 rule 1: on a write failure the previous
/// file is left intact and a `StoreEvent::SaveFailed` is reported — no
/// automatic retry.
#[cfg(unix)]
#[test]
fn writer_reports_save_failed_and_leaves_previous_file() {
    use std::os::unix::fs::PermissionsExt;

    let paths = temp_paths("writer-fail");
    let id = track("writer-fail");
    let mut previous = TrackMarkers::new(id.clone(), RATE, LEN);
    previous.add_point(500).unwrap_or_else(|_| unreachable!());
    let path = paths.file_for(&id);
    std::fs::write(&path, encode(&previous, &PluginIdTable::new()))
        .expect("seed the previous file");

    let dir = path
        .parent()
        .unwrap_or_else(|| unreachable!())
        .to_path_buf();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).expect("chmod ro");
    let probe_blocked = std::fs::File::create(dir.join(".write-probe")).is_err();
    if !probe_blocked {
        // Running as root (or another context that ignores Unix
        // permissions, e.g. some sandboxes): nothing to exercise.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod rw");
        return;
    }

    let (notify_tx, notify_rx) = std::sync::mpsc::channel();
    let (job_tx, handle) = spawn_writer(notify_tx);
    let mut next = TrackMarkers::new(id.clone(), RATE, LEN);
    next.add_point(999).unwrap_or_else(|_| unreachable!());
    job_tx
        .send(PersistJob::Save {
            path: path.clone(),
            bytes: encode(&next, &PluginIdTable::new()),
        })
        .expect("send job");
    drop(job_tx);
    handle.join().expect("writer thread joins");

    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod rw");

    let event = notify_rx.try_recv();
    assert!(
        matches!(event, Ok(StoreEvent::SaveFailed { .. })),
        "expected a SaveFailed event, got is_ok={}",
        event.is_ok()
    );

    let outcome = load(&paths, &id, RATE, LEN, &mut PluginIdTable::new());
    assert_eq!(outcome.warning, None);
    assert_eq!(outcome.state.count(), 1, "the previous file is untouched");
}
