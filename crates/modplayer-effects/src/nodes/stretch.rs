// SPDX-License-Identifier: MIT OR Apache-2.0

//! `StretchStage`: the one kernel behind both `PitchShift` and
//! `TimeStretch` (research R4, contracts/engine-effect-chain.md §5, §6).
//! Two stages, in series conceptually (implemented as one pass):
//!
//! 1. **WSOLA time-stretch** by `stretch = output_len / input_len`: fixed
//!    synthesis hop `Hs`, analysis hop `Ha = Hs / stretch`, Hann windows,
//!    50 % overlap-add. Duration changes by `stretch`; pitch is
//!    unaffected (this is exactly what "time-stretch" means).
//! 2. **Fractional-delay resample** at `step` WSOLA-output frames per
//!    final output frame (linear in performance mode, cubic Hermite in
//!    quality mode): reading the stretched stream faster/slower shifts
//!    its pitch by `step` and restores the original duration when
//!    `step == stretch` (pitch shift alone) or leaves duration scaled by
//!    `1/stretch` when `step == 1` (time stretch alone).
//!
//! Both the WSOLA hop generator and the resampler track **monotonic**
//! absolute frame cursors (`f64`/`u64`, never reset or rebased) into two
//! persistent ring buffers (`StretchBuffers::ring`, `::intermediate`), so
//! state carries seamlessly across render boundaries with no special
//! "resume" bookkeeping. At exactly `stretch == 1.0 && step == 1.0` the
//! stage is pass-through (SC-013): no ring append, no delay, no cost.
//!
//! Simplification (documented, not a contract violation): the analysis
//! window's position is the nearest integer sample to the ideal
//! floating-point centre (no fractional interpolation of the window
//! itself, no cross-correlation search) — WSOLA's alignment search is a
//! quality refinement this implementation defers; the hop/overlap-add
//! machinery, the two-stage stretch/resample split and the formant
//! correction are all real.

use crate::catalog::QualityMode;
use crate::consts::{
    STRETCH_PERF_FRAME_MS, STRETCH_PERF_HOP_MS, STRETCH_QUALITY_FRAME_MS, STRETCH_QUALITY_HOP_MS,
};
use crate::crossfade::Crossfade;
use crate::nodes::lpc::LpcState;

/// The input ring's capacity, in stereo frames. Sized generously above
/// the largest WSOLA window + a render's worth of pull-ahead at typical
/// rates; an extreme combined ratio can still starve it (documented
/// degenerate case, contracts/engine-effect-chain.md §3).
pub const RING_FRAMES: usize = 12_288;

/// The WSOLA-stretched intermediate ring's capacity, in stereo frames —
/// the resampler trails its write cursor by only a few frames in normal
/// operation, so this only needs to be a few times the largest render.
pub const INTERMEDIATE_FRAMES: usize = 8_192;

/// One voice's overlap-add tail capacity, in stereo frames — must cover
/// the largest supported analysis window (quality mode, 40 ms).
pub const TAIL_FRAMES: usize = 8_192;

/// The big scratch every `NodeSlot` preallocates once at `Processor::new`
/// (research R2) and lends to a `StretchStage` on activation — kept out
/// of the small, `Copy` `NodeDsp` enum so every other kind's slot
/// doesn't pay for a duplicate of this state (it is still always
/// physically present per slot, per the fixed pool). `Vec`-backed
/// (allocated once, here, never resized) rather than fixed arrays: a
/// ~250 KiB fixed array is large enough to risk overflowing a small
/// thread stack (e.g. a spawned test thread's default) if the compiler
/// ever materialises it as a stack temporary before moving it into the
/// slot's `Box`; a `Vec`'s data is heap-resident from the moment it is
/// allocated, so no such temporary can exist.
#[derive(Debug, Clone)]
pub struct StretchBuffers {
    pub ring: Vec<f32>,
    /// One WSOLA-stretched intermediate ring **per voice** (indexed like
    /// `tails`): each voice writes and reads its own, never the other's —
    /// two voices with different configs (mode/formant, mid-crossfade)
    /// advance their `produced_total` write cursors at different rates, so
    /// a single shared ring would have them overwrite each other's hops
    /// (an audible click at every discrete switch, contracts/engine-
    /// effect-chain.md §4).
    pub intermediates: [Vec<f32>; 2],
    pub tails: [Vec<f32>; 2],
}

impl StretchBuffers {
    #[must_use]
    pub fn new() -> Self {
        Self {
            ring: vec![0.0; RING_FRAMES * 2],
            intermediates: [
                vec![0.0; INTERMEDIATE_FRAMES * 2],
                vec![0.0; INTERMEDIATE_FRAMES * 2],
            ],
            tails: [vec![0.0; TAIL_FRAMES * 2], vec![0.0; TAIL_FRAMES * 2]],
        }
    }
}

