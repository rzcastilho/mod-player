# Contract: Engine loop region (extends 001 contracts/engine-commands.md and contracts/audio-source.md)

**Crates**: `modplayer-engine` (`processor.rs`, `command.rs`, `event.rs`,
`shared.rs`, new `loop_math.rs`), `modplayer-audio-source` (`lib.rs`:
one provided trait method; `types.rs`: one additive `SourceCommand`),
`modplayer-audio-source-connect` (`rt.rs`: implement the method;
`worker.rs`: forward the hint), `modplayer-audio-source-synthetic`
(`lib.rs`/`host.rs`/`scripted.rs`: carry a store). Implements FR-008,
FR-009, FR-010, FR-011, FR-011a, FR-012; research R1–R8; data-model.md
§1.6, §2. Any PR touching `crates/modplayer-engine/` carries a
**real-time safety note** and the engine maintainer's sign-off
(`CODEOWNERS`, Constitution I / Governance).

## 1. `AudioSource` delta (trait crate, additive)

```rust
pub trait AudioSource: Send + 'static {
    // … 001 methods unchanged …
    /// The current track's retained decoded store, if this source has one.
    /// Real-time contract: returns a reference to an `Arc` the source already
    /// holds — never clones, never drops.
    fn decoded_store(&self) -> Option<&Arc<DecodedStore>> { None }
}
```

