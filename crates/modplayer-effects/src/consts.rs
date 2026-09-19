// SPDX-License-Identifier: MIT OR Apache-2.0

//! Constants shared by the catalog, the DSP kernels and `ChainRt`
//! (data-model.md §1.5).

/// The chain's capacity (FR-002: at least 16), and research R2's
/// preallocated slot-pool size.
pub const MAX_NODES: usize = 16;

/// Continuous parameters ramp linearly over this many milliseconds,
/// restarting from the current value on every retarget (FR-010).
pub const PARAM_RAMP_MS: f32 = 20.0;

/// Every add/remove/bypass/reorder-phase/discrete-switch transition
/// crossfades over this many milliseconds, equal power (FR-005).
pub const SWITCH_CROSSFADE_MS: f32 = 5.0;

/// The number of log-spaced bands the post-chain spectrum folds into
/// (FR-011).
pub const SPECTRUM_BANDS: usize = 64;

/// The spectrum's FFT size, in frames (research R9).
pub const SPECTRUM_FFT: usize = 1_024;

/// The spectrum recomputes every this many new post-chain frames.
pub const SPECTRUM_HOP: usize = 256;

/// The cost-percentage rolling window, in seconds (FR-012a).
pub const COST_WINDOW_SECS: f32 = 1.0;

/// `CostRing`'s fixed capacity, in callbacks (≈ 1 s of callbacks at a
/// typical buffer size) — never resized on the real-time path.
pub const COST_RING_CAPACITY: usize = 1_024;

/// An overload event fires at `render_pct >= 100` or `render_pct >= 90`
/// on this many consecutive callbacks (FR-012).
pub const OVERLOAD_PCT: f32 = 90.0;
pub const OVERLOAD_STREAK: u32 = 3;

/// `tempo_step`'s ratio increment per key press (FR-017).
pub const TEMPO_STEP: f32 = 0.10;

/// The Butterworth Q used at `resonance == 0` (`1/sqrt(2)`), and the cap
/// applied at `resonance == 1` (research R12): `Q = Q_BUTTER + res *
/// (Q_MAX - Q_BUTTER)`.
pub const Q_BUTTER: f32 = std::f32::consts::FRAC_1_SQRT_2;
pub const Q_MAX: f32 = 8.0;

/// WSOLA analysis-frame / hop / search-radius timing for the performance
/// preset (research R4).
pub const STRETCH_PERF_FRAME_MS: f32 = 20.0;
pub const STRETCH_PERF_HOP_MS: f32 = 10.0;
pub const STRETCH_PERF_SEARCH_MS: f32 = 5.0;

/// WSOLA analysis-frame / hop / search-radius timing for the quality
/// preset (research R4).
pub const STRETCH_QUALITY_FRAME_MS: f32 = 40.0;
pub const STRETCH_QUALITY_HOP_MS: f32 = 20.0;
pub const STRETCH_QUALITY_SEARCH_MS: f32 = 10.0;

/// The LPC formant model's filter order (research R4).
pub const LPC_ORDER: usize = 16;

/// The largest single render request the chain must ever service. Mirrors
/// `modplayer_engine::output_stage::MAX_FRAMES`; duplicated here rather
/// than depended on, since this crate has no runtime dependency on the
/// engine (research R1, plan.md "effects -> {}").
pub const MAX_RENDER_FRAMES: usize = 4_096;

/// The largest WSOLA analysis frame, in samples, at the highest supported
/// device/source rate (192 kHz, the quality preset's 40 ms frame,
/// rounded up).
pub const MAX_STRETCH_FRAME: usize = 7_680;

/// `ChainRt`'s two ping-pong buses' capacity, in frames (research R3):
/// enough headroom for the two-pass pull plan to buffer a full render's
/// worth of frames plus every engaged stretch stage's largest analysis
/// frame, so no render ever has to reallocate or truncate.
pub const CHAIN_BUS_FRAMES: usize = 4 * MAX_RENDER_FRAMES + 2 * MAX_STRETCH_FRAME;
