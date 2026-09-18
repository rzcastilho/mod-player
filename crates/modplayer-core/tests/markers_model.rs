// SPDX-License-Identifier: MIT OR Apache-2.0

//! `TrackMarkers` model tests (006, data-model.md §1.5).

use modplayer_audio_source::TrackId;
use modplayer_core::markers::{
    CueSlot, MAX_MARKERS, MarkerError, MarkerKind, PaletteIndex, TrackMarkers,
};
use proptest::prelude::*;

const RATE: u32 = 44_100;
const LEN: u64 = RATE as u64 * 180;

fn track() -> TrackId {
    TrackId::new("spotify:track:markers-model-test").unwrap_or_else(|_| unreachable!())
}

fn markers() -> TrackMarkers {
    TrackMarkers::new(track(), RATE, LEN)
}

/// I3: moving `A` past `B` swaps the two markers' *kinds* (not their ids)
/// so names/colours stay with their own marker (FR-007).
#[test]
fn a_after_b_swaps_kinds_keeps_names() {
    let mut m = markers();
    let (region, a_id) = m.set_loop_a(1_000).unwrap_or_else(|_| unreachable!());
    let (_, b_id) = m.set_loop_b(2_000).unwrap_or_else(|_| unreachable!());
    m.rename(a_id, "A-name").unwrap_or_else(|_| unreachable!());
    m.rename(b_id, "B-name").unwrap_or_else(|_| unreachable!());

    // Drag A past B.
    m.move_marker(a_id, 3_000)
        .unwrap_or_else(|_| unreachable!());

    let region = m.region(region).unwrap_or_else(|| unreachable!());
    assert_eq!(
        region.a,
        Some(b_id),
        "the marker at the lower position is now A"
    );
    assert_eq!(
        region.b,
        Some(a_id),
        "the marker at the higher position is now B"
    );

    let b_marker = m.marker(b_id).unwrap_or_else(|| unreachable!());
    assert!(matches!(b_marker.kind, MarkerKind::RegionStart { .. }));
    assert_eq!(
        b_marker.name, "B-name",
        "names stay with their own marker id"
    );

    let a_marker = m.marker(a_id).unwrap_or_else(|| unreachable!());
    assert!(matches!(a_marker.kind, MarkerKind::RegionEnd { .. }));
    assert_eq!(a_marker.name, "A-name");
}

/// I3: equal positions never swap.
#[test]
fn equal_positions_do_not_swap() {
    let mut m = markers();
    let (region, a_id) = m.set_loop_a(1_000).unwrap_or_else(|_| unreachable!());
    let (_, b_id) = m.set_loop_b(1_000).unwrap_or_else(|_| unreachable!());

    let region = m.region(region).unwrap_or_else(|| unreachable!());
    assert_eq!(region.a, Some(a_id));
    assert_eq!(region.b, Some(b_id));
}

/// `is_armable` requires >= `max(1, rate/1000)` frames (~1 ms); a region
/// shorter than that refuses to arm.
#[test]
fn region_under_1ms_is_not_armable() {
    let mut m = markers();
    let (region, _) = m.set_loop_a(1_000).unwrap_or_else(|_| unreachable!());
    let _ = m.set_loop_b(1_000 + 10).unwrap_or_else(|_| unreachable!()); // 10 frames << 44.1
    assert_eq!(m.arm(region), Err(MarkerError::RegionTooShort));
}

/// A region exactly at the ~1 ms floor is armable, and its effective
/// crossfade shrinks to fit (contracts/engine-loop.md §5).
#[test]
fn region_3ms_is_armable_with_3ms_effective_crossfade() {
    let mut m = markers();
    let three_ms_frames = 3 * RATE as u64 / 1_000; // 132 frames
    let (region, _) = m.set_loop_a(1_000).unwrap_or_else(|_| unreachable!());
    let _ = m
        .set_loop_b(1_000 + three_ms_frames)
        .unwrap_or_else(|_| unreachable!());
    assert!(m.arm(region).is_ok());

    let five_ms_configured = 5 * RATE as u64 / 1_000;
    m.set_crossfade_ms(region, 5)
        .unwrap_or_else(|_| unreachable!());
    let r = m.region(region).unwrap_or_else(|| unreachable!());
    let effective = r.effective_crossfade_frames(&m, RATE);
    assert_eq!(
        effective,
        modplayer_engine::loop_math::effective_crossfade(
            five_ms_configured,
            1_000,
            1_000 + three_ms_frames
        )
    );
    assert_eq!(
        effective, three_ms_frames,
        "shrunk to the 3ms region itself"
    );
}

