// SPDX-License-Identifier: MIT OR Apache-2.0

//! FR-025(a): `Processor::render` must never allocate or deallocate on the
//! real-time path. `assert_no_alloc` installs a global allocator that
//! aborts on any (de)allocation while the guarded closure runs
//! (contracts/engine-commands.md).
//!
//! `clock_monotonic_across_rebuild` (FR-025c, US3 T068) below simulates the
//! controller's snapshot-rebuild path (research R3): a device loss and a
//! sample-rate change are both just "drop the old `Processor`, build a new
//! one from `RtShared` + the last known position" from the engine's point
//! of view — `Arc<RtShared>` is never replaced, so the clock is expected to
//! carry straight through both rebuilds.

use std::sync::Arc;

// `AllocDisabler` itself (the global-allocator guard) is compiled out in
// release builds under the crate's default `disable_release` feature
// (its `assert_no_alloc`/`permit_alloc` become no-ops there instead) —
// gated so this test file still compiles under `cargo test --release`
// (008 T096/T097 need release-mode compilation of the whole crate's test
// suite; `assert_no_alloc` itself stays unconditional, a no-op there).
#[cfg(debug_assertions)]
use assert_no_alloc::AllocDisabler;
use assert_no_alloc::assert_no_alloc;
use modplayer_audio_source::DecodedStore;
use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_effects::catalog::{NodeKind, NodeOwner, ParamId};
use modplayer_engine::{
    CeilingDb, Command, Event, Processor, ProcessorConfig, RtShared, Transport, VolumePercent,
};
use rtrb::RingBuffer;

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn render_never_allocates() {
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<modplayer_engine::Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: 44_100,
        device_rate: 44_100,
        device_channels: 2,
        max_frames: 256,
        transport: Transport::Stopped,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared,
    };
    let mut processor = Processor::new(config, SyntheticSource::new(44_100), command_rx, event_tx);

    let _ = command_tx.push(Command::Play);

    let mut out = vec![0.0f32; 256 * 2];

    for i in 0..1000u32 {
        // Exercise the command-drain path on a good fraction of buffers,
        // including every command variant (PlayTestTone included, even
        // though its effect lands in US1).
        if i % 7 == 0 {
            let _ = command_tx.push(Command::SetMasterVolume(VolumePercent::new(
                (i % 100) as u8,
            )));
        }
        if i % 11 == 0 {
            let _ = command_tx.push(Command::SetCeiling(CeilingDb::new(-1.0 - (i % 5) as f32)));
        }
        if i % 13 == 0 {
            let _ = command_tx.push(Command::PlayTestTone);
        }
        if i % 17 == 0 {
            let _ = command_tx.push(Command::Pause);
        }
        if i % 19 == 0 {
            let _ = command_tx.push(Command::Play);
        }
        // Command::Seek (engine-delta.md §1) and the leftover-carry write
        // (engine-delta.md §2) it interacts with must stay allocation-free
        // too.
        if i % 23 == 0 {
            let _ = command_tx.push(Command::Seek((i as u64) * 37));
        }

        assert_no_alloc(|| {
            processor.render(&mut out);
        });
    }
}

/// Same shape as `render_never_allocates`, but with the device rate
/// different from the source rate so the output stage actually resamples
/// and the leftover-carry path (engine-delta.md §2) is exercised on every
/// buffer, not just when a guard frame happens to be left over at a
/// passthrough rate (which never leaves one).
#[test]
fn render_never_allocates_with_resampling_and_carry() {
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<modplayer_engine::Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: 44_100,
        device_rate: 48_000,
        device_channels: 2,
        max_frames: 256,
        transport: Transport::Stopped,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared,
    };
    let mut processor = Processor::new(config, SyntheticSource::new(44_100), command_rx, event_tx);

    let _ = command_tx.push(Command::Play);

    let mut out = vec![0.0f32; 256 * 2];

    for i in 0..1000u32 {
        if i % 23 == 0 {
            let _ = command_tx.push(Command::Seek((i as u64) * 37));
        }

        assert_no_alloc(|| {
            processor.render(&mut out);
        });
    }
}