| Implementor | Returns |
|---|---|
| `ConnectRtSource` | `self.store.as_ref()` (the same `Arc` 005's retirement ring governs; unchanged ownership) |
| `SyntheticSource` | `self.store.as_ref()` — new field set by `SyntheticHost::attach` (its already-`Complete` store) or `SyntheticSource::with_store` (tests) |
| `ScriptedRt` | the store its `DecodeScript` is filling (so `Progressive`/`FailAt` scripts exercise the uncached seam) |

`SourceCommand::PrefetchHint { frame: u64 }` (additive): the receiver
forwards it to the current decode-ahead's `seek_hint(frame)` and does
**not** touch the `Player`; synthetic/scripted hosts ignore it. Sent by
the controller on arm with `frame = A − effective_crossfade` (FR-008).

## 2. `Command` delta (`Copy`, ≤ 16 bytes — the existing `const` assertion still holds)

| Command | Semantics (applied in `drain_commands`, i.e. at the buffer boundary) |
|---|---|
| `LoopSetA(u64)` | `loop_staged.a = frame` |
| `LoopSetB(u64)` | `loop_staged.b = frame` |
| `LoopSetSeam { crossfade_frames: u32, repeat: u32 }` | `loop_staged.crossfade_frames`, `.repeat` (`0` = infinite) |
| `LoopCommit { reset_wraps: bool }` | `loop_active = Some(loop_staged)` with `wraps = if reset_wraps { 0 } else { previous wraps or 0 }`; a running seam is left to finish (§4 rule 6). Ignored (no-op, `Event::CommandDropped`-free) when `staged.b ≤ staged.a` — the controller never sends that |
| `LoopDisarm` | `loop_active = None`; `seam` keeps running to its end if one is in progress (audio continuity) but no jump happens at B |

Ordering rule for the controller: setters first, `LoopCommit` last, all
through `push_command_retrying` so a momentarily full queue preserves
order (001 rule 3). `Command::Seek`, `Play`, `Pause`, `Stop` do not
touch the loop fields; `Stop` (seek to 0) simply makes the region
armed-inactive.

## 3. `Event` and `RtShared` delta

| Item | Semantics |
|---|---|
| `Event::LoopWrapped { wraps: u32, gapless: bool }` | pushed in the render that performed the jump; `gapless` = the seam frames were all read from the store **and** the store covers `A` |
| `Event::LoopReleased { wraps: u32 }` | pushed when `repeat != 0 && wraps == repeat` right after the wrap that reached it; `loop_active` is already `None` |
| `RtShared::loop_wraps() / set_loop_wraps()` | `AtomicU32`, written after every wrap and on `LoopCommit`/`LoopDisarm` |
| `RtShared::loop_state() / set_loop_state()` | `AtomicU8`: `0` disarmed, `1` armed-inactive, `2` armed-active; written once per render from the segment classification (§4) |

Events remain advisory (dropped on overflow); the atomics are the state
of record for the UI.

## 4. Real-time rules for `Processor::render` (Constitution I)

1. **No allocation, lock, I/O, log** on any path added here: `seam_in`
   (2 × `ceil(0.05 × source_rate)` samples) is allocated in
   `Processor::new`; store reads are `DecodedStore::read_frames`
   (atomics); gains use `f32::sin/cos`. `tests/realtime.rs`'s
   `assert_no_alloc` wrapper gains `render_with_armed_loop_never_allocates`
   (1 000 renders across ≥ 20 wraps with a store attached).
2. **Decision source**: the segment classification uses
   `source.position()` at the start of each segment — never wall time,
   never a UI value. A `Command::Seek` in the same drain is applied before
   classification (it runs in `drain_commands`), so "seek outside → armed
   but inactive" is observed in that very render (FR-012).
3. **Inside** = `a ≤ pos < b`. `pos < a` → fill to `a`, then re-classify
   (natural entry). `pos ≥ b` → plain fill (no jump: FR-012's "a seek that
   lands past B from outside does not trigger").
4. **Seam start**: at the first frame with `pos ≥ b − x` while inside, the
   processor reads `x = effective_crossfade(crossfade_frames, a, b)`
   incoming frames from `decoded_store()` at `a − x` into `seam_in`. If it
   gets fewer than `x`, this seam runs with `x = 0` and `gapless = false`
   (research R7). `SeamRt { a, b, x, gapless }` is captured now.
5. **Seam render**: outgoing frames still come from `source.fill` (both
   feeds work); mixing follows research R3's formula; the frame after the
   seam is `source.fill` from `a` after `source.seek(a)`.
6. **Commit during a seam**: `loop_active` updates but the `SeamRt` in
   flight finishes with its captured `a`/`b`/`x`; the jump at its `b`
   still goes to its `a` (FR-011a: "a seam already in progress completes
   with the old bounds").
7. **Jump**: `source.seek(a)` exactly when `pos == b`; `carry_len` is
   **not** cleared (continuous audio); `wraps += 1`; push `LoopWrapped`;
   evaluate repeat → possibly clear `loop_active` and push `LoopReleased`.
8. **Repeat count**: `repeat == 0` = infinite; `wraps` resets only via
   `LoopCommit { reset_wraps: true }`.
9. **Published position** (research R8): after a same-render wrap,
   `published = if pos − a ≥ leftover { pos − leftover } else { b − (leftover − (pos − a)) }`.
10. **Not playing**: paused/stopped renders classify state (so
    `loop_state` is right) but never seam or jump.
11. **Degenerate bounds**: `b − a < 2` frames is rejected by `LoopCommit`
    (`staged.b ≤ staged.a` or `b − a < 2`); the controller's `is_armable`
    (≥ 1 ms) already prevents it — this is the RT's own guard against a
    zero-length loop spinning inside one render.

## 5. `loop_math` (pure, proptested — Constitution VIII "marker/loop arithmetic")

```rust
pub fn effective_crossfade(configured: u64, a: u64, b: u64) -> u64;   // min(configured, b.saturating_sub(a), a)
pub fn wrap_position(a: u64, b: u64, elapsed: u64) -> u64;            // a + (elapsed − a) % (b − a); elapsed < a → elapsed
pub fn crossfade_gains(index: u64, x: u64) -> (f32, f32);             // (out, in); (1, 0) when x == 0
```

Properties: `effective_crossfade ≤ min(configured, b − a, a)`;
`out² + in² ≈ 1` (±1e-5); `wrap_position(a, b, a + k·(b − a) + r) == a + r`
for `r < b − a`.

## 6. Tests pinning this contract (`crates/modplayer-engine/tests/loop_seam.rs` unless noted)

All run against `SyntheticSource::with_store` (Constitution IV) at
`device_rate == source_rate` unless stated; the store is the synthetic
track fully decoded.

| Test | Asserts |
|---|---|
| `period_is_exact_after_1000_wraps` (proptest: `a ∈ [0, len − 2 s]`, `len_region ∈ [1 ms, 5 s]`, `x ∈ {0, 1, 5, 20, 50} ms`, buffer ∈ {64, 256, 1 024, 4 096}) | after ≥ 1 000 `LoopWrapped`, `source.position() == wrap_position(a, b, clock_frames)` and every wrap's `clock_frames` delta == `b − a` |
| `period_is_exact_under_resampling` (48 kHz device, 44.1 kHz source) | same identity in source frames |
| `seam_is_click_free` | max `|Δsample|` across each of 50 seams ≤ 1.5 × max `|Δsample|` of the unlooped material at the same gain |
| `short_region_shrinks_crossfade` (3 ms region, 5 ms crossfade) | armable; `SeamRt.x == 3 ms`; period exact |
| `a_at_zero_hard_cuts` | `x == 0`, still exact period, `gapless == true` |
| `uncached_seam_hard_cuts_and_reports_not_gapless` (`ScriptedRt`, `DecodeScript::Progressive` behind `A`) | first wrap `gapless == false`, later wraps `true` once covered |
| `repeat_count_releases_after_n` | `LoopReleased { wraps: 3 }` after 3 wraps; playback continues past `b`; `loop_state == 0` |
| `seek_outside_keeps_armed_inactive` | `Command::Seek(b + 1)` → `loop_state == 1`, no jump for 10 renders; `Seek(a + 1)` → `2`, jumps again |
| `natural_entry_from_before_a_activates` | `Seek(a − 1000)` → state `1`, then `2` once `pos ≥ a`, jump at `b` |
| `commit_is_atomic_across_renders` | setters in render N, `LoopCommit` in N+1 → region unchanged in N |
| `edit_while_armed_applies_next_buffer_and_finishes_seam` | new `b` committed mid-seam: seam finishes with old `b`, next iteration uses new `b` |
| `disarm_mid_seam_finishes_seam_without_jump` | |
| `published_position_after_mid_render_wrap` (resampling, `leftover > 0`) | rule 9 |
| `realtime::render_with_armed_loop_never_allocates` (`tests/realtime.rs`) | `assert_no_alloc` |
| `loop_math` proptests (`src/loop_math.rs`) | §5 properties |
| `command_is_copy_and_small` (existing) | still ≤ 16 bytes with the new variants |

Receiver: `rt_feed::decoded_store_returns_current_track_store`,
`worker::prefetch_hint_forwards_to_decode_ahead` (`ScriptedHost` mirror:
`prefetch_hint_is_ignored`).
