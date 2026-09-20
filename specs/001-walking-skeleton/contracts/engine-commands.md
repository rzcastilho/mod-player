# Contract: Engine command/event queues and shared atomics

**Crate**: `modplayer-engine` | **Traces**: FR-006, FR-007, FR-009, FR-010, FR-011, FR-025(a–d)

The command queue is **the only write path** into a running processor. There is no `&mut Processor` reachable from outside the audio callback; every setter is a `Command`.

## Commands (controller → processor, `rtrb` SPSC, capacity 256)

| Command | Payload | Applied | Effect |
|---|---|---|---|
| `SetMasterVolume` | `VolumePercent` | next buffer boundary | `master_gain = v.to_linear()` |
| `SetCeiling` | `CeilingDb` | next buffer boundary | `ceiling_lin = clamp(v).to_linear()` — clamped again here even though the type already clamps (FR-010 defence in depth) |
| `Play` | — | next buffer boundary | `transport = Playing`; position continues from current |
| `Pause` | — | next buffer boundary | `transport = Paused`; position retained |
| `Stop` | — | next buffer boundary | `transport = Stopped`; `source.seek(0)`; position 0 |
| `PlayTestTone` | — | next buffer boundary | (re)starts the test tone from its fade-in; independent of transport |

Rules:

1. `Processor::render` drains **all** pending commands before rendering the first sample of the buffer. A command pushed after `render` started is observed only by the next call. This is the FR-025(b) property: for any command pushed between buffer N and N+1, buffer N is unaffected in full and buffer N+1 is affected from its first sample.
2. Commands are `Copy`, ≤ 16 bytes, no heap.
3. The producer is owned by `PlaybackController`, which applies each command to its shadow state before pushing. If the queue is full (`push` returns `Err`), the controller retries on its next tick; it never blocks and never drops the shadow update.
4. There is **no** command to bypass, remove, or raise the limiter above −0.1 dBFS (FR-009). There is no command to change the source or the output stage: those are construction-time parameters (see `ProcessorConfig`), changed only by rebuilding the processor on the controller thread (research R3).

## Events (processor → controller, `rtrb` SPSC, capacity 256)

| Event | Payload | When |
|---|---|---|
| `ToneFinished` | — | the buffer in which the test tone completed |
| `TrackLooped` | `at_clock: u64` | source position wrapped to 0 while playing |
| `CommandDropped` | `kind: u8` | never expected; emitted if drain finds an unknown discriminant (defensive) |

Push never blocks; on overflow the event is dropped (events are advisory; all state of record lives in atomics or shadow state).

## Shared atomics (`Arc<RtShared>`, created once per app run)

| Field | Ordering | Contract |
|---|---|---|
| `clock_frames: AtomicU64` | `Release` store by processor at end of `render`; `Acquire` load elsewhere | Strictly non-decreasing for the app's lifetime; increases by exactly the source frames consumed for each rendered buffer; survives processor rebuilds (FR-006, FR-025(c)) |
| `position_frames: AtomicU64` | same | Track position; the controller reads it to build the next `ProcessorConfig` on a rebuild |
| `peak_bits: AtomicU32` | `Relaxed` | `f32::from_bits` = max abs sample at the limiter output for the last buffer; never > `ceiling_lin` (FR-025(d)) |
| `negotiated_frames: AtomicU32` | `Relaxed` | frames per callback as observed by the adapter; 0 until the first callback |

## `Processor::render` signature (called only by an output backend)

```rust
/// Render one device buffer. `out` is interleaved with `device_channels` channels and
/// `out.len() / device_channels` frames. Real-time safe: no allocation, lock, I/O, or logging.
pub fn render(&mut self, out: &mut [f32]);
```

Fixed pipeline inside `render`: drain commands → source (or silence) → master gain → + test tone → limiter clamp (`|y| ≤ ceiling_lin`) → peak → output stage (resample to device rate, map channels) → `clock_frames += consumed`.

## Tests that pin this contract (crate `modplayer-engine`, run against `FakeBackend`/direct `render`)

| FR-025 | Test | Assertion |
|---|---|---|
| (a) | `render_never_allocates` | `assert_no_alloc(|| p.render(buf))` for 1000 buffers, including buffers with pending commands and tone start/finish |
| (b) | `command_applies_at_next_boundary` | push `SetMasterVolume(0)` mid-way; buffer N identical to a control render; buffer N+1 all zeros |
| (c) | `clock_monotonic_across_rebuild` | render 100 buffers; rebuild from snapshot (simulated device loss) and again with a 48 kHz device (simulated rate change); clock == Σ consumed frames and never decreases; position continues |
| (d) | `limiter_never_exceeds_ceiling` | for each ceiling in `−6.0..=−0.1` step 0.1, feed the 0 dBFS square segment; `max|y| ≤ ceiling_lin + 1e-6` |
| (e) | `ceiling_and_cap_clamp` | `CeilingDb::new(0.0) == −0.1`, `CeilingDb::new(−20.0) == −6.0`, `VolumePercent::new(150) == 100`, plus a settings-file round-trip with out-of-range values |
