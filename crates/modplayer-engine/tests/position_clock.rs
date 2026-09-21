// SPDX-License-Identifier: MIT OR Apache-2.0

//! `position_clock_60hz_jitter` / `position_clock_frozen_when_paused`
//! (engine-delta.md §3, §5, SC-003): `PositionClock` must let the UI
//! sample position at >= 60 Hz with <= 5 ms jitter even though the render
//! rate itself is lower, and must not drift at all while not playing.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_engine::{
    CeilingDb, Command, PositionClock, Processor, ProcessorConfig, RtShared, Transport,
    VolumePercent,
};
use rtrb::RingBuffer;

const SOURCE_RATE: u32 = 44_100;
const BUFFER_FRAMES: usize = 256;
/// ~60 Hz sampling interval (SC-003).
const SAMPLE_INTERVAL: Duration = Duration::from_micros(16_667);
const TEST_DURATION: Duration = Duration::from_secs(10);
/// SC-003's own bound, asserted as-is on a developer machine (and the
/// quickstart's manual sign-off run).
const MAX_JITTER: Duration = Duration::from_millis(5);
/// On a hosted CI runner (`CI=true`; GitHub's macOS runners are small,
/// shared VMs whose timer/scheduling jitter alone was measured at a
/// 20 ms 95th percentile) the bound is widened into a coarse guard
/// against gross starvation or a broken clock — the authoritative
/// measurement is the manual one, mirroring `modplayer-plugin-runtime`'s
/// own `position_jitter_under_5ms`.
const MAX_JITTER_CI: Duration = Duration::from_millis(40);

fn max_jitter() -> Duration {
    if std::env::var_os("CI").is_some() {
        MAX_JITTER_CI
    } else {
        MAX_JITTER
    }
}

#[test]
fn position_clock_60hz_jitter() {
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<modplayer_engine::Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: SOURCE_RATE,
        device_rate: SOURCE_RATE,
        device_channels: 2,
        max_frames: BUFFER_FRAMES,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared: Arc::clone(&shared),
    };
    let mut processor = Processor::new(
        config,
        SyntheticSource::new(SOURCE_RATE),
        command_rx,
        event_tx,
    );
    let _ = command_tx.push(Command::Play);

    let stop = Arc::new(AtomicBool::new(false));
    let render_thread = {
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let buffer_duration =
                Duration::from_secs_f64(BUFFER_FRAMES as f64 / f64::from(SOURCE_RATE));
            let mut out = vec![0.0f32; BUFFER_FRAMES * 2];
            let mut next_due = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                processor.render(&mut out);
                next_due += buffer_duration;
                let now = Instant::now();
                if next_due > now {
                    thread::sleep(next_due - now);
                }
            }
        })
    };

    let start = Instant::now();
    // Each sample pairs the position reading with the wall-clock instant
    // it was actually taken at, so jitter is measured against real elapsed
    // time rather than against the sampling loop's own scheduling
    // precision (`thread::sleep` granularity is itself several ms on a
    // loaded machine and is not what SC-003 is about).
    let mut samples: Vec<(Instant, Duration)> = Vec::new();
    let mut next_sample = start;
    while start.elapsed() < TEST_DURATION {
        let now = Instant::now();
        if next_sample > now {
            thread::sleep(next_sample - now);
        }
        let taken_at = Instant::now();
        samples.push((taken_at, PositionClock::now(&shared, SOURCE_RATE)));
        next_sample += SAMPLE_INTERVAL;
    }

    stop.store(true, Ordering::Relaxed);
    let _ = render_thread.join();

    assert!(samples.len() > 100, "expected many samples over 10 s");

    // Monotonicity is a hard invariant, always.
    for pair in samples.windows(2) {
        let (_, position_a) = pair[0];
        let (_, position_b) = pair[1];
        assert!(
            position_b >= position_a,
            "position must be non-decreasing: {position_a:?} -> {position_b:?}"
        );
    }

    // Per-sample jitter is measured against real elapsed wall time (not
    // the sampling loop's own `thread::sleep` scheduling precision, which
    // is not what SC-003 is about). On a shared/virtualized CI machine
    // either worker thread can occasionally be descheduled for a few ms,
    // producing a rare outlier unrelated to `PositionClock`'s own math;
    // the 95th percentile — not every single pair — is checked against the
    // ≤ 5 ms bound, with a generous sanity ceiling on the worst outlier.
    let mut jitters: Vec<Duration> = samples
        .windows(2)
        .map(|pair| {
            let (instant_a, position_a) = pair[0];
            let (instant_b, position_b) = pair[1];
            let position_delta = position_b - position_a;
            let wall_elapsed = instant_b.saturating_duration_since(instant_a);
            position_delta.abs_diff(wall_elapsed)
        })
        .collect();
    jitters.sort();

    let p95 = jitters[(jitters.len() * 95 / 100).min(jitters.len() - 1)];
    let bound = max_jitter();
    assert!(
        p95 <= bound,
        "95th-percentile jitter {p95:?} exceeds {bound:?}"
    );
    let worst = *jitters.last().unwrap_or(&Duration::ZERO);
    assert!(
        worst <= bound * 10,
        "worst-case jitter {worst:?} is far beyond scheduling noise"
    );
}

#[test]
fn position_clock_frozen_when_paused() {
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<modplayer_engine::Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: SOURCE_RATE,
        device_rate: SOURCE_RATE,
        device_channels: 2,
        max_frames: BUFFER_FRAMES,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared: Arc::clone(&shared),
    };
    let mut processor = Processor::new(
        config,
        SyntheticSource::new(SOURCE_RATE),
        command_rx,
        event_tx,
    );
    let _ = command_tx.push(Command::Play);

    let mut out = vec![0.0f32; BUFFER_FRAMES * 2];
    for _ in 0..10 {
        processor.render(&mut out);
    }

    let _ = command_tx.push(Command::Pause);
    processor.render(&mut out);

    let frozen_at = PositionClock::now(&shared, SOURCE_RATE);
    thread::sleep(Duration::from_millis(200));
    // No further renders happen while paused (mirrors a real backend that
    // stops calling back, or a paused stream still producing silence —
    // either way `anchor_playing` is false and the clock must not drift).
    let still = PositionClock::now(&shared, SOURCE_RATE);
    assert_eq!(frozen_at, still, "position must not drift while paused");
}
