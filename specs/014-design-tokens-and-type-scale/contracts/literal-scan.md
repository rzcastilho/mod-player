# Contract: The Literal Scan (FR-018, FR-018a)

**Feature**: 014-design-tokens-and-type-scale | **Gate**: runs under
`cargo test --workspace`, i.e. alongside `fmt`, `clippy`, `test` and
`deny` on ubuntu / macos / windows (Constitution VII quality gates)

The scan is what makes US1's contrast fix a property of the app rather
than of one screen: without it, the next view added is free to name its
own colour again (`contracts/ui-panels.md` A4, extended host-wide).

---

## S1 — Where it lives

`crates/modplayer-ui/tests/design_token_literals.rs`, a Rust integration
test. It walks, from `env!("CARGO_MANIFEST_DIR")`:

- `src/**/*.rs` (this crate)
- `../modplayer/src/**/*.rs` (the binary crate)

Rejected alternative: a `scripts/check-design-tokens.sh` beside
`scripts/check-license-headers.sh` — it would need a new CI step, run only
where bash runs, and could not share the exclusion list with the token
module (research R15).

## S2 — What counts as a hit

**Colour**
- `Color32::from_rgb`, `from_rgb_additive`, `from_rgba_premultiplied`,
  `from_rgba_unmultiplied`, `from_gray`, `from_black_alpha`,
  `from_white_alpha`, `from_additive_luminance`
- any `Color32::` **named constant** (`WHITE`, `BLACK`, `RED`, `GREEN`,
  `BLUE`, `YELLOW`, `GRAY`, `LIGHT_*`, `DARK_*`, `KHAKI`, `GOLD`,
  `BROWN`, `ORANGE`, `PLACEHOLDER`, `DEBUG_COLOR`) — `Color32::TRANSPARENT`
  is **not** a hit (it names no colour)
- `Rgba::from_*`, `Hsva::new`, `HsvaGamma`
- a literal `#rrggbb` / `#rrggbbaa` string, or a `0xNN, 0xNN, 0xNN` triple

**Font size**
- `FontId::new(`, `FontId::proportional(`, `FontId::monospace(`
- an assignment to a `.size` field of a `FontId`
- a `const`/`let` named `*FONT_SIZE*` / `*TEXT_SIZE*`

**Separator** (U3)
- `ui.separator()` / `Separator::default()` anywhere under
  `crates/modplayer-ui/src/**`

Radius and spacing literals are **not** scanned — `CornerRadius::from(2u8)`
and `add_space(16.0)` are caught by review and by the token constants being
the only sanctioned source; adding them to the scan would flag legitimate
geometry maths (`ARTWORK_SIZE * 0.15`) and produce false failures. FR-018
names colour and font-size literals only, and the scan implements exactly
that.

## S3 — Exclusions (exhaustive — no other exclusion may be added)

1. `crates/modplayer-ui/src/theme/**` — the token module (FR-001's
   sanctioned home, `contracts/ui-panels.md` A4's sanctioned exception).
2. Test code: any `tests/` directory, and any line inside a
   `#[cfg(test)] mod …` block.
3. `use` statements and doc comments (`///`, `//!`) — an import or a prose
   mention of `Color32` names no colour.

An implementer who needs a fourth exclusion has found a call site that
should be reading a token instead.

## S4 — Failure output

On any hit the test fails with one line per hit —
`path:line: <matched text> — move this into crates/modplayer-ui/src/theme/
and reference a role (FR-018)` — and a final count. A future PR that
regresses gets the file, the line and the fix, not "assertion failed".

## S5 — Baseline (verified in this worktree, 2026-09-22)

17 sites across 9 files must be gone when this feature ships (the full
inventory, with what each becomes, is [research.md](../research.md) R15 and
[data-model.md](../data-model.md) §6). `crates/modplayer/src/**` is already
clean; the scan guards it. The 10 sanctioned sites inside the token module
(8 `MARKER_PALETTE` entries + 2 `Color32::WHITE` in `paint_host_glyph`)
stay inside `theme/`, and the two whites are repointed at a role as part of
FR-016 even though the scan would not flag them.

## S6 — Named tests

- `design_token_literals::no_colour_or_font_literals_outside_theme` (T1)
- `design_token_literals::no_separator_between_panels` (U3)
- `design_token_literals::scan_actually_reaches_both_crates` — a
  self-check: the walker must find > 40 `.rs` files and must include a
  known path from each crate, so a broken path never yields a vacuous pass.
