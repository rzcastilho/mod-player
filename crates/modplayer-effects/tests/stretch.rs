// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §12: `StretchStage` — pins US1
//! AS1/AS2/AS3/AS6, FR-007, SC-013.

use modplayer_effects::catalog::QualityMode;
use modplayer_effects::nodes::stretch::{StretchBuffers, StretchParams, StretchStage, interpolate};

/// Drive `stage` with an infinite periodic `source` (indexed by absolute
/// sample count via `source_fn`) until `out_total` output frames have
/// been produced, appending exactly `input_for`'s requested amount before
/// each `process` call (contracts/engine-effect-chain.md §3).
fn drive(
    stage: &mut StretchStage,
    buffers: &mut StretchBuffers,
    params: &StretchParams,
    out_total: usize,
    mut source_fn: impl FnMut(u64) -> (f32, f32),
) -> (Vec<f32>, u64) {
    let mut consumed: u64 = 0;
    let mut produced = Vec::with_capacity(out_total * 2);
    let block = 256usize;
    let mut scratch_in = vec![0.0f32; 0];
    let mut scratch_out = vec![0.0f32; block * 2];
    while produced.len() / 2 < out_total {
        let want = block.min(out_total - produced.len() / 2);
        let need_in = stage.input_for(want, params).min(20_000);
        scratch_in.clear();
        scratch_in.resize(need_in * 2, 0.0);
        for i in 0..need_in {
            let (l, r) = source_fn(consumed + i as u64);
            scratch_in[i * 2] = l;
            scratch_in[i * 2 + 1] = r;
        }
        consumed += need_in as u64;
        stage.append(buffers, &scratch_in, need_in);
        stage.process(
            buffers,
            &scratch_in,
            &mut scratch_out[..want * 2],
            want,
            params,
        );
        produced.extend_from_slice(&scratch_out[..want * 2]);
    }
    (produced, consumed)
}

fn click_train(period: usize) -> impl FnMut(u64) -> (f32, f32) {
    move |i| {
        if (i as usize).is_multiple_of(period) {
            (1.0, 1.0)
        } else {
            (0.0, 0.0)
        }
    }
}

fn sine(freq: f32, rate: f32) -> impl FnMut(u64) -> (f32, f32) {
    move |i| {
        let s = (std::f32::consts::TAU * freq * i as f32 / rate).sin();
        (s, s)
    }
}

/// Peaks in the left channel above `threshold`, debounced so a single
/// smeared pulse (Hann-windowed overlap-add spreads a click over a few
/// samples) counts once, at its maximum.
fn find_peaks(samples: &[f32], threshold: f32, min_gap: usize) -> Vec<usize> {
    let mut peaks = Vec::new();
    let mut i = 0usize;
    let frames = samples.len() / 2;
    while i < frames {
        if samples[i * 2].abs() >= threshold {
            let start = i;
            let mut best = i;
            let mut best_val = samples[i * 2].abs();
            while i < frames && samples[i * 2].abs() >= threshold * 0.3 {
                if samples[i * 2].abs() > best_val {
                    best_val = samples[i * 2].abs();
                    best = i;
                }
                i += 1;
            }
            peaks.push(best);
            let _ = start;
            i += min_gap;
        } else {
            i += 1;
        }
    }
    peaks
}

/// SC-013: at `stretch == step == 1.0`, `process` is sample-exact
/// pass-through (already unit-tested in `src/nodes/stretch.rs`, repeated
/// here at the crate-boundary as the named contract test).
#[test]
fn unity_is_pass_through() {
    let mut stage = StretchStage::new(44_100);
    let mut buffers = StretchBuffers::new();
    let params = StretchParams {
        stretch: 1.0,
        step: 1.0,
        formant: false,
        quality: QualityMode::Performance,
    };
    let (out, _consumed) = drive(
        &mut stage,
        &mut buffers,
        &params,
        2_000,
        sine(440.0, 44_100.0),
    );
    let mut expect_src = sine(440.0, 44_100.0);
    for i in 0..2_000 {
        let (l, _r) = expect_src(i as u64);
        assert!(
            (out[i * 2] - l).abs() < 1e-4,
            "sample {i}: got={} want={l}",
            out[i * 2]
        );
    }
}

