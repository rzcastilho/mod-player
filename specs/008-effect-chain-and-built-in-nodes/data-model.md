# Data Model: Effect Chain and Built-In Effect Nodes

**Feature**: 008-effect-chain-and-built-in-nodes | **Date**: 2026-09-18 |
**Spec**: [spec.md](spec.md) (Key Entities, FR-001–FR-018) | **Research**: [research.md](research.md)

Four layers, one direction of truth: the **catalog** (§1, pure, shared)
defines what a node *is*; the **core model** (§2) is the controller's
shadow state — the only authority on ids, order and parameter values;
the **real-time copy** (§3) is derived from it by slot index through
`Command`s and publishes numbers back through `RtShared` (§4); the **UI
state** (§5) is per-session view state. Nothing here is persisted (FR-002:
session-scoped, empty at launch).

Units: continuous parameters are `f32` in their natural unit (semitones,
tempo ratio, dB, Hz, Q, normalised 0–1); discrete parameters travel as
`f32` too (`0.0`/`1.0`/`2.0`) so one `Command` variant carries every
parameter. Frames are at the source sample rate everywhere.

---

## 1. Catalog — `modplayer_effects::catalog` (pure, no state)

### 1.1 `NodeKind`

```rust
#[derive(Copy, Eq, Hash)] #[repr(u8)]
pub enum NodeKind { PitchShift = 0, TimeStretch = 1, Gain = 2, Equalizer = 3, Filter = 4, StereoTools = 5 }
// NodeKind::ALL, NodeKind::label_key() -> "effects-kind-pitch-shift" …
```

Exactly the six FR-006 types; the "Add node…" control enumerates `ALL`.

### 1.2 `NodeOwner` (DM-17 owner)

```rust
#[derive(Copy, Eq)]
pub enum NodeOwner { Host, Plugin(PluginId) }   // PluginId(u16), reserved for 009
```

Every node created by this slice is `Host`. `Plugin(_)` is constructible
(engine tests use it) but no controller path creates one. `is_host()`
gates FR-012's auto-bypass.

### 1.3 `ParamId` and `ParamDef`

```rust
#[derive(Copy, Eq)] pub struct ParamId(pub u8);   // per-kind index, see table
pub enum ParamShape { Continuous { min: f32, max: f32, nyquist_clamped: bool }, Discrete { count: u8 } }
pub struct ParamDef { pub id: ParamId, pub key: &'static str, pub shape: ParamShape, pub default: f32, pub unit: Unit }
pub fn params(kind: NodeKind) -> &'static [ParamDef];
pub fn clamp(kind: NodeKind, id: ParamId, value: f32, source_rate: u32) -> f32;   // NaN → default
pub fn is_continuous(kind, id) -> bool;   // continuous → 20 ms ramp; discrete → 5 ms crossfade
```

