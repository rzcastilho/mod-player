// SPDX-License-Identifier: MIT OR Apache-2.0

//! Loop-seam sample-accuracy and click-free-seam tests (006, US1,
//! contracts/engine-loop.md §4–§5).
//!
//! The harness (`build`) carries the full synthetic track's own audio as
//! its retained `DecodedStore` (mirroring `SyntheticHost::new`), so the
//! seam's incoming material read from the store is bit-identical to what
//! `source.fill` produces at the same frames — exactly what a real
//! `ConnectRtSource` guarantees for cached audio (contracts/engine-loop.md
//! §1). All tests here run at `device_rate == source_rate` (passthrough)
//! unless stated: `OutputStage::process`'s passthrough branch always
//! consumes exactly `out_frames` (`crates/modplayer-engine/src/
//! output_stage.rs`), so no leftover-carry frame ever straddles a render
//! boundary and `RtShared::clock_frames()` advances by exactly the
//! requested buffer size every render — letting these tests reason about
//! "source frames since start" without any resampling ambiguity.

use std::sync::Arc;

use modplayer_audio_source::{Availability, SourceEvent, SourceHost, TrackId, TrackRef};
use modplayer_audio_source_synthetic::track::{fill as track_fill, track_len_frames};
use modplayer_audio_source_synthetic::{DecodeScript, ScriptedHost, SyntheticSource};
use modplayer_engine::{
    CeilingDb, Command, Event, Processor, ProcessorConfig, RtShared, Transport, VolumePercent,
};
use proptest::prelude::*;
use rtrb::{Consumer, Producer, RingBuffer};

/// A fully-decoded store carrying the synthetic track's own audio end to
/// end (mirrors `SyntheticHost::new`), so the seam's incoming material is
/// bit-identical to what `source.fill` will later produce there.
fn full_store(rate: u32) -> Arc<modplayer_audio_source::DecodedStore> {
    let len_frames = track_len_frames(rate);
    let store = modplayer_audio_source::DecodedStore::new(rate, len_frames);
    let mut buf = vec![0.0f32; (len_frames * 2) as usize];
    let mut position = 0u64;
    track_fill(&mut buf, &mut position, rate);
    store.write_frames(0, &buf);
    store.set_complete(len_frames);
    store
}

/// Build a `Processor<SyntheticSource>` carrying the full synthetic
/// track's own audio as its retained store (T022, contracts/engine-
/// loop.md §6), at `source_rate`/`device_rate`, `buffer_frames` per
/// render, starting from `position_frames`, already `Playing`.
fn build(
    source_rate: u32,
    device_rate: u32,
    buffer_frames: usize,
    position_frames: u64,
) -> (
    Processor<SyntheticSource>,
    Producer<Command>,
    Consumer<Event>,
    Arc<RtShared>,
) {
    let store = full_store(source_rate);
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(4_096);
    let (event_tx, event_rx) = RingBuffer::<Event>::new(4_096);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate,
        device_rate,
        device_channels: 2,
        max_frames: buffer_frames,
        transport: Transport::Playing,
        position_frames,
        master_volume: VolumePercent::new(100),
        ceiling: CeilingDb::default(),
        shared: Arc::clone(&shared),
    };
    let source = SyntheticSource::with_store(source_rate, store);
    let processor = Processor::new(config, source, command_rx, event_tx);
    let _ = command_tx.push(Command::Play);
    (processor, command_tx, event_rx, shared)
}

/// Push the setters-then-commit sequence a real arm uses (contracts/
/// engine-loop.md §2): setters first, `LoopCommit` last.
fn arm(command_tx: &mut Producer<Command>, a: u64, b: u64, crossfade_frames: u32, repeat: u32) {
    let _ = command_tx.push(Command::LoopSetA(a));
    let _ = command_tx.push(Command::LoopSetB(b));
    let _ = command_tx.push(Command::LoopSetSeam {
        crossfade_frames,
        repeat,
    });
    let _ = command_tx.push(Command::LoopCommit { reset_wraps: true });
}

