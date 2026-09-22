# Implementation Plan: Design Tokens, Type Scale, and Spacing

**Branch**: `014-design-tokens-and-type-scale` | **Date**: 2026-09-22 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/014-design-tokens-and-type-scale/spec.md`

## Summary

Give ModPlayer one source of visual truth. `crates/modplayer-ui/src/theme.rs`
becomes a `theme/` directory module holding **six type roles**
(`display` 22 / `title` 17 / `section` 13-uppercase / `body` 14 /
`secondary` 13 / `mono` 13), a **six-step 4-px spacing scale**, a
**three-step radius scale**, and **ten semantic colour roles** with a light
and a dark value each. Those values are compiled into one `egui::Style` per
theme, installed with `Context::set_style_of` from `App::new` and again at
the top of every `App::update` — so a theme switch or a token change is
visible everywhere on the next frame, with zero code in any view (FR-002).
egui's five built-in text styles are all remapped onto roles, so a widget
that names no style still renders from the scale (FR-003a), and every
colour slot egui exposes is derived from a role rather than left at a
toolkit default (FR-010b).

The user-visible payload: light-theme secondary text goes from **2.96:1 to
6.75:1** against the page and **5.52:1** against a card — because the
defect was `Visuals::weak_text_alpha` multiplying the text colour, and the
fix is setting `weak_text_color` to the `text.secondary` token, which every
existing `ui.weak()` call site inherits untouched (R10). Row titles and
secondary lines separate on size *and* colour; every timestamp, dB, CPU and
duration moves to a monospace role whose digits share one advance width;
prose caps at 72 × the `body` `'0'` advance; panels separate by `xl` space
instead of hairline rules; and the 17 colour/font-size/radius literals
scattered across nine view files move into the token layer, guarded
afterwards by a source scan that fails the build.

Three values in the spec's own table could not ship as written, and the
spec's "floor governs, swatch adjusts" rule resolves all three (research
R12/R13/R14): `surface.raised` measures 1.09:1/1.11:1 against base and is
adjusted to `#e8e8ea` / `#26262c` (1.22:1); `text.disabled` clears 3:1 on
`surface.base` but **fails on `surface.raised`** (2.66:1 / 2.88:1) and is
adjusted to `#828283` / `#76767a`; and four `MARKER_PALETTE` entries are
re-toned into the 0.121–0.300 luminance band with hue and mutual
separation preserved (minimum pairwise hue gap 28°).

