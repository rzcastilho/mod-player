// SPDX-License-Identifier: MIT OR Apache-2.0

//! The output stage: resamples the engine's interleaved-stereo, source-rate
//! mix to the device's rate and maps it to the device's channel count
//! (research R6, R15; contracts/engine-commands.md). Preallocated at
//! construction for `MAX_FRAMES` frames; hard passthrough when
//! `source_rate == device_rate`.

use crate::resample::{resample_linear_stereo, step_for_rates};

/// Upper bound on frames processed per `render` call. Buffer presets top
/// out at 1024 (Safe); this leaves comfortable headroom while keeping the
/// output stage's scratch buffer a small, fixed, preallocated size
/// (data-model.md §3.2: "preallocated `src_buf` ... 2 × 4096 + guard").
pub const MAX_FRAMES: usize = 4096;

/// Resamples and channel-maps the engine's stereo mix to the device.
#[derive(Debug)]
pub struct OutputStage {
    source_rate: u32,
    device_rate: u32,
    device_channels: u16,
    phase: u64,
    step: u64,
    /// Preallocated scratch for the resampled (but not yet channel-mapped)
    /// stereo signal at device rate.
    resampled: Vec<f32>,
}

impl OutputStage {
    /// Construct an output stage converting `source_rate` Hz stereo to
    /// `device_rate` Hz / `device_channels` channels.
    pub fn new(source_rate: u32, device_rate: u32, device_channels: u16) -> Self {
        let device_rate = device_rate.max(1);
        Self {
            source_rate,
            device_rate,
            device_channels: device_channels.max(1),
            phase: 0,
            step: step_for_rates(source_rate, device_rate),
            resampled: vec![0.0; MAX_FRAMES * 2],
        }
    }

    /// True when no rate conversion is needed.
    pub fn is_passthrough(&self) -> bool {
        self.source_rate == self.device_rate
    }

    pub fn device_channels(&self) -> u16 {
        self.device_channels
    }

    /// Frames of source-rate stereo material `process` will need to
    /// produce `out_frames` of device-rate output, including one guard
    /// frame for interpolation lookahead. Capped at `MAX_FRAMES`.
    pub fn required_source_frames(&self, out_frames: usize) -> usize {
        if self.is_passthrough() {
            out_frames.min(MAX_FRAMES)
        } else {
            let sr = u64::from(self.source_rate);
            let dr = u64::from(self.device_rate);
            let needed = (out_frames as u64).saturating_mul(sr).div_ceil(dr) + 1;
            (needed as usize).min(MAX_FRAMES)
        }
    }

    /// Resample `src` (interleaved stereo, `src_frames` frames) and map
    /// channels into `out` (interleaved `device_channels`, `out_frames`
    /// frames). Returns the number of whole source frames actually
    /// consumed (see `resample_linear_stereo`); for passthrough this is
    /// always `min(src_frames, out_frames)`.
    ///
    /// Real-time safe: no allocation (`resampled` is preallocated).
    pub fn process(
        &mut self,
        src: &[f32],
        src_frames: usize,
        out: &mut [f32],
        out_frames: usize,
    ) -> usize {
        let channels = usize::from(self.device_channels);

        if self.is_passthrough() {
            let frames = src_frames.min(out_frames);
            map_channels(
                &src[..frames * 2],
                frames,
                self.device_channels,
                &mut out[..frames * channels],
            );
            for sample in &mut out[frames * channels..out_frames * channels] {
                *sample = 0.0;
            }
            return frames;
        }

        let out_frames = out_frames.min(MAX_FRAMES);
        let consumed = resample_linear_stereo(
            src,
            src_frames,
            &mut self.phase,
            self.step,
            &mut self.resampled[..out_frames * 2],
            out_frames,
        );
        map_channels(
            &self.resampled[..out_frames * 2],
            out_frames,
            self.device_channels,
            &mut out[..out_frames * channels],
        );
        consumed
    }
}

/// Map interleaved `stereo` (`frames` frames) into interleaved `out` with
/// `channels` channels (FR-023): 1 -> `(L+R)/2`; 2 -> passthrough; N>2 ->
/// L/R on channels 0-1, silence elsewhere.
fn map_channels(stereo: &[f32], frames: usize, channels: u16, out: &mut [f32]) {
    match channels {
        1 => {
            for i in 0..frames {
                out[i] = (stereo[i * 2] + stereo[i * 2 + 1]) * 0.5;
            }
        }
        2 => {
            out[..frames * 2].copy_from_slice(&stereo[..frames * 2]);
        }
        n => {
            let n = usize::from(n);
            for i in 0..frames {
                let base = i * n;
                out[base] = stereo[i * 2];
                out[base + 1] = stereo[i * 2 + 1];
                for sample in &mut out[base + 2..base + n] {
                    *sample = 0.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_stereo_copies_through() {
        let mut stage = OutputStage::new(44_100, 44_100, 2);
        assert!(stage.is_passthrough());
        let src = [0.1f32, 0.2, 0.3, 0.4];
        let mut out = [0.0f32; 4];
        let consumed = stage.process(&src, 2, &mut out, 2);
        assert_eq!(consumed, 2);
        assert_eq!(out, src);
    }

    #[test]
    fn passthrough_downmixes_to_mono() {
        let mut stage = OutputStage::new(44_100, 44_100, 1);
        let src = [1.0f32, 0.5];
        let mut out = [0.0f32; 1];
        stage.process(&src, 1, &mut out, 1);
        assert!((out[0] - 0.75).abs() < 1e-6);
    }

    #[test]
    fn passthrough_zero_fills_extra_channels() {
        let mut stage = OutputStage::new(44_100, 44_100, 4);
        let src = [0.2f32, 0.4];
        let mut out = [1.0f32; 4];
        stage.process(&src, 1, &mut out, 1);
        assert_eq!(out, [0.2, 0.4, 0.0, 0.0]);
    }

    #[test]
    fn resampling_consumes_bounded_source_frames() {
        let mut stage = OutputStage::new(48_000, 44_100, 2);
        assert!(!stage.is_passthrough());
        let needed = stage.required_source_frames(256);
        assert!((256..=MAX_FRAMES).contains(&needed));
        let src = vec![0.0f32; needed * 2];
        let mut out = vec![0.0f32; 256 * 2];
        let consumed = stage.process(&src, needed, &mut out, 256);
        assert!(consumed <= needed);
    }
}
