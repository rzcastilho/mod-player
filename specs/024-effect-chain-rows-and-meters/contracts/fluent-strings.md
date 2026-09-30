# Contract: Fluent Strings (024)

File: `locales/en-US/effects.ftl` (the only shipped locale — research R12).
Every key below MUST be added to `crates/modplayer-ui/tests/fluent_keys.rs`
in the matching list (plain / `pct`-arg / `count`-arg / new
`kind`+`position`-arg list) so the exhaustiveness check keeps passing.

## Changed values (key ids unchanged)

| Key | Old value | New value |
|---|---|---|
| `effects-chain-cpu` | `Chain CPU load: { $pct } %` | `Chain CPU: { $pct } % of real-time budget` |
| `effects-overloads` | `Overloads: { $count }` | `Budget overruns: { $count }` |
| `effects-cpu` | `CPU { $pct } %` | `{ $pct } % of budget` |

## New keys

| Key | Args | en-US value |
|---|---|---|
| `effects-chain-cpu-hint` | — | `Share of each audio callback's time spent in the effect chain; an overrun is counted when it stays above 90 % or exceeds 100 %.` |
| `effects-reorder-handle-node` | `$kind`, `$position` | `Reorder { $kind }, position { $position }` |
| `effects-reorder-handle-hint` | — | `Drag to reorder, or focus and press the Up or Down arrow key` (words, not ↑/↓: egui's bundled fonts lack the arrows — T043 D2) |
| `effects-empty-explanation` | — | `Effect nodes process the audio on its way to your speakers — shift pitch, change tempo, shape tone or set level. Nodes run top to bottom in the order you add them.` |
| `effects-empty-add` | — | `Add effect node` |
| `effects-spectrum-tick-100` | — | `100` |
| `effects-spectrum-tick-1k` | — | `1k` |
| `effects-spectrum-tick-10k` | — | `10k` |
| `effects-spectrum-ref-0db` | — | `0 dB` |
| `effects-spectrum-ref-minus30` | — | `−30` |
| `effects-spectrum-ref-minus60` | — | `−60` |

## Kept unchanged (still used)

`effects-reorder-handle` (= `Reorder`, the name prefix), `effects-add`,
`effects-add-node`, `effects-bypass`, `effects-remove`,
`effects-auto-bypassed`, `effects-mode-note`, `effects-peak`,
`effects-rms`, `effects-pre`, `effects-post`, `effects-spectrum`, all
kind/param/mode keys.

## Rules

- No user-visible literal is added in Rust source; format-only glue
  (`"{}."`, padding) is not user text.
- `$pct` receives an already-formatted, right-padded integer string.
- pt-BR: not shipped by the app yet; adding a `pt-BR` bundle later is
  purely additive (every string above is a key).