Two toolkit findings shape the plan. egui bundles a **single-weight** font
stack (Ubuntu-Light + Hack) and `FontId` has no weight axis, so FR-003's
degradation clause is not an edge case — it is the only path on every
platform, and "weight" is carried by size, uppercase and colour (R2/R3).
And `RichText::extra_letter_spacing` *does* exist in 0.36, so `section`'s
0.04 em tracking ships rather than being omitted (R5). Nothing touches the
real-time path, the plugin API, the locales, or any persisted data (R21).

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`; edition 2024) — unchanged from 001–013.

**Primary Dependencies**: existing only — egui/eframe **0.36.2** (+`accesskit`), `epaint`/`ecolor` 0.36.2 (transitive; `Color32::blend`, `gamma_multiply`, `Fonts::glyph_width`, `RichText::extra_letter_spacing`, `Context::set_style_of`, `egui::__run_test_ctx`), `fluent-templates` 0.15. **No new crate, no new dependency, no new feature flag, no dev-dependency** (Constitution X); `crates/modplayer-ui/Cargo.toml` is unchanged. Explicitly rejected: a system-font loader (`font-kit`/`fontdb`) and `egui_kittest` (research R2, R4).

**Storage**: none. No persisted value changes: `PaletteIndex` keying is preserved across the marker re-tone (FR-015), the settings file gains no field, and no migration exists to write. The token set is compile-time `const`/`static` data.

**Testing**: `cargo test --workspace` — new `crates/modplayer-ui/tests/{design_token_contrast.rs, design_token_literals.rs, design_token_roles.rs}` plus unit suites in `src/theme/{tokens,style,contrast}.rs`; the three existing `theme` tests (`marker_color_indexes_the_fixed_palette`, `overlay_color_maps_every_token`, `marker_palette_has_eight_distinct_colours`) must pass unchanged, as must `accessibility.rs`, `fluent_keys.rs` and every view suite (FR-019 forbids behaviour change). Font metrics are measured headlessly through `egui::__run_test_ctx` — no rendering harness and no new dev-dependency. Manual scenarios M1–M10 in [quickstart.md](quickstart.md), executed by the implementing agent (Governance › Manual Scenario Sign-Off), with `target/manual-walk/contrast.py` as pixel evidence. CI gates unchanged (fmt, clippy `-D warnings`, test, deny, licence headers, API-reference regeneration diff) on ubuntu / macos / windows.

**Target Platform**: Desktop macOS, Windows 10+, Linux. The token values and the font stack are **identical** on all three — egui's embedded default fonts, not platform fonts, precisely so metrics and the 72-character measure do not diverge (research R2, Constitution X).

**Project Type**: Desktop application — the 13-crate Cargo workspace is unchanged. One crate is modified (`crates/modplayer-ui`); `crates/modplayer` is only scanned, not edited.

**Performance Goals**: zero work on the audio thread — no engine or effects file is touched. Per-frame UI cost added: two `Arc<Style>` clones and two option writes in `theme::apply_tokens` (the `Style`s are built once into a `OnceLock`; `all_styles_mut`/`style_mut_of` were rejected because they `Arc::make_mut` a whole `Style`, including its `BTreeMap`, on every call — research R7). `body_measure` costs one `Fonts::glyph_width` lookup per prose block per frame. The 30 Hz repaint budget (`REPAINT_INTERVAL` 33 ms) is unaffected.

**Constraints**: Constitution I — no real-time path edit; PR note "N/A". II/III — no plugin-facing change at all; plugin panels inherit tokens through the `Style`/`Visuals` that `contracts/ui-panels.md` A4 already binds them to (FR-015b). VI — no credential, network, file or telemetry surface; no asset added. VII — `forbid(unsafe_code)` holds, no `unwrap`/`expect` outside tests, SPDX headers on the five new `theme/` files, public items documented. VIII — every NFR this feature promises (NFR-6.4 colour never sole carrier, NFR-6.5 contrast minimums) is pinned by an automated test, not a manual grep. IX — untriggered: `api/v1.toml` and `docs/plugin-api/v1.md` are byte-identical after this feature. X — no crate, trait or flag; identical on three platforms; every string stays externalized and naturally cased (`section` uppercases at draw time). **Out of scope** (FR-019/FR-020): layout restructuring, hover/focus/pressed states, list-row/tab/panel-card redesign, the high-contrast variant, any text-scale setting or reflow.

**Scale/Scope**: `src/theme/` ≈ 620 LOC (`tokens.rs` 220, `style.rs` 260, `contrast.rs` 70, `mod.rs` 70) + `markers.rs` moved ≈ 145 unchanged; ≈ 380 LOC of unit tests. View edits ≈ 260 LOC across 24 files (17 literal sites, 8 `ui.separator()` sites, ~30 role applications, ~15 measure caps). New integration tests ≈ 420 LOC. Values changed: 2 surfaces, 2 disabled roles, 4 palette entries. Locales: 0 keys. Plugin API: 0 changes. Dependencies: 0 changes.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | No | ✅ PASS | No file under `crates/modplayer-engine/` or `crates/modplayer-effects/` is modified; the feature is entirely `crates/modplayer-ui` presentation data. The tokens are `const`/`static` values read on the UI thread only; nothing enters a queue, an atomic or a buffer callback. 008's latency and loop-seam tests still gate unchanged. PR real-time note: "N/A — no real-time path changes." |
| II. Plugins Are Guests (non-negotiable) | No | N/A | No gateway, runtime, permission, budget or refusal path is touched. Plugin panels are affected only in the sense that they inherit the shared `Style`/`Visuals` they were already required to read (`contracts/ui-panels.md` A4); no plugin gains or loses a capability, and no plugin code runs differently. |
| III. Host Primitives, Plugin Behaviors | **Yes** (preserved) | ✅ PASS | The `MARKER_PALETTE` re-tone deliberately does **not** repoint the ratified `OverlayColor::Positive`/`Warning` → `MARKER_PALETTE[2]`/`[3]` mapping (`contracts/overlays-settings-notify.md` §1.2, O8) at the new `positive`/`warning` roles — FR-015a, scope boundary. Markers stay host primitives keyed by `PaletteIndex`; only the rendered hue moves. |
| IV. Audio Source Is Replaceable and Isolated | No | N/A | No source crate, trait or implementation is touched; every automated test here is a pure value/metric test needing no audio source at all. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | No | N/A | No sample data, buffer, cache or file path appears anywhere in this feature. `decoded_store_boundary.rs` is unchanged. |
| VI. Security and Privacy by Default | No | ✅ PASS | No credential, network call, file write, asset or telemetry surface. No font or image is bundled (research R2/R3 chose the already-embedded toolkit stack precisely to avoid adding a binary asset and its `cargo deny` licence surface). |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | No new crate (Constitution X's "why is an existing crate insufficient" question is answered in the Structure Decision); `#![forbid(unsafe_code)]` unaffected; no `unwrap`/`expect` outside tests — `radius::full` clamps rather than casting blindly, and `roles()` is total over `dark_mode`; SPDX headers on all five new `theme/` files (`scripts/check-license-headers.sh` gates it); public token items carry doc comments, and `contrast::ratio` carries a doc example that runs under `cargo test --doc`; `cargo deny` unaffected (zero dependency change). |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | The two NFRs this feature exists for are pinned by automated tests, not by review: **NFR-6.5** by `design_token_contrast.rs` (C1–C7: every text role × theme × surface, the raised-surface floor, `text.on-accent`, the status roles, the disabled composite as egui actually blends it, and all 8 palette entries against both bases — 40+ assertions), and **NFR-6.4** by the existing `accessibility.rs` and `plugins_view.rs` suites, which must still pass because every colour-coded state keeps its text label. FR-018's source rule is an automated scan (`design_token_literals.rs`) with a self-check against a vacuous pass, not a manual grep. Manual scenarios M1–M10 are executed by the implementing agent with pixel evidence (Governance). **Test-first (the MUST, in full)**: every test that pins a piece of public behaviour is written before the code that implements it — `design_token_contrast.rs`, `design_token_literals.rs`, `design_token_roles.rs` **and** the `#[cfg(test)]` suites inside `tokens.rs`/`style.rs` (tasks.md T004–T007) all land, red and asserting data-model.md's target values, before `theme::{contrast,tokens,style,mod}` and the view edits (T008–T015, Phases 4–7). Phase 2's first execution inverted that order; the deviation and its correction are recorded in Complexity Tracking below (Constitution VIII, FR-018b). |
| IX. One Plugin API Definition | No | ✅ PASS | **Untriggered by construction** (FR-015b): tokens arrive through the shared style, which A4 already mandates plugin panels read, so no request, event, DTO, permission or schema line changes. `api/v1.toml` and `docs/plugin-api/v1.md` must be byte-identical after this feature — the existing `api_reference.rs` regeneration-diff test in CI is the check. No written change request is required or included. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | No new trait, feature flag, crate or dependency; the `theme/` directory module replaces one file with five in the same crate. **Portability**: the font stack is egui's embedded default on all three platforms, chosen over platform fonts so text metrics, the 72-character measure and digit alignment are identical everywhere (research R2). **Externalized strings**: no Fluent key is added, removed or re-cased; `section` uppercases at draw time with Unicode default casing, correct for en-US and pt-BR (FR-003b, R6). **Keyboard/accessible names**: unchanged — FR-019 forbids interaction changes and the existing `accessibility.rs` suite gates it. **User override**: the user can still switch Light/Dark/System in Settings › Appearance in one action, and that switch now reaches every screen (SC-007). |
| Governance: area-maintainer sign-off | No | N/A | No change under `crates/modplayer-engine/`, `crates/modplayer-capability-gateway/` or `crates/modplayer-plugin-runtime/` (GOV-3.2), so no additional sign-off beyond normal review. Requirement ids (FR-, SC-, US-, NFR-, C-, GOV-) are referenced throughout spec, plan, contracts, data model and test names. |
| Governance: Manual Scenario Sign-Off | **Yes** | ⚠️ OPEN | [quickstart.md](quickstart.md) §3 defines M1–M10, executed by the implementing agent against the real build on macOS with `screencapture` + `contrast.py` evidence; each result (pass / deviation) is recorded on its `tasks.md` task, and any deviation is written back into quickstart.md/research.md. **Open on this host**: M1/M2 could not be executed — the sign-in launch gate hides every section that renders `text.secondary`, and the account session is revoked (quickstart.md § 4, Complexity Tracking below). US1's checkpoint is marked not reached; the feature cannot be signed off until the scenarios run on a host with a signed-in account. |