/// (FR-025c, US3 T068): render 100 buffers, then rebuild `Processor` twice
/// from a shared snapshot — once simulating a device loss (same rates) and
/// once simulating an externally-triggered sample-rate change (44.1 kHz
/// source into a 48 kHz device) — and assert the clock never decreases and
/// keeps advancing by the frames actually consumed each time, and that
/// track position is carried forward rather than reset.
#[test]
fn clock_monotonic_across_rebuild() {
    fn build_processor(
        device_rate: u32,
        shared: &Arc<RtShared>,
    ) -> (Processor<SyntheticSource>, rtrb::Producer<Command>) {
        let (command_tx, command_rx) = RingBuffer::<Command>::new(256);
        let (event_tx, _event_rx) = RingBuffer::<modplayer_engine::Event>::new(256);
        let config = ProcessorConfig {
            source_rate: 44_100,
            device_rate,
            device_channels: 2,
            max_frames: 256,
            transport: Transport::Playing,
            position_frames: shared.position_frames(),
            master_volume: VolumePercent::new(80),
            ceiling: CeilingDb::default(),
            shared: Arc::clone(shared),
        };
        let processor = Processor::new(config, SyntheticSource::new(44_100), command_rx, event_tx);
        (processor, command_tx)
    }

    let shared = Arc::new(RtShared::new());

    // Stage 1: normal playback, source rate == device rate (passthrough),
    // so each 256-frame buffer consumes exactly 256 source frames.
    let (mut processor, mut command_tx) = build_processor(44_100, &shared);
    let _ = command_tx.push(Command::Play);
    let mut out = vec![0.0f32; 256 * 2];
    let mut expected_clock: u64 = 0;
    for _ in 0..100 {
        processor.render(&mut out);
        expected_clock += 256;
        assert_eq!(shared.clock_frames(), expected_clock);
    }
    let position_after_stage1 = shared.position_frames();
    assert!(
        position_after_stage1 > 0,
        "position must advance while playing"
    );

    // Stage 2: simulated device loss — drop the old processor, rebuild
    // from the same shared snapshot at the same rates (contracts/engine-
    // commands.md; research R3). The clock must not reset or go backwards,
    // and position must continue from where stage 1 left off, not from 0.
    drop(processor);
    let (mut processor, mut command_tx) = build_processor(44_100, &shared);
    let _ = command_tx.push(Command::Play);
    assert_eq!(
        shared.clock_frames(),
        expected_clock,
        "rebuilding must not reset the clock"
    );
    for _ in 0..100 {
        let before = shared.clock_frames();
        processor.render(&mut out);
        expected_clock += 256;
        let after = shared.clock_frames();
        assert_eq!(after, expected_clock);
        assert!(
            after >= before,
            "clock must never decrease across a rebuild"
        );
    }
    assert!(
        shared.position_frames() >= position_after_stage1,
        "position must continue forward across a rebuild, not reset"
    );

    // Stage 3: simulated externally-triggered sample-rate change — rebuild
    // again, this time into a 48 kHz device. Per-buffer consumed source
    // frames are no longer a fixed constant (the output stage resamples),
    // so this stage only asserts the FR-025(c) invariant itself: the clock
    // never decreases and keeps advancing, and the source rate (44.1 kHz)
    // is unaffected by the device-rate change.
    drop(processor);
    let (mut processor, mut command_tx) = build_processor(48_000, &shared);
    let _ = command_tx.push(Command::Play);
    let clock_before_rate_change = shared.clock_frames();
    assert_eq!(
        clock_before_rate_change, expected_clock,
        "a sample-rate-change rebuild must not reset the clock either"
    );
    let mut prev_clock = clock_before_rate_change;
    for _ in 0..100 {
        processor.render(&mut out);
        let now = shared.clock_frames();
        assert!(
            now >= prev_clock,
            "clock must never decrease across a sample-rate-change rebuild"
        );
        assert!(now > prev_clock, "clock must keep advancing every buffer");
        prev_clock = now;
    }
    assert!(
        prev_clock > clock_before_rate_change,
        "the source-rate clock must keep advancing after a device-rate change"
    );
}

