# Contract: `[now_playing_panels]` in `settings.toml`

**Feature**: 021-transport-bar-and-panel-layout | Requirements: FR-006; 016 FR-019/FR-020 | Model: [data-model.md](../data-model.md) §1, §2

This contract extends 016 `contracts/panel-card.md` P1–P7 with a fourth flag.
Every P-rule not restated here is unchanged.

## Wire shape

```toml
[now_playing_panels]
effect_chain_open = false   # absent → false (016)
transport_open    = false   # absent → false (016)
queue_open        = true    # absent → false (016)
markers_open      = true    # NEW — absent → true
```

**N1 — Additive.** An older file without `markers_open` loads with
`markers_open = true`. An older file with no section at all loads with Markers
open and the other three closed. `SCHEMA_VERSION` stays `1`.

**N2 — Independent.** Writing any one flag leaves the other three stored values
byte-identical, in both directions, for all four flags (016 P6 extended).

**N3 — One setter.** `PlaybackController::set_now_playing_panel_open(
NowPlayingPanel::Markers, open)` persists the flag immediately.
`now_playing_panel_open(NowPlayingPanel::Markers)` reads it from the shadow
state and never reads the disk (016 P2/P3).

**N4 — Every path writes through the setter.** The bar toggle, the card header
disclosure and the `Toggle*` host actions all call the setter. The UI crate
keeps no other mirror of the flags (016 P1).

**N5 — Round trip.** For all 16 combinations of the four bools, a
serialise → parse → `into_settings` round trip preserves the combination
(proptest, Constitution VIII "state serialisation").

## Test obligations

| ID | Assertion | Location |
|---|---|---|
| T-N1 | Absent key gives `markers_open` = true; absent section gives (true, false, false, false) | `crates/modplayer-core/src/settings/model.rs` unit |
| T-N2 | Independence over all four flags, collapse → restore and expand → restore | same |
| T-N3 | Controller set, then reload the store, then read back, for `Markers` | `crates/modplayer-core/src/controller.rs` unit (extends the existing all-variants test at about l.5158) |
| T-N5 | proptest round trip | `settings/model.rs` |
