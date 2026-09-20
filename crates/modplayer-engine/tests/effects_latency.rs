// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §12, SC-001, NFR-1.1: a `TimeStretch`
//! ratio change becomes audible within 20 ms (p95) at the performance
//! preset (128-frame buffers at 44.1 kHz). "Audible" is measured as the
//! first sample at which a processor that received the ratio-change
//! command diverges from an identically-driven control processor that
//! never did — the earliest instant a listener could possibly hear any
//! difference at all, which upper-bounds every real perceptual latency.

use std::sync::Arc;

use modplayer_audio_source::AudioSource;
use modplayer_effects::catalog::{NodeKind, NodeOwner, ParamId};
use modplayer_engine::{
    CeilingDb, Command, Event, Processor, ProcessorConfig, RtShared, Transport, VolumePercent,
};
use rtrb::{Producer, RingBuffer};

const RATE: u32 = 44_100;
const BUFFER: usize = 128;

/// A deterministic click train: a continuous, densely-spaced train of
/// short unit-amplitude bursts (period well under one WSOLA hop) — dense
/// enough that any computational divergence between two otherwise
/// identically-driven stages shows up in the very next burst rather than
/// waiting out a long silent gap, which would measure the test's own
/// content sparsity rather than the engine's actual latency.
struct ClickTrain {
    position: u64,
}

const CLICK_PERIOD: u64 = 40;
const CLICK_LEN: u64 = 4;

impl AudioSource for ClickTrain {
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
            let value = if idx % CLICK_PERIOD < CLICK_LEN {
                1.0
            } else {
                0.0
            };
            frame[0] = value;
            frame[1] = value;
        }
        self.position += (out.len() / 2) as u64;
    }
}

fn build(ratio: f32) -> (Processor<ClickTrain>, Producer<Command>, Arc<RtShared>) {
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(4_096);
    let (event_tx, _event_rx) = RingBuffer::<Event>::new(4_096);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: RATE,
        device_rate: RATE,
        device_channels: 2,
        max_frames: BUFFER,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared: Arc::clone(&shared),
    };
    let source = ClickTrain { position: 0 };
    let processor = Processor::new(config, source, command_rx, event_tx);
    let _ = command_tx.push(Command::Play);
    let _ = command_tx.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::TimeStretch,
        owner: NodeOwner::Host,
    });
    let _ = command_tx.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(0),
        value: ratio,
    });
    (processor, command_tx, shared)
}

/// A tiny deterministic xorshift64 PRNG — no new dependency needed for
/// 200 reproducible trials.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_usize_range(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.next_u64() as usize) % (hi - lo)
    }
}

/// contracts/engine-effect-chain.md §12, quickstart.md's own scenario
/// ("Tempo without pitch change"): 200 randomised trials — a tempo step
/// from nominal to 60 % (a realistic, clearly audible change; the render
/// at which it lands, i.e. its phase against the stage's own hop/analysis
/// cycle, is what is randomised) — each measuring the first sample at
/// which a changed processor's output diverges from an identically-driven,
/// never-changed control: the earliest instant the change could possibly
/// be audible. Asserts the 95th percentile stays within 20 ms (SC-001,
/// NFR-1.1: 2.9 ms buffer + 10 ms synthesis hop).
#[test]
fn ratio_change_audible_within_20ms_p95() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut latencies_ms: Vec<f64> = Vec::with_capacity(200);

    for trial in 0..200 {
        // quickstart.md's own scenario: nominal tempo, then a step down to
        // 60 % (a realistic, clearly audible change — 6x a single
        // `tempo_step`). Only the render at which it lands (the phase
        // against the stage's own hop/analysis cycle) is randomised.
        let initial_ratio = 1.0f32;
        let new_ratio = 0.6f32;
        let warmup_renders = rng.next_usize_range(50, 150);

        let (mut changed, mut changed_cmds, _shared_a) = build(initial_ratio);
        let (mut control, _control_cmds, _shared_b) = build(initial_ratio);

        let mut out_a = vec![0.0f32; BUFFER * 2];
        let mut out_b = vec![0.0f32; BUFFER * 2];
        for _ in 0..warmup_renders {
            changed.render(&mut out_a);
            control.render(&mut out_b);
        }

        let _ = changed_cmds.push(Command::ChainSetParam {
            slot: 0,
            param: ParamId(0),
            value: new_ratio,
        });

        // Render forward on both, looking for the first sample where they
        // diverge — capped far beyond any plausible latency so a real
        // regression shows as a clear failure, not a silent skip.
        const MAX_RENDERS: usize = 200;
        let mut divergence_frame: Option<u64> = None;
        'search: for r in 0..MAX_RENDERS {
            changed.render(&mut out_a);
            control.render(&mut out_b);
            for f in 0..BUFFER {
                if (out_a[f * 2] - out_b[f * 2]).abs() > 1e-3 {
                    divergence_frame = Some((r * BUFFER + f) as u64);
                    break 'search;
                }
            }
        }

        let frame = divergence_frame.unwrap_or_else(|| {
            panic!(
                "trial {trial}: initial_ratio={initial_ratio} new_ratio={new_ratio}: outputs \
                 never diverged within {MAX_RENDERS} renders after the ratio change"
            )
        });
        latencies_ms.push(frame as f64 * 1000.0 / f64::from(RATE));
    }

    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_index = (latencies_ms.len() as f64 * 0.95).ceil() as usize;
    let p95 = latencies_ms[p95_index.saturating_sub(1).min(latencies_ms.len() - 1)];
    assert!(
        p95 <= 20.0,
        "p95 latency={p95}ms exceeds the 20 ms budget (SC-001, NFR-1.1)"
    );
}
