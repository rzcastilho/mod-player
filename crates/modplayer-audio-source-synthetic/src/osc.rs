// SPDX-License-Identifier: MIT OR Apache-2.0

//! Oscillator primitives shared by the test tone (`tone.rs`, US1) and the
//! synthetic test track (`track.rs`, US2): a phase accumulator plus pure
//! waveform-shape functions, so phase stays continuous across `fill` calls
//! and across segment boundaries (contracts/audio-source.md).

/// A continuous phase accumulator, `0.0..1.0` cycle fraction.
#[derive(Debug, Clone, Copy, Default)]
pub struct Oscillator {
    phase: f64,
}

impl Oscillator {
    pub fn new() -> Self {
        Self { phase: 0.0 }
    }

    /// The current phase, `0.0..1.0`.
    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// Set the phase directly, wrapping into `0.0..1.0`.
    pub fn set_phase(&mut self, phase: f64) {
        self.phase = phase.rem_euclid(1.0);
    }

    /// Return the current phase and advance by `freq_hz / sample_rate`,
    /// wrapping at 1.0. Calling this once per sample and feeding the
    /// returned phase into a waveform-shape function keeps phase
    /// continuous across calls, buffers, and (for a fixed frequency)
    /// segment boundaries.
    pub fn advance(&mut self, freq_hz: f64, sample_rate: f64) -> f64 {
        let current = self.phase;
        self.phase = (self.phase + freq_hz / sample_rate).rem_euclid(1.0);
        current
    }
}

/// Sine wave shape at `phase` (`0.0..1.0`), peak amplitude 1.0.
pub fn sine(phase: f64) -> f32 {
    (phase * std::f64::consts::TAU).sin() as f32
}

/// Square wave shape at `phase` (`0.0..1.0`), amplitude ±1.0, 50% duty cycle.
pub fn square(phase: f64) -> f32 {
    if phase < 0.5 { 1.0 } else { -1.0 }
}

/// Sawtooth wave shape at `phase` (`0.0..1.0`), linear ramp from -1.0 to
/// 1.0 across each cycle.
pub fn sawtooth(phase: f64) -> f32 {
    (2.0 * phase - 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_accumulates_and_wraps() {
        let mut osc = Oscillator::new();
        let sample_rate = 4.0;
        let freq = 1.0; // one full cycle every 4 samples
        let phases: Vec<f64> = (0..8).map(|_| osc.advance(freq, sample_rate)).collect();
        assert_eq!(phases, vec![0.0, 0.25, 0.5, 0.75, 0.0, 0.25, 0.5, 0.75]);
    }

    #[test]
    fn waveform_shapes_at_key_phases() {
        assert!((sine(0.0)).abs() < 1e-9);
        assert!((sine(0.25) - 1.0).abs() < 1e-9);
        assert_eq!(square(0.0), 1.0);
        assert_eq!(square(0.5), -1.0);
        assert!((sawtooth(0.0) - (-1.0)).abs() < 1e-9);
        assert!((sawtooth(0.5) - 0.0).abs() < 1e-9);
    }
}