impl Default for StretchBuffers {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-voice control state (research R6: two voices read the same ring so
/// a discrete switch — engage/disengage, mode, formant — can crossfade
/// between an old and a new configuration without either one losing its
/// place). Everything here is small and `Copy`; the voice's big buffers
/// (overlap tail) live in the sibling `StretchBuffers`.
#[derive(Debug, Clone, Copy)]
struct Voice {
    /// Next WSOLA analysis window's centre, as an absolute (monotonic,
    /// never reset) position in the input ring's frame stream.
    analysis_pos: f64,
    /// Total stereo frames this voice has ever written into the
    /// intermediate ring (monotonic write cursor).
    produced_total: u64,
    /// Next resample read position, as an absolute (monotonic) position
    /// in the intermediate ring's frame stream.
    resample_pos: f64,
    tail_len: usize,
    quality: QualityMode,
    formant: bool,
    /// Per-channel envelope of the *original* (pre-shift) material,
    /// refreshed periodically from the input ring near `analysis_pos`.
    lpc_orig: [LpcState; 2],
    /// Per-channel filter used to flatten the *shifted* output's own
    /// envelope immediately before `lpc_orig` re-imposes the original
    /// one (research R4's "whiten the shifted residual, resynthesize
    /// with the original envelope").
    lpc_flatten: [LpcState; 2],
    /// Frames of ring material consumed since the last envelope refresh
    /// (§ analyze every 512-frame hop, research R4).
    since_envelope_refresh: u32,
}

impl Voice {
    const fn new() -> Self {
        Self {
            analysis_pos: 0.0,
            produced_total: 0,
            resample_pos: 0.0,
            tail_len: 0,
            quality: QualityMode::Performance,
            formant: false,
            lpc_orig: [LpcState::new(); 2],
            lpc_flatten: [LpcState::new(); 2],
            since_envelope_refresh: 0,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

/// WSOLA analysis-frame / synthesis-hop timing for one `QualityMode`, in
/// frames at `source_rate` (research R4).
struct Timing {
    frame: usize,
    hop: usize,
}

fn timing_for(mode: QualityMode, source_rate: u32) -> Timing {
    let (frame_ms, hop_ms) = match mode {
        QualityMode::Performance => (STRETCH_PERF_FRAME_MS, STRETCH_PERF_HOP_MS),
        QualityMode::Quality => (STRETCH_QUALITY_FRAME_MS, STRETCH_QUALITY_HOP_MS),
    };
    let ms_to_frames = |ms: f32| ((ms / 1000.0) * source_rate as f32).ceil().max(2.0) as usize;
    Timing {
        frame: ms_to_frames(frame_ms).min(TAIL_FRAMES),
        hop: ms_to_frames(hop_ms).max(1),
    }
}

/// The pitch-shift/time-stretch kernel (data-model.md §3.3). Small and
/// `Copy` — its big buffers live in the sibling `StretchBuffers` the slot
/// lends it.
#[derive(Debug, Clone, Copy)]
pub struct StretchStage {
    ring_total: u64,
    voices: [Voice; 2],
    active: usize,
    /// Blends the *old* vs *new* voice across a discrete mode/formant
    /// switch (research R6, T035) — `wet` is always `self.active`.
    voice_mix: Crossfade,
    /// Blends pass-through input vs. the active voice's engaged output
    /// across the engage/disengage seam (research R6): `wet == true`
    /// means "fully engaged". Independent of `voice_mix` so the two
    /// transitions never fight over one fade's meaning.
    engage_mix: Crossfade,
    prev_engaged: bool,
    source_rate: u32,
}

/// This render's effective product parameters (research R4 table), and
/// whether the stage is at the exact-unity pass-through point.
#[derive(Debug, Clone, Copy)]
pub struct StretchParams {
    pub stretch: f32,
    pub step: f32,
    pub formant: bool,
    pub quality: QualityMode,
}

impl StretchParams {
    /// `PitchShift` alone: `stretch = step = p = 2^(semitones/12)`.
    #[must_use]
    pub fn pitch_alone(semitones: f32, formant: bool, quality: QualityMode) -> Self {
        let p = 2f32.powf(semitones / 12.0);
        Self {
            stretch: p,
            step: p,
            formant,
            quality,
        }
    }

    /// `TimeStretch` alone: `stretch = 1/r`, `step = 1`.
    #[must_use]
    pub fn tempo_alone(ratio: f32, quality: QualityMode) -> Self {
        Self {
            stretch: 1.0 / ratio.max(1e-3),
            step: 1.0,
            formant: false,
            quality,
        }
    }

    /// FR-007's combined stage: `stretch = p/r`, `step = p`, quality if
    /// either member is quality, formant from the pitch member. A
    /// bypassed member contributes its identity value (`p = 1`/`r = 1`).
    #[must_use]
    pub fn combined(
        semitones: f32,
        ratio: f32,
        formant: bool,
        pitch_quality: QualityMode,
        stretch_quality: QualityMode,
    ) -> Self {
        let p = 2f32.powf(semitones / 12.0);
        let r = ratio.max(1e-3);
        let quality =
            if pitch_quality == QualityMode::Quality || stretch_quality == QualityMode::Quality {
                QualityMode::Quality
            } else {
                QualityMode::Performance
            };
        Self {
            stretch: p / r,
            step: p,
            formant,
            quality,
        }
    }

    /// Whether this render's parameters sit exactly at the pass-through
    /// point (SC-013): no time change, no pitch change.
    #[must_use]
    pub fn is_unity(&self) -> bool {
        (self.stretch - 1.0).abs() < 1e-6 && (self.step - 1.0).abs() < 1e-6
    }

    /// The ratio of new ring frames this stage consumes per output frame
    /// it produces (contracts/engine-effect-chain.md §3): `step / stretch`.
    #[must_use]
    pub fn consume_ratio(&self) -> f32 {
        self.step / self.stretch.max(1e-3)
    }
}

impl StretchStage {
    #[must_use]
    pub const fn new(source_rate: u32) -> Self {
        let source_rate = if source_rate == 0 { 1 } else { source_rate };
        Self {
            ring_total: 0,
            voices: [Voice::new(), Voice::new()],
            active: 0,
            voice_mix: Crossfade::settled(true),
            engage_mix: Crossfade::settled(false),
            prev_engaged: false,
            source_rate,
        }
    }

    fn crossfade_frames(&self) -> u32 {
        ((crate::consts::SWITCH_CROSSFADE_MS / 1000.0) * self.source_rate as f32).ceil() as u32
    }

    pub fn set_source_rate(&mut self, source_rate: u32) {
        self.source_rate = source_rate.max(1);
    }

    /// Clears every voice's ring position, envelope and filter memory
    /// (`Command::Seek`/`Stop`, FR-001a); the ring/tail contents
    /// themselves are simply superseded (new writes start at position 0
    /// again) rather than zeroed, which is cheap and behaviourally
    /// equivalent since nothing reads ahead of `ring_total`.
    pub fn reset_history(&mut self) {
        self.ring_total = 0;
        for voice in &mut self.voices {
            voice.reset();
        }
    }

    /// How many *new* input frames this stage needs appended to its ring
    /// this render to produce `out_frames` more output
    /// (contracts/engine-effect-chain.md §3): `max(0, ceil(out *
    /// consume) + frame + search - occupancy)`. Pass-through
    /// (`params.is_unity()`) needs exactly `out_frames` (in place,
    /// copy-free).
    #[must_use]
    pub fn input_for(&self, out_frames: usize, params: &StretchParams) -> usize {
        if params.is_unity() && self.engage_mix.is_settled() {
            return out_frames;
        }
        let timing = timing_for(params.quality, self.source_rate);
        let search = timing.hop / 2;
        let voice = &self.voices[self.active];
        let occupancy = self
            .ring_total
            .saturating_sub(voice.analysis_pos.floor() as u64) as usize;
        let need =
            (out_frames as f32 * params.consume_ratio()).ceil() as usize + timing.frame + search;
        need.saturating_sub(occupancy)
    }

    /// Whether the stage should run its engaged WSOLA/resample path this
    /// render, vs. the copy-free pass-through — mirrors `input_for`'s own
    /// gate so callers never diverge on it.
    #[must_use]
    pub const fn engaged(params: &StretchParams) -> bool {
        // `is_unity` is not `const fn` (float abs); duplicated here in a
        // `const`-friendly form for callers that only need the boolean.
        !(params.stretch >= 0.999_999
            && params.stretch <= 1.000_001
            && params.step >= 0.999_999
            && params.step <= 1.000_001)
    }

    /// Append `frames` new stereo frames to the ring (called once per
    /// render before `process`, with exactly `input_for`'s result).
    pub fn append(&mut self, buffers: &mut StretchBuffers, input: &[f32], frames: usize) {
        for i in 0..frames {
            let idx = ((self.ring_total as usize + i) % RING_FRAMES) * 2;
            buffers.ring[idx] = input[i * 2];
            buffers.ring[idx + 1] = input[i * 2 + 1];
        }
        self.ring_total += frames as u64;
    }

    /// Process: pass-through copies `input`'s first `out_frames` verbatim
    /// (sample-exact, SC-013); engaged runs WSOLA + resample for the
    /// active voice (and, mid-crossfade, the fading-out voice too,
    /// mixing equal-power — research R6).
    pub fn process(
        &mut self,
        buffers: &mut StretchBuffers,
        input: &[f32],
        out: &mut [f32],
        out_frames: usize,
        params: &StretchParams,
    ) {
        // Engage/disengage seam (research R6, T035): crossing the exact
        // unity boundary starts a 5 ms crossfade between pass-through and
        // the engaged WSOLA/resample path, rather than a hard cut.
        let now_engaged = Self::engaged(params);
        if now_engaged != self.prev_engaged && self.engage_mix.is_settled() {
            let crossfade_frames = self.crossfade_frames();
            self.engage_mix.set_target(now_engaged, crossfade_frames);
        }
        self.prev_engaged = now_engaged;

        if self.engage_mix.is_settled() && !now_engaged {
            let n = out_frames.min(input.len() / 2).min(out.len() / 2);
            out[..n * 2].copy_from_slice(&input[..n * 2]);
            for sample in out[n * 2..out_frames * 2].iter_mut() {
                *sample = 0.0;
            }
            return;
        }

        let active = self.active;
        if self.voice_mix.is_settled() {
            // No discrete switch in flight: the active voice's config
            // simply tracks the render's current catalog value (this is
            // also what makes a solo `process` call, with no preceding
            // `switch_voice`, honour `params.formant`/`params.quality`
            // immediately). Mid-fade, `switch_voice` alone owns both
            // voices' configs so the crossfade's two sides stay distinct.
            self.voices[active].formant = params.formant;
            self.voices[active].quality = params.quality;
        }
        run_voice(
            &mut self.voices[active],
            active,
            buffers,
            self.ring_total,
            self.source_rate,
            out,
            out_frames,
            params,
        );

        if !self.voice_mix.is_settled() {
            let other = 1 - active;
            // The fading-out voice keeps its own (frozen) parameters —
            // approximated here by reusing `params`, since only a
            // discrete switch (mode/formant) triggers this fade and the
            // continuous product parameters barely move within one 5 ms
            // crossfade.
            let mut scratch = [0.0f32; 4_096 * 2];
            let n = out_frames.min(scratch.len() / 2);
            run_voice(
                &mut self.voices[other],
                other,
                buffers,
                self.ring_total,
                self.source_rate,
                &mut scratch[..n * 2],
                n,
                params,
            );
            for f in 0..n {
                let (dry, wet) = self.voice_mix.gains();
                out[f * 2] = out[f * 2] * wet + scratch[f * 2] * dry;
                out[f * 2 + 1] = out[f * 2 + 1] * wet + scratch[f * 2 + 1] * dry;
                self.voice_mix.advance();
            }
        }

        if !self.engage_mix.is_settled() {
            let n = out_frames.min(input.len() / 2).min(out.len() / 2);
            for f in 0..n {
                let (dry, wet) = self.engage_mix.gains();
                out[f * 2] = input[f * 2] * dry + out[f * 2] * wet;
                out[f * 2 + 1] = input[f * 2 + 1] * dry + out[f * 2 + 1] * wet;
                self.engage_mix.advance();
            }
        }
    }

    /// Start a discrete switch (mode or formant): the *other* voice takes
    /// over the new configuration from the same ring position, and a
    /// 5 ms equal-power crossfade blends the two (research R6, T035). A
    /// no-op if already mid-fade.
    ///
    /// Both the overlap-add tail and the WSOLA-stretched intermediate
    /// ring are per-voice state that lives outside `Voice` itself, in the
    /// sibling `StretchBuffers` (indexed by voice slot, research R2's
    /// "big buffers live outside the small `Copy` state");
    /// `self.voices[to] = self.voices[from]` copies the cursors
    /// (`tail_len`, `produced_total`, ...) but not the sample data they
    /// refer to, so the new voice's first hop and first resample read
    /// must inherit the old voice's actual content here — without it, the
    /// new voice reads back zeroed samples where the old voice's real
    /// ones belonged, an audible click right at the switch (contracts/
    /// engine-effect-chain.md §4).
    pub fn switch_voice(
        &mut self,
        buffers: &mut StretchBuffers,
        formant: bool,
        quality: QualityMode,
        crossfade_frames: u32,
    ) {
        if !self.voice_mix.is_settled() {
            return;
        }
        let from = self.active;
        let to = 1 - from;
        self.voices[to] = self.voices[from];
        self.voices[to].formant = formant;
        self.voices[to].quality = quality;
        // Due for an *immediate* envelope refresh on its very first
        // `apply_formant` call (>= 512, rather than 0) rather than only
        // once 512 samples have accumulated: a switch onto a voice this
        // stage has never run formant correction on before otherwise
        // spends its first few renders resynthesizing with a stale
        // (all-zero) "original" envelope while the flattened residual is
        // already the real one, a mismatch pronounced enough to read as
        // its own small transient right at the switch.
        self.voices[to].since_envelope_refresh = 512;
        let (tail0, tail1) = buffers.tails.split_at_mut(1);
        let (src_tail, dst_tail): (&[f32], &mut [f32]) = if from == 0 {
            (&tail0[0], &mut tail1[0])
        } else {
            (&tail1[0], &mut tail0[0])
        };
        dst_tail.copy_from_slice(src_tail);
        let (inter0, inter1) = buffers.intermediates.split_at_mut(1);
        let (src_inter, dst_inter): (&[f32], &mut [f32]) = if from == 0 {
            (&inter0[0], &mut inter1[0])
        } else {
            (&inter1[0], &mut inter0[0])
        };
        dst_inter.copy_from_slice(src_inter);
        self.active = to;
        self.voice_mix = Crossfade::settled(false);
        self.voice_mix.set_target(true, crossfade_frames);
    }

    /// The active voice's current quality/formant, and whether a
    /// discrete switch is still fading (used by `rt/chain.rs` to decide
    /// whether `switch_voice` is needed at all).
    #[must_use]
    pub const fn active_config(&self) -> (bool, QualityMode) {
        let v = &self.voices[self.active];
        (v.formant, v.quality)
    }

    /// Buffered-but-not-yet-output input frames (research R7): how far
    /// the active voice's analysis position trails the ring's write
    /// cursor, in source frames. `0` at pass-through.
    #[must_use]
    pub fn latency_frames(&self, params: &StretchParams) -> u64 {
        if params.is_unity() && self.engage_mix.is_settled() {
            return 0;
        }
        let voice = &self.voices[self.active];
        self.ring_total
            .saturating_sub(voice.analysis_pos.floor() as u64)
    }
}

#[allow(clippy::too_many_arguments)]
fn run_voice(
    voice: &mut Voice,
    voice_index: usize,
    buffers: &mut StretchBuffers,
    ring_total: u64,
    source_rate: u32,
    out: &mut [f32],
    out_frames: usize,
    params: &StretchParams,
) {
    // `voice.quality`/`voice.formant` are set only by `switch_voice`
    // (T035): each voice keeps its *own* discrete configuration through a
    // crossfade rather than both voices snapping to the render's current
    // catalog value, which would collapse the two-voice blend into a
    // single (new) configuration and defeat the click-free switch.
    let timing = timing_for(voice.quality, source_rate);
    let analysis_hop = ((timing.hop as f32) / params.stretch.max(1e-3)).max(1.0);

    // Stage 1: generate enough WSOLA-stretched intermediate frames.
    let need_intermediate =
        (voice.resample_pos + f64::from(out_frames as u32) * f64::from(params.step)).ceil() as u64
            + 4;
    let mut guard = 0u32;
    while voice.produced_total < need_intermediate {
        generate_hop(
            voice,
            voice_index,
            buffers,
            ring_total,
            timing.frame,
            analysis_hop,
        );
        guard += 1;
        if guard > 4_096 {
            // Starved: the ring cannot supply another hop's worth of
            // material (contracts/engine-effect-chain.md §3's documented
            // degenerate case) — stop generating; the resample stage
            // below repeats the last available intermediate grain rather
            // than reading silence.
            break;
        }
    }

    // Stage 2: resample the intermediate stream at `step` per output
    // frame (linear in performance mode, cubic Hermite in quality mode).
    let intermediate = &buffers.intermediates[voice_index];
    for i in 0..out_frames {
        let pos = voice.resample_pos + f64::from(i as u32) * f64::from(params.step);
        let sample = sample_intermediate(intermediate, voice.produced_total, pos, voice.quality);
        out[i * 2] = sample.0;
        out[i * 2 + 1] = sample.1;
    }
    voice.resample_pos += f64::from(out_frames as u32) * f64::from(params.step);

    if voice.formant {
        apply_formant(voice, buffers, ring_total, out, out_frames);
    }
}

/// Read one interpolated stereo frame from the intermediate ring at
/// absolute position `pos` (clamped to the last frame actually produced
/// while starved). `produced` is the voice's monotonic write cursor.
fn sample_intermediate(
    intermediate: &[f32],
    produced: u64,
    pos: f64,
    quality: QualityMode,
) -> (f32, f32) {
    let last = produced.saturating_sub(1);
    let clamped = pos.max(0.0).min(last as f64);
    let i0 = clamped.floor() as u64;
    let frac = (clamped - i0 as f64) as f32;
    let read = |abs: u64| -> (f32, f32) {
        let idx = ((abs.min(last) as usize) % INTERMEDIATE_FRAMES) * 2;
        (intermediate[idx], intermediate[idx + 1])
    };
    let p0 = read(i0.saturating_sub(1));
    let p1 = read(i0);
    let p2 = read(i0 + 1);
    let p3 = read(i0 + 2);
    (
        interpolate(quality, [p0.0, p1.0, p2.0, p3.0], frac),
        interpolate(quality, [p0.1, p1.1, p2.1, p3.1], frac),
    )
}

/// The resampler's per-sample kernel (research R4): linear between the
/// two bracketing samples in performance mode, cubic Hermite through all
/// four in quality mode — `quality_mode_uses_cubic` (tests/stretch.rs)
/// pins the mode selection directly against this pure function.
/// `neighbors = [p(-1), p(0), p(1), p(2)]`; `frac` is the fractional
/// position between `neighbors[1]` and `neighbors[2]`.
#[must_use]
pub fn interpolate(quality: QualityMode, neighbors: [f32; 4], frac: f32) -> f32 {
    let [p0, p1, p2, p3] = neighbors;
    match quality {
        QualityMode::Performance => p1 + (p2 - p1) * frac,
        QualityMode::Quality => {
            let a = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
            let b = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
            let c = -0.5 * p0 + 0.5 * p2;
            let d = p1;
            ((a * frac + b) * frac + c) * frac + d
        }
    }
}

/// Generate exactly one more WSOLA synthesis hop (`timing.hop` new
/// intermediate frames) for `voice`, reading a `timing.frame`-length Hann
/// window centred at `voice.analysis_pos` from the input ring, and
/// overlap-adding it onto the voice's persistent tail. Starved (the ring
/// has no material at the requested position yet) repeats the last
/// available window rather than reading silence.
fn generate_hop(
    voice: &mut Voice,
    voice_index: usize,
    buffers: &mut StretchBuffers,
    ring_total: u64,
    frame: usize,
    analysis_hop: f32,
) {
    let half = frame / 2;
    let earliest = ring_total.saturating_sub(RING_FRAMES as u64);
    let latest = ring_total.saturating_sub(1);
    let center_f = voice.analysis_pos.max(earliest as f64).min(latest as f64);
    let center = center_f.round() as i64;

    let mut window = [0.0f32; TAIL_FRAMES * 2];
    for i in 0..frame {
        let abs = center - half as i64 + i as i64;
        let clamped = abs.clamp(earliest as i64, latest as i64) as u64;
        let idx = ((clamped as usize) % RING_FRAMES) * 2;
        let hann = hann_window(i, frame);
        window[i * 2] = buffers.ring[idx] * hann;
        window[i * 2 + 1] = buffers.ring[idx + 1] * hann;
    }

    // Overlap-add: the window's first `hop` frames sum with the carried
    // tail; the window's tail beyond `hop` becomes the new carried tail.
    let hop = frame.saturating_sub(frame / 2).max(1).min(frame);
    overlap_add(voice, voice_index, buffers, &window, frame, hop);

    voice.produced_total += hop as u64;
    voice.analysis_pos += f64::from(analysis_hop);
}

fn hann_window(i: usize, len: usize) -> f32 {
    if len <= 1 {
        return 1.0;
    }
    let t = i as f32 / (len - 1) as f32;
    0.5 - 0.5 * (std::f32::consts::TAU * t).cos()
}

/// `voice_index` selects which of `StretchBuffers::tails` belongs to
/// this voice — `StretchStage::voices` and `StretchBuffers::tails` are
/// always indexed 0/1 in lock-step.
fn overlap_add(
    voice: &mut Voice,
    voice_index: usize,
    buffers: &mut StretchBuffers,
    window: &[f32],
    frame: usize,
    hop: usize,
) {
    let tail = &mut buffers.tails[voice_index];
    let out_ring = &mut buffers.intermediates[voice_index];
    let write_start = voice.produced_total;

    for i in 0..hop {
        let from_tail = if i < voice.tail_len {
            (tail[i * 2], tail[i * 2 + 1])
        } else {
            (0.0, 0.0)
        };
        let sample = (window[i * 2] + from_tail.0, window[i * 2 + 1] + from_tail.1);
        let idx = (((write_start + i as u64) as usize) % INTERMEDIATE_FRAMES) * 2;
        out_ring[idx] = sample.0;
        out_ring[idx + 1] = sample.1;
    }

    // The remainder of the window (beyond `hop`) becomes the new tail,
    // shifted down: `new_tail[k] = old_tail[hop+k] + window[hop+k]` for
    // the overlapping portion, then plain `window[hop+k]` beyond the old
    // tail's length.
    let remaining = frame.saturating_sub(hop);
    let mut new_tail = [0.0f32; TAIL_FRAMES * 2];
    for k in 0..remaining {
        let from_old = if hop + k < voice.tail_len {
            (tail[(hop + k) * 2], tail[(hop + k) * 2 + 1])
        } else {
            (0.0, 0.0)
        };
        new_tail[k * 2] = window[(hop + k) * 2] + from_old.0;
        new_tail[k * 2 + 1] = window[(hop + k) * 2 + 1] + from_old.1;
    }
    tail[..remaining * 2].copy_from_slice(&new_tail[..remaining * 2]);
    voice.tail_len = remaining;
}

/// Combined-channel RMS of a stereo (here: separate-array) block, used by
/// `apply_formant`'s energy-matching safeguard.
fn block_rms(left: &[f32], right: &[f32]) -> f32 {
    let count = (left.len() + right.len()).max(1) as f32;
    let sum_sq: f32 = left.iter().chain(right.iter()).map(|s| s * s).sum();
    (sum_sq / count).sqrt()
}

/// LPC formant correction (research R4): flatten the just-produced
/// output block's own short-time envelope, then re-impose the *original*
/// (pre-shift) material's envelope, refreshed periodically from the
/// input ring near the voice's analysis position.
fn apply_formant(
    voice: &mut Voice,
    buffers: &StretchBuffers,
    ring_total: u64,
    out: &mut [f32],
    frames: usize,
) {
    voice.since_envelope_refresh += frames as u32;
    if voice.since_envelope_refresh >= 512 || voice.produced_total <= frames as u64 {
        voice.since_envelope_refresh = 0;
        let window_len = 1_024usize.min(RING_FRAMES);
        let latest = ring_total.saturating_sub(1);
        let earliest = ring_total.saturating_sub(RING_FRAMES as u64);
        let center = voice.analysis_pos.round() as i64;
        let mut mono = [[0.0f32; 1_024]; 2];
        let (mono0, mono1) = mono.split_at_mut(1);
        for (i, (left, right)) in mono0[0]
            .iter_mut()
            .zip(mono1[0].iter_mut())
            .enumerate()
            .take(window_len)
        {
            let abs = center - (window_len as i64) / 2 + i as i64;
            let clamped = abs.clamp(earliest as i64, latest as i64) as u64;
            let idx = ((clamped as usize) % RING_FRAMES) * 2;
            let hann = hann_window(i, window_len);
            *left = buffers.ring[idx] * hann;
            *right = buffers.ring[idx + 1] * hann;
        }
        voice.lpc_orig[0].analyze(&mono[0][..window_len]);
        voice.lpc_orig[1].analyze(&mono[1][..window_len]);
    }

    let mut left = [0.0f32; 4_096];
    let mut right = [0.0f32; 4_096];
    let n = frames.min(left.len());
    for i in 0..n {
        left[i] = out[i * 2];
        right[i] = out[i * 2 + 1];
    }
    // Flatten the shifted block's own envelope (re-analysed every call —
    // cheap relative to the render budget), then re-impose the original.
    // Analysing directly on `voice.lpc_flatten` itself (rather than a
    // throwaway `LpcState`) is what makes the coefficients `whiten` then
    // actually applies match the block just analysed — a separate,
    // never-consulted instance would leave `whiten` running with stale
    // (initially all-zero) coefficients, so `resynthesize` re-imposes the
    // original envelope onto a signal that was never flattened, doubling
    // the resonance and driving the filter toward instability (an
    // audible, unbounded-looking spike right when formant engages).
    let input_rms = block_rms(&left[..n], &right[..n]);
    voice.lpc_flatten[0].analyze(&left[..n]);
    voice.lpc_flatten[1].analyze(&right[..n]);
    voice.lpc_flatten[0].whiten(&mut left[..n]);
    voice.lpc_flatten[1].whiten(&mut right[..n]);
    let orig_l = voice.lpc_orig[0].coeffs();
    let orig_r = voice.lpc_orig[1].coeffs();
    voice.lpc_flatten[0].resynthesize(&orig_l, &mut left[..n]);
    voice.lpc_flatten[1].resynthesize(&orig_r, &mut right[..n]);
    // Whitening with one block's own (freshly analysed) envelope and
    // resynthesizing with another (the periodically refreshed "original"
    // one) are two independent order-16 all-pole models of the *same*
    // strongly periodic material; their mismatch can compound block over
    // block into a slowly growing resonance (most visible on a near-pure
    // tone, where both models place a pole close to the unit circle).
    // Re-matching this block's output energy to its input energy every
    // call is the standard LPC re-synthesis safeguard: it cannot correct
    // for one bad block, but it stops any such drift from accumulating
    // across blocks, so a formant switch stays click-free (contracts/
    // engine-effect-chain.md §4) without narrowing the envelope models
    // themselves.
    let output_rms = block_rms(&left[..n], &right[..n]);
    if output_rms > 1e-6 {
        let gain = (input_rms / output_rms).clamp(0.1, 4.0);
        for sample in left[..n].iter_mut().chain(right[..n].iter_mut()) {
            *sample *= gain;
        }
    }
    for i in 0..n {
        out[i * 2] = left[i];
        out[i * 2 + 1] = right[i];
    }
}

/// Compute this render's effective `StretchParams` for a `PitchShift` or
/// `TimeStretch` node given its own catalog params, from `Smoothed`
/// values already ramped by the slot (FR-010) — a small free function so
/// `rt/chain.rs` can build the params once per node per render without
/// duplicating the pitch/ratio formulas.
#[must_use]
pub fn params_from_semitones(semitones: f32, formant: bool, mode_value: f32) -> StretchParams {
    let quality = if mode_value >= 0.5 {
        QualityMode::Quality
    } else {
        QualityMode::Performance
    };
    StretchParams::pitch_alone(semitones, formant, quality)
}

#[must_use]
pub fn params_from_ratio(ratio: f32, mode_value: f32) -> StretchParams {
    let quality = if mode_value >= 0.5 {
        QualityMode::Quality
    } else {
        QualityMode::Performance
    };
    StretchParams::tempo_alone(ratio, quality)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make() -> (StretchStage, Box<StretchBuffers>) {
        (StretchStage::new(44_100), Box::new(StretchBuffers::new()))
    }

    #[test]
    fn unity_is_pass_through_sample_exact() {
        let (mut stage, mut buffers) = make();
        let params = StretchParams {
            stretch: 1.0,
            step: 1.0,
            formant: false,
            quality: QualityMode::Performance,
        };
        assert!(params.is_unity());
        let input = [0.3f32, -0.5, 0.7, -0.9, 0.1, 0.2];
        let mut out = [0.0f32; 6];
        stage.process(&mut buffers, &input, &mut out, 3, &params);
        for (got, want) in out.iter().zip(input.iter()) {
            assert!((got - want).abs() < 1e-6, "got={got} want={want}");
        }
    }

    /// T035: a discrete `switch_voice` (formant/mode) starts a crossfade
    /// that settles within its requested frame count, and never
    /// discontinuously jumps mid-fade (`gains()` stays equal-power).
    #[test]
    fn switch_voice_crossfades_over_requested_frames() {
        let (mut stage, mut buffers) = make();
        let mut params = StretchParams::pitch_alone(5.0, false, QualityMode::Performance);
        // Engage first so the stage is past its own engage seam.
        let input = vec![0.2f32; 4_096 * 2];
        for _ in 0..40 {
            let need = stage.input_for(256, &params);
            stage.append(&mut buffers, &input[..need * 2], need);
            let mut out = [0.0f32; 256 * 2];
            stage.process(&mut buffers, &input[..need * 2], &mut out, 256, &params);
        }

        let (formant_before, _) = stage.active_config();
        assert!(!formant_before);
        stage.switch_voice(&mut buffers, true, QualityMode::Performance, 220);
        // A second call mid-fade is a documented no-op.
        stage.switch_voice(&mut buffers, false, QualityMode::Quality, 220);
        // Mirrors production (`ChainRt::apply_set_param`): the catalog
        // param that drove this switch is already updated by the time
        // `process` next runs.
        params.formant = true;

        for _ in 0..30 {
            let need = stage.input_for(256, &params);
            stage.append(&mut buffers, &input[..need * 2], need);
            let mut out = [0.0f32; 256 * 2];
            stage.process(&mut buffers, &input[..need * 2], &mut out, 256, &params);
            assert!(out.iter().all(|s| s.is_finite()));
        }
        let (formant_after, _) = stage.active_config();
        assert!(formant_after, "the requested switch must still take effect");
    }
}