/// I6: arming a region disarms whichever other region was armed.
#[test]
fn only_one_region_armed() {
    let mut m = markers();
    let (region1, _) = m.set_loop_a(1_000).unwrap_or_else(|_| unreachable!());
    let _ = m.set_loop_b(2_000).unwrap_or_else(|_| unreachable!());
    let region2 = m.new_loop_region();
    let _ = m.set_loop_a(3_000).unwrap_or_else(|_| unreachable!());
    let _ = m.set_loop_b(4_000).unwrap_or_else(|_| unreachable!());

    m.arm(region1).unwrap_or_else(|_| unreachable!());
    assert!(m.region(region1).unwrap_or_else(|| unreachable!()).armed);

    m.arm(region2).unwrap_or_else(|_| unreachable!());
    assert!(!m.region(region1).unwrap_or_else(|| unreachable!()).armed);
    assert!(m.region(region2).unwrap_or_else(|| unreachable!()).armed);
}

/// `arm` always sets `wraps` to 0 (I6/I7) — including across a
/// disarm/re-arm cycle, so a stale count from a previous armed session
/// never leaks into the next one.
#[test]
fn arm_resets_wraps() {
    let mut m = markers();
    let (region, _) = m.set_loop_a(1_000).unwrap_or_else(|_| unreachable!());
    let _ = m.set_loop_b(2_000).unwrap_or_else(|_| unreachable!());

    m.arm(region).unwrap_or_else(|_| unreachable!());
    assert_eq!(m.region(region).unwrap_or_else(|| unreachable!()).wraps, 0);

    m.disarm();
    m.arm(region).unwrap_or_else(|_| unreachable!());
    assert_eq!(m.region(region).unwrap_or_else(|| unreachable!()).wraps, 0);
}

/// I9: deleting a region endpoint clears that side and disarms the
/// region, leaving it incomplete.
#[test]
fn delete_endpoint_makes_region_incomplete_and_disarmed() {
    let mut m = markers();
    let (region, a_id) = m.set_loop_a(1_000).unwrap_or_else(|_| unreachable!());
    let _ = m.set_loop_b(2_000).unwrap_or_else(|_| unreachable!());
    m.arm(region).unwrap_or_else(|_| unreachable!());

    m.delete(a_id).unwrap_or_else(|_| unreachable!());

    let r = m.region(region).unwrap_or_else(|| unreachable!());
    assert!(r.a.is_none());
    assert!(!r.armed);
    assert!(!r.is_complete());
}

/// I9: deleting the last remaining endpoint removes the region entirely.
#[test]
fn delete_last_endpoint_removes_region() {
    let mut m = markers();
    let (region, a_id) = m.set_loop_a(1_000).unwrap_or_else(|_| unreachable!());

    m.delete(a_id).unwrap_or_else(|_| unreachable!());

    assert!(m.region(region).is_none());
    assert!(m.regions().is_empty());
}

