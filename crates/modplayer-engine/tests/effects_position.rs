// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §9, research R7: the published
//! position subtracts the chain's buffered lead on top of 006's leftover
//! carry, advances at `source_rate * advance_rate` even with a loop
//! region active, and every stretch stage's history resets on
//! `Command::Seek`/`Stop` — never on a loop wrap (FR-001a, SC-011).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_audio_source::AudioSource;
use modplayer_effects::catalog::{NodeKind, NodeOwner, ParamId};
use modplayer_engine::{
    CeilingDb, Command, Event, Processor, ProcessorConfig, RtShared, Transport, VolumePercent,
};
use rtrb::{Consumer, Producer, RingBuffer};

const RATE: u32 = 44_100;
const BUFFER: usize = 128;

/// A deterministic sine source that also mirrors its own raw position into
/// a shared atomic the test keeps a handle to — letting a test read the
/// *true* source-domain position independent of anything `Processor`
/// publishes (which is exactly the lead-subtracted value under test).
struct TrackedSource {
    position: u64,
    mirror: Arc<AtomicU64>,
}

impl TrackedSource {
    fn new() -> (Self, Arc<AtomicU64>) {
        let mirror = Arc::new(AtomicU64::new(0));
        (
            Self {
                position: 0,
                mirror: Arc::clone(&mirror),
            },
            mirror,
        )
    }
}

impl AudioSource for TrackedSource {
    fn sample_rate(&self) -> u32 {
        RATE
    }

    fn len_frames(&self) -> Option<u64> {
        None
    }

    fn position(&self) -> u64 {
        self.position
    }

    fn seek(&mut self, frame: u64) {
        self.position = frame;
        self.mirror.store(frame, Ordering::Relaxed);
    }

    fn fill(&mut self, out: &mut [f32]) {
        for (i, frame) in out.chunks_exact_mut(2).enumerate() {
            let idx = self.position + i as u64;
            let sample = (std::f32::consts::TAU * 440.0 * idx as f32 / RATE as f32).sin() * 0.5;
            frame[0] = sample;
            frame[1] = sample;
        }
        self.position += (out.len() / 2) as u64;
        self.mirror.store(self.position, Ordering::Relaxed);
    }
}

/// `build`'s harness handle tuple — named so clippy's `type_complexity`
/// lint doesn't flag the return type spelled out inline.
type Harness = (
    Processor<TrackedSource>,
    Producer<Command>,
    Consumer<Event>,
    Arc<RtShared>,
    Arc<AtomicU64>,
);

fn build(position_frames: u64) -> Harness {
    let (source, mirror) = TrackedSource::new();
    let (command_tx, command_rx) = RingBuffer::<Command>::new(4_096);
    let (event_tx, event_rx) = RingBuffer::<Event>::new(4_096);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: RATE,
        device_rate: RATE,
        device_channels: 2,
        max_frames: BUFFER,
        transport: Transport::Playing,
        position_frames,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared: Arc::clone(&shared),
    };
    let processor = Processor::new(config, source, command_rx, event_tx);
    (processor, command_tx, event_rx, shared, mirror)
}

fn render_n(processor: &mut Processor<TrackedSource>, n: usize) {
    let mut out = vec![0.0f32; BUFFER * 2];
    for _ in 0..n {
        processor.render(&mut out);
    }
}

/// contracts/engine-effect-chain.md §9: `lead = carry_len +
/// chain.latency_frames()`; an engaged rate-changing stage's buffered
/// lookahead must be subtracted from the published position on top of
/// 006's tiny (<= `MAX_GUARD` = 8 frames) leftover-carry lead.
#[test]
fn lead_is_subtracted() {
    let (mut processor, mut commands, _events, shared, mirror) = build(0);
    let _ = commands.push(Command::Play);

    // No node: the published position must track the raw source position
    // to within 006's own leftover-carry bound (passthrough rate, so that
    // bound is 0 in practice — `OutputStage`'s passthrough branch always
    // consumes every frame it is given).
    render_n(&mut processor, 20);
    let raw = mirror.load(Ordering::Relaxed);
    let published = shared.position_frames();
    assert!(
        raw.saturating_sub(published) <= 8,
        "raw={raw} published={published}: no engaged node must mean no meaningful lead"
    );
    assert_eq!(
        processor.chain_latency_frames(),
        0,
        "no engaged node must mean zero chain latency"
    );

    // Engage a `TimeStretch` node (ratio away from unity) and let its
    // WSOLA ring build up a real, multi-hundred-frame lookahead.
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::TimeStretch,
        owner: NodeOwner::Host,
    });
    let _ = commands.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(0),
        value: 0.5,
    });
    render_n(&mut processor, 200);

    let latency = processor.chain_latency_frames();
    assert!(
        latency > 0,
        "an engaged stretch stage must buffer a real lead"
    );

    let raw = mirror.load(Ordering::Relaxed);
    let published = shared.position_frames();
    let observed_lead = raw.saturating_sub(published);
    // Exact to within the leftover-carry's own small bound: the published
    // position is `raw - (carry_len + latency)`, `carry_len <= 8`.
    assert!(
        observed_lead.abs_diff(latency) <= 8,
        "observed_lead={observed_lead} chain_latency_frames={latency}"
    );
}

