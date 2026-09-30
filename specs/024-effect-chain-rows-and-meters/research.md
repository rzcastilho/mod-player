# Research: Effect Chain Rows and Meters (024)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Date**: 2026-09-29

The spec's clarify pass fixed every product-level decision (strings, tick
frequencies, reference levels, band boundaries, empty-state flow). The
Technical Context therefore carried no open unknowns; the items
below resolve the *implementation* unknowns found by reading the current
code (`crates/modplayer-ui/src/effects_view.rs`,
`crates/modplayer-ui/src/widgets/chain_meters.rs`,
`locales/en-US/effects.ftl`, the `modplayer-ui` test suite) and egui 0.36.2's
source.

---

## R1 — How the four row zones are laid out inside `horizontal_wrapped`

**Decision**: Each zone is drawn as one atomic `ui.horizontal` (identity,
state, actions) or `ui.vertical`/`ui.horizontal` (parameters — equalizer
bands already stack vertically) inside the row's existing
`horizontal_wrapped`. Zones are separated by a vertical `ui.separator()`
plus `theme::space::LG` spacing, so the grouping is visible through both
a rule and whitespace (SC-001). Before drawing each zone the row asks a
pure helper, `zone_breaks(available_width, &[zone_widths]) -> [bool; 4]`,
whether that zone must start a new line; the widths come from the
previous frame's measured zone rects kept in egui temp memory keyed by
`(NodeId, zone)`. On the very first frame (no measurement yet) the
parameters zone starts its own line — exactly today's `end_row()`
behaviour — so a first frame never overshoots the column.

**Rationale**: egui's `horizontal_wrapped` can only wrap *before* a
widget whose size it can predict; an atomic child `horizontal` has no
size until drawn, which is why 008's manual walk (research R15) found
rows painting over the plugin dock and added the unconditional
`end_row()`. Remembering last frame's zone widths is the standard egui
answer ("measure then lay out"), keeps zones whole (FR-016: a zone never
splits), and makes the line-break rule a pure, unit-testable function.

**Alternatives considered**:
- *`egui::Grid` with four columns* — rejected: the parameters column
  (8 EQ bands vs. one slider) makes every row the height/width of the
  largest, and a grid cannot wrap columns at 960 px.
- *Always four lines, one zone per line* — rejected: wastes vertical
  space in the single scroll region (021) and reads as four rows, not
  one row with four zones.
- *Keep the unconditional `end_row()` only* — rejected as the whole rule,
  since at wide widths the actions zone would then always sit on the
  params line even if params wrapped; kept as the first-frame fallback.

## R2 — Visual separation between zones

**Decision**: `ui.separator()` (vertical, since the parent is a
horizontal layout) between adjacent zones on the same line, using the
existing separator stroke from the theme; no zone gets its own frame.
The actions zone is right-aligned when it shares a line with the
parameters zone (`ui.with_layout(Layout::right_to_left(Align::Center))`
inside the remaining width), and is always preceded by
`destructive_gap` (FR-005) even when it wraps to its own line.

**Rationale**: Separator + spacing is the lightest grouping that a user
can point to (US1 AS1) and matches 016's list-row conventions; nested
frames inside the row's `Frame::group` would read as cards-in-cards.