/// Drain every event currently queued, discarding it.
fn drain(event_rx: &mut Consumer<Event>) {
    while event_rx.pop().is_ok() {}
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]
    /// After >= 1 000 wraps, the published position always equals
    /// `loop_math::wrap_position(a, b, clock_frames)` exactly (0-sample
    /// drift, NFR-1.2/SC-002) — checked at *every* render, not just at a
    /// wrap's own boundary: a wrap frequently lands mid-buffer (its exact
    /// instant is sub-render), so comparing clock deltas *between wrap
    /// events* is itself off by up to a buffer's worth of frames; the
    /// `wrap_position` formula has no such ambiguity because it is
    /// re-derived fresh from the *global*, unambiguous `clock_frames`
    /// counter every single render.
    #[test]
    fn period_is_exact_after_1000_wraps(
        a in 0u64..=10_000,
        // >= 2 frames: `LoopCommit` deliberately refuses anything shorter
        // (`drain_commands`), so a 1-frame region never arms and the
        // published position stays linear — the wrap formula below would
        // then be asserting against a loop that does not exist.
        len_region in 2u64..=2_000,
        x_index in 0usize..5,
        buffer_index in 0usize..4,
    ) {
        const XS: [u32; 5] = [0, 1, 5, 20, 50];
        const BUFFERS: [usize; 4] = [64, 256, 1_024, 4_096];
        let x = XS[x_index];
        let buffer = BUFFERS[buffer_index];
        let b = a + len_region;

        let (mut processor, mut command_tx, mut event_rx, shared) =
            build(44_100, 44_100, buffer, 0);
        arm(&mut command_tx, a, b, x, 0);

        let mut out = vec![0.0f32; buffer * 2];
        let target_wraps: u32 = 1_000;
        let mut wraps_seen: u32 = 0;
        let mut renders = 0u64;
        while wraps_seen < target_wraps {
            processor.render(&mut out);
            renders += 1;
            let clock = shared.clock_frames();
            let expected = modplayer_engine::loop_math::wrap_position(a, b, clock);
            prop_assert_eq!(shared.position_frames(), expected);
            while let Ok(event) = event_rx.pop() {
                if let Event::LoopWrapped { .. } = event {
                    wraps_seen += 1;
                }
            }
            prop_assert!(renders < 5_000_000, "runaway: never reached {target_wraps} wraps");
        }
    }
}

/// Same identity, in source frames, when the device resamples (48 kHz
/// device / 44.1 kHz source) — `RtShared::clock_frames` is a source-rate
/// count, unaffected by the output stage's resampling.
#[test]
fn period_is_exact_under_resampling() {
    let a = 10_000u64;
    let b = 12_000u64;
    let x = 220u32;
    let buffer = 256usize;

    let (mut processor, mut command_tx, mut event_rx, shared) = build(44_100, 48_000, buffer, 0);
    arm(&mut command_tx, a, b, x, 0);

    let mut out = vec![0.0f32; buffer * 2];
    let mut wraps_seen = 0u32;
    for _ in 0..200_000u32 {
        processor.render(&mut out);
        let clock = shared.clock_frames();
        let expected = modplayer_engine::loop_math::wrap_position(a, b, clock);
        assert_eq!(shared.position_frames(), expected, "clock={clock}");
        while let Ok(event) = event_rx.pop() {
            if let Event::LoopWrapped { .. } = event {
                wraps_seen += 1;
            }
        }
        if wraps_seen >= 50 {
            break;
        }
    }
    assert!(wraps_seen >= 50, "wraps_seen={wraps_seen}");
}

