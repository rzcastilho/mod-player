# Research: Markers, Loop Regions, and Cue Points

**Feature**: 006-markers-loops-and-cues | **Date**: 2026-09-17 | **Spec**: [spec.md](spec.md)

Every unknown in plan.md's Technical Context is resolved here. Format per
decision: Decision / Rationale / Alternatives considered. The spec's
Clarifications already fixed the product-level defaults (keys, limits,
persistence rules); this file fixes the *engineering* choices the spec
left to the plan (where the loop runs, how the seam reads audio, how the
engine learns about a region, the track-id filename encoding, the drag
zoom-assist mechanics, persistence threading). Headless session: every
decision below was taken without a human; the ones that could plausibly
go another way are also listed in plan.md's Complexity Tracking.

Code read for this research (all in this worktree): `crates/modplayer-engine/src/{processor,command,event,shared}.rs`,
`crates/modplayer-audio-source/src/{lib,decoded,host,types}.rs`,
`crates/modplayer-audio-source-connect/src/{rt,events,worker}.rs`,
`crates/modplayer-audio-source-synthetic/src/{lib,host}.rs`,
`crates/modplayer-core/src/{controller,transport,notifications,settings/model,settings_registry,library/persist,analysis/cache}.rs`,
`crates/modplayer-ui/src/{now_playing,theme,shell,waveform/*}.rs`,
`specs/005-now-playing-waveform/{plan,contracts/*}.md`, `.specify/memory/constitution.md`.

---

## R1. Where the loop runs: the engine `Processor`, not the source

**Decision**: The loop region (A, B, effective crossfade, repeat count,
wraps-so-far, armed/active state) lives in `modplayer_engine::Processor`
as a `loop_region: Option<LoopRt>` field. `Processor::render` splits each
buffer at B, renders the seam and re-seeks its `AudioSource` to A itself.
The decision is taken per render from `source.position()` — the engine's
own frame cursor, which is what `RtShared::clock_frames` /
`position_frames` advance from — never from UI or wall time (FR-009,
FR-6.1.4/6.1.5).

