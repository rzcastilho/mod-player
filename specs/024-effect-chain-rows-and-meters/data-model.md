# Data Model: Effect Chain Rows and Meters (024)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md)

This feature is presentation-only (FR-014). It adds **no** persisted data,
**no** engine/`RtShared` fields, and **no** change to `ChainView`,
`NodeRow`, `MeterSnapshot`, `LevelPair` or any `modplayer-core` /
`modplayer-effects` type. The "entities" below are (a) the existing data
the panel reads, restated so the zones can be mapped onto it, (b) the
per-viewer egui temp-memory keys this feature adds, and (c) the pure
presentation helpers whose inputs/outputs are unit-tested.

---

## 1. Existing read-only inputs (unchanged, owned by 008)

| Source (`modplayer-core`) | Field | Used by zone / widget |
|---|---|---|
| `NodeRow` | `id: NodeId` | handle id, drop target, zone-width memory key |
| | `index: usize` (0-based) | identity: position `index + 1`; handle name `$position` |
| | `kind: NodeKind` | identity: `tr(kind.label_key())`; handle name `$kind`; parameters dispatch |
| | `owner: NodeOwner` | identity: `effects-owner-host` / `effects-owner-plugin` |
| | `bypassed: bool` | state: `Bypass` toggle |
| | `auto_bypassed: bool` | state: "Auto-bypassed (over budget)" note (moved from params) |
| | `cost_pct: f32` | state: "{ $pct } % of budget" |
| | `mode_note: bool` | state: "Quality mode auto-switched" note (moved from params) |
| | `params: [f32; N]` | parameters: per-kind controls (unchanged ranges/suffixes) |
| `ChainView` | `nodes: Vec<NodeRow>` | row list; `is_empty()` selects the empty state |
| | `capacity: usize` | add row's "chain is full" refusal (unchanged) |
| | `total_cost_pct: f32` | header: "Chain CPU: { $pct } % of real-time budget" |
| | `overload_count: u32` | header: "Budget overruns: { $count }" |
| | `over_budget: bool` | header: over-budget badge (unchanged) |
| `MeterSnapshot` | `pre`, `post: LevelPair` | level pairs (linear peak/RMS L/R) |
| | `spectrum: [f32; 64]` | spectrum bars (linear magnitude 0..1+) |

Validation: none new — values are already clamped/validated by 008 and the
widgets clamp dB to `SCALE_MIN_DB..=SCALE_MAX_DB` before display.

## 2. Row zones (presentation structure)

```text
EffectNodeRow (one dnd_drop_zone, Frame::group)
├── Zone::Identity    handle "⠿" · "{index+1}." · kind label (truncate) · owner label (truncate)
├── Zone::State       Bypass toggle · "{pct} % of budget" (mono) · [auto-bypassed note] · [mode note]
├── Zone::Parameters  per-kind controls (unit suffix inside value box), mode/type/filter combos
└── Zone::Actions     destructive_gap · Remove (Variant::Destructive)
```

| Zone | Focus stops (in order) | Non-focusable content |
|---|---|---|
| Identity | drag handle (`Button` role, name "Reorder {kind}, position {n}") | position, kind, owner labels |
| State | Bypass toggle | budget figure, notes |
| Parameters | kind's controls in existing order | name labels |
| Actions | Remove | — |

Rules:
- Zones render in the order above (⇒ Tab order, research R11).
- A zone is atomic: it is never split across a line; the break rule is
  `zone_breaks` (§4.1).
- Parameters zone with no controls (future kind) still allocates its
  separator slot so the four-zone structure remains (Edge Cases).
- Unit-bearing controls keep their existing `.suffix()` (" st", " %",
  " dB", " Hz"); unitless ones (Q, resonance, width, balance) unchanged
  (FR-002).

## 3. Per-viewer egui temp memory (new)

All are `ctx.memory().data` **temp** entries (not persisted, reset on
restart), consistent with the existing `add_kind_memory_id`.

| Id | Type | Written when | Cleared when |
|---|---|---|---|
| `Id::new("effects-zone-width").with((node_id, zone_idx))` | `f32` | after each zone is drawn (its measured width) | row removed (stale entries are harmless; egui GC drops unused temp data) |
| `Id::new("now-playing-effect-chain-add-revealed")` | `bool` | empty-state primary button activated → `true` | chain has ≥ 1 node (removed) |
| `Id::new("now-playing-effect-chain-add-focus-pending")` | `bool` | same activation → `true` | the frame focus is requested on the kind combo, or chain non-empty |
| `add_kind_memory_id()` (existing) | `NodeKind` | unchanged | unchanged |