/// max `|Δsample|` across 50 seams <= 1.5x the unlooped material's, at the
/// same gain (NFR-1.3, SC-001) — measured on the *actual* rendered output,
/// not just the pure `loop_math` formula (already proptested separately).
#[test]
fn seam_is_click_free() {
    let rate = 44_100u32;
    let a = 2_000u64;
    let b = 2_300u64;
    let x = 50u64;
    let buffer = 128usize;
    let wraps_target = 50u64;

    let (mut processor, mut command_tx, mut event_rx, _shared) = build(rate, rate, buffer, 0);
    arm(&mut command_tx, a, b, x as u32, 0);

    // Capture the whole continuous left-channel stream; at passthrough
    // (see module doc) it maps 1:1, in order, to the source-frame clock.
    let total_needed = a + wraps_target * (b - a) + 10;
    let mut captured: Vec<f32> = Vec::with_capacity(total_needed as usize + buffer);
    let mut out = vec![0.0f32; buffer * 2];
    let mut wraps_seen = 0u64;
    loop {
        processor.render(&mut out);
        for frame in out.chunks_exact(2) {
            captured.push(frame[0]);
        }
        while let Ok(event) = event_rx.pop() {
            if let Event::LoopWrapped { .. } = event {
                wraps_seen += 1;
            }
        }
        if wraps_seen >= wraps_target && captured.len() as u64 >= total_needed {
            break;
        }
        assert!(captured.len() < 50_000_000, "runaway capture");
    }

    // Baseline: max first-difference of the unlooped material, taken from
    // the pre-entry stretch `[0, a)` — the same synthetic sine, untouched
    // by the loop mechanism.
    let baseline = (1..a as usize)
        .map(|i| (captured[i] - captured[i - 1]).abs())
        .fold(0.0f32, f32::max);
    assert!(baseline > 0.0, "baseline must be nonzero on a sine segment");

    // Each of the `wraps_target` seams spans source frames
    // `[k*(b-a)+a-x, k*(b-a)+a)` (research R3's derivation, re-applied
    // independently here); include one frame past the end to cover the
    // seam-to-post-jump transition too.
    let mut max_seam_delta = 0.0f32;
    for k in 1..=wraps_target {
        let seam_start = k * (b - a) + a - x;
        let seam_end = k * (b - a) + a;
        for i in seam_start.saturating_sub(1)..=seam_end {
            if i == 0 || i as usize >= captured.len() {
                continue;
            }
            let delta = (captured[i as usize] - captured[i as usize - 1]).abs();
            max_seam_delta = max_seam_delta.max(delta);
        }
    }

    assert!(
        max_seam_delta <= 1.5 * baseline + 1e-6,
        "max_seam_delta={max_seam_delta} baseline={baseline}"
    );
}

/// A 3-frame region with a 5-frame configured crossfade still arms and
/// wraps with an exact period and full gapless coverage — proving the
/// crossfade was shrunk to fit the region rather than mis-clamped or
/// panicking (contracts/engine-loop.md §5's `effective_crossfade`,
/// exercised end to end through the engine).
#[test]
fn short_region_shrinks_crossfade() {
    let a = 1_000u64;
    let b = 1_003u64; // 3-frame region
    let configured = 5u32; // wider than the region
    let buffer = 8usize;

    let (mut processor, mut command_tx, mut event_rx, shared) = build(44_100, 44_100, buffer, 0);
    arm(&mut command_tx, a, b, configured, 0);

    let mut out = vec![0.0f32; buffer * 2];
    let mut wraps_seen = 0u32;
    let mut all_gapless = true;
    for _ in 0..10_000 {
        processor.render(&mut out);
        let clock = shared.clock_frames();
        let expected = modplayer_engine::loop_math::wrap_position(a, b, clock);
        assert_eq!(shared.position_frames(), expected);
        while let Ok(event) = event_rx.pop() {
            if let Event::LoopWrapped { gapless, .. } = event {
                wraps_seen += 1;
                all_gapless &= gapless;
            }
        }
        if wraps_seen >= 20 {
            break;
        }
    }
    assert!(wraps_seen >= 20, "wraps_seen={wraps_seen}");
    assert!(all_gapless, "cached audio must stay gapless");
}

/// `A == 0` forces `effective_crossfade` to 0 (`min(configured, b-a, a)`);
/// the loop still wraps with an exact period and reports `gapless == true`
/// (contracts/engine-loop.md §6).
#[test]
fn a_at_zero_hard_cuts() {
    let a = 0u64;
    let b = 500u64;
    let configured = 50u32;
    let buffer = 64usize;

    let (mut processor, mut command_tx, mut event_rx, shared) = build(44_100, 44_100, buffer, 0);
    arm(&mut command_tx, a, b, configured, 0);

    let mut out = vec![0.0f32; buffer * 2];
    let mut wraps_seen = 0u32;
    let mut all_gapless = true;
    for _ in 0..10_000 {
        processor.render(&mut out);
        let clock = shared.clock_frames();
        let expected = modplayer_engine::loop_math::wrap_position(a, b, clock);
        assert_eq!(shared.position_frames(), expected);
        while let Ok(event) = event_rx.pop() {
            if let Event::LoopWrapped { gapless, .. } = event {
                wraps_seen += 1;
                all_gapless &= gapless;
            }
        }
        if wraps_seen >= 20 {
            break;
        }
    }
    assert!(wraps_seen >= 20, "wraps_seen={wraps_seen}");
    assert!(all_gapless, "x == 0 is still an exact, gapless hard cut");
}

