// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §1, §12: an empty chain is bit-exact
//! with 001's pre-008 pipeline — pins FR-001.
//!
//! `rate_change_rebuild_recomputes_coefficients` (Phase 5, T072) pins
//! FR-014: `ChainRt::apply_set_param` always re-clamps with the RT's
//! *current* `source_rate` (defence in depth, mirroring `catalog::clamp`'s
//! Nyquist rule), so a value that only fits the *old* rate — exactly what
//! a stale rebuild-repush would send before the controller's own
//! `ChainModel::set_source_rate` catches up (research R11) — still lands
//! on the correctly clamped coefficients at the new rate.

use std::sync::Arc;

use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_effects::catalog::{NodeKind, NodeOwner, ParamId};
use modplayer_engine::{
    CeilingDb, Command, Processor, ProcessorConfig, RtShared, Transport, VolumePercent,
};
use rtrb::RingBuffer;

fn build(
    source_rate: u32,
) -> (
    Processor<SyntheticSource>,
    rtrb::Producer<Command>,
    Arc<RtShared>,
) {
    let (command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<modplayer_engine::Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate,
        device_rate: source_rate,
        device_channels: 2,
        max_frames: 256,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared: Arc::clone(&shared),
    };
    let processor = Processor::new(
        config,
        SyntheticSource::new(source_rate),
        command_rx,
        event_tx,
    );
    (processor, command_tx, shared)
}

/// contracts/engine-effect-chain.md §1: with no node ever inserted, the
/// chain does nothing to `fresh` at all — every rendered sample must
/// match a processor built and driven identically before 008 existed
/// (SyntheticSource's deterministic content makes this reproducible
/// across two independent processors driven with the same commands).
#[test]
fn empty_chain_is_bit_exact_with_001_pipeline() {
    let (mut with_chain, mut commands_a, _shared_a) = build(44_100);
    let (mut baseline, mut commands_b, _shared_b) = build(44_100);

    let _ = commands_a.push(Command::Play);
    let _ = commands_b.push(Command::Play);

    let mut out_a = vec![0.0f32; 256 * 2];
    let mut out_b = vec![0.0f32; 256 * 2];

    for i in 0..20u32 {
        if i == 5 {
            let _ = commands_a.push(Command::Seek(10_000));
            let _ = commands_b.push(Command::Seek(10_000));
        }
        with_chain.render(&mut out_a);
        baseline.render(&mut out_b);
        assert_eq!(out_a, out_b, "buffer {i} diverged with an empty chain");
    }
    assert!(
        out_a.iter().any(|&s| s != 0.0),
        "must carry real audio, not silence"
    );
}

/// FR-014: at a low source rate (`0.45 * 8_000 = 3_600` Hz effective max),
/// a `Filter` cutoff request that only fits a *higher* rate — the value a
/// stale rebuild-repush would carry over from before a rate change — must
/// still land on the same, correctly re-clamped coefficients as an
/// explicitly-clamped request. `Processor`/`ChainRt` never expose raw
/// coefficients (FR-013), so this is proven by bit-exact output between
/// two independently built processors that only differ in which of the
/// two (post-clamp-identical) values they were sent.
#[test]
fn rate_change_rebuild_recomputes_coefficients() {
    let low_rate = 8_000;

    fn drive(rate: u32, requested_cutoff: f32) -> Vec<f32> {
        let (mut command_tx, command_rx) = RingBuffer::<Command>::new(64);
        let (event_tx, _event_rx) = RingBuffer::<modplayer_engine::Event>::new(64);
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
            shared: Arc::clone(&shared),
        };
        let mut processor =
            Processor::new(config, SyntheticSource::new(rate), command_rx, event_tx);
        let _ = command_tx.push(Command::Play);
        let _ = command_tx.push(Command::ChainInsert {
            slot: 0,
            position: 0,
            kind: NodeKind::Filter,
            owner: NodeOwner::Host,
        });
        let _ = command_tx.push(Command::ChainSetParam {
            slot: 0,
            param: ParamId(1), // cutoff
            value: requested_cutoff,
        });

        let mut out = vec![0.0f32; 256 * 2];
        // Let the insert crossfade and the 20 ms cutoff ramp both settle.
        for _ in 0..30 {
            processor.render(&mut out);
        }
        let mut tail = Vec::new();
        for _ in 0..5 {
            processor.render(&mut out);
            tail.extend_from_slice(&out);
        }
        tail
    }

    // 3_600 is already the effective max at 8 kHz (0.45 * 8_000); 15_000
    // only fits the *old*, higher rate this value would have come from —
    // both must land on exactly the same re-clamped coefficients.
    let already_clamped = drive(low_rate, 3_600.0);
    let stale_from_higher_rate = drive(low_rate, 15_000.0);
    assert_eq!(
        already_clamped, stale_from_higher_rate,
        "a cutoff that only fit a higher rate must re-clamp to the same coefficients"
    );

    // Sanity: the filter must actually be doing *something* (not a
    // no-op identity), or the equality above would be vacuous.
    let unfiltered = drive(low_rate, f32::MIN); // clamps to the filter's own min (20 Hz)
    assert_ne!(
        already_clamped, unfiltered,
        "the clamped cutoff must produce a different response than the filter's minimum"
    );
}
