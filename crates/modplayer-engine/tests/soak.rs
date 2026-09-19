// SPDX-License-Identifier: MIT OR Apache-2.0

//! T096/T097 (008-effect-chain-and-built-in-nodes, research R16,
//! Constitution VIII): the soak obligations 001 deferred "until an
//! effect chain exists". `reference_chain_soak_60s` drives a real
//! `Processor` through SC-003's reference chain (pitch shift + time
//! stretch + 8-band equalizer) continuously for 60 s, applying a steady
//! stream of parameter drags, reorders and bypasses, and asserts both
//! that the render path never allocates (`assert_no_alloc`, Constitution
//! I) and that resident memory stays flat (a coarse `peak_rss` sample
//! every 10 s — good enough to catch a gross leak, not a precision
//! profiling tool). `reference_chain_soak_24h` is the same run scaled to
//! `MODPLAYER_SOAK_HOURS` hours (default 24) and `reference_chain_
//! under_half_core` asserts SC-003/NFR-1.8's < 50 %-of-one-core budget;
//! both are `#[ignore = "manual"]` because CI runs debug builds, whose
//! timing would be meaningless, and because hosted runners cannot spare
//! 24 h (quickstart.md Release measurements).

use std::sync::Arc;
use std::time::{Duration, Instant};

// See `realtime.rs`'s identical comment: `AllocDisabler` is compiled out
// under `cargo test --release` (the crate's default `disable_release`
// feature); `assert_no_alloc` itself stays unconditional (a no-op there),
// which is exactly what `reference_chain_under_half_core` needs — it
// measures wall time, not allocations, and must build in release.
#[cfg(debug_assertions)]
use assert_no_alloc::AllocDisabler;
use assert_no_alloc::assert_no_alloc;
use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_effects::catalog::{NodeKind, NodeOwner, ParamId};
use modplayer_engine::{
    CeilingDb, Command, Event, Processor, ProcessorConfig, RtShared, Transport, VolumePercent,
};
use rtrb::RingBuffer;

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const RATE: u32 = 44_100;
const BLOCK: usize = 256;

/// A coarse resident-set-size sample for this process, in kilobytes.
/// Shells out to the platform's own process-inspection tool rather than
/// adding a dependency for it (Constitution X: no new dependency for a
/// test-only, best-effort leak smoke test); `None` if that tool is
/// unavailable or its output is unexpected — a missing sample is never
/// treated as a leak.
fn peak_rss_kb() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                return rest.trim().trim_end_matches("kB").trim().parse().ok();
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        let pid = std::process::id().to_string();
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout).trim().parse().ok()
    }
    #[cfg(target_os = "windows")]
    {
        let pid = std::process::id();
        let out = std::process::Command::new("wmic")
            .args([
                "process",
                "where",
                &format!("ProcessId={pid}"),
                "get",
                "WorkingSetSize",
            ])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        text.lines()
            .filter_map(|l| l.trim().parse::<u64>().ok())
            .next()
            .map(|bytes| bytes / 1024)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Builds a `Processor` already playing SC-003's reference chain (pitch
/// shift in slot 0, time stretch in slot 1, an 8-band equalizer in slot
/// 2), rendering `BLOCK`-frame buffers at `RATE`.
fn build_reference_chain_processor() -> (Processor<SyntheticSource>, rtrb::Producer<Command>) {
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: RATE,
        device_rate: RATE,
        device_channels: 2,
        max_frames: BLOCK,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared,
    };
    let processor = Processor::new(config, SyntheticSource::new(RATE), command_rx, event_tx);
    let _ = command_tx.push(Command::Play);
    let _ = command_tx.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::PitchShift,
        owner: NodeOwner::Host,
    });
    let _ = command_tx.push(Command::ChainInsert {
        slot: 1,
        position: 1,
        kind: NodeKind::TimeStretch,
        owner: NodeOwner::Host,
    });
    let _ = command_tx.push(Command::ChainInsert {
        slot: 2,
        position: 2,
        kind: NodeKind::Equalizer,
        owner: NodeOwner::Host,
    });
    (processor, command_tx)
}