### Empty-state transitions

```text
          nodes.len() == 0                          nodes.len() >= 1
 ┌──────────────────────────┐  activate primary  ┌────────────────────────┐
 │ Collapsed                │ ─────────────────▶ │ Revealed               │
 │ explanation + [Add effect│                    │ explanation + combo +  │
 │ node] (Primary)          │                    │ [Add] (Primary), focus │
 └──────────────────────────┘                    │ on combo (one-shot)    │
            ▲                                    └───────────┬────────────┘
            │ last node removed (flags cleared while ≥1 node) │ Add → node added
            │                                                 ▼
            │                                   ┌────────────────────────┐
            └────────────────────────────────── │ Populated              │
                                                │ rows + ordinary        │
                                                │ "Add node…" row        │
                                                └────────────────────────┘
```

The swap happens in the frame `nodes.len()` changes (the view is rebuilt
at the top of `show`), so no stale state is drawn (Edge Cases).

## 4. Pure presentation helpers (new, unit-tested)

### 4.1 `zone_breaks`

`fn zone_breaks(available: f32, widths: [Option<f32>; 4], gap: f32) -> [bool; 4]`

- `true` at index *i* ⇒ call `ui.end_row()` before zone *i*.
- `widths[i] == None` (first frame) ⇒ Parameters (i = 2) breaks; others
  follow the fit rule assuming 0 width.
- Fit rule: running line width + `gap` + `widths[i]` > `available` ⇒
  break (index 0 never breaks).
- Invariant: a zone is never split; output depends only on inputs.

### 4.2 `format_db` (chain_meters, changed)

`fn format_db(db: f32) -> String` → always 8 chars:
`" -inf dB"`, `"-60.0 dB"`, `"-12.3 dB"`, `" -6.0 dB"`, `"  0.0 dB"`.
Precondition: `db` already clamped to −60..=0 or non-finite.

### 4.3 Spectrum axis model

| Constant | Value | Meaning |
|---|---|---|
| `SPECTRUM_MIN_HZ` / `SPECTRUM_MAX_HZ` | 20 / 20 000 | bars' existing mapping |
| `MAJOR_TICKS_HZ` | `[(100, "100"), (1_000, "1k"), (10_000, "10k")]` | labeled, labels via Fluent keys (§contracts) |
| `MINOR_TICKS_HZ` | `[50, 200, 500, 2_000, 5_000]` | unlabeled |
| `REFERENCE_DB` | `[0.0, -30.0, -60.0]` | horizontal lines, labeled in gutter |
| `SPECTRUM_FLOOR_DB` | −60 (existing) | bottom of plot |

- `fn freq_to_frac(hz: f32) -> f32 = log10(hz/20)/log10(1000)`, clamped 0..1.
  `freq_to_frac(100) ≈ 0.233`, `(1000) ≈ 0.566`, `(10000) ≈ 0.900`.
- `fn db_to_frac(db) = (db − (−60)) / 60` clamped (inverse of the bar
  height rule): 0 dB → 1.0, −30 → 0.5, −60 → 0.0.
- `fn tick_label_spans(plot_width, label_widths: [f32; 3]) -> [(f32, f32); 3]`
  — centred on `freq_to_frac * plot_width`, clamped inside `[0, plot_width]`.
  Invariant (tested for `plot_width` from the 160 px outer minimum up to
  420 px): spans don't overlap each other and lie within the plot.

### 4.4 `spectrum_segments`

`fn spectrum_segments(value: f32) -> ([(Band, f32, f32); 3], usize)` —
fractions of plot height, no allocation.

| `value` (linear) | dBFS | Segments |
|---|---|---|
| ≤ 0.001 | ≤ −60 | none |
| 0.1 | −20 | positive 0→0.667 |
| 0.7 | ≈ −3.1 | positive 0→0.9, warning 0.9→0.948 |
| ≥ 1.0 | ≥ 0 | positive 0→0.9, warning 0.9→1.0, danger cap (top 3 px, converted to a fraction by the caller) |

Boundaries reuse `controls::BAND_WARNING_DB` (−6) and `SCALE_MAX_DB` (0).

## 5. Fluent keys

See [contracts/fluent-strings.md](./contracts/fluent-strings.md) for the
full key list (changed values + new keys).
