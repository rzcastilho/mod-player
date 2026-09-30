# Contract: Chain Header Figures, Level Pairs and Spectrum (024)

Surfaces: `effects_view::show_header` and
`crates/modplayer-ui/src/widgets/chain_meters.rs` (`level_pair`,
`spectrum` — signatures unchanged:
`level_pair(ui, side_label_key, LevelPair)`, `spectrum(ui, &[f32])`).

## H — Header

- H1 Whole-chain figure: `tr_args("effects-chain-cpu", pct)` →
  "Chain CPU: { $pct } % of real-time budget", `pct = format!("{:>3.0}",
  total_cost_pct)`, rendered via `theme::mono_text`.
- H2 Hover text on H1: `tr("effects-chain-cpu-hint")`.
- H3 Overruns: `tr_args("effects-overloads", count)` → "Budget overruns:
  { $count }", mono.
- H4 Over-budget badge: unchanged key, text and condition.
- H5 Row figure (state zone): `tr_args("effects-cpu", pct)` →
  "{ $pct } % of budget", `{:>3.0}`, mono.
- H6 Idle chain: figures read "  0 %" / "0" and remain labeled.

## L — Level pairs

- L1 Readouts `"{peak}: {format_db}"` / `"{rms}: {format_db}"` via
  `theme::mono_text`; `format_db` output is always 8 characters
  (data-model §4.2), so readout width is constant from −∞ to 0 dB.
- L2 Bars, banding and −6/0 dB scale marks: unchanged (015/M9).
- L3 AccessKit: role `ProgressIndicator`, name starts with
  `tr(side_label_key)` (unchanged shape; values now padded).

## S — Spectrum

Layout inside the allocated rect (outer width
`available_width().clamp(160, 420)`):

```text
┌gutter┬──────────── plot ────────────┐
│ 0 dB │───────────────────────────── │ ← 0 dBFS line
│      │   ▌▌ ▌▌▌  bars  ▌             │
│ −30  │───────────────────────────── │ ← −30 dBFS line
│      │ ▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌   │
│ −60  │─┴──┴┴┴─┴──┴┴┴─┴──┴───────── │ ← −60 dBFS line + tick marks
│      │  100     1k      10k         │ ← tick-label strip
└──────┴──────────────────────────────┘
```

- S1 Bars: 64 bands, `x` and height mapping unchanged
  (`spectrum_bar_height`, 60 dB log scale, 20 Hz–20 kHz log x).
- S2 Bar colors: `spectrum_segments` — `positive` below −6 dBFS,
  `warning` −6..0 dBFS, `danger` 3 px cap when the band is ≥ 0 dBFS.
  Colors from `controls::band_color`. No `selection.bg_fill`.
- S3 Reference lines at 0, −30, −60 dBFS across the plot, stroke
  `controls::SCALE_MARK_WIDTH` in `mark_color(roles, false)`
  (`text.secondary`), drawn after the bars.
- S4 Gutter labels `tr("effects-spectrum-ref-0db")`, `…-ref-minus30`,
  `…-ref-minus60` ("0 dB", "−30", "−60"), `mono` small, right-aligned in
  the gutter, vertically centred on their line (clamped inside the rect).
- S5 Major ticks at 100 Hz, 1 kHz, 10 kHz (tick length 4 px) with labels
  `tr("effects-spectrum-tick-100")`, `…-1k`, `…-10k` in the strip below
  the plot; minor ticks (2 px) at 50, 200, 500, 2 k, 5 k Hz, unlabeled.
- S6 No label overlaps a bar, a line label, or another label, for any
  outer width in 160..=420 px.
- S7 Silence: bars absent; lines, ticks and labels still drawn.
- S8 AccessKit: unchanged — `ProgressIndicator`, name
  `"{effects-spectrum}, 64"`, value = peak band centre frequency (e.g.
  "1.2 kHz") or none when silent. Ticks/labels are painted, not separate
  nodes (their meaning is carried by the widget's name/value).
- S9 Painting only; no allocation per bar beyond egui's shape list.