/// A `ScriptedRt` behind a `DecodeScript::Progressive` fill: the first
/// wrap (seam starting before the store covers `A`) reports
/// `gapless == false`; once the fill catches up, later wraps report
/// `gapless == true` (contracts/engine-loop.md §6, FR-008's uncached
/// fallback).
#[test]
fn uncached_seam_hard_cuts_and_reports_not_gapless() {
    let rate = 44_100u32;
    let a = 5_000u64;
    let b = 5_200u64;
    // Deliberately narrower than the region: leaves a genuine pre-seam
    // phase (150 frames) after every jump, spanning several renders, so
    // the *next* cycle's seam-start check happens in a later render than
    // the one that just jumped — giving the test room to call `poll()`
    // (advancing the progressive fill) strictly *between* the two,
    // rather than racing a same-render re-seam immediately after the
    // jump (which would see the same stale, pre-poll coverage).
    let crossfade_frames = 50u32;
    let buffer = 64usize;

    let mut host = ScriptedHost::new();
    host.script_decode(DecodeScript::Progressive {
        frames_per_tick: 6_000,
    });
    let track = TrackRef::new(
        TrackId::new("spotify:track:uncached-seam").unwrap_or_else(|_| unreachable!()),
        "Title",
        vec!["Artist".to_string()],
        None,
        None,
        200, // ms; len_frames = 200*44_100/1000 = 8_820 >= b
        Availability::Available,
    );
    host.emit(SourceEvent::TrackStarted {
        track,
        program: None,
        position_ms: 0,
        playing: true,
    });
    // `attach` snapshots `current_store` directly (set synchronously by
    // `emit`, not by `poll`) — the store starts `Filling`, 0 covered.
    let source = host.attach(a);

    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(4_096);
    let (event_tx, mut event_rx) = RingBuffer::<Event>::new(4_096);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: rate,
        device_rate: rate,
        device_channels: 2,
        max_frames: buffer,
        transport: Transport::Playing,
        position_frames: a,
        master_volume: VolumePercent::new(100),
        ceiling: CeilingDb::default(),
        shared: Arc::clone(&shared),
    };
    let mut processor = Processor::new(config, source, command_rx, event_tx);
    let _ = command_tx.push(Command::Play);
    arm(&mut command_tx, a, b, crossfade_frames, 0);

    let mut out = vec![0.0f32; buffer * 2];
    let mut gapless_flags = Vec::new();
    let mut polled_once = false;
    for _ in 0..10_000 {
        processor.render(&mut out);
        let mut wrapped_this_render = false;
        while let Ok(event) = event_rx.pop() {
            if let Event::LoopWrapped { gapless, .. } = event {
                gapless_flags.push(gapless);
                wrapped_this_render = true;
            }
        }
        if wrapped_this_render && !polled_once {
            // Simulate the decode-ahead catching up, right after the first
            // (uncached) wrap: one poll fully covers past `b`.
            let _ = host.poll();
            polled_once = true;
        }
        if gapless_flags.len() >= 5 {
            break;
        }
    }
    assert!(
        gapless_flags.len() >= 2,
        "need at least 2 wraps to compare before/after coverage"
    );
    assert!(
        !gapless_flags[0],
        "first wrap must be a hard cut: the store did not yet cover A"
    );
    assert!(
        gapless_flags[1..].iter().all(|&g| g),
        "later wraps must be gapless once the decode-ahead catches up"
    );
}

/// `LoopReleased { wraps: 3 }` fires right after the 3rd wrap; playback
/// continues past `b` afterward with no further wraps and
/// `loop_state == 0` (FR-011).
#[test]
fn repeat_count_releases_after_n() {
    let a = 1_000u64;
    let b = 1_200u64;
    let buffer = 64usize;

    let (mut processor, mut command_tx, mut event_rx, shared) = build(44_100, 44_100, buffer, 0);
    arm(&mut command_tx, a, b, 20, 3);

    let mut out = vec![0.0f32; buffer * 2];
    let mut released = None;
    for _ in 0..10_000 {
        processor.render(&mut out);
        while let Ok(event) = event_rx.pop() {
            if let Event::LoopReleased { wraps } = event {
                released = Some(wraps);
            }
        }
        if released.is_some() {
            break;
        }
    }
    assert_eq!(released, Some(3));
    assert_eq!(shared.loop_state(), 0);

    let clock_before = shared.clock_frames();
    let mut wrapped_again = false;
    for _ in 0..50 {
        processor.render(&mut out);
        while let Ok(event) = event_rx.pop() {
            if let Event::LoopWrapped { .. } = event {
                wrapped_again = true;
            }
        }
    }
    assert!(!wrapped_again, "released region must not wrap again");
    assert!(
        shared.clock_frames() > clock_before,
        "playback must keep advancing past B"
    );
}