| Kind | `ParamId` | key | shape | default | unit / display |
|---|---|---|---|---|---|
| PitchShift | 0 `semitones` | `effects-param-semitones` | cont −12..=+12 | 0 | st, 0.01 step |
| | 1 `formant` | `effects-param-formant` | disc 2 (off/on) | 0 | toggle |
| | 2 `mode` | `effects-param-mode` | disc 2 (performance/quality) | 0 | combo |
| TimeStretch | 0 `ratio` | `effects-param-ratio` | cont 0.25..=2.0 | 1.0 | shown as `25%..=200%` |
| | 1 `mode` | `effects-param-mode` | disc 2 | 0 | combo |
| Gain | 0 `level_db` | `effects-param-level` | cont −60..=+12 | 0 | dB |
| | 1 `mute` | `effects-param-mute` | disc 2 | 0 | toggle |
| Equalizer | `16 + 4·b + 0` `freq[b]` (b = 0..8) | `effects-param-band-freq` | cont 20..=20 000, nyquist_clamped | 63·2^b Hz | Hz |
| | `16 + 4·b + 1` `gain[b]` | `effects-param-band-gain` | cont −24..=+24 | 0 | dB |
| | `16 + 4·b + 2` `q[b]` | `effects-param-band-q` | cont 0.1..=10 | 1.0 | Q |
| | `16 + 4·b + 3` `type[b]` | `effects-param-band-type` | disc 3 (peak/low-shelf/high-shelf) | 0 | combo |
| Filter | 0 `mode` | `effects-param-filter-mode` | disc 2 (high-pass/low-pass) | 0 | combo |
| | 1 `cutoff` | `effects-param-cutoff` | cont 20..=20 000, nyquist_clamped | 20 | Hz |
| | 2 `resonance` | `effects-param-resonance` | cont 0..=1 | 0 | 0–1 (`Q_MAX = 8` at 1.0, research R12) |
| StereoTools | 0 `width` | `effects-param-width` | cont 0..=2 | 1.0 | × |
| | 1 `balance` | `effects-param-balance` | cont −1..=+1 | 0 | L/R |
| | 2 `mono_sum` | `effects-param-mono-sum` | disc 2 | 0 | toggle |
| | 3 `phase_invert` | `effects-param-phase-invert` | disc 2 | 0 | toggle (effective only while `mono_sum` = 1) |
| | 4 `channel_swap` | `effects-param-channel-swap` | disc 2 | 0 | toggle |

`nyquist_clamped`: effective max = `min(max, 0.45 × source_rate)` (FR-006,
AS 3.4b). Every default is transparent (SC-013). The EQ band ids start at
16 so the single `u8` never collides with a future per-node parameter.

### 1.4 Auto-switch rule (FR-008) — pure

```rust
pub enum QualityMode { Performance = 0, Quality = 1 }
pub struct ModeState { pub mode: QualityMode, pub auto_switched: bool }
/// In stage-use range: |semitones| <= 3 for PitchShift, 0.5 <= ratio <= 1.5 for TimeStretch.
pub fn stage_use(kind: NodeKind, value: f32) -> bool;
/// Rule (1)/(2) of FR-008, applied on every semitones/ratio change.
pub fn mode_after_value_change(kind, value: f32, state: ModeState) -> ModeState;
/// Rule (3): an explicit user mode set clears the flag.
pub fn mode_after_user_set(mode: QualityMode) -> ModeState;
```

### 1.5 Constants (`modplayer_effects::consts`)

| Constant | Value | Source |
|---|---|---|
| `MAX_NODES` | 16 | FR-002 (≥ 16), research R2 |
| `PARAM_RAMP_MS` | 20 | FR-010 |
| `SWITCH_CROSSFADE_MS` | 5 | FR-005 |
| `SPECTRUM_BANDS` | 64 | FR-011 |
| `SPECTRUM_FFT` | 1 024, hop 256 | research R9 |
| `COST_WINDOW_SECS` | 1.0 (ring cap 1 024 callbacks) | FR-012a |
| `OVERLOAD_PCT` / `OVERLOAD_STREAK` | 90 / 3 | FR-012 |
| `TEMPO_STEP` | 0.10 | FR-017 |
| `Q_BUTTER`, `Q_MAX` | 0.7071, 8.0 | research R12 |
| `STRETCH_PERF` / `STRETCH_QUALITY` | frame 20/40 ms, hop 10/20 ms, search 5/10 ms | research R4 |
| `LPC_ORDER` | 16 | research R4 |
| `CHAIN_BUS_FRAMES` | `4·MAX_FRAMES + 2·frame_max` | research R3 |

---

## 2. Core model — `modplayer_core::effects` (controller shadow state)

### 2.1 `NodeId`

`#[derive(Copy, Eq, Hash, Ord)] pub struct NodeId(u32)` — monotonic per
session, never reused (a removed node's id is dead forever, so a stale
UI reference can never hit a re-used slot).

### 2.2 `NodeModel` (DM-17 EffectNode)