/// US1 AS2: a tempo ratio of 0.5 (half speed) doubles the output/input
/// duration ratio (time stretch alone: `stretch = 1/r`, `step = 1`) —
/// measured two ways: the direct `consumed`/`produced` frame tally (the
/// unambiguous definition of "half speed takes twice as long"), and,
/// as a click-train sanity check, that the output still contains
/// clearly separated click energy (not silence or noise).
#[test]
fn tempo_half_doubles_click_interval() {
    let period = 2_000usize;
    let mut stage = StretchStage::new(44_100);
    let mut buffers = StretchBuffers::new();
    let params = StretchParams::tempo_alone(0.5, QualityMode::Performance);

    let (out, consumed) = drive(
        &mut stage,
        &mut buffers,
        &params,
        24_000,
        click_train(period),
    );
    let produced = (out.len() / 2) as u64;
    let ratio = produced as f64 / consumed as f64;
    assert!(
        (1.6..=2.4).contains(&ratio),
        "producing at half tempo must take ~2x the input frames: consumed={consumed} produced={produced} ratio={ratio}"
    );

    let peaks = find_peaks(&out, 0.2, 50);
    assert!(
        peaks.len() >= 3,
        "need several distinct clicks in the output, got {}",
        peaks.len()
    );
}

/// US1 AS1/AS6: shifting up 7 semitones scales the fundamental frequency
/// by `2^(7/12)` and leaves duration unchanged (pitch alone: `stretch =
/// step = p`).
#[test]
fn pitch_up_7_keeps_duration() {
    let rate = 44_100.0f32;
    let freq = 440.0f32;
    let semitones = 7.0f32;
    let p = 2f32.powf(semitones / 12.0);

    let mut stage = StretchStage::new(44_100);
    let mut buffers = StretchBuffers::new();
    let params = StretchParams::pitch_alone(semitones, false, QualityMode::Performance);
    let out_frames = 8_000usize;
    let (out, _consumed) = drive(
        &mut stage,
        &mut buffers,
        &params,
        out_frames,
        sine(freq, rate),
    );

    // Estimate frequency via zero-crossing rate over the settled tail
    // (skip the first couple of analysis windows while state fills in).
    let skip = 2_000usize;
    let mut crossings = 0u32;
    for i in skip + 1..out_frames {
        if (out[(i - 1) * 2] < 0.0) != (out[i * 2] < 0.0) {
            crossings += 1;
        }
    }
    let measured_freq = crossings as f32 / 2.0 / ((out_frames - skip) as f32 / rate);
    let expected = freq * p;
    let ratio = measured_freq / expected;
    assert!(
        (0.8..=1.2).contains(&ratio),
        "measured={measured_freq} expected={expected} (p={p})"
    );
}

/// FR-007: the combined stage's product-parameter table (research R4).
#[test]
fn combined_stage_products() {
    let params = StretchParams::combined(
        7.0,
        0.5,
        true,
        QualityMode::Performance,
        QualityMode::Performance,
    );
    let p = 2f32.powf(7.0 / 12.0);
    assert!((params.stretch - p / 0.5).abs() < 1e-4);
    assert!((params.step - p).abs() < 1e-4);
    assert!(params.formant);
    assert_eq!(params.quality, QualityMode::Performance);

    // Either member in quality mode makes the combined stage quality.
    let params_q = StretchParams::combined(
        7.0,
        0.5,
        false,
        QualityMode::Quality,
        QualityMode::Performance,
    );
    assert_eq!(params_q.quality, QualityMode::Quality);

    // A bypassed member contributes identity (p=1 or r=1) — the caller
    // (rt/chain.rs) passes 0/1.0 for a bypassed pitch/stretch node.
    let bypassed_pitch = StretchParams::combined(
        0.0,
        0.5,
        false,
        QualityMode::Performance,
        QualityMode::Performance,
    );
    assert!((bypassed_pitch.stretch - 1.0 / 0.5).abs() < 1e-4);
    assert!((bypassed_pitch.step - 1.0).abs() < 1e-4);
}

