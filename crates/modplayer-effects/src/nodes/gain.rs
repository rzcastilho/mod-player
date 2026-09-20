// SPDX-License-Identifier: MIT OR Apache-2.0

//! `GainDsp`: the trivial first kernel (T012), used to prove the chain
//! pipeline end to end before any other node kind exists.
//! `y = x * 10^(dB/20)`; mute is its own 5 ms linear fade to silence,
//! independent of `level_db`'s 20 ms ramp (contracts/engine-effect-
//! chain.md §5).

use crate::smooth::Smoothed;

/// `10^(db/20)`, the decibel-to-linear-amplitude conversion every gain
/// stage in this crate uses.
#[must_use]
pub fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// The gain kernel's per-slot state: just the mute fade — `level_db`
/// itself is the slot's own `Smoothed` parameter, passed in each render.
#[derive(Debug, Clone, Copy)]
pub struct GainDsp {
    /// `1.0` = unmuted, `0.0` = fully muted; ramps linearly over the 5 ms
    /// mute fade whenever the discrete `mute` parameter toggles.
    mute_gain: Smoothed,
}

impl GainDsp {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            mute_gain: Smoothed::new(1.0),
        }
    }

    /// Retarget the mute fade (called when the `mute` parameter changes).
    pub fn set_muted(&mut self, muted: bool, fade_frames: u32) {
        self.mute_gain
            .set_target(if muted { 0.0 } else { 1.0 }, fade_frames);
    }

    /// Process `frames` stereo frames of `buf` in place. `level_db` is
    /// this slot's `level_db` parameter, advanced one sample per frame.
    pub fn process(&mut self, buf: &mut [f32], frames: usize, level_db: &mut Smoothed) {
        for i in 0..frames {
            let gain = db_to_linear(level_db.advance()) * self.mute_gain.advance();
            buf[i * 2] *= gain;
            buf[i * 2 + 1] *= gain;
        }
    }
}

impl Default for GainDsp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `gain_identity_at_defaults` (contracts/engine-effect-chain.md §12)
    // lives in `tests/nodes.rs`.

    #[test]
    fn mute_fades_to_silence() {
        let mut dsp = GainDsp::new();
        dsp.set_muted(true, 4);
        let mut level = Smoothed::new(0.0);
        let mut buf = vec![1.0f32; 4 * 2];
        dsp.process(&mut buf, 4, &mut level);
        assert!(
            buf[6].abs() < 1e-6 && buf[7].abs() < 1e-6,
            "must be silent after the fade completes"
        );
        assert!(buf[0].abs() > buf[6].abs(), "must fade down, not jump");
    }
}