| Field | Type | Rules |
|---|---|---|
| `id` | `NodeId` | unique |
| `slot` | `u8` | `0..MAX_NODES`; the RT slot this node occupies; freed on remove |
| `kind` | `NodeKind` | fixed at add |
| `owner` | `NodeOwner` | `Host` for every node created here |
| `params` | `Vec<f32>` (len = `params(kind).len()`, indexed by `ParamId` position) | always clamped at the *current* source rate; re-clamped on rate change |
| `bypassed` | `bool` | user toggle |
| `auto_bypassed` | `bool` | set only from `Event::AutoBypassed`; cleared by a user un-bypass |
| `mode_state` | `Option<ModeState>` | `Some` for PitchShift/TimeStretch only (FR-008) |
| `orphaned` | `bool` | always `false` here (reserved, DM-17) |

Position in the chain = index in `ChainModel::order`.

### 2.3 `ChainModel` (DM-18 EffectChain)

| Field | Type | Rules |
|---|---|---|
| `nodes` | `Vec<NodeModel>` in processing order | `len() <= MAX_NODES` |
| `free_slots` | bit set of 16 | slot allocator |
| `next_id` | `u32` | |
| `source_rate` | `u32` | for Nyquist clamps; updated on attach |

Operations (all pure; each returns the `Command`s to push, so the
controller is a thin adapter and the model is fully unit-testable):