/// `set_len_frames` (FR-018, SC-012): only markers beyond the new,
/// shorter length are clamped down to it and flagged `clamped`; a marker
/// already inside the new length is untouched (position and `clamped`
/// both unchanged).
#[test]
fn set_len_flags_clamped_markers_only() {
    let new_len = LEN / 4;
    let mut m = markers();
    let inside = m.add_point(new_len / 2).unwrap_or_else(|_| unreachable!());
    let at_new_len = m.add_point(new_len).unwrap_or_else(|_| unreachable!());
    let beyond = m.add_point(LEN - 1).unwrap_or_else(|_| unreachable!());

    m.set_len_frames(new_len);

    let inside_marker = m.marker(inside).unwrap_or_else(|| unreachable!());
    assert_eq!(
        inside_marker.position,
        new_len / 2,
        "already inside: unmoved"
    );
    assert!(!inside_marker.clamped, "was never beyond the new length");

    let at_len_marker = m.marker(at_new_len).unwrap_or_else(|| unreachable!());
    assert_eq!(
        at_len_marker.position, new_len,
        "exactly at the new length: unmoved"
    );
    assert!(
        !at_len_marker.clamped,
        "a marker already at (not beyond) the new length is not flagged"
    );

    let beyond_marker = m.marker(beyond).unwrap_or_else(|| unreachable!());
    assert_eq!(
        beyond_marker.position, new_len,
        "clamped down to the new length"
    );
    assert!(beyond_marker.clamped, "was beyond the new length");

    // Growing back past every marker's position clears no `clamped` flags
    // by itself (only `move_marker`/`set_loop_*`/`set_cue` do, on the
    // *next* explicit set) — `set_len_frames` only ever adds flags.
    m.set_len_frames(LEN);
    assert!(
        m.marker(beyond).unwrap_or_else(|| unreachable!()).clamped,
        "growing the length back does not retroactively clear `clamped`"
    );
}

/// I1: every creation path (point, a fresh region endpoint, a fresh cue)
/// refuses at the 64-marker limit (SC-005, FR-002), counting every kind.
#[test]
fn limit_is_64_across_all_kinds() {
    let mut m = markers();
    for i in 0..MAX_MARKERS {
        m.add_point(i as u64)
            .unwrap_or_else(|e| unreachable!("marker {i}: {e}"));
    }
    assert_eq!(m.count(), MAX_MARKERS);

    assert_eq!(m.add_point(0), Err(MarkerError::LimitReached));
    assert_eq!(m.set_loop_a(0).unwrap_err(), MarkerError::LimitReached);
    assert_eq!(
        m.set_cue(CueSlot::new(1).unwrap_or_else(|| unreachable!()), 0)
            .unwrap_err(),
        MarkerError::LimitReached
    );
}

/// Move-only paths (`I`/`O` on an existing endpoint, `Shift+n` on an
/// occupied slot) never check the limit, even sitting exactly at 64
/// (contracts/marker-service.md §7, data-model.md §1.5's I1 note).
#[test]
fn move_paths_never_hit_limit() {
    let mut m = markers();
    let _ = m.set_loop_a(100).unwrap_or_else(|e| unreachable!("{e}"));
    let _ = m.set_loop_b(200).unwrap_or_else(|e| unreachable!("{e}"));
    let cue_slot = CueSlot::new(1).unwrap_or_else(|| unreachable!());
    let _ = m
        .set_cue(cue_slot, 300)
        .unwrap_or_else(|e| unreachable!("{e}"));
    // 2 region endpoints + 1 cue + 61 points = 64.
    for i in 0..(MAX_MARKERS - 3) {
        m.add_point(1_000 + i as u64)
            .unwrap_or_else(|e| unreachable!("point {i}: {e}"));
    }
    assert_eq!(m.count(), MAX_MARKERS);

    assert!(
        m.set_loop_a(150).is_ok(),
        "moving A must not check the limit"
    );
    assert!(
        m.set_loop_b(250).is_ok(),
        "moving B must not check the limit"
    );
    assert!(
        m.set_cue(cue_slot, 350).is_ok(),
        "moving an occupied cue slot must not check the limit"
    );
    assert_eq!(m.count(), MAX_MARKERS, "moves create no new marker");

    // A genuinely new marker still refuses.
    assert_eq!(m.add_point(0), Err(MarkerError::LimitReached));
}