**Rationale**: Constitution I and III name loop regions a host primitive
evaluated on the real-time path by the engine clock. The `Processor` is
the one type that runs in the audio callback for every source (synthetic
and Connect alike), already drains `Command`s at buffer boundaries
(FR-011a's rule for free) and already owns `source.seek()`. Putting the
loop in `ConnectRtSource` would duplicate it in every `AudioSource`
implementor (Constitution IV: every crate must run on the synthetic source)
and would leave the synthetic source unable to loop.

**Alternatives considered**: (a) loop inside each `AudioSource` — rejected
(duplication, synthetic source could not exercise the seam tests); (b)
loop in the controller via `seek_frames` on a timer — rejected outright
(FR-009 forbids UI/timer evaluation; Constitution I); (c) a new
`modplayer-loop` crate — rejected (Constitution X: no crate without a
second consumer; the code is ~250 LOC inside the existing RT pipeline).

## R2. How the seam gets random-access audio: `AudioSource::decoded_store()`

**Decision**: Add one *provided* method to the `AudioSource` trait in the
dependency-free trait crate:

```rust
fn decoded_store(&self) -> Option<&Arc<DecodedStore>> { None }
```

`ConnectRtSource` returns its current-track store (the `Arc` it already
holds; no new ownership, the retirement discipline of 005 R1 is untouched).
`SyntheticSource` gains an `Option<Arc<DecodedStore>>` that
`SyntheticHost::attach` (which already builds a `Complete` store) and
`ScriptedHost::attach` install. The `Processor` reads the incoming seam
frames `[A − x, A)` with `DecodedStore::read_frames` (atomics +
`OnceLock::get` only — real-time safe by 005's contract), which
Constitution V's guard test already permits from `modplayer-engine`.

**Rationale**: The seam needs the x frames *ending at A* while the source
is still delivering the frames ending at B; only the retained decoded
store offers sample-exact random access without a second decoder. The
trait crate already owns `DecodedStore`, so the method costs no
dependency. A provided method with a `None` default is additive: every
existing implementor (including `ScriptedRt`) keeps compiling; a source
without a store simply loops with a hard cut (x_eff = 0) — still gapless
in period, only the crossfade degrades — and the wrap itself goes through
the ordinary seek (FR-008's uncached rule).

**Alternatives considered**: (a) send the `Arc<DecodedStore>` to the
`Processor` through the command queue — rejected (`Command` is `Copy`, ≤ 16
bytes; an `Arc` is neither and dropping it on the RT is forbidden by 005
R1); (b) a dedicated `rtrb` ring carrying stores into the `Processor` — a
second copy of the receiver's retirement machinery for no benefit; (c)
have the `Processor` buffer the last 50 ms it rendered after A in a ring
so the seam needs no store — rejected: the crossfade mixes the frames
*before* A (`[A − x, A)`), which were possibly never rendered (a seek
straight to A, or A = 0), so a render-history ring cannot supply them; it
would also make the seam depend on what was played, not on the markers.

## R3. Seam mathematics (FR-010) and the drift proof

**Decision**: Pre-roll equal-power crossfade of length
`x = min(configured_frames, B − A, A)` frames:

```
for pos in [B − x, B):            t = (pos − (B − x) + 1) / (x + 1)   ∈ (0, 1)
    out = src(pos) · cos(t·π/2) + store(A − x + (pos − (B − x))) · sin(t·π/2)
at pos == B:                       source.seek(A); wraps += 1
```

`configured_frames = round(crossfade_ms × source_rate / 1000)`. `x = 0`
is a hard cut. The gain curve is evaluated per frame with `f32::cos/sin`
(no table allocation; ≤ 2 205 frames per wrap at 44.1 kHz / 50 ms).

**Why the period is exact**: the source cursor moves `A → B` by natural
`fill`, and the re-seek happens exactly when `position() == B`, so every
loop iteration consumes exactly `B − A` source frames regardless of `x`
or of the device rate/resampler. The cumulative deviation after N wraps
is therefore 0 by construction: `cursor = A + ((elapsed − A) mod (B − A))`
where `elapsed` is the source-frame clock. The seam-accuracy test
(quickstart §1) asserts exactly this after 1 000 wraps for
`x ∈ {0, 1, 5, 50} ms`, regions shorter than x, and A = 0.

**Why it is click-free**: `sin²+cos²=1` keeps the summed power constant;
the last seam frame mixes `store(A − 1)` at gain → 1 with `src(B − 1)` at
gain → 0, and the next frame is `src(A)` — continuous with `store(A − 1)`
because both are the same decoded material. The click-free test measures
the max first-difference across the seam against the max first-difference
of the un-looped material and requires the ratio ≤ 1.5.

**Alternatives considered**: post-roll seam (mix `[B, B + x)` into
`[A, A + x)` after the jump) — rejected: the frames after B may not be
decoded yet (streaming) and the period would still be `B − A` but the
first frame after the jump would be a mix, so `cursor` at the seam would
not equal `A` exactly (harder to test, no benefit); linear crossfade —
rejected (−3 dB dip at the midpoint, audible on sustained material).

## R4. Getting the region into the engine: staged commands + one commit

**Decision**: Five new `Command` variants, all `Copy` and ≤ 16 bytes so
`contracts/engine-commands.md`'s size assertion is untouched:

| Command | Payload | Effect on `Processor` |
|---|---|---|
| `LoopSetA(u64)` | frame | writes `staged.a` |
| `LoopSetB(u64)` | frame | writes `staged.b` |
| `LoopSetSeam { crossfade_frames: u32, repeat: u32 }` | `repeat == 0` = infinite | writes `staged.crossfade/repeat` |
| `LoopCommit { reset_wraps: bool }` | | copies `staged → active` (armed); `reset_wraps` zeroes `wraps` (arm) or keeps them (edit-while-armed) |
| `LoopDisarm` | | `active = None`, wraps published as-is |

The controller always pushes the four setters *then* `LoopCommit` in one
`push_command_retrying` sequence; the `Processor` only ever changes its
active region at a `LoopCommit`, so a queue that fills mid-sequence (the
setters land in one render, the commit in the next) can never expose a
half-updated region — the previous region keeps rendering until the
commit arrives. An in-progress seam completes with the old bounds because
the seam gains are computed from the bounds captured at the render in
which the seam started (FR-011a).

**Rationale**: `Command` must stay `Copy` ≤ 16 bytes; a single
`SetLoop { a, b, x, n }` is 24 bytes. Staging keeps the existing contract
and gives FR-011a's "next buffer boundary, atomically" semantics.

**Alternatives considered**: raise the `Command` bound to 24 bytes —
rejected (contract change with a documented invariant and a `const`
assertion; staging is ~20 LOC); carry the region in `RtShared` atomics
and let the RT read it — rejected (four atomics cannot be read
consistently without a seqlock, which is exactly the "atomic commit"
staging gives for free).

## R5. Reporting wraps and release: events + atomics

**Decision**: `Event::LoopWrapped { wraps: u32, gapless: bool }` pushed
per wrap and `Event::LoopReleased { wraps: u32 }` when the repeat count is
reached; plus two `RtShared` atomics as the state of record —
`loop_wraps: AtomicU32` and `loop_state: AtomicU8` (`0` disarmed, `1`
armed-inactive, `2` armed-active). The controller's `tick()` starts
draining `event_rx` (allocated since 001 but never drained) and:

1. on ≥ 1 `LoopWrapped` this tick, sends **one** coalesced
   `SourceCommand::Seek(A_ms)` to the source host (throttled to at most
   one per 250 ms after the first, `LOOP_RESEEK_INTERVAL`), so the
   streaming half's `Player` follows the loop: the ring refills from A
   after an uncached (ring-feed) wrap, the Connect cluster position stays
   inside the region, and the `Player` never reaches its own end of
   track while the RT is looping;
2. on `LoopReleased`, marks the region disarmed in the marker model
   (`armed = false`, wraps shown) — the RT already stopped looping;
3. while `loop_state == 2` (armed-active), `SourceEvent::EndOfTrack` is
   ignored by `map_source_event` (the `Player` is re-seeked on the next
   throttled tick instead) — a region ends at `B < len`, so the RT never
   legitimately reaches the end while active.

`SourceEvent::Seeked` is already dropped by the reducer (`map_source_event`
returns `None`) and a `Reposition` marker never moves `cursor` on the
store feed (005 rule 4), so re-seeking the `Player` cannot disturb the RT
cursor or transport state.

**Rationale**: events may be dropped on overflow (001's contract), so the
UI's wraps-remaining/"armed but inactive" readout comes from atomics;
events carry only the *edge* the controller acts on. The re-seek
throttle bounds `spirc.set_position_ms` calls to ≤ 4/s for sub-250 ms
regions while still re-seeking immediately after the first wrap.

**Alternatives considered**: no `Player` re-seek — rejected (the `Player`
keeps decoding to the end of the track, emits `EndOfTrack`, the queue
advances mid-loop); re-seek on every wrap — rejected (a 1 ms region would
call into librespot 1 000×/s).

## R6. Track-change and rebuild handling in the engine

**Decision**: The controller pushes `LoopDisarm` on every current-track
change (including a same-identity reload — FR-016's "armed flag cleared
on every track change") and on `clear_for_sign_out`. On a stream rebuild
(`ProcessorConfig` from shadow state, 001 R3) the controller re-pushes
the staged setters + `LoopCommit { reset_wraps: false }` when a region is
armed, so the loop survives device changes with its wrap count. `Command::Stop`
does not disarm (arming is not a transport action, FR-008; stopped at 0
is simply "armed but inactive").

**Rationale**: the marker model in core is the shadow state (as for
volume/ceiling); the RT copy is derived from it exactly like every other
`Processor` field.

## R7. Uncached seam and the ring feed

**Decision**: At the render in which the seam should start (`pos ≥ B −
x`), the `Processor` checks `store.read_frames(A − x, …)` returned all `x`
frames; if not (store absent, `Filling` behind, `Failed`), it renders
this wrap with `x_eff = 0` (hard cut at B) and reports `gapless = false`.
The jump itself is `source.seek(A)`: `ConnectRtSource` picks the store
feed when `covers(A)` and the ring otherwise (005 restart-point rule), so
an uncached A produces 005's ordinary seek (gap possible) and the
controller's re-seek (R5) refills the ring. Arming a region sends
`SourceCommand::Seek`-free cache pressure via the existing decode-ahead
seek hint: the controller calls `analysis`-independent
`source_host.command(SourceCommand::PrefetchHint { frame: A − x })` — **new
additive `SourceCommand`** the receiver forwards to `DecodeAhead::seek_hint`
(already exists) and the synthetic/scripted hosts ignore. This is FR-008's
"arming MUST request caching of the region's audio ahead of the playhead".

**Rationale**: NFR-1.2 scopes the 0-sample guarantee to cached audio; the
decode-ahead already follows seek hints (005 R3), so a hint is the
cheapest way to pull the seam into the store without seeking the `Player`.

**Alternatives considered**: sending a real `SourceCommand::Seek` on arm —
rejected (moves the `Player` and the cluster position while the user
listens elsewhere in the track); doing nothing — rejected (FR-008 MUST).

## R8. Position publication across a mid-buffer wrap

**Decision**: `Processor::render` publishes `published = source.position()
− leftover` today; after a wrap in the same render, if `position() − A <
leftover` the published position is `B − (leftover − (position() − A))`
(the carried guard frames belong to the pre-wrap material). The carry
itself stays valid across a loop wrap (the scratch content is continuous
through the seam), so `carry_len` is **not** cleared on a loop wrap —
unlike `Command::Seek`, which still clears it.

**Rationale**: keeps the ≤ 8-frame carry exact and the published playhead
monotone inside the region; `PositionClock`'s extrapolation may overshoot
B by at most one buffer between renders (bounded by
`anchor_buffer_frames`, 005 R8) — a display-only effect noted in the UI
contract.

## R9. Marker model and persistence live in `modplayer-core::markers`

**Decision**: New module `crates/modplayer-core/src/markers/` — `model.rs`
(`Marker`, `MarkerKind`, `LoopRegion`, `TrackMarkers` with every rule of
FR-001/002/006/007/011/013/018/019 as pure functions), `store.rs`
(`TrackStatePaths`, JSON DTO, atomic write, read rules, writer thread),
and `mod.rs` (re-exports + `MarkerError`). The `PlaybackController` owns
one `TrackMarkers` for the current track plus the nudge step, and exposes
the mutation API in `contracts/marker-service.md`. Pure loop arithmetic
shared with the engine (`effective_crossfade_frames`, `wrap_position`)
lives in `modplayer_engine::loop_math` (core already depends on engine).

**Rationale**: Constitution III (host primitive in a core crate),
Constitution X (no new crate without a second consumer — plugins are that
consumer and do not exist yet), and 003–005 precedent (queue, library,
analysis are all `modplayer-core` modules).

## R10. Per-track file name: lowercase hex of the `TrackId` bytes

**Decision**: `<data_local_dir>/ModPlayer/track-state/<hex(track_id_bytes)>.json`
(override `MODPLAYER_TRACK_STATE_DIR`). `TrackId` is ASCII ≤ 64 bytes, so
the name is ≤ 128 hex chars + `.json` (< 255 everywhere), uses only
`[0-9a-f]`, and decodes back to the exact id.

**Rationale**: FR-016 asks for a filesystem-safe *reversible* encoding.
Base62 track ids are case-sensitive while the default macOS and Windows
filesystems are case-insensitive, so any encoding that keeps letters as
letters can alias two ids; hex is case-free, reversible, needs no
dependency and no escaping rules. 005 used `sha256(track_id)` for the
analysis cache, which is not reversible (FR-016 wants reversibility so a
"clear per-track state" tool in a later slice can list tracks).

**Alternatives considered**: percent-encoding (`spotify%3Atrack%3A…`) —
case aliasing on case-insensitive filesystems; sha256 — irreversible;
a single `track-state.json` map keyed by id — one write rewrites every
track's state and a crash mid-write risks all of them (NFR-2.8).

## R11. Persistence threading: debounce on the controller, write on a writer thread

**Decision**: Every mutation marks the current track's state dirty; the
controller serializes the whole `TrackMarkers` at most once per 250 ms
(`TRACK_STATE_DEBOUNCE`, checked in `tick()` like `flush_persistence`)
and hands the bytes + path to a dedicated `markers::store::spawn_writer`
thread (mirrors `library::persist::spawn_writer`) which does
`<name>.json.tmp` → `write_all` → `sync_all` → `rename`. Track change,
`clear_for_sign_out` and `shutdown` flush immediately and, on shutdown,
join the writer. Loads are synchronous on the controller thread at track
change (`load` reads ≤ 64 KiB — larger files are treated as unreadable —
so it costs ≈ 0.1 ms) so the state is in place before the track is
considered ready (spec assumption on FR-11.3.2 ordering).

**Rationale**: FR-017 says "debounced on the controller thread (never the
RT)"; 004/005 established that disk *writes* go to a background thread so
a slow disk never stalls a frame. A synchronous load is the simplest way
to honour the "restored before the track is ready" ordering and is
bounded by the file-size cap.

**Alternatives considered**: reuse `library::persist::PersistJob` —
rejected (library-specific enum, different paths/debounce); async load —
rejected (ordering guarantee would need a "markers pending" UI state for
a ~0.1 ms read).

## R12. File format: serde_json, schema 1, unknown keys ignored

**Decision**: JSON via `serde_json` (already a core dependency), schema in
data-model.md §4. `schema_version: 1`; `#[serde(default)]` on every
optional field; `deny_unknown_fields` **not** set (unknown keys ignored);
newer `schema_version` → empty + `track-state-newer-version`, file left
untouched until the next mutation. Positions are stored as frames at the
recorded `sample_rate` (44 100 for Connect, the source's rate otherwise);
on load, if the file's `sample_rate` differs from the current source rate
the positions are rescaled (`frames × new / old`, rounded) — no such
case exists today, kept for forward compatibility.

**Rationale**: matches 001's `settings.toml` and 004's `index.json`
conventions; a 64-marker file is < 8 KiB, so binary compactness (005's
`.mpwf`) buys nothing.

## R13. Marker colour palette: eight fixed accent tokens in `theme.rs`

**Decision**: `pub const MARKER_PALETTE: [Color32; 8]` in
`crates/modplayer-ui/src/theme.rs` — the only place these literals
appear — chosen for ≥ 3:1 contrast against both egui light and dark
panel backgrounds; the persisted value is the index. Defaults by kind:
region `0`, point `1`, cue `2`.

**Rationale**: the spec fixes an index palette; 005's "no new colour
literals outside `theme.rs`" rule is honoured by putting them *in*
`theme.rs`. egui's `Visuals` has one accent (`selection.bg_fill`), not
eight, so a derived palette is not possible.

## R14. Drag zoom-assist: relative-delta drag in detail space + animated zoom

**Decision**: A marker drag (from a glyph on either lane) is *relative*:
`live = live + Δx_pointer × detail.frames_per_pixel()` each frame,
starting from the marker's own position (not the pointer's frame). Each
frame during the drag the detail window recenters on `live` and shrinks
its width by ×0.8 per frame toward `assist_target = max(rect_width_px ×
2.5 ms, DETAIL_MIN_WINDOW_MS)` (the existing 200 ms floor), so within
~10 frames the detail space
resolves ≤ 2.5 ms per pixel (spec: ≤ 5 ms placement) whichever waveform
the drag started on. Release commits `live` (clamped to `[0, len]`,
FR-019); `Esc` restores the original position and the pre-drag detail
window; after a commit the detail keeps its zoomed window with follow
suspended (005 §5 rule).

**Rationale**: an *absolute* mapping (`detail_space.frame_at(pointer_x)`)
feeds back on itself once the window recenters on the live position (a
still pointer would keep sliding). Relative deltas are the standard
precision-drag pattern and are what makes the overview usable for fine
placement.

**Alternatives considered**: absolute mapping with recenter only at drag
start — cannot reach 5 ms from the overview (≈ 300 ms/px for a 4-minute
track at 800 px); modifier-key "fine mode" — an extra gesture the spec
does not ask for.

## R15. Marker lanes above each waveform (glyph hit-testing and Tab order)

**Decision**: Each waveform gets a 14 px **marker lane** allocated
directly above its rect and sharing its `TimeSpace` x-mapping; marker
glyphs (a 10 px triangle for points, a bracket for A/B, a numbered square
for cues) are `ui.interact`-ed widgets in the lane (`Sense::click_and_drag`,
`Role::Button`, name from `marker-glyph-name`), so they never overlap the
waveform's own click-to-seek surface and `Tab` reaches them in visual
order between 005's overview and detail stops. The vertical marker lines
and the armed-region span are painted *inside* the waveform through the
overlay hook (R16).

**Rationale**: avoids egui hit-test ambiguity between two overlapping
`click_and_drag` widgets, gives every marker an unambiguous accessible
node (FR-022) and keeps 005's waveform widget unchanged.

## R16. Overlay hook: implement 005 §6's promised `overlays` parameter

**Decision**: `waveform::overview` / `waveform::detail` gain an
`overlays: &mut dyn FnMut(&Painter, &TimeSpace)` argument, called after
`paint::paint`'s peaks and *before* the playhead (the playhead line moves
out of `paint::paint` into a separate `paint::playhead` call). Marker
lines, the A/B span (solid accent when armed-active, hatched when
armed-inactive, outline when disarmed) and the clamped-marker warning
glyph are drawn by `markers_overlay` in `now_playing.rs`'s new
`markers.rs` sibling.

**Rationale**: 005 contracts/ui-waveform.md §6 named this hook as the
attachment point for markers but shipped without the parameter; adding
it is the contract's own extension path (FR-026).

## R17. Keyboard scope for view-level shortcuts

**Decision**: `M`, `I`, `O`, `L`, `1`–`8`, `Shift+1`–`8` are consumed by
`now_playing::show` (via `consume_key`) when the Now Playing section is
shown and **no text field owned by the view has focus** — the view keeps
the `Id`s of its text fields (inline rename, repeat-count and crossfade
fields) and checks `ctx.memory(|m| m.focused())` against them. `←`/`→`,
`Shift+←/→`, `Delete`/`Backspace`, `F2`/`Enter`, `Esc` are handled by the
focused marker widget (`response.has_focus()`), exactly like 005's
waveform keys. `Ctrl/Cmd+1..5` (shell) keep precedence because the shell
runs `handle_shortcuts` first and those use the command modifier while
`1`–`8` here require *no* modifier and `Shift+n` only shift.

**Rationale**: FR-004a's scope ("same as 003's `Space`"); `Ctx::wants_keyboard_input`
is true for *any* focused widget, including a focused marker glyph, so
it cannot be the guard.

## R18. Nudge step setting

**Decision**: `[markers] nudge_step_ms = 10` as a new `RawMarkers` table
in `RawSettings` (`#[serde(default)]`, older files load unchanged),
clamped to `1..=1000` in `RawSettings::into_settings` (no `InvalidField`
— numbers clamp silently per 001), stored in `AudioSettings` as
`nudge_step_ms: u16`, edited by a `DragValue` in Settings › Playback
(`setting-nudge-step` / `setting-nudge-step-desc`, registered in
`settings_registry::DESCRIPTORS` so Settings search finds it). The
controller applies it through `set_nudge_step_ms` (settings write path
as `set_device_name`).

## R19. Tests required by the constitution (FR-028) — where they live

| Test | Crate / file | Covers |
|---|---|---|
| `loop_seam::period_is_exact_after_1000_wraps` (proptest over `A`, `B − A ∈ [1 ms, 5 s]`, `x ∈ 0..=50 ms`) | `modplayer-engine/tests/loop_seam.rs` | NFR-1.2, SC-002 |
| `loop_seam::seam_is_click_free` (ratio ≤ 1.5 on the synthetic sine) | same | NFR-1.3, SC-001 |
| `loop_seam::short_region_shrinks_crossfade`, `::a_at_zero_hard_cuts`, `::uncached_seam_hard_cuts_and_reports_not_gapless` | same | FR-010, EC-4.2 |
| `loop_seam::repeat_count_releases_after_n`, `::seek_outside_keeps_armed_inactive`, `::commit_is_atomic_across_renders`, `::edit_while_armed_applies_next_buffer` | same | FR-011, FR-011a, FR-012 |
| `realtime::render_with_armed_loop_never_allocates` (`assert_no_alloc`) | `modplayer-engine/tests/realtime.rs` | Constitution I |
| `loop_math` proptests (`effective_crossfade ≤ min(x, B−A, A)`, `wrap_position` identity) | `modplayer-engine/src/loop_math.rs` | Constitution VIII |
| `markers::model` proptests (swap keeps A ≤ B, clamp ≤ len, limit never exceeded, cue slot uniqueness, sort stability) | `modplayer-core/tests/markers_model.rs` | FR-002, FR-007, FR-013, FR-019 |
| `markers::store` round-trip proptest + `crash_mid_write_keeps_previous_file` (`.tmp` written, no rename) + read-rule tests | `modplayer-core/tests/markers_store.rs` | NFR-2.8, FR-016, FR-017, SC-009, SC-014 |
| controller wiring (`arm_pushes_setters_then_commit`, `track_change_disarms_and_saves_then_loads`, `loop_wrapped_reseeks_source_once_per_tick`, `end_of_track_ignored_while_loop_active`, `release_event_disarms_model`) | `modplayer-core/tests/controller_markers.rs` | FR-008, FR-016, R5, R6 |
| UI (`now_playing`/`accessibility`/`fluent_keys`/`markers` tests in contracts/ui-markers.md §8) | `modplayer-ui/tests/markers.rs` + extended files | FR-003–FR-005, FR-020–FR-023 |
| `decoded_store_boundary` (unchanged) still passes with the engine's `read_frames` caller | `modplayer/tests` | Constitution V |

`proptest` is added to `modplayer-engine`'s `[dev-dependencies]` (workspace
dependency, dev-only). No new runtime dependency anywhere.

## R20. Dependency additions

None at runtime. Dev: `proptest` for `modplayer-engine` (already a
workspace dev dependency used by four crates). `serde_json`, `serde`,
`directories` are already `modplayer-core` dependencies.

## Open verifications (to confirm during implementation; none blocks the design)

1. egui 0.36 `ui.interact` in the marker lane must win hover over the
   waveform rect below it — they do not overlap by construction (R15), so
   this is only a layout check.
2. librespot `Player::seek` on already-downloaded data emits `Seeked`
   (not `Loading`) — if it emits `Loading`, the reducer's `buffering`
   flag may flicker at ≤ 4 Hz on short regions; mitigation would be to
   suppress `Input::Loading` while `loop_state == 2`, same as `EndOfTrack`.
3. `DragValue` for the crossfade/repeat fields: confirm it exposes
   `Role::SpinButton`/`Slider` with the label so the accessibility test can
   name it; otherwise fall back to a `TextEdit` with numeric validation.

## Findings from the manual walk (2026-09-17)

Recorded per Constitution › Manual Scenario Sign-Off; per-scenario results
are on T096 in [tasks.md](tasks.md), harness notes in
[quickstart.md](quickstart.md).

**R15 amendment — "`Tab` reaches them" was necessary but not sufficient.**
The glyphs are `Sense::click_and_drag()`, which is `CLICK | FOCUSABLE |
DRAG`, so egui does put them in the tab order and `Tab` genuinely lands on
them — R15 is correct as far as it goes. But the focused-marker key table
(`handle_focused_marker_keys`) is gated on `WaveformState::focused_marker`,
and that field was written only from `clicked()`/`drag_started()`. A
keyboard-only user could therefore focus a glyph and still not nudge,
rename, recolour or delete it, silently failing FR-022 and M15.
`markers::lane` now also adopts the marker when its glyph `has_focus()`,
so keyboard focus and pointer focus are equivalent. Pinned by
`markers.rs::tab_focus_on_a_glyph_enables_the_marker_key_table`, which
drives focus through egui rather than presetting `focused_marker` (the
pre-existing nudge test preset it, which is why the gap survived).

**New open verification — output-device change breaks the source feed
(003, blocks M16).** Rebuilding the output stream re-`attach()`es the
source, allocating a fresh sample ring, but `ConnectSource::
handle_initialize` early-returns while the worker is alive, so the worker's
`RingSink` keeps writing into the orphaned previous ring while the new
`ConnectRtSource` reads one nothing feeds: audio stops and the published
position freezes. 006 only surfaces this (M16 is the first scenario to
switch devices mid-playback); the loop model and engine state survive the
rebuild correctly, and `stream_rebuild_repushes_armed_region` still holds.
Fixing it means a receiver-side rewire — hand the running worker the new
producer on re-attach (which implies rebuilding the sink and `Player`,
whose producer is move-only and built once), or keep the ring alive across
stream rebuilds. Out of 006's scope; M16 stays failing until it lands.