/// A `Seek` outside `[a, b)` keeps the region armed-inactive (no jump);
/// seeking back inside re-activates it and wraps resume (FR-012).
#[test]
fn seek_outside_keeps_armed_inactive() {
    let a = 1_000u64;
    let b = 1_100u64;
    let buffer = 64usize;

    let (mut processor, mut command_tx, mut event_rx, shared) = build(44_100, 44_100, buffer, 0);
    arm(&mut command_tx, a, b, 10, 0);
    let _ = command_tx.push(Command::Seek(b + 1));

    let mut out = vec![0.0f32; buffer * 2];
    for _ in 0..10 {
        processor.render(&mut out);
        assert_eq!(shared.loop_state(), 1);
        while let Ok(event) = event_rx.pop() {
            assert!(
                !matches!(event, Event::LoopWrapped { .. }),
                "must not jump while outside the region"
            );
        }
    }

    let _ = command_tx.push(Command::Seek(a + 1));
    let mut saw_active = false;
    let mut wrapped = false;
    for _ in 0..1_000 {
        processor.render(&mut out);
        if shared.loop_state() == 2 {
            saw_active = true;
        }
        while let Ok(event) = event_rx.pop() {
            if let Event::LoopWrapped { .. } = event {
                wrapped = true;
            }
        }
        if wrapped {
            break;
        }
    }
    assert!(saw_active, "seeking back inside must reactivate the region");
    assert!(wrapped, "wraps must resume once re-activated");
}

/// A `Seek` landing before `A` classifies as armed-inactive; the region
/// activates and wraps once playback naturally reaches `A` (FR-012).
#[test]
fn natural_entry_from_before_a_activates() {
    let a = 2_000u64;
    let b = 2_100u64;
    let buffer = 64usize;

    let (mut processor, mut command_tx, mut event_rx, shared) = build(44_100, 44_100, buffer, 0);
    arm(&mut command_tx, a, b, 10, 0);
    let _ = command_tx.push(Command::Seek(a - 1_000));

    let mut out = vec![0.0f32; buffer * 2];
    processor.render(&mut out);
    assert_eq!(
        shared.loop_state(),
        1,
        "still before A right after the seek"
    );

    let mut wrapped = false;
    for _ in 0..1_000 {
        processor.render(&mut out);
        while let Ok(event) = event_rx.pop() {
            if let Event::LoopWrapped { .. } = event {
                wrapped = true;
            }
        }
        if wrapped {
            break;
        }
    }
    assert!(
        wrapped,
        "must jump once playback reaches B via natural entry"
    );
}

/// Setters pushed in render N with no `LoopCommit` leave the region
/// unchanged (still disarmed, for a first arm) until the commit lands in
/// render N+1 (FR-011a).
#[test]
fn commit_is_atomic_across_renders() {
    let a = 1_000u64;
    let b = 1_100u64;
    let buffer = 64usize;

    let (mut processor, mut command_tx, mut event_rx, shared) = build(44_100, 44_100, buffer, 500);
    let _ = command_tx.push(Command::LoopSetA(a));
    let _ = command_tx.push(Command::LoopSetB(b));
    let _ = command_tx.push(Command::LoopSetSeam {
        crossfade_frames: 10,
        repeat: 0,
    });

    let mut out = vec![0.0f32; buffer * 2];
    processor.render(&mut out); // render N: setters only, no commit
    assert_eq!(shared.loop_state(), 0, "not committed yet: still disarmed");

    let _ = command_tx.push(Command::LoopCommit { reset_wraps: true });
    processor.render(&mut out); // render N+1: commit lands
    assert_ne!(
        shared.loop_state(),
        0,
        "region recognised as armed after commit"
    );
    drain(&mut event_rx);
}

