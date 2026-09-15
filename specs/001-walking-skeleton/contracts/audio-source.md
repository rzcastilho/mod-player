# Contract: `AudioSource` trait

**Crate**: `modplayer-audio-source` (trait only) | **Implementors this slice**: `modplayer-audio-source-synthetic::SyntheticSource`

This is the constitution's sanctioned single-abstraction seam (Principle IV). The engine is generic over it (`Processor<S: AudioSource>`) so the real-time path carries no trait object and no dynamic dispatch.

```rust
/// A producer of interleaved stereo f32 frames at its own fixed sample rate.
///
/// Real-time contract: `fill` and `seek` are called from the audio callback and
/// MUST NOT allocate, lock, block, log, or perform I/O.
pub trait AudioSource: Send + 'static {
    /// Sample rate of the frames produced by `fill`. Constant for the lifetime of the value.
    fn sample_rate(&self) -> u32;

    /// Total length in frames of the current material, if finite. The synthetic
    /// track is finite and loops; `None` means "unbounded / streaming".
    fn len_frames(&self) -> Option<u64>;

    /// Current read position in frames (0 ≤ pos < len when finite).
    fn position(&self) -> u64;

    /// Move the read position. Wraps modulo `len_frames` when finite.
    fn seek(&mut self, frame: u64);

    /// Write exactly `out.len() / 2` stereo frames into `out` (interleaved L,R),
    /// advancing `position` by that many frames (wrapping at `len_frames`).
    /// Must fill the whole slice; silence is written explicitly.
    fn fill(&mut self, out: &mut [f32]);
}
```

## `SyntheticSource` guarantees (FR-008)

| Item | Guarantee |
|---|---|
| Constructor | `SyntheticSource::new(sample_rate: u32)`; `Default` = 44 100 Hz |
| Determinism | Output is a pure function of `(sample_rate, position)`; two instances at the same position produce bit-identical frames |
| Track layout | 10 s 440 Hz sine @ −12 dBFS → 5 s 1 kHz square @ 0 dBFS → 5 s sawtooth sweep 100→2000 Hz @ −12 dBFS → 2 s silence; loops |
| Phase | Oscillator phase is continuous across `fill` calls and across the sine segment; no discontinuity inside segment 1 |
| Test tone | `TestTone::new(sample_rate)`; `render_add(out: &mut [f32]) -> bool` adds the tone into `out` (sum, not overwrite) and returns `true` while still playing; 440 Hz, 1.0 s, 10 ms linear fades, −20 dBFS peak |

## Tests that pin this contract (crate `modplayer-audio-source-synthetic`)

- `fill` of 22 s at 44 100 Hz produces segment peaks of exactly 0.2512 (−12 dBFS) ± 1e-4, 1.0, 0.2512, 0.0 in order.
- Two sources seeked to the same frame produce identical buffers (determinism).
- `TestTone` peak is 0.1 ± 1e-6 and its first/last 10 ms are monotone ramps.
- A 60 s continuous `fill` in 256-frame chunks has no sample-to-sample jump larger than the theoretical maximum for a 440 Hz sine at that rate (dropout / duplicate-frame detector, backs FR-025 and US2-5).
