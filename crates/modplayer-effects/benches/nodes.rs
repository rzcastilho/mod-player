// SPDX-License-Identifier: MIT OR Apache-2.0

//! T094: per-node-kind criterion benches (research R16, quickstart.md
//! Release measurements table). `cargo bench -p modplayer-effects --
//! nodes`. Every group covers 128/256/1024-frame blocks at 44.1 kHz
//! (typical device buffer sizes, contracts/engine-effect-chain.md §12);
//! `stretch` additionally crosses both `QualityMode`s and formant on/off,
//! since those are the kernel's own dominant cost dimensions.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use modplayer_effects::catalog::{self, NodeKind, QualityMode};
use modplayer_effects::nodes::stretch::{StretchBuffers, StretchParams, StretchStage};
use modplayer_effects::nodes::{Eq8Dsp, FilterDsp, GainDsp, StereoDsp};
use modplayer_effects::smooth::Smoothed;

const RATE: u32 = 44_100;
const BLOCK_SIZES: [usize; 3] = [128, 256, 1024];
const MAX_PARAMS: usize = 48;

/// Every catalog default for `kind`, laid out by `ParamId` — the same
/// array shape `NodeSlot::params` uses (mirrors `tests/nodes.rs`).
fn default_params(kind: NodeKind) -> [Smoothed; MAX_PARAMS] {
    let mut params = [Smoothed::new(0.0); MAX_PARAMS];
    for def in catalog::params(kind) {
        params[def.id.0 as usize] = Smoothed::new(def.default);
    }
    params
}

/// A full-scale interleaved-stereo test tone, cheap to synthesise and
/// representative of a real render's dynamic range.
fn stereo_block(n: usize) -> Vec<f32> {
    (0..n)
        .flat_map(|i| {
            let x = (i as f32 * 0.037).sin();
            [x, -x]
        })
        .collect()
}

fn bench_gain(c: &mut Criterion) {
    let mut group = c.benchmark_group("nodes/gain");
    for &frames in &BLOCK_SIZES {
        let src = stereo_block(frames);
        group.bench_with_input(
            BenchmarkId::from_parameter(frames),
            &frames,
            |b, &frames| {
                let mut dsp = GainDsp::new();
                let mut level = Smoothed::new(-6.0);
                let mut buf = src.clone();
                b.iter(|| {
                    buf.copy_from_slice(&src);
                    dsp.process(&mut buf, frames, &mut level);
                    black_box(&buf);
                });
            },
        );
    }
    group.finish();
}

fn bench_eq(c: &mut Criterion) {
    let mut group = c.benchmark_group("nodes/eq");
    for &frames in &BLOCK_SIZES {
        let src = stereo_block(frames);
        group.bench_with_input(
            BenchmarkId::from_parameter(frames),
            &frames,
            |b, &frames| {
                let mut dsp = Eq8Dsp::new();
                // Every band lightly engaged (peak, +3 dB) so the bench
                // exercises the real recompute/process path, not just an
                // early identity short-circuit.
                let mut params = default_params(NodeKind::Equalizer);
                for band in 0..8u8 {
                    params[catalog::ParamId::eq_band(band, 0).0 as usize] =
                        Smoothed::new(500.0 * f32::from(band + 1));
                    params[catalog::ParamId::eq_band(band, 1).0 as usize] = Smoothed::new(3.0);
                    params[catalog::ParamId::eq_band(band, 2).0 as usize] = Smoothed::new(1.0);
                }
                let mut buf = src.clone();
                b.iter(|| {
                    buf.copy_from_slice(&src);
                    dsp.process(&mut buf, frames, &mut params, RATE);
                    black_box(&buf);
                });
            },
        );
    }
    group.finish();
}

fn bench_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("nodes/filter");
    for &frames in &BLOCK_SIZES {
        let src = stereo_block(frames);
        group.bench_with_input(
            BenchmarkId::from_parameter(frames),
            &frames,
            |b, &frames| {
                let mut dsp = FilterDsp::new();
                let mut params = default_params(NodeKind::Filter);
                params[1] = Smoothed::new(1_000.0); // cutoff
                params[2] = Smoothed::new(0.5); // resonance, mid-range
                let mut buf = src.clone();
                b.iter(|| {
                    buf.copy_from_slice(&src);
                    dsp.process(&mut buf, frames, &mut params, RATE);
                    black_box(&buf);
                });
            },
        );
    }
    group.finish();
}

fn bench_stereo(c: &mut Criterion) {
    let mut group = c.benchmark_group("nodes/stereo");
    for &frames in &BLOCK_SIZES {
        let src = stereo_block(frames);
        group.bench_with_input(
            BenchmarkId::from_parameter(frames),
            &frames,
            |b, &frames| {
                let mut dsp = StereoDsp::new();
                let mut params = default_params(NodeKind::StereoTools);
                params[0] = Smoothed::new(1.4); // width
                params[1] = Smoothed::new(0.2); // balance
                let mut buf = src.clone();
                b.iter(|| {
                    buf.copy_from_slice(&src);
                    dsp.process(&mut buf, frames, &mut params);
                    black_box(&buf);
                });
            },
        );
    }
    group.finish();
}

fn bench_stretch(c: &mut Criterion) {
    let mut group = c.benchmark_group("nodes/stretch");
    for &frames in &BLOCK_SIZES {
        for quality in [QualityMode::Performance, QualityMode::Quality] {
            for formant in [false, true] {
                let id = format!(
                    "{frames}f_{}_formant_{formant}",
                    if quality == QualityMode::Quality {
                        "quality"
                    } else {
                        "performance"
                    }
                );
                group.bench_function(BenchmarkId::new("pitch_up_7", id), |b| {
                    let mut stage = StretchStage::new(RATE);
                    let mut buffers = StretchBuffers::new();
                    let params = StretchParams::pitch_alone(7.0, formant, quality);
                    let mut scratch_out = vec![0.0f32; frames * 2];
                    // Warm the ring so the steady-state cost (not the
                    // one-off fill) is what gets measured.
                    for _ in 0..8 {
                        let need_in = stage.input_for(frames, &params).min(20_000);
                        let input = stereo_block(need_in.max(1));
                        stage.append(&mut buffers, &input, need_in);
                        stage.process(&mut buffers, &input, &mut scratch_out, frames, &params);
                    }
                    b.iter(|| {
                        let need_in = stage.input_for(frames, &params).min(20_000);
                        let input = stereo_block(need_in.max(1));
                        stage.append(&mut buffers, &input, need_in);
                        stage.process(&mut buffers, &input, &mut scratch_out, frames, &params);
                        black_box(&scratch_out);
                    });
                });
            }
        }
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_gain,
    bench_eq,
    bench_filter,
    bench_stereo,
    bench_stretch
);
criterion_main!(benches);