/// Default names/colours by kind (data-model.md §1.3): a `Point`'s default
/// name counts prior point markers (`"Marker 1"`, `"Marker 2"`, …) with
/// colour index `1`; region markers default to an empty name and colour
/// `0`; cues default to an empty name and colour `2`.
#[test]
fn default_names_and_colors_by_kind() {
    let mut m = markers();
    let p1 = m.add_point(100).unwrap_or_else(|e| unreachable!("{e}"));
    let p2 = m.add_point(200).unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(
        m.marker(p1).unwrap_or_else(|| unreachable!()).name,
        modplayer_core::tr_args("marker-default-name", &[("n", "1".to_string())])
    );
    assert_eq!(
        m.marker(p2).unwrap_or_else(|| unreachable!()).name,
        modplayer_core::tr_args("marker-default-name", &[("n", "2".to_string())])
    );
    assert_eq!(
        m.marker(p1).unwrap_or_else(|| unreachable!()).color,
        PaletteIndex::new(1)
    );

    let (_region, a_id) = m.set_loop_a(1_000).unwrap_or_else(|e| unreachable!("{e}"));
    let a = m.marker(a_id).unwrap_or_else(|| unreachable!());
    assert_eq!(a.name, "");
    assert_eq!(a.color, PaletteIndex::new(0));

    let slot = CueSlot::new(3).unwrap_or_else(|| unreachable!());
    let cue_id = m
        .set_cue(slot, 3_000)
        .unwrap_or_else(|e| unreachable!("{e}"));
    let cue = m.marker(cue_id).unwrap_or_else(|| unreachable!());
    assert_eq!(cue.name, "");
    assert_eq!(cue.color, PaletteIndex::new(2));
}

/// `rename` (data-model.md §1.5): trims surrounding whitespace, truncates
/// past `MAX_NAME_CHARS`, an empty rename on a `Point` is a no-op (keeps
/// the old name), and a non-`Point` kind accepts an empty rename (clears
/// its custom name back to the role-only label).
#[test]
fn rename_trims_truncates_and_keeps_old_on_empty_point() {
    let mut m = markers();
    let p = m.add_point(1_000).unwrap_or_else(|e| unreachable!("{e}"));
    m.rename(p, "  Verse  ")
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(m.marker(p).unwrap_or_else(|| unreachable!()).name, "Verse");

    let long = "x".repeat(100);
    m.rename(p, &long).unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(
        m.marker(p)
            .unwrap_or_else(|| unreachable!())
            .name
            .chars()
            .count(),
        modplayer_core::markers::MAX_NAME_CHARS
    );

    let before = m.marker(p).unwrap_or_else(|| unreachable!()).name.clone();
    m.rename(p, "   ").unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(
        m.marker(p).unwrap_or_else(|| unreachable!()).name,
        before,
        "an empty rename on a Point keeps the old name"
    );

    let (_region, a_id) = m.set_loop_a(2_000).unwrap_or_else(|e| unreachable!("{e}"));
    m.rename(a_id, "A-label")
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(
        m.marker(a_id).unwrap_or_else(|| unreachable!()).name,
        "A-label"
    );
    m.rename(a_id, "").unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(
        m.marker(a_id).unwrap_or_else(|| unreachable!()).name,
        "",
        "a non-Point kind accepts an empty rename"
    );
}

/// I5 (006 US4, FR-013): a slot holds at most one cue; `set_cue` on an
/// already-occupied slot moves the existing marker (same id) rather than
/// creating a second one, and each slot is independent of the others.
#[test]
fn cue_slot_is_unique_and_set_moves() {
    let mut m = markers();
    let slot1 = CueSlot::new(1).unwrap_or_else(|| unreachable!());
    let slot2 = CueSlot::new(2).unwrap_or_else(|| unreachable!());

    let id1 = m
        .set_cue(slot1, 1_000)
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(m.count(), 1);
    assert_eq!(m.cue(slot1).map(|c| c.position), Some(1_000));

    // Re-setting the same slot moves the same marker, not a new one.
    let id1_again = m
        .set_cue(slot1, 5_000)
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(id1_again, id1, "same marker id, not a new one");
    assert_eq!(m.count(), 1, "no new marker was created");
    assert_eq!(m.cue(slot1).map(|c| c.position), Some(5_000));

    // A different slot is independent.
    let id2 = m
        .set_cue(slot2, 2_000)
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_ne!(id2, id1);
    assert_eq!(m.count(), 2);
    assert_eq!(
        m.cue(slot1).map(|c| c.position),
        Some(5_000),
        "slot1 unaffected"
    );
    assert_eq!(m.cue(slot2).map(|c| c.position), Some(2_000));
}