/// Drives the reference chain for `seconds` of simulated playback,
/// continuously dragging parameters, reordering and bypassing nodes,
/// asserting `assert_no_alloc` on every render and that a coarse
/// `peak_rss` sample taken every 10 s of simulated time stays flat.
fn run_reference_chain_soak(seconds: u64) {
    let (mut processor, mut command_tx) = build_reference_chain_processor();
    let mut out = vec![0.0f32; BLOCK * 2];

    let renders_per_10s = (RATE as u64 * 10 / BLOCK as u64) as u32;
    let total_renders = renders_per_10s.saturating_mul((seconds / 10).max(1) as u32);
    let mut rss_samples: Vec<u64> = Vec::new();

    for i in 0..total_renders {
        // A steady stream of parameter drags, reorders and bypasses —
        // never allowed to interrupt the render path itself (Constitution
        // I): every edit is a `Command` drained at the next buffer
        // boundary, exactly like real UI-driven use.
        match i % 37 {
            1 => {
                let semitones = ((i % 25) as f32) - 12.0;
                let _ = command_tx.push(Command::ChainSetParam {
                    slot: 0,
                    param: ParamId(0),
                    value: semitones,
                });
            }
            5 => {
                let ratio = 0.5 + (i % 15) as f32 * 0.1;
                let _ = command_tx.push(Command::ChainSetParam {
                    slot: 1,
                    param: ParamId(0),
                    value: ratio,
                });
            }
            9 => {
                let band = (i % 8) as u8;
                let gain_db = ((i % 12) as f32) - 6.0;
                let _ = command_tx.push(Command::ChainSetParam {
                    slot: 2,
                    param: ParamId::eq_band(band, 1),
                    value: gain_db,
                });
            }
            13 => {
                let _ = command_tx.push(Command::ChainMove {
                    slot: 0,
                    position: (i % 3) as u8,
                });
            }
            17 => {
                let _ = command_tx.push(Command::ChainSetBypass {
                    slot: 1,
                    bypassed: (i / 17).is_multiple_of(2),
                });
            }
            21 => {
                let _ = command_tx.push(Command::ChainSetBypass {
                    slot: 0,
                    bypassed: (i / 21).is_multiple_of(2),
                });
            }
            25 => {
                let _ = command_tx.push(Command::ChainMove {
                    slot: 2,
                    position: (i % 3) as u8,
                });
            }
            29 => {
                let formant = (i / 29).is_multiple_of(2);
                let _ = command_tx.push(Command::ChainSetParam {
                    slot: 0,
                    param: ParamId(1),
                    value: if formant { 1.0 } else { 0.0 },
                });
            }
            _ => {}
        }

        assert_no_alloc(|| {
            processor.render(&mut out);
        });

        if i > 0
            && i.is_multiple_of(renders_per_10s)
            && let Some(kb) = peak_rss_kb()
        {
            rss_samples.push(kb);
        }
    }

    if rss_samples.len() >= 2 {
        let first = *rss_samples.first().unwrap_or(&0) as f64;
        let last = *rss_samples.last().unwrap_or(&0) as f64;
        // A generous drift bound: `assert_no_alloc` above already proves
        // the render path itself never allocates, so this only guards
        // against a leak in the command-queue/edit path outside the RT
        // path — a coarse smoke test, not a precision budget.
        assert!(
            last <= first * 1.5 + 4_096.0,
            "resident memory grew from {first} KB to {last} KB over a {seconds} s soak run, \
             possible leak (samples: {rss_samples:?})"
        );
    }
}

#[test]
fn reference_chain_soak_60s() {
    run_reference_chain_soak(60);
}

/// Manual: `MODPLAYER_SOAK_HOURS=24 cargo test --release -p
/// modplayer-engine -- --ignored reference_chain_soak_24h`
/// (quickstart.md Release measurements).
#[test]
#[ignore = "manual"]
fn reference_chain_soak_24h() {
    let hours: u64 = std::env::var("MODPLAYER_SOAK_HOURS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(24);
    run_reference_chain_soak(hours.saturating_mul(3_600));
}

/// Manual, release-only: `cargo test --release -p modplayer-engine --
/// --ignored reference_chain_under_half_core` (quickstart.md Release
/// measurements). SC-003/NFR-1.8: the reference chain (pitch shift +
/// time stretch + 8-band equalizer) must cost < 50 % of one core at
/// 128 frames / 44.1 kHz, i.e. < 1.45 ms per render.
#[test]
#[ignore = "manual"]
fn reference_chain_under_half_core() {
    let block = 128usize;
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: RATE,
        device_rate: RATE,
        device_channels: 2,
        max_frames: block,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared,
    };
    let mut processor = Processor::new(config, SyntheticSource::new(RATE), command_rx, event_tx);
    let _ = command_tx.push(Command::Play);
    let _ = command_tx.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::PitchShift,
        owner: NodeOwner::Host,
    });
    let _ = command_tx.push(Command::ChainInsert {
        slot: 1,
        position: 1,
        kind: NodeKind::TimeStretch,
        owner: NodeOwner::Host,
    });
    let _ = command_tx.push(Command::ChainInsert {
        slot: 2,
        position: 2,
        kind: NodeKind::Equalizer,
        owner: NodeOwner::Host,
    });
    let _ = command_tx.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(0),
        value: 7.0,
    });
    let _ = command_tx.push(Command::ChainSetParam {
        slot: 1,
        param: ParamId(0),
        value: 0.8,
    });
    for band in 0..8u8 {
        let _ = command_tx.push(Command::ChainSetParam {
            slot: 2,
            param: ParamId::eq_band(band, 0),
            value: 500.0 * f32::from(band + 1),
        });
        let _ = command_tx.push(Command::ChainSetParam {
            slot: 2,
            param: ParamId::eq_band(band, 1),
            value: 3.0,
        });
        let _ = command_tx.push(Command::ChainSetParam {
            slot: 2,
            param: ParamId::eq_band(band, 2),
            value: 1.0,
        });
    }

    let mut out = vec![0.0f32; block * 2];
    // Settle every insert/param crossfade and ramp before measuring.
    for _ in 0..500 {
        processor.render(&mut out);
    }

    let iterations = 2_000u32;
    let start = Instant::now();
    for _ in 0..iterations {
        processor.render(&mut out);
    }
    let elapsed = start.elapsed();
    let per_render = elapsed / iterations;
    let budget = Duration::from_micros(1_450);
    assert!(
        per_render < budget,
        "reference chain render averaged {per_render:?}, budget {budget:?} (SC-003, NFR-1.8)"
    );
}