**Pre-Phase-0 result**: PASS (no violations).

**Post-Phase-1 re-check**: PASS. The design adds no crate, trait, flag,
dependency or `unsafe`; the real-time path, the plugin API and the persisted
data are untouched; the three value deviations from the spec's table
(`surface.raised`, `text.disabled`, four palette entries) are applications
of the spec's own floor-over-swatch rule, computed and recorded in
research.md R12–R14; and the one architectural liberty — a `theme/`
directory instead of a single `theme.rs` — is the option FR-001 explicitly
grants. Everything the re-check might have flagged is in Complexity
Tracking below.

**Implementation-phase re-check (2026-09-22)**: two deviations arose while
building and are recorded in Complexity Tracking — Constitution VIII's
test-first order (inverted in Phase 2's first pass, corrected in tasks.md)
and Governance › Manual Scenario Sign-Off (M1/M2 unexecuted, open). Every
other row stands as re-checked above.

## Project Structure

### Documentation (this feature)

```text
specs/014-design-tokens-and-type-scale/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: decisions R1–R21, verified against egui 0.36.2 and this worktree
├── data-model.md        # Phase 1: the token set — module surface, type scale, spacing, radius, colour roles, Visuals map, call-site model, validation rules
├── quickstart.md        # Phase 1: automated gates (named suites) + manual scenarios M1–M10
├── contracts/
│   ├── design-tokens.md # Rules T1–T11, A1–A9, C1–C7, U1–U6 with the test that pins each
│   └── literal-scan.md  # The FR-018a scan: location, patterns, exhaustive exclusions, baseline, failure output
├── checklists/          # (created by /speckit-checklist, if run)
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

Existing layout (001–013) is kept; `+` marks new files, `~` modified,
`→` moved.

```text
crates/
├── modplayer-ui/
│   ├── Cargo.toml                          (unchanged — no new dependency)
│   ├── src/
│   │   ├── theme.rs                        → split into src/theme/ (FR-001's directory option)
│   │   ├── theme/
│   │   │   ├── mod.rs                      + apply(), apply_tokens(), roles(), divider(), section_label(), body_measure(), re-exports
│   │   │   ├── tokens.rs                   + Roles{LIGHT,DARK}, space, radius, text, text_scale()
│   │   │   ├── style.rs                    + build_style(Theme) -> Style: text_styles, Spacing, Visuals (data-model.md §5.3)
│   │   │   ├── contrast.rs                 + ratio(), relative_luminance(), composite() — shared by module and tests
│   │   │   └── markers.rs                  + MARKER_PALETTE (4 entries re-toned), marker_color, overlay_color, paint_host_glyph (whites → role)
│   │   ├── app.rs                          ~ apply_tokens in App::new and first statement of App::update
│   │   ├── rows.rs                         ~ body/secondary roles; radius::MD for artwork
│   │   ├── now_playing.rs                  ~ display role; mono for position/duration; 3 separators → space::XL
│   │   ├── markers.rs                      ~ mono role; label colours → roles (3 whites, 1 FontId gone)
│   │   ├── plugins_view.rs                 ~ health dots → positive/warning/danger; mono for CPU/memory; add_space → space::LG
│   │   ├── plugin_overlays.rs              ~ LABEL_FONT_SIZE → secondary role; white → role
│   │   ├── waveform/paint.rs               ~ time labels → mono role
│   │   ├── widgets/{peak_meter,chain_meters,skeleton,initials}.rs ~ danger role, mono readouts, radius::SM/MD, initials sizing
│   │   ├── detail_view.rs, library_view.rs, search_view.rs, queue_view.rs, effects_view.rs, transport_view.rs,
│   │   │   welcome.rs, privacy_notice.rs, getting_started.rs, sign_in.rs, device_check.rs, shell.rs, notifications.rs,
│   │   │   plugin_panels.rs                ~ title/section/body/secondary/mono roles; body_measure on prose blocks
│   │   └── settings/{mod,account,controls,plugins,audio,playback,language,about,appearance}.rs
│   │                                        ~ section headers; 2 separators → space::XL, 3 → theme::divider; prose measure
│   └── tests/
│       ├── design_token_contrast.rs        + C1–C7 (FR-018b)
│       ├── design_token_literals.rs        + T1, U3 (FR-018a) — scans this crate's and modplayer's src/
│       ├── design_token_roles.rs           + U1, U2, U6, A2 (mono fields, measure, theme switch, per-frame apply)
│       └── {accessibility,fluent_keys,plugins_view,markers,rows,plugin_overlays,waveform,now_playing,…}.rs
│                                            (unchanged — they gate FR-019's "no behaviour change")
└── modplayer/src/**                        (unchanged — scanned by design_token_literals.rs as a regression guard)
```

**Structure Decision**: keep the single 13-crate Cargo workspace under
`crates/` with one crate per architectural component (Constitution VII) and
add **no crate**. The whole feature lives in `crates/modplayer-ui`, the crate
that already owns every pixel: `crates/modplayer-ui/src/theme.rs` — named by
FR-001 and by the source review's § 5 preamble as "the one token module", and
already the sanctioned home of `MARKER_PALETTE`/`overlay_color`/
`paint_host_glyph` under `contracts/ui-panels.md` A4 — is expanded in place
into the `crates/modplayer-ui/src/theme/` directory module that FR-001
explicitly permits, because one file cannot carry ten colour roles × two
themes, a seven-entry text-style map, a full `Visuals` construction per theme
and a WCAG implementation without becoming unreviewable. Its existing
residents move to `theme/markers.rs` and keep their public paths through
`mod.rs` re-exports, so no call site outside the module changes its imports
and the three existing `theme` tests carry over verbatim. The application
point is `crates/modplayer-ui/src/app.rs`, where `theme::apply` is already
called (`App::new:117`) — `apply_tokens` joins it there and at the top of
`App::update`, which is the only place that runs before every view on every
frame. The verification lives beside the other UI integration suites in
`crates/modplayer-ui/tests/`, from where it can reach both
`crates/modplayer-ui/src/**` and `crates/modplayer/src/**` for the FR-018a
scan without a CI-configuration change. `crates/modplayer` (the binary),
`crates/modplayer-capability-gateway`, `crates/modplayer-plugin-runtime`,
`crates/modplayer-engine`, `crates/modplayer-effects`, `crates/modplayer-core`,
`plugins/`, `locales/` and `docs/` are untouched; the workspace dependency
graph is unchanged.

## Design notes that tasks must respect

1. **Tests before the code they pin** (Constitution VIII): every suite —
   `design_token_contrast.rs`, `design_token_literals.rs`,
   `design_token_roles.rs` and the `#[cfg(test)]` modules in `tokens.rs`
   and `style.rs` — lands first, against the target values, so the module's
   red→green transition is observed and each view edit is observable as a
   literal count going down.
2. **Move before edit**: `theme.rs` → `theme/markers.rs` is a pure move in
   its own commit (the three existing tests must pass untouched) before any
   value changes. The re-tone of the four palette entries is a separate,
   reviewable change.
3. **One construction site**: `style::build_style(theme)` is the only
   function that writes a `Style`, `Visuals` or `Spacing` field. No view,
   and no other `theme/` file, mutates style state.
4. **`apply_tokens` is idempotent and cheap** (R7): build once into a
   `OnceLock<(Arc<Style>, Arc<Style>)>`, then two `set_style_of` calls.
   Never `all_styles_mut`/`style_mut_of` per frame — they `Arc::make_mut`
   the whole `Style`.
5. **The US1 fix is one line plus its token** (R10): `weak_text_color =
   Some(text_secondary)` and `weak_text_alpha = 1.0`. Do not chase
   `ui.weak()` call sites — they inherit. `overlay_color(Neutral, …)`
   inherits too.
6. **Floor over swatch, computed not guessed** (R12/R13/R14): the three
   adjusted value sets are data-model.md §5.1/§5.4 verbatim. If an
   implementer changes any value, the contrast test — not review — decides.
7. **`xl` is 24 px** (R19). US4 acceptance scenario 2's "(32px)" is a spec
   typo; the token name governs and FR-007/FR-008 both say `xl`.
8. **Panel separation vs in-panel rules** (R20): the five panel-level
   `ui.separator()` sites become `add_space(space::XL)`; the three
   in-panel group rules become `theme::divider(ui)`. No `ui.separator()`
   survives, and the scan enforces it (U3).
9. **`section` goes through `section_label`** (T6), never through a
   hand-uppercased string and never through a catalogue entry — `fluent_keys.rs`
   must stay green.
10. **The measure is a maximum** (U2): `set_max_width(available.min(measure))`,
    on prose blocks only — never on row titles, table cells, tooltips or
    notifications, which FR-006 excludes.
11. **Threshold logic is not this feature's** (U4): health dots and the peak
    meter keep their existing comparisons; only the colour they resolve to
    changes. Every colour-coded state keeps its text label (NFR-6.4).
12. **No geometry, no interaction** (A8, FR-019): `expansion`,
    `interaction`, `animation_time`, `striped`, `handle_shape`,
    `interact_size`, `slider_width` and friends keep egui's values. Giving
    `widgets.hovered`/`active`/`open` token *colours* is required by
    FR-010b and is not a new state.
13. **No plugin-facing change** (A9): if a task finds itself editing
    `api/v1.toml`, a gateway DTO or a bundled package, it is out of scope —
    the regeneration-diff test must show no diff.
14. **Radius helper, not a cast**: `radius::full(h)` rounds and clamps into
    `u8`; no `as u8` on an unclamped float anywhere (Constitution VII).
15. **Manual scenarios M1–M10 are executed by the implementing agent**
    (Governance); each deviation becomes a regression test.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. The headless assumptions, the spec deviations
forced by computation, and the plan-level decisions are recorded here for
traceability:

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| **Constitution VIII deviation (recorded, corrected) — Phase 2's first pass wrote the token module before the suites that pin it** | Commit `510aee5` landed `theme::{contrast,tokens,style,mod}` and the `app.rs` wiring (now tasks.md T008–T015) ahead of T004–T007, so the required red→green transition was never observed for those suites. Correction: tasks.md Phase 2 is re-ordered tests-first and carries the note on T016; the suites assert data-model.md §5.1/§5.4 values verbatim rather than the code's output, and T018 re-derives every contrast ratio independently, so the tests still pin the contract and not the implementation (Constitution VIII, FR-018b, NFR-6.5). | Leaving the order as executed and calling VIII satisfied — that re-reads the MUST as "some tests before the views", which is not what it says. Re-writing the module test-first from scratch — pure churn: the values are contract-derived, so a re-run would reproduce the same code with no new information. |
| **Governance › Manual Scenario Sign-Off deviation (open) — M1/M2 not executed on the 2026-09-22 host** | The shell's sign-in launch gate (`App::launch_step`) hides Library and Settings › Appearance, the host's account session is revoked, and completing OAuth requires the maintainer's live Spotify account in a browser — which an unattended agent must not drive on the user's behalf. `ui.weak()` (the `text.secondary` role) exists only in `rows.rs`, so a gated, empty Library leaves no pixel to sample (SC-001/SC-002). What *was* captured on the real build: `contrast.py` on the sign-in surface reads `bg=#ffffff text=#1c1c1e` — the light `surface.base`/`text.primary` tokens exactly. Recorded in quickstart.md § 4 and on tasks.md T019/T020; US1's Phase 3 checkpoint is marked **not reached**. | Declaring US1 verified on automated evidence alone — the scenarios gate completion, so that would be a silent redefinition of the rule. Faking a signed-in state or adding a demo-library mode to reach the rows — fabricated evidence, and a source change outside this feature's scope (FR-019). |
| **Spec deviation — `text.disabled` adjusted to `#828283` / `#76767a` (R13)** | Computed: the spec's `#8e8e93`/`#6c6c72` clear 3:1 on `surface.base` but measure **2.66:1 / 2.88:1** against the `surface.raised` values FR-013 forces — and FR-012 requires 3:1 "against both surfaces in both themes". The spec's own floor-over-swatch rule applies; the chosen values also equal egui's disabled composite (R11), so the explicit token and the toolkit's alpha path agree by construction. | Leaving the spec's values — ships a role that fails its own stated floor on every card. Exempting `surface.raised` — reopens the exact hole the "both surfaces" clarification closed. |
| **Spec deviation — `surface.raised` = `#e8e8ea` / `#26262c`, not `#eaeaec` / `#25252a` (R12)** | FR-013 names those approximations; they measure 1.201:1 / 1.205:1 — passing by 0.001 and 0.005, fragile to any future rounding or value tweak. The chosen pair measures 1.224:1 / 1.222:1 with the same elevation direction. | The spec's approximations — a floor that a rounding change can flip is not a guarantee. |
| **Spec deviation — verified literal inventory is 17 sites across a *different* 9 files (R15)** | The spec names `artwork.rs` and `plugin_assets.rs`; both are verified clean of colour, font-size and radius literals today. The three files the spec omits (`widgets/chain_meters.rs`, `widgets/skeleton.rs`, `rows.rs`) carry radius literals. The count (17) and the outcome (zero) are unchanged. | Taking the spec's file list on trust — tasks would hunt literals that do not exist and miss three files that do. |
| **Spec reading — `xl` = 24 px, against US4 AC2's "(32px)" (R19)** | FR-007 defines the scale (`xl` 24, `xxl` 32) and FR-008 names "the `xl` step"; two of three statements agree and the third is a parenthetical gloss. | Using 32 — contradicts the scale definition and would make `xl` and `xxl` the same value. |
| **`theme.rs` becomes the `theme/` directory module** | FR-001 explicitly permits it "if it outgrows one file", and it does: ten roles × two themes, a seven-entry style map, two full `Visuals` constructions and a WCAG implementation in one file would be unreviewable. | One file — reviewability; a `modplayer-tokens` crate — Constitution X (one consumer, no second crate justified). |
| **Font stack is egui's embedded default (Ubuntu-Light + Hack), not the OS UI font (R2)** | FR-003 says "the platform's default font family" and forbids bundling a typeface; egui bundles its stack already, and it is identical on macOS/Windows/Linux, which Constitution X requires and which the 72-character measure and digit alignment depend on. | `font-kit`/`fontdb` — a new dependency (Constitution X), per-platform metrics divergence, extra `cargo deny` licence surface. |
| **No bold text anywhere in the shipped app (R3)** | The embedded stack is single-weight and `FontId` has no weight axis; FR-003's fallback clause ("falls back to the single available weight … distinction carried by size and, for `section`, by uppercase") is therefore the only path. Emphasis is carried by size + `text.primary` vs `text.secondary` colour, which still satisfies US2's "size **and/or** weight". | Bundling a semibold face — FR-003 forbids it. Double-draw synthetic bold — no `TextStyle`-level hook, must be done per call site (re-creating FR-018's defect), and smears at 13 px. |
| **`apply_tokens` runs every frame rather than only on change (R7)** | FR-002 requires "applied once per frame to the whole application" and a next-frame guarantee after any token or theme change; an idempotent per-frame install makes that structural instead of call-site discipline. Cost is two `Arc` clones. | Apply-once-at-startup — any view that mutated style state, or a later token change, would persist silently. |
| **Per-theme `disabled_alpha` (0.55 light / 0.44 dark) (R11)** | egui renders disabled widgets by multiplying opacity, not by a colour slot, so the token and the α must agree or the two disabled paths disagree on screen. A single α cannot land both themes on their token. | One α for both — one theme's disabled text then misses its token (and, in light, the 3:1 floor on raised). |
| **Literal scan as a Rust test, not a shell script (R15)** | Runs under the existing `cargo test --workspace` gate on all three CI platforms with no YAML change, and shares the exclusion list with the token module. | `scripts/check-design-tokens.sh` (the licence-header precedent) — needs a new CI step and cannot share code. |
| **Scan covers colour and font-size literals only, not radius/spacing (contracts/literal-scan.md S2)** | FR-018/FR-018a name colour and font-size literals; radius/spacing patterns (`CornerRadius::from((size * 0.15) as u8)`) cannot be distinguished from legitimate geometry maths without false failures. | Scanning them too — false positives on `ARTWORK_SIZE * 0.15`-style geometry would push implementers toward `#[allow]`-style escapes, weakening the rule that matters. |
| **`MARKER_PALETTE[7]` (olive) left at a 3.05:1 thin pass (R14)** | FR-015 names exactly four failing entries and says indices 0, 1, 4 and 7 "are left alone"; 3.05:1 clears the floor as computed. The contrast test pins it, so any future drift fails CI. | Re-toning it anyway — an unrequested visual change to a marker colour users already have on screen. |
| **`paint_host_glyph`'s two `Color32::WHITE` uses repointed at a role** | They are inside the token module and so invisible to the scan, but a white mark on a light-theme warning triangle is the same defect FR-016 removes elsewhere ("the opaque white marker and overlay labels"). | Leaving them — the feature would ship a known unreadable mark that no test would catch. |