/// 006, contracts/engine-loop.md §6: `Processor::render` must never
/// allocate with an armed loop region either — the seam's incoming
/// frames come from a preallocated `seam_in`, read from `decoded_store()`
/// via `DecodedStore::read_frames` (atomics only). 1 000 renders across a
/// short region wraps well past 20 times.
#[test]
fn render_with_armed_loop_never_allocates() {
    let rate = 44_100u32;
    let len_frames = 5_000u64;
    let store = DecodedStore::new(rate, len_frames);
    let buf = vec![0.1f32; (len_frames * 2) as usize];
    store.write_frames(0, &buf);
    store.set_complete(len_frames);

    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, mut event_rx) = RingBuffer::<Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: rate,
        device_rate: rate,
        device_channels: 2,
        max_frames: 256,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared,
    };
    let source = SyntheticSource::with_store(rate, store);
    let mut processor = Processor::new(config, source, command_rx, event_tx);

    let _ = command_tx.push(Command::LoopSetA(1_000));
    let _ = command_tx.push(Command::LoopSetB(1_100));
    let _ = command_tx.push(Command::LoopSetSeam {
        crossfade_frames: 50,
        repeat: 0,
    });
    let _ = command_tx.push(Command::LoopCommit { reset_wraps: true });

    let mut out = vec![0.0f32; 256 * 2];
    let mut wraps = 0u32;
    for _ in 0..1_000u32 {
        assert_no_alloc(|| {
            processor.render(&mut out);
        });
        while let Ok(event) = event_rx.pop() {
            if let Event::LoopWrapped { .. } = event {
                wraps += 1;
            }
        }
    }
    assert!(wraps >= 20, "wraps={wraps}");
}

/// contracts/engine-effect-chain.md §10, Constitution I: driving every
/// `Chain*` command variant (add/param/move/bypass/remove) through a
/// 16-node chain covering every built-in `NodeKind` (cycled by slot),
/// interleaved with `Seek`, must never allocate.
#[test]
fn render_with_full_chain_never_allocates() {
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: 44_100,
        device_rate: 44_100,
        device_channels: 2,
        max_frames: 256,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared,
    };
    let mut processor = Processor::new(config, SyntheticSource::new(44_100), command_rx, event_tx);
    let _ = command_tx.push(Command::Play);

    let mut out = vec![0.0f32; 256 * 2];

    for i in 0u32..2_000 {
        let slot = (i % 16) as u8;
        match i % 16 {
            0 => {
                let kind = NodeKind::ALL[slot as usize % NodeKind::ALL.len()];
                let _ = command_tx.push(Command::ChainInsert {
                    slot,
                    position: 0,
                    kind,
                    owner: NodeOwner::Host,
                });
            }
            3 => {
                let _ = command_tx.push(Command::ChainSetParam {
                    slot,
                    param: ParamId(0),
                    value: -6.0,
                });
            }
            6 => {
                let _ = command_tx.push(Command::ChainSetParam {
                    slot,
                    param: ParamId(1),
                    value: 1.0,
                });
            }
            8 => {
                let _ = command_tx.push(Command::ChainMove { slot, position: 0 });
            }
            9 => {
                // EQ band 0's discrete `type` (id 19) for an `Equalizer`
                // slot — exercises `Eq8Dsp::switch_band_type`'s shadow
                // biquad; a harmless no-op (unknown id) for every other
                // kind.
                let value = if (i / 16) % 3 == 0 { 1.0 } else { 0.0 };
                let _ = command_tx.push(Command::ChainSetParam {
                    slot,
                    param: ParamId(19),
                    value,
                });
            }
            10 => {
                let _ = command_tx.push(Command::ChainSetBypass {
                    slot,
                    bypassed: true,
                });
            }
            12 => {
                let _ = command_tx.push(Command::ChainSetBypass {
                    slot,
                    bypassed: false,
                });
            }
            13 => {
                // `mono_sum` (id 2) for a `StereoTools` slot — exercises
                // `StereoDsp::set_flags_target`'s dual-config crossfade; a
                // harmless resonance/q/no-op ramp for every other kind.
                let value = if (i / 16) % 2 == 0 { 1.0 } else { 0.0 };
                let _ = command_tx.push(Command::ChainSetParam {
                    slot,
                    param: ParamId(2),
                    value,
                });
            }
            15 => {
                let _ = command_tx.push(Command::ChainRemove { slot });
            }
            _ => {}
        }
        if i % 23 == 0 {
            let _ = command_tx.push(Command::Seek(u64::from(i) * 37));
        }

        assert_no_alloc(|| {
            processor.render(&mut out);
        });
    }
}