/// contracts/engine-effect-chain.md §9, research R7: with a loop region
/// armed and active (exercising `rewind_in_loop`'s branch, not the
/// unlooped `saturating_sub`), the published position still advances at
/// `source_rate * advance_rate` — here `advance_rate = 0.5` (half-speed
/// `TimeStretch`) — measured between two points that do not straddle a
/// wrap (`RtShared::loop_wraps()` unchanged across the window).
#[test]
fn position_advances_at_ratio_with_loop() {
    let (mut processor, mut commands, _events, shared, _mirror) = build(0);
    let _ = commands.push(Command::Play);
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::TimeStretch,
        owner: NodeOwner::Host,
    });
    let _ = commands.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(0),
        value: 0.5,
    });
    // A loop region far ahead of where the measurement window sits, so
    // the region is "armed-active" (the `Some((a, b))` branch of the
    // position rewind rule runs every render once `raw >= a`) but never
    // actually wraps during this test (contracts/engine-effect-chain.md
    // §9's `rewind_in_loop` reduces to the unlooped case when `raw < b`).
    let _ = commands.push(Command::LoopSetA(1_000));
    let _ = commands.push(Command::LoopSetB(1_000_000));
    let _ = commands.push(Command::LoopSetSeam {
        crossfade_frames: 0,
        repeat: 0,
    });
    let _ = commands.push(Command::LoopCommit { reset_wraps: true });

    // Warm up past the region's entry and the stretch stage's own startup.
    render_n(&mut processor, 400);
    assert_eq!(
        shared.loop_wraps(),
        0,
        "the region must not have wrapped yet"
    );
    assert_eq!(shared.loop_state(), 2, "the region must be armed-active");

    let p1 = shared.position_frames();
    let renders = 100usize;
    render_n(&mut processor, renders);
    assert_eq!(
        shared.loop_wraps(),
        0,
        "the measurement window must not straddle a wrap"
    );
    let p2 = shared.position_frames();

    let advance_rate = 0.5f64;
    let expected = (renders * BUFFER) as f64 * advance_rate;
    let actual = (p2 - p1) as f64;
    let tolerance = expected * 0.05 + 32.0;
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual={actual} expected={expected} tolerance={tolerance}"
    );
}

/// A source whose content is a hard step: `+LEVEL` at every position
/// before `mid`, `-LEVEL` at or after it — deterministic and independent
/// of how the position got there (seek or play-through), so a test can
/// tell *which* side of the step the stretch stage's own analysis window
/// is currently reading from, not just how loud the output is.
struct SteppedSource {
    position: u64,
    mid: u64,
}

const STEP_LEVEL: f32 = 0.5;

impl AudioSource for SteppedSource {
    fn sample_rate(&self) -> u32 {
        RATE
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
        for (i, frame) in out.chunks_exact_mut(2).enumerate() {
            let idx = self.position + i as u64;
            let level = if idx < self.mid {
                STEP_LEVEL
            } else {
                -STEP_LEVEL
            };
            frame[0] = level;
            frame[1] = level;
        }
        self.position += (out.len() / 2) as u64;
    }
}

fn build_stepped(
    mid: u64,
    position_frames: u64,
) -> (
    Processor<SteppedSource>,
    Producer<Command>,
    Consumer<Event>,
    Arc<RtShared>,
) {
    let source = SteppedSource {
        position: position_frames,
        mid,
    };
    let (command_tx, command_rx) = RingBuffer::<Command>::new(4_096);
    let (event_tx, event_rx) = RingBuffer::<Event>::new(4_096);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: RATE,
        device_rate: RATE,
        device_channels: 2,
        max_frames: BUFFER,
        transport: Transport::Playing,
        position_frames,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared: Arc::clone(&shared),
    };
    let processor = Processor::new(config, source, command_rx, event_tx);
    (processor, command_tx, event_rx, shared)
}