**Alternatives considered**: per-zone `Frame::group` (too heavy, halves
contrast of the row's own drop-zone frame highlight); spacing only
(fails SC-001 on dense rows like Stereo tools with five controls).

## R3 — Drag-handle affordance in egui 0.36

**Decision**: Replace the `"⋮"` glyph with `"⠿"` (U+283F braille
six-dot, renders as a two-column grip in egui's default font — already
used for no other control) drawn in `roles.text_secondary`.
`Ui::dnd_drag_source` already applies `on_hover_cursor(CursorIcon::Grab)`
(egui-0.36.2 `ui.rs` L2681); the handle additionally calls
`ctx.set_cursor_icon(CursorIcon::Grabbing)` while
`ctx.is_being_dragged(handle_id)` (the drag branch of
`dnd_drag_source` sets no cursor). Keyboard focus keeps using the app's
single `paint_focus_ring` pass — no view-local ring.

**Rationale**: Glyph visible without hover (US1 AS3 "independent of
hover"), cursor for pointer users, focus ring for keyboard users —
three independent carriers. Glyph availability must be verified by the
render test in the quickstart (if the default font lacks U+283F, fall
back to `"⋮⋮"`, recorded in the tasks).

**Alternatives considered**: painting six dots with `painter.circle_filled`
(pixel-perfect but adds a custom-painted widget whose size must be
allocated manually, for no user-visible gain); keeping `"⋮"` plus a
hover highlight (fails "independent of hover").

## R4 — Handle accessible name vs. existing test lookups

**Decision**: Add `effects-reorder-handle-node = Reorder { $kind }, position { $position }`
and use it as the handle's `WidgetInfo` label. Keep the existing
`effects-reorder-handle = Reorder` key, now used as the handle's hover
tooltip prefix and as the prefix that tests match
(`name.starts_with(&tr("effects-reorder-handle"))`). The three existing
exact-name lookups (`tests/effects_view.rs` L430,
`tests/accessibility.rs` L485, plus any `find_one(... "Reorder")`) move
to prefix matching or to the new node-specific name.

**Rationale**: NFR-6.2 needs distinguishable names when there are N
handles; keeping the old key avoids an orphan key in `fluent_keys.rs`'s
exhaustiveness check and keeps the "Reorder" prefix contract the spec
requires (FR-013).

**Alternatives considered**: adding arguments to the existing key
(breaks every existing `tr("effects-reorder-handle")` call — Fluent
would render the missing-arg placeholders into test expectations).

## R5 — Budget labels: rename keys or change values?

**Decision**: Keep the key *ids* `effects-chain-cpu`, `effects-overloads`
and `effects-cpu` and change their *values* to
"Chain CPU: { $pct } % of real-time budget",
"Budget overruns: { $count }" and "{ $pct } % of budget". Add
`effects-chain-cpu-hint` for the header hover text. Render each figure
through `theme::mono_text` with the percentage formatted as
`format!("{:>3.0}", pct)` so the digits do not shift between 5 % and
100 %.

**Rationale**: Every existing test builds its expectation with
`tr_args("effects-overloads", …)`/`tr_args("effects-cpu", …)`, so a
value change keeps those tests green while the user-facing text changes
(FR-006); `fluent_keys.rs`'s `EFFECTS_PCT_ARG_KEYS`/`EFFECTS_COUNT_ARG_KEYS`
remain valid. Whole-label `mono` follows the precedent in
`plugins_view.rs` L104 (`cpu_label` rendered wholly mono); mixing fonts
inside a translated sentence would force a `LayoutJob` and split the
Fluent message around the number, which breaks word order for
translators.

**Alternatives considered**: new key ids (`effects-chain-budget` …) —
churns ~10 test call-sites for no benefit; `LayoutJob` with only the
digits mono — rejected above.

## R6 — Fixed-width ("tabular") level readouts

**Decision**: `chain_meters::format_db` becomes
`format!("{db:>5.1} dB")` for finite values and `" -inf dB"` for
silence — every readout is exactly 8 characters (`" -inf dB"`,
`"-60.0 dB"`, `" -6.0 dB"`, `"  0.0 dB"`). Values are already clamped to
`SCALE_MIN_DB..=SCALE_MAX_DB` (−60..0) before formatting, so no wider
value exists. Labels stay `theme::mono_text`, so equal char count ⇒
equal pixel width.

**Rationale**: egui's default monospace font has no `tnum` feature
switch; fixed char count in a monospace face is the tabular-figure
equivalent. Directly unit-testable (clarification "same char count for
−∞, −60.0, −6.0, 0.0 dB").

**Alternatives considered**: `ui.add_sized` fixed-width labels —
depends on font metrics and zoom; padding with figure space U+2007 —
unnecessary in a monospace face.

`peak_meter::format_db` (the transport meter, "dBFS") is **not**
changed — out of scope and pinned by its own tests.

## R7 — Spectrum axes: geometry at the 160 px minimum

**Decision**: The widget keeps its outer size rule
(`available_width().clamp(160.0, 420.0)` wide) and grows in height by one
label strip. Inside it: a left **gutter** sized to the widest of the
three amplitude labels ("0 dB", "−30", "−60", measured with
`painter.layout_no_wrap` in the `mono` small font, ≈ 28–32 px), the
**plot** rect to the right of it (bars, reference lines, ticks), and a
**tick-label strip** below the plot (one small-font line). Frequency →
x uses the bars' own mapping, `x = plot.left + plot.width *
log10(f/20)/log10(1000)`; −30/−60/0 dBFS → y via `spectrum_bar_height`'s
inverse (0 dB = top, −30 = mid, −60 = bottom). Tick labels are centred on
their tick and clamped inside `[plot.left, plot.right]`.

At the minimum (160 px outer, ≈128 px plot): 100 Hz → 29.9 px, 1 kHz →
72.3 px, 10 kHz → 114.6 px; the widest label ("10k", 3 mono chars ≈
21 px at the small size) leaves ≥ 20 px between neighbours and the
"10k" label is clamped 3 px left to stay inside the plot. A pure helper
`tick_label_rects(plot_width, label_widths)` returns the laid-out spans
so the no-overlap rule is a unit test (SC-005, FR-008).

**Rationale**: Gutter + strip keeps labels off the bars in all cases
(clarification), reuses the existing log mapping so ticks and bars can
never disagree, and keeps the widget inside 021's column width rule.

**Alternatives considered**: labels overlaid in the plot's top corners
(overlap loud bars — rejected by clarification); right-hand gutter
(conventional axis side is left; both are equivalent for egui).

## R8 — Spectrum banding (positive / warning / danger)

**Decision**: A pure helper `spectrum_segments(value) -> SmallVec of
(Band, from_frac, to_frac)` splits each bar: `positive` from 0 to
`min(h, frac(−6 dBFS) = 0.9)`, `warning` from 0.9 to `min(h, 1.0)`, and —
because `spectrum_bar_height` clamps at 1.0 so a ≥ 0 dBFS bar has no
height *above* the 0 dB line — a band at or above 0 dBFS
(`value >= 1.0`) additionally paints a **danger cap**: the top
`controls::CEILING_MARK_WIDTH * 1.5` (3 px) of the bar in `danger`.
Colors come from `controls::band_color(roles, Band::…)`, the same
function `level_pair`/`peak_meter` use. The labeled 0 dB reference line
is drawn *after* the bars so it stays visible and marks the
danger boundary without color (NFR-6.4). Implemented as a fixed-size
array (max 3 segments), no allocation per bar.

**Rationale**: Mirrors `draw_meter_half`'s segmentation exactly, so the
spectrum and the level pairs obey one convention (FR-009); the cap is
the only way to show "≥ 0 dBFS" on a scale whose top *is* 0 dBFS
without changing the bar scale (FR-014).

**Alternatives considered**: extending the scale to +6 dBFS (changes
what the bars mean — violates FR-014 and the clarification "0 dBFS top");
coloring whole bars by their peak band (a −7 dB bar would be entirely
green and a −5 dB bar entirely amber — a visible jump that misreports
level).

## R9 — Empty state: reveal flow, focus, "one primary"

**Decision**: In `show`, when `view.nodes.is_empty()` draw
`show_empty_state` (explanation label + `button(ui, Variant::Primary,
tr("effects-empty-add"))`) instead of `show_add_row`. Temp-memory id
`now-playing-effect-chain-add-revealed: bool`. Activating the button
sets it `true` and a one-shot `…-focus-pending` flag; the next frame
draws the existing add row (combo + `Primary` "Add" confirm) and, if the
flag is set, calls `memory.request_focus` on the combo button's id, then
clears the flag. Whenever `view.nodes` is non-empty, both flags are
removed — so the next time the chain empties it starts collapsed. The
header figures, level pairs and spectrum render before the branch, so
they stay visible (FR-010). The non-empty add row keeps the default
"Add" button (FR-012), so exactly one `Primary` exists in the panel in
every state.

**Rationale**: Per-viewer convenience state belongs in egui temp memory
(the add-kind selection already lives there, `add_kind_memory_id`);
nothing else in the app needs it. Enter/Space activation comes free
from `button` (a `Button` responds to keyboard activation when focused).

**Alternatives considered**: skipping the two-step and showing
explanation + combo + primary "Add" together (the clarification chose
two-step to keep the dropdown out of first view); persisting the
revealed flag in `SettingsStore` (no product value; spec says per-viewer
temp memory).

**Test impact**: the existing tests that add nodes by clicking "Add" on
an empty chain (`seventeenth_add_is_refused_inline`,
`add_each_kind_appends_at_defaults_and_shows_zero_cost`, others that
start empty) must first activate "Add effect node" or add via
`controller.chain_add_node` in setup. This is a test-fixture change,
not a behaviour change (FR-011).

## R10 — Truncating kind/owner text

**Decision**: Kind and owner labels are `egui::Label::new(..).truncate()`
inside the identity zone, whose max width is capped at
`IDENTITY_TEXT_MAX_WIDTH = 180.0` px via `ui.set_max_width` in a scope.
egui 0.36's truncated `Label` shows the full text as a tooltip on hover
by itself (`Label::show_tooltip_when_elided`, default `true`).

**Rationale**: No custom tooltip code; FR-016's "ellipsis + full text on
hover" is the widget's built-in behaviour.

## R11 — Focus order identity → state → parameters → actions

**Decision**: No explicit focus-order API is used: egui's Tab order is
widget-creation order, so drawing zones in that order yields the
required order. A test tabs through a two-node chain and asserts the
sequence handle → bypass → first param … → remove → next row's handle.

**Rationale**: egui has no tab-index; creation order is the contract.
Moving the auto-bypassed / mode notes into the state zone does not add
focus stops (they are plain labels).

## R12 — Localization scope (pt-BR)

**Decision**: New and changed strings go into `locales/en-US/effects.ftl`
only. The repository ships no `pt-BR` locale directory and
`modplayer-core::i18n` embeds only `en-US` ("en-US is the only shipped
locale this slice"). Every string is an externalized Fluent key, so a
future pt-BR bundle is purely additive.

**Rationale**: Same resolution 023-markers-panel-structure recorded
(plan.md: "The strings are en-US only. pt-BR is project-level work
outside this feature"). Creating a one-file `pt-BR` locale here would
ship a locale the loader never selects — dead content that no test can
exercise.

**Alternatives considered**: add `locales/pt-BR/effects.ftl` anyway —
rejected (unused, untestable, and the rest of the app would be
un-translated). Recorded as a deviation from the spec's literal
"en-US and pt-BR" wording in plan.md Complexity Tracking.

## R13 — Performance of the added painting

**Decision**: Nothing new is measured beyond the existing UI budget. The
spectrum adds 3 reference lines, 8 tick marks and 6 small galleys (egui
caches galleys by text+font), and per-bar segmentation is ≤ 3
`rect_filled` per band (≤ 192 rects). Zone-width memory is 4 `f32`s per
row, ≤ 16 rows. All UI-thread; nothing touches `RtShared` beyond the
snapshot reads that already exist.

**Rationale**: Well inside a 60 fps frame; the real-time path is not
involved (Principle I).

## R14 — 021 C13 viewport test narrowed (implementation deviation, T042)

**Decision**: `now_playing.rs::reserved_height_keeps_volume_meter_and_queue_card_in_a_960x640_viewport`
now asserts the Queue card's *heading* stays inside the 960×640
viewport, rather than the whole Queue card rect. Its master-volume and
peak-meter assertions (the actual 2026-09-19 clipping defect C13 pins)
are unchanged.

**Rationale**: With an empty chain, this feature's empty-state
explanation (contract R4.2) and the spectrum's tick-label strip
(contract S5, +12 px) grow the Effect Chain card enough to push the
Queue card's bottom edge to y=655 (15 px past 640). Measured: dropping
the tick strip alone still overflowed by 3 px; dropping the explanation
alone passed. Since 021 (contract S1) every card sits in one scroll
region, so a card body running below the fold is reachable by scrolling
and is not the clipping defect C13 guards against. Any chain with ≥ 1
node already exceeded the old whole-card bound.

**Alternatives considered**: compact the design (spectrum plot 40→28 px
plus a smaller explanation text) to keep the test byte-for-byte —
rejected by the user: it trades the contracted legibility (S4–S6, R4.2)
for a pre-scroll-region layout guarantee.

## R15 — Manual walk deviations (T043, 2026-09-29)

- **M8 / contract R5 violated at 960×640**: the Stereo tools Parameters
  zone is one atomic `ui.horizontal` and is wider than the real centre
  column (~740 pt after the nav rail and card insets). It overflows past
  the card and window edge. `rows_fit_panel_at_960_px` renders
  `effects_view::show` alone across 960 pt, so it cannot catch this.
  Needs a fix: let an over-wide Parameters zone wrap internally, and
  re-test inside the full Now Playing layout.
- **R3 claim wrong**: egui's bundled font has no U+283F "⠿", nor the ↑/↓
  arrows used in the handle hint. Both render as tofu (the handle's
  tofu pre-dates 024). The R3 fallback, plus arrow-free or icon-drawn
  hint text, is needed.
- **Spectrum band colours (S2) not observable with real music**: bands
  peaked around −12 dBFS even with Gain +12 dB and a 0 dB post-chain
  peak. The unit tests remain the evidence.

**Resolution (same day)**: the M8 overflow is fixed by making the
Parameters zone `horizontal_wrapped`, and by making `switch` end the row
first when it would cross a wrapping row's edge. It is guarded by
`now_playing.rs::effect_rows_fit_the_real_centre_column_at_960x640`,
which lays out the real shell. The grip is now painted
(`effects_view::paint_grip`) instead of the U+283F glyph. The handle
hint says "Up or Down arrow key" in words, guarded by
`handle_hint_uses_only_glyphs_the_bundled_fonts_have`.
