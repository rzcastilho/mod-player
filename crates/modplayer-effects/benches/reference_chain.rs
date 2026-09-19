// SPDX-License-Identifier: MIT OR Apache-2.0

//! T095: SC-003's reference chain (pitch shift + time stretch + 8-band
//! EQ) at 128 frames / 44.1 kHz, reported as `ns/render` against the
//! 2.9 ms callback period (`< 1.45 ms/render`, 50 % of one core,
//! NFR-1.8). `cargo bench -p modplayer-effects -- reference_chain`.
//!
//! This bench exercises `ChainRt` directly (not `Processor`), which is
//! sufficient: every render-path cost this feature adds lives inside
//! `ChainRt::process`'s node loop (contracts/engine-effect-chain.md
//! §3/§7); `Processor`'s own surrounding work (decode, master gain,
//! limiter) is unchanged by this feature and already covered by 001's
//! baseline.

use std::time::Instant;

use criterion::{Criterion, criterion_group, criterion_main};

use modplayer_effects::catalog::{NodeKind, NodeOwner, ParamId};
use modplayer_effects::rt::chain::ChainRt;

const RATE: u32 = 44_100;
const FRAMES: usize = 128;

fn build_reference_chain() -> ChainRt {
    let mut chain = ChainRt::new(RATE);
    chain.apply_insert(0, 0, NodeKind::PitchShift, NodeOwner::Host);
    chain.apply_insert(1, 1, NodeKind::TimeStretch, NodeOwner::Host);
    chain.apply_insert(2, 2, NodeKind::Equalizer, NodeOwner::Host);

    // +7 semitones, non-adjacent-equivalent product ratio, all 8 EQ
    // bands lightly engaged — representative tone-shaping load, not the
    // (cheaper) identity defaults.
    chain.apply_set_param(0, ParamId(0), 7.0);
    chain.apply_set_param(1, ParamId(0), 0.8);
    for band in 0..8u8 {
        chain.apply_set_param(2, ParamId::eq_band(band, 0), 500.0 * f32::from(band + 1));
        chain.apply_set_param(2, ParamId::eq_band(band, 1), 3.0);
        chain.apply_set_param(2, ParamId::eq_band(band, 2), 1.0);
    }

    // Drain the insert/ramp crossfades so the measured loop sees steady-
    // state cost, not the one-off fade-in.
    for _ in 0..64 {
        let planned = chain.plan(FRAMES);
        let input = chain.input_buffer_mut(planned);
        for (i, s) in input.iter_mut().enumerate() {
            *s = ((i / 2) as f32 * 0.037).sin();
        }
        let _ = chain.process(FRAMES, Instant::now);
    }
    chain
}

fn bench_reference_chain(c: &mut Criterion) {
    let mut chain = build_reference_chain();
    c.bench_function("reference_chain_128f_44_1k", |b| {
        b.iter(|| {
            let planned = chain.plan(FRAMES);
            let input = chain.input_buffer_mut(planned);
            for (i, s) in input.iter_mut().enumerate() {
                *s = ((i / 2) as f32 * 0.037).sin();
            }
            let out = chain.process(FRAMES, Instant::now);
            std::hint::black_box(out);
        });
    });
}

criterion_group!(benches, bench_reference_chain);
criterion_main!(benches);