fn render_mean_left(processor: &mut Processor<SteppedSource>, renders: usize) -> f32 {
    let mut out = vec![0.0f32; BUFFER * 2];
    let mut last_left_sum = 0.0f32;
    for _ in 0..renders {
        processor.render(&mut out);
        last_left_sum = out.chunks_exact(2).map(|f| f[0]).sum::<f32>() / BUFFER as f32;
    }
    last_left_sum
}

/// contracts/engine-effect-chain.md §2, FR-001a: `Command::Seek` clears
/// every stretch ring/voice, biquad and LPC state (`ChainRt::
/// reset_history`) — an engaged stage cuts over to the new material
/// almost immediately; a loop wrap must never reset it, so an (otherwise
/// identical) discontinuity at a wrap instead shows the stage's own
/// buffered material persisting for a while. `SteppedSource`'s content
/// depends only on absolute position, so whichever side of the step the
/// output currently reflects reveals whether the stage is still reading
/// its pre-transition buffered content.
#[test]
fn seek_resets_history_wrap_does_not() {
    // -- Seek: the source jumps straight from the "+" region into a
    // brand-new, entirely "-" one; the reset makes the stage re-prime
    // itself from scratch on exactly that new material.
    let mid = 1_000_000u64;
    let (mut processor, mut commands, _events, _shared) = build_stepped(mid, 0);
    let _ = commands.push(Command::Play);
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::TimeStretch,
        owner: NodeOwner::Host,
    });
    let _ = commands.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(0),
        value: 0.5,
    });
    let _ = render_mean_left(&mut processor, 200); // settle, still "+"

    let _ = commands.push(Command::Seek(mid + 100_000));
    let after_seek = render_mean_left(&mut processor, 2);
    assert!(
        after_seek < 0.0,
        "Seek must reset history so the stage reads the new (\"-\") material almost \
         immediately: after_seek={after_seek}"
    );

    // -- Wrap: an otherwise identical hard-cut discontinuity (crossfade
    // 0), but reached by looping rather than seeking. Without a reset,
    // the stage's own still-buffered pre-wrap ("+") material must keep
    // showing up in the output for a while after the wrap event fires.
    // `a` sits in the "+" side, `b` in the "-" side, so the region is
    // entered on "+" and, by the time it wraps at `b`, is reading "-" —
    // the wrap itself then jumps straight back from "-" (at `b`) to "+"
    // (at `a`), the same kind of hard discontinuity the seek above
    // crossed, but reached by looping instead.
    let region_len = 5_000u64;
    let a = mid - region_len;
    let b = mid + region_len;
    let (mut processor, mut commands, mut events, _shared) = build_stepped(mid, a);
    let _ = commands.push(Command::Play);
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::TimeStretch,
        owner: NodeOwner::Host,
    });
    let _ = commands.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(0),
        value: 0.5,
    });
    let _ = commands.push(Command::LoopSetA(a));
    let _ = commands.push(Command::LoopSetB(b));
    let _ = commands.push(Command::LoopSetSeam {
        crossfade_frames: 0,
        repeat: 0,
    });
    let _ = commands.push(Command::LoopCommit { reset_wraps: true });

    let mut out = vec![0.0f32; BUFFER * 2];
    let mut wrapped = false;
    for _ in 0..region_len * 2 {
        processor.render(&mut out);
        while let Ok(event) = events.pop() {
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
        "the region must wrap at least once within the budget"
    );

    // Right after the wrap, `b`'s side ("-") is what the stage was
    // reading just before it; `a`'s side ("+") is the genuinely new
    // material. A reset (as `Seek` does, above) would cut over to "+"
    // almost immediately; *not* resetting means the stage's own
    // still-buffered "-" material must keep dominating the output for a
    // while instead.
    let after_wrap = render_mean_left(&mut processor, 2);
    assert!(
        after_wrap < 0.0,
        "a loop wrap must never reset history: the stage's still-buffered \
         pre-wrap (\"-\") material must still dominate the output right after \
         the wrap, unlike Seek's clean cut above: after_wrap={after_wrap}"
    );
}
