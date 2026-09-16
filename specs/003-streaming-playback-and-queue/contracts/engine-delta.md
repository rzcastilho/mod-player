# Contract: Engine deltas (extends 001 contracts/engine-commands.md)

**Crate**: `modplayer-engine` — every change here needs the engine
maintainer's sign-off and a "real-time safety" note in the PR (Constitution
I, Governance). Nothing in this delta allocates, locks, blocks, logs or
performs I/O inside `render`.

## 1. Commands (controller → processor)

| Command | Added | Semantics |
|---|---|---|
| `Seek(u64)` | new | `source.seek(frame)` at the next buffer boundary; position atomics reflect it after that render. `Copy`, keeps `size_of::<Command>() ≤ 16` (static assert unchanged) |

All 001 commands unchanged. `Stop` still seeks the source to 0.

## 2. Leftover carry replaces the rewind-by-`seek`

`render` previously fetched `needed` source frames and, when the output
stage consumed fewer, called `source.seek(position − leftover)`. New rule:

- `OutputStage` (or `Processor`) keeps a preallocated `carry: [f32; 2 * MAX_GUARD]`
  with the unconsumed guard frames and prepends them to the next render's
  source fetch; the source is asked only for `needed − carried` frames.
- Published position: `shared.set_position_frames(source.position() − carried)`.
- Invariant (test): for the synthetic source, the rendered output over any
  sequence of buffer sizes is bit-identical to 001's behaviour (the
  dropout/duplicate-frame detector from 001 still passes at every preset
  and rate pair).

Rationale: a ring-buffer source cannot un-consume frames (research R4/R11);
the carry is source-agnostic and removes a `seek` from the hot path.

## 3. Shared atomics — position anchor (`RtShared`)

| Field | Type | Writer | Semantics |
|---|---|---|---|
| `anchor_position_frames` | `AtomicU64` | processor, once per render | `position_frames` at the end of the render |
| `anchor_instant_nanos` | `AtomicU64` | processor | `Instant` (monotonic, converted to nanos since a process-start epoch stored once in `RtShared::new`) captured in the same render |
| `anchor_generation` | `AtomicU64` | processor | incremented before and after writing the pair (seqlock); readers retry on an odd/changed generation |
| `anchor_playing` | `AtomicBool` | processor | whether the source advanced in that render (intent `Playing` and no underrun) |

`PositionClock::now(shared: &RtShared, source_rate: u32) -> Duration`:
reads the anchor with the seqlock; when `anchor_playing`, returns
`position + (now − anchor_instant) × source_rate`, capped at
`position + one_buffer_duration × 2`; otherwise returns `position`
frozen. Consumers: the UI (each frame), tests.

`Instant::now()` inside `render` is accepted as real-time safe (vDSO /
`mach_absolute_time`, no syscall on macOS/Linux, `QueryPerformanceCounter`
on Windows). Documented in the crate's real-time note.

## 4. `Transport` (unchanged)

`Stopped | Playing | Paused`. Buffering is a host concept (core
`TransportState`); the engine keeps rendering the source while `Playing`,
and a streaming source writes silence and freezes `position()` when it has
no data (contracts/audio-source-host.md §4).

## 5. Tests that pin this delta (`modplayer-engine`)

- `seek_command_applies_at_boundary`: push `Seek(1000)` mid-run; the render
  that drains it starts from frame 1000 (synthetic determinism).
- `leftover_carry_is_bit_exact_with_001`: golden comparison across
  44.1k→48k, 44.1k→44.1k, 48k→44.1k at 128/256/1024 frames.
- `render_never_allocates` (existing `assert_no_alloc`) still passes with the
  carry and anchor writes.
- `position_clock_60hz_jitter`: with `FakeBackend` rendering 256-frame
  buffers on a timer thread, sampling `PositionClock` every 16.6 ms for 10 s
  yields strictly non-decreasing values whose per-sample delta deviates from
  the ideal by ≤ 5 ms (SC-003).
- `position_clock_frozen_when_paused`: no drift while `Paused`.
