// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §7 (FR-011, SC-008): pre-chain
//! peak/RMS reflects neither the chain nor master volume; post-chain
//! peak/RMS reflects both. The post-chain spectrum peaks in the band
//! covering a known tone's frequency.

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
        master_volume: VolumePercent::new(100),
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

/// FR-011: pre-chain peak/RMS is measured on the raw decoded frames,
/// before the chain or master gain ever touch them; post-chain reflects
/// both. Driven with a -20 dB gain node and a 50% master volume — either
/// alone already halves-or-more the signal, so a post that stayed close
/// to pre would mean the meter tap points were wired to the wrong place.
#[test]
fn pre_ignores_nodes_and_master_post_reflects_both() {
    let (mut processor, mut commands, shared) = build(44_100);
    let _ = commands.push(Command::Play);
    let _ = commands.push(Command::SetMasterVolume(VolumePercent::new(50)));
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::Gain,
        owner: NodeOwner::Host,
    });
    let _ = commands.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(0), // level_db
        value: -20.0,
    });

    let mut out = vec![0.0f32; 256 * 2];
    // Settle the insert crossfade and the level ramp.
    for _ in 0..30 {
        processor.render(&mut out);
    }

    let (pre_peak_l, _pre_peak_r, pre_rms_l, _pre_rms_r) = shared.pre_level();
    let (post_peak_l, _post_peak_r, post_rms_l, _post_rms_r) = shared.post_level();

    // SyntheticSource's first 10 s is a -12 dBFS 440 Hz sine
    // (~0.2512 peak) — well clear of silence, so this also proves pre
    // is reading real content, not a stale zero.
    assert!(
        pre_peak_l > 0.2,
        "pre must reflect the raw source: {pre_peak_l}"
    );
    assert!(pre_rms_l > 0.1, "pre RMS must be non-trivial: {pre_rms_l}");

    // -20 dB alone is a 0.1x factor; a 50% master on top can only shrink
    // it further, so post must be well under half of pre regardless of
    // the exact master-volume curve.
    assert!(
        post_peak_l < pre_peak_l * 0.5,
        "post ({post_peak_l}) must reflect the -20 dB node and 50% master, unlike pre ({pre_peak_l})"
    );
    assert!(
        post_rms_l < pre_rms_l * 0.5,
        "post RMS ({post_rms_l}) must likewise reflect both, unlike pre ({pre_rms_l})"
    );
}

/// FR-011/SC-008: the post-chain spectrum's peak band covers a known
/// tone's frequency. `Command::PlayTestTone` sums a fixed 440 Hz sine
/// into `fresh` after master gain, independent of transport — a
/// deterministic signal purpose-built for this kind of check.
#[test]
fn spectrum_peaks_in_expected_band() {
    let (mut processor, mut commands, shared) = build(44_100);
    let _ = commands.push(Command::PlayTestTone);

    let mut out = vec![0.0f32; 256 * 2];
    // >= 1024 (SPECTRUM_FFT) + several hops' worth of frames.
    for _ in 0..60 {
        processor.render(&mut out);
    }

    let bands = shared.spectrum();
    assert!(
        shared.spectrum_generation() > 0,
        "the spectrum must have recomputed at least once"
    );
    let Some((peak_band, &peak_value)) = bands
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
    else {
        panic!("bands must be non-empty (64 bands)");
    };
    assert!(
        peak_value > 0.01,
        "expected a clear spectral peak, got {bands:?}"
    );
    // Below ~2 kHz, `SpectrumRing`'s 64 log-spaced bands are narrower
    // than one 1024-point FFT bin (`44_100 / 1024 ≈ 43 Hz`) — its
    // monotonic-bin floor makes band index track FFT bin index almost
    // 1:1 down there, i.e. `round(440 * 1024 / 44_100)`.
    let expected_bin = (440.0f32 * 1024.0 / 44_100.0).round() as usize;
    assert!(
        peak_band.abs_diff(expected_bin) <= 2,
        "440 Hz peaked in band {peak_band}, expected near bin/band {expected_bin}: {bands:?}"
    );
}

/// FR-011 with an engaged rate-changing node: a pitch-shift stage's pull
/// plan asks the source for an uneven number of frames per render (0, 1,
/// or hundreds), so a per-render pre-chain measurement flickered to
/// `-60 dB` / `peak == rms` on the real app (2026-09-19 manual walk, M3).
/// The pre meter must keep reading the steady -12 dBFS sine on every
/// render once warmed up — never a zero window, and with a sine's
/// `peak / rms ≈ √2` (never a one-frame `peak == rms` window).
#[test]
fn pre_level_is_steady_under_an_engaged_stretch_stage() {
    let (mut processor, mut commands, shared) = build(44_100);
    let _ = commands.push(Command::Play);
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::PitchShift,
        owner: NodeOwner::Host,
    });
    let _ = commands.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(0), // semitones
        value: 7.0,
    });
    let _ = commands.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(1), // formant
        value: 1.0,
    });

    let mut out = vec![0.0f32; 256 * 2];
    for _ in 0..60 {
        processor.render(&mut out);
    }

    for render in 0..200 {
        processor.render(&mut out);
        let (peak_l, _peak_r, rms_l, _rms_r) = shared.pre_level();
        assert!(
            peak_l > 0.2,
            "render {render}: pre peak dropped to {peak_l} under an engaged stretch stage"
        );
        let ratio = peak_l / rms_l;
        assert!(
            (1.2..=1.7).contains(&ratio),
            "render {render}: pre peak/rms {ratio} is not a sine's √2 — window too short ({peak_l}/{rms_l})"
        );
    }
}
