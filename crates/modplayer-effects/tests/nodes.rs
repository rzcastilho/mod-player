// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §12: node-kernel identity and
//! transfer-function tests (US3, FR-006, FR-009, SC-013).

use modplayer_effects::catalog::{self, NodeKind};
use modplayer_effects::nodes::{Eq8Dsp, FilterDsp, GainDsp, StereoDsp};
use modplayer_effects::smooth::Smoothed;

const MAX_PARAMS: usize = 48;

/// Every catalog default for `kind`, laid out by `ParamId` (data-model.md
/// §1.3) — the same array shape `NodeSlot::params` uses.
fn default_params(kind: NodeKind) -> [Smoothed; MAX_PARAMS] {
    let mut params = [Smoothed::new(0.0); MAX_PARAMS];
    for def in catalog::params(kind) {
        params[def.id.0 as usize] = Smoothed::new(def.default);
    }
    params
}

/// A full-scale sine, interleaved stereo (identical L/R).
fn sine_stereo(freq: f32, rate: u32, n: usize) -> Vec<f32> {
    let mut buf = vec![0.0f32; n * 2];
    for i in 0..n {
        let x = (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin();
        buf[i * 2] = x;
        buf[i * 2 + 1] = x;
    }
    buf
}

/// The steady-state amplitude of the left channel's last `window`
/// samples (RMS × √2 — exact for a pure sinusoid, and a good estimate
/// for a settled IIR response to one).
fn steady_amplitude_l(buf: &[f32], window: usize) -> f32 {
    let n = buf.len() / 2;
    let start = n.saturating_sub(window);
    let mut sum_sq = 0.0f64;
    for i in start..n {
        let x = f64::from(buf[i * 2]);
        sum_sq += x * x;
    }
    let rms = (sum_sq / (n - start) as f64).sqrt();
    (rms * std::f64::consts::SQRT_2) as f32
}

/// contracts/engine-effect-chain.md §5: every kernel is sample-exact
/// identity at its catalog defaults — the property every added effect
/// must preserve so an empty-seeming chain never colours the sound
/// (SC-013).
#[test]
fn gain_eq_filter_stereo_identity_at_defaults() {
    let original = [0.3f32, -0.5, 0.7, -0.9, 0.0, 1.0];

    let mut gain = GainDsp::new();
    let mut level = Smoothed::new(0.0);
    let mut buf = original;
    gain.process(&mut buf, 3, &mut level);
    for (got, want) in buf.iter().zip(original.iter()) {
        assert!((got - want).abs() < 1e-6, "gain: got={got} want={want}");
    }

    let mut eq = Eq8Dsp::new();
    let mut params = default_params(NodeKind::Equalizer);
    let mut buf = original;
    eq.process(&mut buf, 3, &mut params, 44_100);
    assert_eq!(buf, original, "eq at defaults must be bit-exact");

    let mut filter = FilterDsp::new();
    let mut params = default_params(NodeKind::Filter);
    let mut buf = original;
    filter.process(&mut buf, 3, &mut params, 44_100);
    assert_eq!(
        buf, original,
        "filter at its documented default must be bit-exact"
    );

    let mut stereo = StereoDsp::new();
    let mut params = default_params(NodeKind::StereoTools);
    let mut buf = original;
    stereo.process(&mut buf, 3, &mut params);
    assert_eq!(buf, original, "stereo tools at defaults must be bit-exact");
}

/// contracts/engine-effect-chain.md §5: a peaking band's gain at its own
/// centre frequency is the specified `gain_db`, independent of `Q`.
#[test]
fn eq_peak_plus_6db_at_1khz() {
    let mut eq = Eq8Dsp::new();
    let mut params = default_params(NodeKind::Equalizer);
    params[16] = Smoothed::new(1_000.0); // band 0 freq
    params[17] = Smoothed::new(6.0); // band 0 gain
    params[18] = Smoothed::new(1.0); // band 0 q

    let rate = 44_100;
    let mut buf = sine_stereo(1_000.0, rate, 4_096);
    eq.process(&mut buf, 4_096, &mut params, rate);
    let amp = steady_amplitude_l(&buf, 2_000);
    let gain_db = 20.0 * amp.log10();
    assert!(
        (gain_db - 6.0).abs() < 0.3,
        "expected ~+6 dB at 1 kHz, got {gain_db} dB"
    );
}

/// contracts/engine-effect-chain.md §5: the high-pass filter at
/// `Q_BUTTER` (`resonance == 0`) is −3 dB at its own cutoff.
#[test]
fn filter_hp_minus_3db_at_cutoff() {
    let mut filter = FilterDsp::new();
    let mut params = default_params(NodeKind::Filter);
    params[1] = Smoothed::new(1_000.0); // cutoff
    params[2] = Smoothed::new(0.0); // resonance

    let rate = 44_100;
    let mut buf = sine_stereo(1_000.0, rate, 4_096);
    filter.process(&mut buf, 4_096, &mut params, rate);
    let amp = steady_amplitude_l(&buf, 2_000);
    let gain_db = 20.0 * amp.log10();
    assert!(
        (gain_db - (-3.0103)).abs() < 0.3,
        "expected ~-3 dB at cutoff, got {gain_db} dB"
    );
}

/// research R12: `resonance == 1.0` maps to exactly `Q_MAX`, and the
/// resulting filter is stable (finite output, no blow-up) at the cap.
#[test]
fn resonance_caps_at_q_max() {
    let mut filter = FilterDsp::new();
    let mut params = default_params(NodeKind::Filter);
    params[1] = Smoothed::new(1_000.0); // cutoff
    params[2] = Smoothed::new(1.0); // resonance == 1.0 -> Q_MAX

    let rate = 44_100;
    let mut buf = sine_stereo(1_000.0, rate, 4_096);
    filter.process(&mut buf, 4_096, &mut params, rate);
    assert!(
        buf.iter().all(|s| s.is_finite()),
        "resonant filter must stay stable"
    );
    let amp = steady_amplitude_l(&buf, 2_000);
    assert!(
        amp.is_finite() && amp < 100.0,
        "amplitude must stay bounded, got {amp}"
    );
}

/// research R13: width/balance/mono-sum/invert/swap in the fixed order,
/// exercised through the full `StereoDsp` (not just the free `apply`
/// helper unit-tested in `nodes/stereo.rs`).
#[test]
fn stereo_truth_table() {
    let mut stereo = StereoDsp::new();
    let mut params = default_params(NodeKind::StereoTools);
    params[0] = Smoothed::new(0.0); // width = 0 -> mono via mid
    let mut buf = [1.0f32, -1.0];
    stereo.process(&mut buf, 1, &mut params);
    assert!(buf[0].abs() < 1e-6 && buf[1].abs() < 1e-6);

    let mut stereo = StereoDsp::new();
    let mut params = default_params(NodeKind::StereoTools);
    params[1] = Smoothed::new(1.0); // balance = full right
    let mut buf = [0.5f32, 0.5];
    stereo.process(&mut buf, 1, &mut params);
    assert!(buf[0].abs() < 1e-6, "left muted at full-right balance");
    assert!((buf[1] - 0.5).abs() < 1e-6, "right unchanged");
}

/// research R13: with mono sum on and phase invert on, a centre-panned
/// (identical L/R) signal cancels to silence — the classic phase-invert
/// mono-cancellation trick.
#[test]
fn mono_sum_invert_cancels_centre() {
    let mut stereo = StereoDsp::new();
    stereo.set_flags_target(true, true, false, 0);
    let mut params = default_params(NodeKind::StereoTools);
    let mut buf = [0.6f32, 0.6, -0.4, -0.4];
    stereo.process(&mut buf, 2, &mut params);
    for s in buf {
        assert!(s.abs() < 1e-6, "centre content must cancel, got {s}");
    }
}