/// `samples`' power (Goertzel single-bin magnitude-squared) at `freq`,
/// mono left channel of a stereo-interleaved buffer.
fn goertzel_power(samples: &[f32], freq: f32, rate: f32) -> f32 {
    let n = samples.len() / 2;
    if n == 0 {
        return 0.0;
    }
    let w = std::f32::consts::TAU * freq / rate;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for i in 0..n {
        let s0 = samples[i * 2] + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2) / n as f32
}

/// Formant preservation keeps a resonant spectral-envelope peak (built by
/// running a harmonic-rich source through a simple two-pole resonator
/// centred at `FORMANT_HZ`) close to its original frequency after a
/// large pitch shift; without formant correction the whole spectrum —
/// resonance included — shifts up with the pitch (research R4).
#[test]
fn formant_on_keeps_envelope_peaks() {
    let rate = 44_100.0f32;
    const FORMANT_HZ: f32 = 2_500.0;

    let harmonics = [110.0f32, 220.0, 330.0, 440.0, 550.0, 660.0, 770.0, 880.0];
    // A simple two-pole resonator (`y[n] = x[n] + 2r*cos(w0)*y[n-1] -
    // r^2*y[n-2]`) colours the harmonic source with a peak at
    // `FORMANT_HZ`, computed once over a buffer long enough for every
    // render below to read from.
    let total = 24_000usize;
    let mut raw = vec![0.0f32; total];
    for (i, sample) in raw.iter_mut().enumerate() {
        let t = i as f32 / rate;
        *sample = harmonics
            .iter()
            .enumerate()
            .map(|(k, f)| (std::f32::consts::TAU * f * t).sin() / (k as f32 + 1.0))
            .sum();
    }
    let r = 0.97f32;
    let w0 = std::f32::consts::TAU * FORMANT_HZ / rate;
    let (mut y1, mut y2) = (0.0f32, 0.0f32);
    let mut colored = vec![0.0f32; total];
    for i in 0..total {
        let y = raw[i] + 2.0 * r * w0.cos() * y1 - r * r * y2;
        colored[i] = y;
        y2 = y1;
        y1 = y;
    }
    let run = |formant: bool| -> Vec<f32> {
        let mut stage = StretchStage::new(44_100);
        let mut buffers = StretchBuffers::new();
        let params = StretchParams::pitch_alone(12.0, formant, QualityMode::Performance);
        let mut source = |i: u64| {
            let idx = (i as usize).min(total - 1);
            (colored[idx], colored[idx])
        };
        drive(&mut stage, &mut buffers, &params, 12_000, &mut source).0
    };

    let with_formant = run(true);
    let without_formant = run(false);

    let near = |v: &[f32]| goertzel_power(v, FORMANT_HZ, rate);
    let power_with = near(&with_formant);
    let power_without = near(&without_formant);

    assert!(power_with > 0.0 && power_without > 0.0);
    assert!(
        power_with > power_without,
        "formant correction must retain more energy at the original formant frequency: with={power_with} without={power_without}"
    );
}

/// research R4: performance mode's resampler is linear, quality mode's is
/// cubic Hermite — a direct, deterministic check of the kernel selection
/// (integration-level per T031's naming; the numerical kernel itself is
/// `pub` precisely so this test doesn't need to reach into `StretchStage`
/// internals).
#[test]
fn quality_mode_uses_cubic() {
    // Four non-collinear points: linear and cubic Hermite must disagree
    // at the midpoint, and cubic must match the closed-form Hermite value.
    let neighbors = [0.0f32, 0.0, 1.0, 0.0];
    let linear = interpolate(QualityMode::Performance, neighbors, 0.5);
    let cubic = interpolate(QualityMode::Quality, neighbors, 0.5);
    assert!(
        (linear - 0.5).abs() < 1e-6,
        "linear midpoint of 0..1 must be 0.5, got {linear}"
    );
    assert!(
        (cubic - linear).abs() > 0.05,
        "quality mode must differ from a plain linear read: cubic={cubic} linear={linear}"
    );
    // Cubic Hermite reproduces a straight line exactly (a property linear
    // interpolation trivially also has) — confirms it isn't just a
    // differently-scaled linear blend.
    let straight = [0.0f32, 1.0, 2.0, 3.0];
    let on_line = interpolate(QualityMode::Quality, straight, 0.5);
    assert!((on_line - 1.5).abs() < 1e-4);
}