| Op | Result | Commands emitted |
|---|---|---|
| `add(kind, owner)` | `Ok(NodeId)` or `Err(ChainError::Full)` | `ChainInsert{slot, position: len, kind, owner}` then one `ChainSetParam` per non-default… none needed (defaults are the RT's defaults too) |
| `remove(id)` | `Ok(())`/`Err(UnknownNode)` | `ChainRemove{slot}` |
| `move_to(id, index)` | `Ok(())` | `ChainMove{slot, position}` |
| `move_by(id, ±1)` | `Ok(())` (no-op at the ends) | `ChainMove` |
| `set_bypass(id, on)` | `Ok(())`; clears `auto_bypassed` when `on == false` | `ChainSetBypass{slot, bypassed}` |
| `set_param(id, ParamId, requested)` | `Ok(clamped: f32)` — **the clamped value**, SC-005; runs the FR-008 rule and may emit a second command for `mode` | `ChainSetParam{slot, param, value}` (+ `ChainSetParam{…mode…}`) |
| `set_mode(id, QualityMode)` | `Ok(())`; `mode_after_user_set` | `ChainSetParam{…mode…}` |
| `set_source_rate(rate)` | re-clamps every `nyquist_clamped` param; returns the changed `(NodeId, ParamId, f32)`s | `ChainSetParam` for each changed value |
| `first_time_stretch()` | `Option<NodeId>` (chain order, bypass ignored) | — |
| `replay()` | every command needed to rebuild the RT copy from scratch (research R11) | `ChainInsert`/`ChainSetParam`/`ChainSetBypass` per node |
| `costliest()` | `Option<NodeId>` by last published cost | — |

`ChainError { Full, UnknownNode, WrongKind }` — `thiserror`, ordinary
`Err` values (never panics).

### 2.4 `ChainView` (projection for the UI, built per frame)

```rust
pub struct ChainView { pub nodes: Vec<NodeRow>, pub total_cost_pct: f32, pub over_budget: bool, pub overload_count: u32, pub capacity: usize }
pub struct NodeRow { pub id: NodeId, pub index: usize, pub kind: NodeKind, pub owner: NodeOwner, pub params: Vec<f32>,
                     pub bypassed: bool, pub auto_bypassed: bool, pub mode_note: bool, pub cost_pct: f32,
                     pub combined_with_neighbour: bool /* FR-007, for a tooltip only */ }
```

Costs come from `RtShared::node_cost[slot]`; everything else from the
model.

### 2.5 `MeterSnapshot` (Chain Meter Snapshot entity)

```rust
pub struct MeterSnapshot { pub pre: LevelPair, pub post: LevelPair, pub spectrum: [f32; 64], pub spectrum_generation: u32, pub advance_rate: f32 }
pub struct LevelPair { pub peak_l: f32, pub peak_r: f32, pub rms_l: f32, pub rms_r: f32 }   // linear amplitude
```

Read from `RtShared` by `PlaybackController::chain_meters()`; never
stored, never exposed beyond the UI.

### 2.6 Controller additions (`PlaybackController`)

| Field | Type | Purpose |
|---|---|---|
| `chain` | `ChainModel` | shadow state |
| `over_budget_notified` | `bool` | keyed `Warning` lifecycle (research R10) |
| `end_of_track_pending` | `bool` | research R8 rule 2 |
| `engine_ended_track_seq` | `Option<u32>` | swallow the Player's `EndOfTrack` after an engine-mirrored one |

---

## 3. Real-time copy — `modplayer_effects::rt::ChainRt` (owned by `Processor`)

### 3.1 `NodeSlot`

| Field | Type | Rules |
|---|---|---|
| `active` | `bool` | free slots are skipped |
| `kind` / `owner` | `NodeKind` / `NodeOwner` | from `ChainInsert` |
| `dsp` | `NodeDsp` | enum: `Gain(GainDsp)`, `Eq(Eq8Dsp)`, `Filter(FilterDsp)`, `Stereo(StereoDsp)`, `Stretch(StretchStage)`; every variant's buffers preallocated in the slot (`stretch_buffers: StretchBuffers` sized at construction, lent to the variant on activation) |
| `params` | `[Smoothed; MAX_PARAMS]` (`MAX_PARAMS = 48`) | current/target/step/remaining per `ParamId` |
| `mix` | `Crossfade` | dry/wet state for add/remove/bypass (research R6): `target ∈ {0, 1}`, `frames_left` |
| `bypassed`, `auto_bypassed` | `bool` | |
| `pending_move` | `Option<u8>` | reorder phase 2 target position (research R6) |
| `removing` | `bool` | free the slot when `mix` reaches 0 |
| `cost` | `CostRing` | `[f32; 1024]` ring + running sum + len `N` |

### 3.2 `ChainRt`

| Field | Type | Rules |
|---|---|---|
| `slots` | `[NodeSlot; MAX_NODES]` | |
| `order` | `[u8; MAX_NODES]` + `len` | slot indices in processing order |
| `bus_a`, `bus_b` | `Vec<f32>` (`CHAIN_BUS_FRAMES × 2`) | ping-pong (research R3) |
| `plan` | `[u32; MAX_NODES + 1]` | per-render input frame counts |
| `combined` | `[bool; MAX_NODES]` | slot `i` is the *second* member of an adjacent pitch/stretch pair this render (its own `StretchStage` idles; the first member's stage runs with the product parameters) |
| `spectrum` | `SpectrumRing` | 1 024-frame mono ring + FFT scratch + 64 bands |
| `render_pct_window` | `CostRing` | whole-render percentage (FR-012) |
| `over_90_streak`, `under_90_streak` | `u32` | overload state machine |
| `last_auto_bypass_at` | `Option<u64>` (callback counter) | ≤ 1 per window |

Invariants: `Σ plan == frames pulled`; every stretch ring occupancy ≤
capacity; `order` never references an inactive slot; a slot in
`removing` still processes until its fade ends.

### 3.3 `StretchStage`

| Field | Type |
|---|---|
| `ring` | input ring (frames, stereo), `write`, `read` cursors |
| `voices` | `[Voice; 2]` — `{ analysis_pos: f64, synth_phase: usize, tail: [f32; frame_max·2], lpc: LpcState, mode: PassThrough | Wsola(QualityMode), formant: bool }` |
| `active`, `fading_from` | voice indices + `Crossfade` |
| `stretch`, `step` | `Smoothed` products (research R4) |
| `latency_frames()` | `ring.occupancy − consumed_but_unrendered` (0 in pass-through) |

---

## 4. `RtShared` additions (atomics, all `Relaxed` gauges except the anchor seqlock)

| Field | Type | Written | Read |
|---|---|---|---|
| `anchor_advance_rate_bits` | `AtomicU32` (inside the seqlock) | every render | `PositionClock` (FR-001a) |
| `node_cost_bits[MAX_NODES]` | `[AtomicU32; 16]` | every render | UI per-node % |
| `chain_cost_bits` | `AtomicU32` | every render | UI whole-chain % |
| `render_pct_bits` | `AtomicU32` | every render | diagnostics |
| `overload_count` | `AtomicU32` | on overload event | UI counter (NFR-8.2) |
| `over_budget` | `AtomicBool` | set on event, cleared after a clean window | controller notification lifecycle |
| `pre_peak_l/r`, `pre_rms_l/r`, `post_peak_l/r`, `post_rms_l/r` | `AtomicU32` ×8 | every render | meters |
| `spectrum_bits[64]`, `spectrum_generation` | `[AtomicU32; 64]`, `AtomicU32` | every ≥ 256 post-chain frames | spectrum |

`AnchorSnapshot` gains `advance_rate: f32`. The existing `peak_bits`
(limiter output, 001) is unchanged.

---

## 5. UI state (per session, `modplayer-ui`)

| State | Where | Rules |
|---|---|---|
| panel open | egui temp memory `now-playing-effect-chain-open` | toggled by header button and `E`; survives track changes; not persisted |
| add-node selection | local `ComboBox` | resets after add |
| inline refusal | `Option<&'static str>` in `EffectsViewState` | `effects-chain-full`, cleared on next successful add |
| meter ballistics | `EffectsViewState { peak_hold: [f32; 4], rms_avg: [f32; 4], last_spectrum_gen }` | 300 ms RMS integration, 1 s peak hold — display only |
| dnd | egui `DragAndDrop` payload `NodeId` | drop index → `controller.chain_move_to` |
| focused handle | egui focus | `↑`/`↓` → `chain_move_by(±1)` via the handle's claim set |

---

## 6. Notifications (keys in `modplayer_core::notifications`)

| Key | Severity | Args | Lifecycle |
|---|---|---|---|
| `effect-chain-over-budget` | Warning | `$node` (kind label), `$owner` | raised on `Event::Overload` if not visible; re-used while `over_budget`; `dismiss_by_key` when `over_budget` turns false |
| `effect-chain-auto-bypassed` | Warning | `$node` | raised on `Event::AutoBypassed` (never in this slice's UI paths; engine test only) |
| `effects-no-time-stretch` | Info | — | raised by `tempo_step` if not visible (held-key coalescing) |

---

## 7. Action catalog delta (007 registry)

| Action | id | scope | repeat | enabled | default |
|---|---|---|---|---|---|
| `ToggleEffectChain` (new) | `host.nav.toggle_effect_chain` | NowPlaying | no | yes | `E` |
| `TempoStepUp` | unchanged | | yes | **yes** (was no) | `Equals`, `Plus` |
| `TempoStepDown` | unchanged | | yes | **yes** | `Minus` |

`CATALOG: [ActionDef; 45]`.

---

## 8. Engine `Command`/`Event` delta (full contract in [contracts/engine-effect-chain.md](contracts/engine-effect-chain.md))

```rust
Command::ChainInsert    { slot: u8, position: u8, kind: NodeKind, owner: NodeOwner }   // 6 bytes
Command::ChainRemove    { slot: u8 }
Command::ChainMove      { slot: u8, position: u8 }
Command::ChainSetBypass { slot: u8, bypassed: bool }
Command::ChainSetParam  { slot: u8, param: ParamId, value: f32 }                       // 8 bytes
Event::Overload       { costliest_slot: u8, render_pct: u16 }
Event::AutoBypassed   { slot: u8 }
```

`size_of::<Command>() <= 16` still holds (const-asserted).
