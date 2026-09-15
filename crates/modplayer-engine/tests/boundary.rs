// SPDX-License-Identifier: MIT OR Apache-2.0

//! FR-025(b): a command pushed between two `render` calls affects only the
//! next buffer, from its first sample. Buffer N (rendered before the
//! command was pushed) is unaffected; buffer N+1 reflects it in full
//! (contracts/engine-commands.md).
//!
//! Uses a local constant-signal `AudioSource` so the property is visible
//! regardless of the synthetic track's own content — any real-time-safe
//! `AudioSource` must satisfy this contract, which is exactly what
//! `Processor` being generic over `AudioSource` (Constitution Principle IV)
//! guarantees.
//!
//! US2 acceptance 4 (master volume, ceiling, buffer preset and device
//! changes during playback apply only at the next buffer boundary with no
//! audible glitch or clock discontinuity): master volume is covered above
//! by `command_applies_at_next_boundary`; `ceiling_command_applies_at_next_boundary`
//! below covers the ceiling; buffer-preset/device changes are, at the
//! engine level, a processor rebuild sharing the same `Arc<RtShared>`
//! (research R3) — `preset_or_device_change_rebuild_has_no_clock_discontinuity`
//! covers that.

use std::sync::Arc;

use modplayer_audio_source::AudioSource;
use modplayer_engine::{
    CeilingDb, Command, Processor, ProcessorConfig, RtShared, Transport, VolumePercent,
};
use rtrb::RingBuffer;

/// A constant full-scale stereo signal: enough to prove gain changes are
/// audible, with no dependency on the synthetic track's segment content.
struct ConstantSource {
    position: u64,
}

impl AudioSource for ConstantSource {
    fn sample_rate(&self) -> u32 {
        44_100
    }

    fn len_frames(&self) -> Option<u64> {
        None
    }

    fn position(&self) -> u64 {
        self.position
    }

    fn seek(&mut self, frame: u64) {
        self.position = frame;
    }

    fn fill(&mut self, out: &mut [f32]) {
        out.fill(1.0);
        self.position += (out.len() / 2) as u64;
    }
}

#[test]
fn command_applies_at_next_boundary() {
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<modplayer_engine::Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: 44_100,
        device_rate: 44_100,
        device_channels: 2,
        max_frames: 64,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared,
    };
    let mut processor =
        Processor::new(config, ConstantSource { position: 0 }, command_rx, event_tx);

    // Buffer N: rendered before any command is pushed.
    let mut buffer_n = vec![0.0f32; 64 * 2];
    processor.render(&mut buffer_n);
    assert!(
        buffer_n.iter().any(|&s| s != 0.0),
        "buffer N must carry the (non-muted) signal"
    );

    // Push the mute command between render calls.
    let _ = command_tx.push(Command::SetMasterVolume(VolumePercent::new(0)));

    // Buffer N+1: must be all zero from its very first sample.
    let mut buffer_n_plus_1 = vec![1.234f32; 64 * 2];
    processor.render(&mut buffer_n_plus_1);
    assert!(
        buffer_n_plus_1.iter().all(|&s| s == 0.0),
        "buffer N+1 must be silent from its first sample"
    );
}

#[test]
fn ceiling_command_applies_at_next_boundary() {
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<modplayer_engine::Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: 44_100,
        device_rate: 44_100,
        device_channels: 2,
        max_frames: 64,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(100),
        ceiling: CeilingDb::default(), // -1.0 dBFS
        shared,
    };
    let mut processor =
        Processor::new(config, ConstantSource { position: 0 }, command_rx, event_tx);

    // Buffer N: the full-scale constant source clamped to the default
    // -1.0 dBFS ceiling, unaffected by any command pushed afterwards.
    let default_ceiling_lin = CeilingDb::default().to_linear();
    let mut buffer_n = vec![0.0f32; 64 * 2];
    processor.render(&mut buffer_n);
    assert!(
        buffer_n
            .iter()
            .all(|&s| (s - default_ceiling_lin).abs() < 1e-6),
        "buffer N must reflect only the original (default) ceiling"
    );

    // Push a much tighter ceiling between renders.
    let tight = CeilingDb::new(-6.0);
    let _ = command_tx.push(Command::SetCeiling(tight));

    // Buffer N+1: clamped to the new ceiling from its very first sample.
    let mut buffer_n_plus_1 = vec![0.0f32; 64 * 2];
    processor.render(&mut buffer_n_plus_1);
    let tight_lin = tight.to_linear();
    assert!(
        buffer_n_plus_1
            .iter()
            .all(|&s| (s - tight_lin).abs() < 1e-6),
        "buffer N+1 must reflect the new (tighter) ceiling from its first sample"
    );
}