/// A `LoopCommit` arriving while a seam is in flight updates `loop_active`
/// but the running seam finishes with its *own* captured bounds; the
/// following wrap already reflects the edit (FR-011a).
#[test]
fn edit_while_armed_applies_next_buffer_and_finishes_seam() {
    let a = 1_000u64;
    let b_old = 1_100u64;
    let b_new = 1_080u64;
    let x = 50u32;
    let buffer = 64usize;

    let (mut processor, mut command_tx, mut event_rx, shared) = build(44_100, 44_100, buffer, a);
    arm(&mut command_tx, a, b_old, x, 0);

    let mut out = vec![0.0f32; buffer * 2];
    processor.render(&mut out); // enters the seam window [1050, 1100)
    let mut saw_wrap_yet = false;
    while let Ok(event) = event_rx.pop() {
        if let Event::LoopWrapped { .. } = event {
            saw_wrap_yet = true;
        }
    }
    assert!(!saw_wrap_yet, "must not have wrapped yet");

    let _ = command_tx.push(Command::LoopSetB(b_new));
    let _ = command_tx.push(Command::LoopCommit { reset_wraps: false });

    processor.render(&mut out); // finishes the OLD seam, jumps exactly once
    let mut wraps_this_render = 0u32;
    while let Ok(event) = event_rx.pop() {
        if let Event::LoopWrapped { .. } = event {
            wraps_this_render += 1;
        }
    }
    assert_eq!(
        wraps_this_render, 1,
        "the in-flight seam finishes exactly once, using its OLD bounds"
    );

    // The exact clock value the jump happened at is sub-render (it can
    // land mid-buffer), so it can't be pinned down precisely from the
    // outside — but from here on the region is constant at `[a, b_new)`,
    // so a fresh (clock, position) pair taken *right now* is a valid
    // baseline every later render's published position must stay
    // consistent with, at that new period.
    let baseline_clock = shared.clock_frames();
    let baseline_pos = shared.position_frames();
    assert!(
        (a..b_new).contains(&baseline_pos),
        "baseline_pos={baseline_pos} must already be inside the new region"
    );

    for _ in 0..1_000 {
        processor.render(&mut out);
        let clock = shared.clock_frames();
        let period = b_new - a;
        let expected = a + ((baseline_pos - a) + (clock - baseline_clock)) % period;
        assert_eq!(
            shared.position_frames(),
            expected,
            "clock={clock}: the next cycle already uses the NEW period"
        );
    }
}

/// A `LoopDisarm` arriving while a seam is in flight lets the seam finish
/// (audio continuity) but suppresses the jump at `B`: playback continues
/// forward past `B` rather than back to `A`.
#[test]
fn disarm_mid_seam_finishes_seam_without_jump() {
    let a = 1_000u64;
    let b = 1_100u64;
    let x = 50u32;
    let buffer = 64usize;

    let (mut processor, mut command_tx, mut event_rx, shared) = build(44_100, 44_100, buffer, a);
    arm(&mut command_tx, a, b, x, 0);

    let mut out = vec![0.0f32; buffer * 2];
    processor.render(&mut out); // enters the seam window, no wrap yet
    drain(&mut event_rx);

    let _ = command_tx.push(Command::LoopDisarm);
    let clock_before = shared.clock_frames();
    processor.render(&mut out); // the seam finishes, but must not jump
    let mut wrapped = false;
    while let Ok(event) = event_rx.pop() {
        if let Event::LoopWrapped { .. } = event {
            wrapped = true;
        }
    }
    assert!(!wrapped, "a disarm mid-seam must suppress the jump");
    assert_eq!(shared.loop_state(), 0);
    assert_eq!(
        shared.clock_frames() - clock_before,
        buffer as u64,
        "no discontinuity: the full buffer was still delivered forward"
    );

    for _ in 0..20 {
        processor.render(&mut out);
    }
    let mut wrapped_again = false;
    while let Ok(event) = event_rx.pop() {
        if let Event::LoopWrapped { .. } = event {
            wrapped_again = true;
        }
    }
    assert!(!wrapped_again, "disarmed: no further wraps");
}

/// research R8: once a wrap has happened, the published position never
/// underflows back below `A` even when the output stage resamples (the
/// carried guard frame(s) can straddle the wrap boundary).
#[test]
fn published_position_after_mid_render_wrap() {
    let a = 1_000u64;
    let b = 1_064u64;
    let x = 10u32;
    let buffer = 64usize;

    let (mut processor, mut command_tx, mut event_rx, shared) = build(44_100, 48_000, buffer, a);
    arm(&mut command_tx, a, b, x, 0);

    let mut out = vec![0.0f32; buffer * 2];
    let mut checked_any_wrap = false;
    for _ in 0..2_000 {
        processor.render(&mut out);
        let mut wrapped = false;
        while let Ok(event) = event_rx.pop() {
            if let Event::LoopWrapped { .. } = event {
                wrapped = true;
            }
        }
        if wrapped {
            checked_any_wrap = true;
            let published = shared.position_frames();
            assert!(
                published >= a && published < b,
                "published={published} a={a} b={b}"
            );
        }
    }
    assert!(checked_any_wrap, "must have observed at least one wrap");
}