/// Deleting a cue marker frees its slot: the slot reports empty
/// (`cue(slot) == None`) and a later `set_cue` on it creates a fresh
/// marker rather than resurrecting the deleted one.
#[test]
fn delete_cue_frees_slot() {
    let mut m = markers();
    let slot = CueSlot::new(4).unwrap_or_else(|| unreachable!());
    let id = m
        .set_cue(slot, 1_000)
        .unwrap_or_else(|e| unreachable!("{e}"));

    m.delete(id).unwrap_or_else(|e| unreachable!("{e}"));

    assert!(m.cue(slot).is_none(), "slot is empty after delete");
    assert_eq!(m.count(), 0);

    let new_id = m
        .set_cue(slot, 2_000)
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_ne!(new_id, id, "a fresh marker, not the deleted one");
    assert_eq!(m.cue(slot).map(|c| c.position), Some(2_000));
}

proptest! {
    /// I4: every set position clamps to `0..=len_frames` (FR-019).
    #[test]
    fn positions_clamp_to_len(pos in 0u64..10_000_000, len in 1u64..1_000_000) {
        let id = TrackId::new("spotify:track:proptest-clamp").unwrap_or_else(|_| unreachable!());
        let mut m = TrackMarkers::new(id, RATE, len);
        let marker_id = m.add_point(pos).unwrap_or_else(|e| unreachable!("{e}"));
        let position = m.position_of(marker_id).unwrap_or_else(|| unreachable!());
        prop_assert!(position <= len);
        prop_assert_eq!(position, pos.min(len));
    }
}

/// One step of a random op sequence for `markers_stay_sorted_after_any_
/// sequence` — every mutation the model exposes, applied against whatever
/// currently exists (an index modulo the current count, so it is always a
/// real marker once any exist).
#[derive(Debug, Clone)]
enum Op {
    AddPoint(u64),
    SetLoopA(u64),
    SetLoopB(u64),
    SetCue(u8, u64),
    MoveExisting(usize, u64),
    DeleteExisting(usize),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..LEN).prop_map(Op::AddPoint),
        (0..LEN).prop_map(Op::SetLoopA),
        (0..LEN).prop_map(Op::SetLoopB),
        (1u8..=8, 0..LEN).prop_map(|(slot, pos)| Op::SetCue(slot, pos)),
        (0usize..64, 0..LEN).prop_map(|(idx, pos)| Op::MoveExisting(idx, pos)),
        (0usize..64).prop_map(Op::DeleteExisting),
    ]
}

proptest! {
    /// I1/I2 hold after *any* sequence of mutations, not just the
    /// hand-picked scenarios above (contracts/marker-service.md §7).
    #[test]
    fn markers_stay_sorted_after_any_sequence(ops in prop::collection::vec(op_strategy(), 0..50)) {
        let mut m = markers();
        for op in ops {
            match op {
                Op::AddPoint(pos) => {
                    let _ = m.add_point(pos);
                }
                Op::SetLoopA(pos) => {
                    let _ = m.set_loop_a(pos);
                }
                Op::SetLoopB(pos) => {
                    let _ = m.set_loop_b(pos);
                }
                Op::SetCue(slot, pos) => {
                    if let Some(slot) = CueSlot::new(slot) {
                        let _ = m.set_cue(slot, pos);
                    }
                }
                Op::MoveExisting(idx, pos) => {
                    if !m.markers().is_empty() {
                        let id = m.markers()[idx % m.markers().len()].id;
                        let _ = m.move_marker(id, pos);
                    }
                }
                Op::DeleteExisting(idx) => {
                    if !m.markers().is_empty() {
                        let id = m.markers()[idx % m.markers().len()].id;
                        let _ = m.delete(id);
                    }
                }
            }
            prop_assert!(m.markers().len() <= MAX_MARKERS, "I1: over the limit");
            for pair in m.markers().windows(2) {
                prop_assert!(
                    (pair[0].position, pair[0].id) <= (pair[1].position, pair[1].id),
                    "I2: not sorted by (position, id)"
                );
            }
        }
    }
}