/// US2 acceptance 4's buffer-preset/device-change half, at the engine
/// level: a buffer-preset or device change is not a `Command` (it changes
/// `ProcessorConfig`'s construction-time parameters), so the controller
/// rebuilds a fresh `Processor` sharing the same `Arc<RtShared>` (research
/// R3). This proves that rebuild carries the clock forward with no
/// discontinuity: it never resets, never jumps backward, and advances by
/// exactly the new processor's consumed frames — and the signal itself
/// keeps playing (not a glitch of silence) across the boundary.
#[test]
fn preset_or_device_change_rebuild_has_no_clock_discontinuity() {
    let shared = Arc::new(RtShared::new());

    // "Buffer N": a stream opened at one buffer preset (64 frames).
    let (command_tx_a, command_rx_a) = RingBuffer::<Command>::new(256);
    let (event_tx_a, _event_rx_a) = RingBuffer::<modplayer_engine::Event>::new(256);
    let config_a = ProcessorConfig {
        source_rate: 44_100,
        device_rate: 44_100,
        device_channels: 2,
        max_frames: 64,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared: Arc::clone(&shared),
    };
    let mut processor_a = Processor::new(
        config_a,
        ConstantSource { position: 0 },
        command_rx_a,
        event_tx_a,
    );
    drop(command_tx_a);

    let mut buffer_n = vec![0.0f32; 64 * 2];
    processor_a.render(&mut buffer_n);
    let clock_after_n = shared.clock_frames();
    assert_eq!(clock_after_n, 64);

    // "Buffer N+1": a fresh processor rebuilt at a different buffer preset
    // (256 frames, e.g. Balanced -> Safe), sharing the same `Arc<RtShared>`
    // and carrying the position forward from it — exactly what a buffer
    // preset or device change does at the controller level.
    let (command_tx_b, command_rx_b) = RingBuffer::<Command>::new(256);
    let (event_tx_b, _event_rx_b) = RingBuffer::<modplayer_engine::Event>::new(256);
    let config_b = ProcessorConfig {
        source_rate: 44_100,
        device_rate: 44_100,
        device_channels: 2,
        max_frames: 256,
        transport: Transport::Playing,
        position_frames: shared.position_frames(),
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared: Arc::clone(&shared),
    };
    let mut processor_b = Processor::new(
        config_b,
        ConstantSource { position: 0 },
        command_rx_b,
        event_tx_b,
    );
    drop(command_tx_b);

    let mut buffer_n_plus_1 = vec![0.0f32; 256 * 2];
    processor_b.render(&mut buffer_n_plus_1);

    assert!(
        shared.clock_frames() > clock_after_n,
        "clock must never reset or go backward across a rebuild"
    );
    assert_eq!(
        shared.clock_frames() - clock_after_n,
        256,
        "clock must advance by exactly the new processor's consumed frames"
    );
    assert!(
        buffer_n_plus_1.iter().any(|&s| s != 0.0),
        "buffer N+1 must still carry the signal, not a glitch of silence"
    );
}

/// Regression (found by quickstart M2.4 on macOS): a processor built for a
/// small `max_frames` must survive a callback that delivers more frames
/// than requested — the backend, not the preset, decides the real buffer
/// size. The real-time thread must never panic on `scratch` indexing.
#[test]
fn render_tolerates_callback_larger_than_max_frames() {
    let (_command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<modplayer_engine::Event>::new(256);
    let config = ProcessorConfig {
        source_rate: 44_100,
        device_rate: 44_100,
        device_channels: 2,
        max_frames: 256,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared: Arc::new(RtShared::new()),
    };
    let mut processor =
        Processor::new(config, ConstantSource { position: 0 }, command_rx, event_tx);

    let mut buffer = vec![0.0f32; 1024 * 2];
    processor.render(&mut buffer);
    assert!(
        buffer.iter().all(|&s| s != 0.0),
        "the whole 1024-frame buffer must be rendered"
    );
}
