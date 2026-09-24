# Implementation Plan: High-Contrast Appearance Option

**Branch**: `feature/017-high-contrast-appearance` | **Date**: 2026-09-23 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/017-high-contrast-appearance/spec.md`

## Summary

Add a high-contrast appearance as a **second, independent axis** beside
the existing System/Light/Dark theme. Turning it on promotes secondary
text to the primary text colour, raises every border and divider from an
8 %-alpha hairline to full-alpha `text.primary`, thickens the focus ring
from 2 px to 3 px, and swaps `accent`/`positive`/`warning`/`danger` for
variants that clear 7:1 against both surfaces — while leaving marker
colours alone, because a marker's colour is data, and giving every
palette-coloured shape a 1 px outline instead so it stays distinguishable.
The choice persists beside `theme`, applies on the next frame, and reaches
docked plugin panels through the same applied style object the host reads.

The spec arrived fully clarified — sixteen decisions across two sessions,
no open markers — so Phase 0's question was not *what* to build but
whether the existing code can deliver it the way FR-017 demands: one
selection site, no call-site branching, no plugin-side change. Four
findings shaped the design, each read out of this worktree or the vendored
egui 0.36.2 source rather than assumed:

1. **The style cache holds exactly two entries, and its two production
   callers are outnumbered 12:1 by test callers** (research R1).
   `theme/mod.rs:50` is a `OnceLock<(Arc<Style>, Arc<Style>)>`; high
   contrast needs four styles. But `apply_tokens` has only **two**
   production call sites (`app.rs:125`, `app.rs:223`) against **25** in
   test code. So the cache widens to `[Arc<Style>; 4]` and a *new*
   `apply_tokens_for(ctx, high_contrast)` carries the flag, with
   `apply_tokens(ctx)` kept as a normal-mode wrapper. That is not
   politeness: the regression net proving "nothing changed outside high
   contrast" (FR-002, US1 AS-3) **is** those 25 suites passing unmodified,
   and an edited regression test proves nothing.

2. **Most colour call sites hold only `&Visuals`, so the axis has to
   travel inside `Visuals`** (research R2). `overlay_color` and
   `paint_host_glyph` sit on the *plugin* draw path; adding a `bool`
   parameter to them would put the flag at every call site, which is
   exactly what FR-017 forbids. Instead the axis is recovered from the
   applied visuals as FR-005 restated:
   `weak_text_color == Some(widgets.noninteractive.fg_stroke.color)` is
   true in high contrast and false otherwise — verified against all four
   tables *and* against a bare `Visuals::dark()` (whose `weak_text_color`
   is `None`, so the existing unit tests that pass one keep classifying as
   normal mode). Every existing signature is unchanged. The predicate's
   precondition is itself asserted, so a future feature that equalised a
   normal table's text roles fails loudly instead of silently enabling
   high contrast.

3. **A plain `bool` settings field would discard the entire settings
   file** (research R10). `store.rs:123-131` turns any serde error into
   `AudioSettings::default()` + `Unreadable`, and serde's derived
   `Deserialize for bool` errors on a non-boolean — so `high_contrast =
   yes` would reset theme, volume, device, keybindings and panel state.
   FR-003 asks for the opposite. The field is therefore `toml::Value`
   (already a dependency) validated late, exactly as `theme: String` is.
   That is the single highest-consequence finding in this plan and it has
   its own contract clause, **A5**.

4. **One of FR-011's four outline sites is already satisfied by FR-007**
   (research R7). The Markers panel row's colour swatch is an
   `egui::Button::new("").fill(color)`; `Button::fill` overrides only the
   frame's fill, and `Style::button_style` takes `frame.stroke` from
   `visuals.bg_stroke` (`widget_style.rs:158-166`), which `style.rs:188`
   sets to the divider. Raising the divider gives the swatch a 1 px
   `text.primary` outline with **no code at that call site** — the same
   colour and width FR-012 specifies, arrived at independently. It still
   gets a test, because a consequence nobody wrote down is a consequence
   somebody later deletes.

Two things are measured, recorded, and deliberately **not** gated, so
their absence reads as a decision: the outline's contrast against the fill
it encircles (2.73:1 at worst — no requirement states a floor, and raising
it needs the per-entry tuning FR-012 rules out), and a `pt-BR` catalogue
(`locales/` holds only `en-US`; a two-key second locale is worse than
none). Both are in Complexity Tracking.

All eight of FR-009's swatches were recomputed and land exactly on the
ratios the spec states. FR-010's conditional does **not** fire — today's
`text.on-accent` holds 8.65:1 against both high-contrast accents — so no
counterpart value is added. And dark `warning` genuinely already clears
7:1, which is why FR-009's table leaves that one cell unchanged; a test of
the form "every high-contrast role differs from its normal value" would be
wrong and must not be written.

Nothing touches the real-time path, the plugin API schema, the audio
source or any credential surface. One boolean is added to `settings.toml`
with `SCHEMA_VERSION` unchanged at `1`. Two Fluent keys. Zero new semantic
colour roles, crates, dependencies or feature flags.

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by
`rust-toolchain.toml`; edition 2024) — unchanged from 001–016.

**Primary Dependencies**: existing only — egui/eframe **0.36.2**
(+`accesskit` 0.24.1), `epaint`/`ecolor` 0.36.2 (transitive), `serde` +
`toml` (already `modplayer-core` dependencies, `Cargo.toml:20-22`),
`proptest` (already a dev-dependency). APIs relied on, each read in the
vendored source: `Context::set_style_of` / `style_of`,
`Visuals::weak_text_color` (an `Option<Color32>`, `None` by default),
`Color32::gamma_multiply`, `Stroke`, `Shape::convex_polygon`,
`Painter::rect_stroke` / `line_segment` / `text`,
`Button::fill`'s fill-only override (`widgets/button.rs:346-348`) and
`Style::button_style`'s `frame.stroke = visuals.bg_stroke`
(`widget_style.rs:158-166`), `egui::__run_test_ctx`, `Context::run_ui`.
**No new crate, no new dependency, no new feature flag, no new
dev-dependency** (Constitution X); no `Cargo.toml` changes anywhere.

**Storage**: one boolean added to the **existing** `[appearance]` table of
`settings.toml`, beside `theme`. Typed `toml::Value` in the raw struct and
validated in `into_settings` (research R10) so a malformed value recovers
to `false` with an `InvalidField` instead of discarding the file.
`SCHEMA_VERSION` stays `1` (`settings/model.rs:69`) — an added field with
a serde default inside an existing table is not a schema change, the
`[onboarding]` and `[now_playing_panels]` precedents. No migration; files
written before this feature load unchanged.

**Testing**: `cargo test --workspace`. One new integration suite
(`crates/modplayer-ui/tests/high_contrast.rs`), four extended
(`design_token_contrast.rs`, `markers.rs`, `plugin_overlays.rs`,
`fluent_keys.rs`), plus `#[cfg(test)]` additions in
`theme/{tokens,controls,markers,style,mod}.rs`,
`modplayer-core/src/settings/model.rs` and
`modplayer-core/tests/settings.rs` (the Constitution VIII proptest).
Everything is asserted from **values**, headlessly, through
`egui::__run_test_ctx` / `Context::run_ui` and `ClippedShape` inspection —
the route 014, 015 and 016 all used; no rendering or screenshot harness.
The regression net is the *unmodified* existing suites listed in
[quickstart.md](quickstart.md) § 2.3. Manual scenarios M1–M11 in
[quickstart.md](quickstart.md) § 3, executed by the implementing agent
(Governance › Manual Scenario Sign-Off). CI gates unchanged (fmt, clippy
`-D warnings`, test, deny, licence headers, API reference diff) on
ubuntu / macos / windows.

**Target Platform**: Desktop macOS, Windows 10+, Linux. Every value is
identical on all three — the four role tables, the two widths and the
persisted boolean are the same everywhere, and no platform API is
consulted (FR-014 forbids following an OS high-contrast preference, so
there is no per-platform branch to diverge).

**Project Type**: Desktop application — the 13-crate Cargo workspace is
unchanged. Two crates are modified (`crates/modplayer-ui`,
`crates/modplayer-core`) plus `locales/en-US/settings.ftl`.

**Performance Goals**: zero work on the audio thread — no engine or
effects file is touched. Per-frame UI cost added: **none in the steady
state.** `apply_tokens_for` is two `Arc` clones and two `set_style_of`
calls, identical in cost to today's `apply_tokens`; the four styles are
built once into a `OnceLock` and never rebuilt, so toggling the setting
allocates nothing. The outline paint adds at most one extra `Shape` per
palette-coloured marker actually on screen (bounded by the visible lane,
not by the track's marker count) and, for a plugin `Label`, four extra
text shapes — only while high contrast is on. The 30 Hz repaint budget
(`REPAINT_INTERVAL` 33 ms) is unaffected.

**Constraints**: Constitution I — no real-time path edit; PR note "N/A".
II — no gateway, runtime, permission or budget change. III — markers and
loop regions stay host primitives; only their outline changes. V — no
sample data anywhere. VI — no credential, network, asset or telemetry
surface; the one new persisted value is a boolean about colours. VII —
`forbid(unsafe_code)` holds; no new file, so no new SPDX header; no
`unwrap`/`expect` outside tests. VIII — the NFRs this feature promises
(NFR-6.5 contrast, NFR-6.2 roles/states, NFR-6.4 colour never the sole
carrier) are pinned by the new contrast block plus the existing suites
passing unmodified, and the new persisted field gets its proptest. IX —
untriggered: `api/v1.toml` and `docs/plugin-api/v1.md` byte-identical,
`OverlayColor` and `HostGlyph` gain no variant. X — no crate, trait or
flag; two externalized strings; identical on three platforms.
**Out of scope** (FR-014, Scope boundary): keyboard operability,
accessible names/roles/states, the 200 % text-scale requirement, any OS
high-contrast auto-follow, a fourth `Theme` value, `surface.base` /
`surface.raised`, the `MARKER_PALETTE` hues, and every component's layout
and interaction behaviour.

**Scale/Scope**: `theme/tokens.rs` ≈ 60 LOC added (two fields, two tables,
`for_theme`, `is_high_contrast`, one line in `divider_color_for`);
`theme/mod.rs` ≈ 25 LOC changed; `theme/style.rs` ≈ 10 LOC changed;
`theme/controls.rs` ≈ 15 LOC added; `theme/markers.rs` ≈ 60 LOC added
(four helpers + glyph casings); `src/markers.rs` ≈ 70 LOC changed (six
paint sites); `src/plugin_overlays.rs` ≈ 50 LOC changed (four arms);
`src/settings/appearance.rs` ≈ 25 LOC added; `app.rs` 2 lines;
`settings_registry.rs` 6 lines; `settings/model.rs` ≈ 40 LOC added;
`controller.rs` ≈ 20 LOC added; new/extended tests ≈ 700 LOC. New semantic
colour roles: **0**. New crates / dependencies / feature flags / traits:
**0**. Locale keys: **2**. Persisted fields: **1**. Plugin API changes:
**0**. `Theme` variants added: **0**.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | No | ✅ PASS | No file under `crates/modplayer-engine/`, `crates/modplayer-effects/` or `crates/modplayer-audio-io/` is modified (research R16). Nothing here allocates, locks or blocks on the audio thread: the whole feature is table selection, paint and one settings write on the UI thread. `apply_tokens_for` is allocation-free after its first call (contract S5), so even the toggle frame adds no allocation anywhere. PR real-time note: **"N/A — no real-time path changes."** |
| II. Plugins Are Guests (non-negotiable) | No | N/A | No Capability Gateway call, no plugin runtime invocation, no permission, budget, refusal path or manifest field is touched. A plugin gains and loses nothing. Toggling high contrast runs **no plugin code at all** — a docked panel simply re-reads the applied style on its next frame (contract P5), which is why FR-013's "zero plugin-side change" is structural here rather than a convention. |
| III. Host Primitives, Plugin Behaviors | **Yes** (preserved) | ✅ PASS | Markers, loop regions and cue points stay host primitives owned by `modplayer-core`; this feature changes only whether an outline is stroked around the colour the host already resolves for them. `MARKER_PALETTE` and the 011 `OverlayColor` → palette mapping are byte-identical (contracts H9, P1). No DSP, no scripting tier, no threshold logic moves. |
| IV. Audio Source Is Replaceable and Isolated | No | N/A | No source crate, trait or implementation is touched. Every automated test here is a value assertion over token tables, settings data or a headless frame's `Shape` list — no live Connect session is required by any of them, and the synthetic path the existing UI suites use is unchanged. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | No | N/A | No sample buffer, decoded frame, cache file, debug path or export surface appears anywhere in this feature. The only new persisted data is one boolean describing a colour preference. |
| VI. Security and Privacy by Default | No | ✅ PASS | No credential, network call, bundled asset, font, image or telemetry surface. `settings.toml` gains one boolean — no secret, nothing to scrub. Zero dependency change, so `cargo deny`'s licence and advisory surface is unchanged. The quickstart's live-token line is quoted from the constitution for completeness and is used by **no** scenario here. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | **No new file**, so `#![forbid(unsafe_code)]` and the SPDX header check are unaffected; no `unsafe` is added. **No `unwrap`/`expect` outside tests**: the malformed-value path is `toml::Value::as_bool()` → `Option`, handled with a `match` that pushes `InvalidField::HighContrast` (never a `panic`); `for_theme` is a total `match` over `(bool, bool)`; `overlay_outline` is a total `match` over the five `OverlayColor` variants; `marker_outline` returns `Option` via `bool::then`. **Public items carry doc comments**: `for_theme`, `is_high_contrast`, `marker_outline`, `overlay_outline`, `casing_width`, `focus_ring_width`, `apply_tokens_for`, the two `Roles` fields, `InvalidField::HighContrast`, and the controller pair. `is_high_contrast` and `focus_ring_width` each carry a runnable doc example (they need no `egui::Context`, only a `Visuals`/`Roles`), so `rtk cargo test -p modplayer-ui --doc` must report **new** doctests, not zero — mirroring the runnable example already on `theme::contrast::ratio` (`contrast.rs:27-32`). `cargo deny` unaffected (zero dependency change). |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | **NFR-6.5 (contrast)** is the feature's whole point and is machine-checked, never eyeballed: contract H17/H18/M3 assert 7:1 for four roles × two surfaces × two themes plus both text values, 3:1 for the divider, 4.5:1 for `text_on_accent` vs the high-contrast accent, and ≥7:1 for the marker outline against both surfaces — SC-008's "100 % of the pairs named in FR-018", through the existing `theme::contrast::ratio`. **NFR-6.2 (roles/states)** by `tests/accessibility.rs` passing **unmodified** plus A14's `Role::CheckBox` + `Toggled` assertion on the one new node. **NFR-6.4 (colour never the sole carrier)** by M8/M9 — a marker's identity survives on hue *plus* outline, and a focused glyph stays distinguishable by stroke width, not colour. **Property-based tests (the MUST for new serialized state)**: `high_contrast` is state serialization, so it gets a `proptest!` round-trip over an arbitrary `(Theme, bool)` pair in `crates/modplayer-core/tests/settings.rs` (contract A10), beside the existing `plugin_panels_round_trip_proptest` — the example-based A2–A4 cases do not discharge this principle on their own. No marker/loop arithmetic or manifest parsing changes, so no other proptest is owed. **Test-first (the MUST, in full)**: every suite lands red against data-model.md's values before the code. 014's Phase 2 inverted this once and had to record it; 015's and 016's plans both warned about it; `tasks.md` must sequence it correctly from the start. **Regression tests for the two lowest-visibility clauses** — A5 (a malformed field must not discard the file) and M7 (the swatch's outline is inherited, not written) — exist precisely because both are the kind of behaviour a later refactor silently removes. |
| IX. One Plugin API Definition | No | ✅ PASS | **Untriggered by construction**: no request, event, DTO, permission or schema line changes. `OverlayColor` gains no variant and `HostGlyph` gains none — `overlay_outline` is a *host-side* function keyed on the existing tokens, not a new API surface. `crates/modplayer-capability-gateway/api/v1.toml` and `docs/plugin-api/v1.md` must be byte-identical, and the existing `api_reference.rs` regeneration-diff test in CI is the check (contract P6). No written change request is required or included. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | **No new trait, crate, feature flag or dependency.** **YAGNI, concretely**: FR-010's conditional counterpart for `text_on_accent` is **not** added because research R5 measured it unnecessary (8.65:1); no fourth `Theme` variant; no OS-preference detection; no eleventh colour role for the divider (the existing alpha is raised instead); `apply_tokens` stays a thin wrapper rather than becoming a second mechanism; `overlay_outline` is keyed on the token rather than scanning the palette. **Portability**: identical values on all three platforms and, because FR-014 forbids following an OS preference, there is no platform branch at all — the one platform-specific thing here is the manual-walk recipe, which the constitution already scopes to macOS. **Externalized strings**: two new keys in `locales/en-US/settings.ftl` with `fluent_keys.rs`'s inventory extended; no Rust string literal labels the control. Only `en-US` exists in this repo (research R13) and no half-populated `pt-BR` is created — recorded in Complexity Tracking so the omission is deliberate. **User override**: high contrast *is* a user override, reversible in one action, and every existing one-action control keeps its behaviour — the user can still take transport focus back and disable any plugin in one action. |
| Governance: area-maintainer sign-off | No | N/A | No change under `crates/modplayer-engine/`, `crates/modplayer-capability-gateway/` or `crates/modplayer-plugin-runtime/` (GOV-3.2), so no additional sign-off beyond normal review. Requirement ids (FR-, SC-, US-, NFR-, C-, GOV-) are referenced throughout spec, plan, contracts, data model and test names. |
| Governance: Manual Scenario Sign-Off | **Yes** | ⚠️ PLANNED, PARTIALLY AT RISK | [quickstart.md](quickstart.md) § 3 defines M1–M11, executed by the implementing agent against the real build on macOS with `screencapture` **plus point-sampled pixel measurement** (this feature's claim is a measured one, so eyeballing a capture would not discharge it). Each result is recorded on its `tasks.md` task and any deviation written back into quickstart.md/research.md. **Known host risk**: 015 recorded M1–M10 *all* not executed — the process wedged inside `AccountService::launch_resolve_session` → `KeyringSecureStore::get` → `SecKeychainFindGenericPassword` before `eframe::run_native` was reached, so no window ever formed; 014 hit the sign-in gate one step later. **This feature is materially less exposed than 015 or 016**: M1, M2, M4, M5, M6, M9, M10 and M11 need only the Settings screen or any rendered chrome, so they are reachable whenever a window forms at all. Only M3 (four views), M7 (a loaded track with markers) and M8 (a docked plugin panel) sit behind the sign-in gate. If the block recurs, the required outcome is to record each blocked scenario **not executed** with the reason and mark the affected checkpoints **not reached** — never to sign off on automated evidence alone, never to fabricate a signed-in state, and never to modify the host's real Keychain to route around it (quickstart § 4). |

**Pre-Phase-0 result**: PASS (no violations).

**Post-Phase-1 re-check**: PASS. The design adds no crate, trait, flag,
dependency or `unsafe`; the real-time path, the plugin API schema, the
audio source and every credential surface are untouched. Phase 1 surfaced
five things the re-check would otherwise have flagged, all recorded in
Complexity Tracking: the two new fields on a struct the spec never
mentions (D1), the settings field's non-obvious type (D2), the
`apply_tokens` signature that is deliberately *not* changed (D3), a
measured ratio that is reported but not gated (D4), and a locale
catalogue that is deliberately not created (D5). None is a principle
violation; each is a place where the spec's letter and the code's reality
needed reconciling in writing rather than in a surprise during
implementation.

## Project Structure

### Documentation (this feature)

```text
specs/017-high-contrast-appearance/
├── plan.md                        # This file
├── spec.md                        # Feature specification (input, Clarified 2026-09-23)
├── research.md                    # Phase 0: decisions R1–R16, verified against
│                                  #   egui 0.36.2 and this worktree; all FR-009
│                                  #   ratios recomputed
├── data-model.md                  # Phase 1: the value set — the four role tables,
│                                  #   the persisted field and its recovery table,
│                                  #   the two widths, the outline-per-shape map,
│                                  #   full file map
├── quickstart.md                  # Phase 1: automated gates + the unmodified
│                                  #   regression net + manual scenarios M1–M11
├── contracts/
│   ├── high-contrast-tokens.md    # H1–H19: tables, promotion, divider, ring,
│   │                              #   the 7:1/3:1/4.5:1 floors, the no-literals gate
│   ├── marker-outline.md          # M1–M12 (outline), P1–P6 (plugin reach),
│   │                              #   S1–S5 (the single selection site)
│   └── appearance-setting.md      # A1–A22: persistence + recovery, independence,
│                                  #   the control, live apply, out-of-scope pins
├── checklists/                    # (populated by /speckit-checklist)
└── tasks.md                       # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

The existing layout (001–016) is kept. **No file is created and none is
deleted.** `~` marks modified, `=` marks unchanged-but-load-bearing as a
gate. Line numbers are from this worktree at commit `e773439`.

```text
crates/
├── modplayer-core/
│   ├── Cargo.toml                   = unchanged (toml/serde/proptest already present)
│   ├── src/
│   │   ├── settings/
│   │   │   ├── model.rs           ~ AudioSettings.high_contrast beside theme (:108);
│   │   │   │                        RawAppearance.high_contrast: toml::Value (:489)
│   │   │   │                        + default_high_contrast(); InvalidField::
│   │   │   │                        HighContrast + field_name() arm (:214-224);
│   │   │   │                        into_settings() recovery beside the theme
│   │   │   │                        match (:691); to_raw() emits Value::Boolean
│   │   │   │                        (:564); SCHEMA_VERSION unchanged at 1 (:69)
│   │   │   └── store.rs           = NOT modified — its total-discard path
│   │   │                            (:123-131) is the reason model.rs uses
│   │   │                            toml::Value (research R10)
│   │   ├── settings_registry.rs   ~ one SettingDescriptor after appearance.theme
│   │   │                            (:158-163)
│   │   └── controller.rs          ~ high_contrast shadow field beside theme
│   │                                (:453-456), populated at :808;
│   │                                high_contrast() / set_high_contrast()
│   └── tests/
│       └── settings.rs            ~ A10 — the (Theme, bool) proptest round-trip
│                                    beside plugin_panels_round_trip_proptest
│                                    (Constitution VIII; proptest already a dev-dep)
└── modplayer-ui/
    ├── Cargo.toml                   = unchanged — no new dependency
    ├── src/
    │   ├── theme/
    │   │   ├── tokens.rs          ~ Roles.divider_alpha + Roles.high_contrast;
    │   │   │                        LIGHT_HIGH_CONTRAST / DARK_HIGH_CONTRAST;
    │   │   │                        for_theme(); is_high_contrast(); roles()
    │   │   │                        (:65) now two-bit; divider_color_for() (:72)
    │   │   │                        one-line change
    │   │   ├── controls.rs        ~ FOCUS_RING_WIDTH_HIGH_CONTRAST;
    │   │   │                        focus_ring_width(); focus_ring() body (:98).
    │   │   │                        HOVER_ALPHA/PRESSED_ALPHA/FOCUS_RING_GAP
    │   │   │                        untouched (FR-020, H7/H8)
    │   │   ├── markers.rs         ~ MARKER_OUTLINE_WIDTH; marker_outline();
    │   │   │                        overlay_outline(); casing_width();
    │   │   │                        paint_host_glyph gains an outline parameter.
    │   │   │                        MARKER_PALETTE (:23-32) byte-identical
    │   │   ├── style.rs           ~ build_style(theme, high_contrast); exactly one
    │   │   │                        line inside visuals() changes (:98)
    │   │   ├── mod.rs             ~ STYLES -> OnceLock<[Arc<Style>; 4]> (:50);
    │   │   │                        apply_tokens_for(); apply_tokens() kept as the
    │   │   │                        normal-mode wrapper; re-exports
    │   │   └── contrast.rs        = unchanged — ratio()/composite() reused as-is
    │   ├── markers.rs             ~ O1–O6: outline at the six palette paint sites
    │   │                            (:84-96, :227-260, :399-430, :601 unchanged)
    │   ├── plugin_overlays.rs     ~ O7–O10: outline at the Line/Region/Label/Glyph
    │   │                            arms (:97, :118, :151, :163-192)
    │   ├── settings/
    │   │   ├── appearance.rs      ~ the checkbox below the combo, its focus guard,
    │   │   │                        its persist + set_high_contrast branch
    │   │   └── mod.rs             = unchanged — focus already routes to appearance
    │   ├── app.rs                 ~ :125 and :223 -> apply_tokens_for(.., hc)
    │   ├── shell.rs               = unchanged — its two apply_tokens calls
    │   │                            (:142, :190) are inside #[cfg(test)]
    │   └── widgets/controls.rs    = unchanged — switch()/SwitchKind::Checkbox
    │                                (:81, :132) reused as-is; its five test-module
    │                                apply_tokens calls unmodified
    └── tests/
        ├── high_contrast.rs       + NEW — H1–H16, S1–S5, M1–M2, P2–P5
        ├── design_token_contrast.rs  ~ NEW high-contrast block (H17/H18/M3);
        │                               the five existing loops kept VERBATIM
        ├── markers.rs             ~ M4–M10
        ├── plugin_overlays.rs     ~ M11–M12
        ├── fluent_keys.rs         ~ two keys in the Appearance block (:530-535)
        ├── design_token_literals.rs  = unmodified; EXPECTED_BASELINE_HITS stays 0
        ├── design_token_roles.rs     = unmodified
        ├── interaction_states.rs     = unmodified (FR-020)
        ├── control_variants.rs       = unmodified
        ├── control_inventory.rs      = unmodified
        ├── accessibility.rs          = unmodified (FR-014)
        ├── actions.rs                = unmodified
        └── controls.rs               = unmodified

locales/
└── en-US/settings.ftl               ~ setting-high-contrast{,-desc} after :50
                                       (no pt-BR catalogue exists — research R13)
```

**Structure Decision**: the existing 13-crate Cargo workspace is kept
unchanged, and this feature is confined to **two** of its crates plus one
locale file. `crates/modplayer-ui/src/theme/` holds every value and every
selection — the four `Roles` tables, the two widths, the outline helpers
and the one `apply_tokens_for` that installs a style pair — because
014 established it as the single construction site and FR-017/FR-019
require this feature to have exactly one too.
`crates/modplayer-core/src/settings/` and `settings_registry.rs` hold the
persisted boolean, its recovery and its search row, mirroring `theme`'s
own shape field-for-field. `crates/modplayer-ui/src/{markers.rs,
plugin_overlays.rs}` are the only paint files that change, and they change
only to *call* a theme function and honour an `Option<Stroke>` — neither
file ever names the high-contrast flag, which contract S3 enforces with a
source scan. No new crate is justified: every value here belongs to an
existing module that already owns its neighbours (Constitution X's "adding
a new crate requires stating why an existing dependency is insufficient" —
it is not insufficient).

## Complexity Tracking

> Filled for the five Phase-1 decisions that the Constitution Check would
> otherwise flag as unexplained. **None is a principle violation** — each
> is a deliberate divergence between the spec's letter and the code's
> reality, recorded here rather than left to surface during
> implementation.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| **D1** — Two fields added to `Roles` (`divider_alpha`, `high_contrast`) that the spec never names, and the struct grows from 11 fields to 13 | FR-007 needs the divider's multiplier to vary per table while staying "never a role of its own, never a call-site literal" (014 contract T10); FR-008/FR-012 need two widths selectable inside the theme module without any call site reading a flag (FR-017). `Roles` already carries a non-colour scalar (`disabled_alpha`) for exactly this reason. | A parallel `HighContrast` struct would mean two tables to keep in sync and would force `focus_ring`, `variant_paint`, `hover_fill` and `divider_color_for` each to accept both. An eleventh *colour* role for the divider is forbidden outright by 014 contract T10 and would fail `design_token_literals.rs`'s premise. |
| **D2** — `RawAppearance.high_contrast` is `toml::Value`, not `bool` — a surprising type for a boolean | `store.rs:123-131` converts **any** serde error into `AudioSettings::default()` + `Unreadable`, and serde's derived `Deserialize for bool` errors on a non-boolean. A plain `bool` would make `high_contrast = yes` silently reset theme, volume, device, keybindings and panel state — the opposite of FR-003 and of the spec's eighth Edge Case. `theme: String` + late validation is the established pattern for the same reason. | `bool` (see above). `Option<bool>` cannot distinguish absent from malformed and still errors on a non-boolean rather than yielding `None`. A hand-written `Deserialize` visitor is more code than `toml::Value` for an identical result and would be the only such visitor in the file. Contract **A5** exists solely to stop a later "simplification" back to `bool`. |
| **D3** — `apply_tokens(ctx)` keeps its one-argument signature and gains a sibling, rather than taking the flag | The regression net proving FR-002 / US1 AS-3 ("no change outside high contrast") **is** the 25 existing test call sites passing unmodified. Changing the signature edits all 25, and an edited regression test proves nothing. `apply_tokens` delegates to `apply_tokens_for(ctx, false)`, so there is still one construction path (014 design note 3). | Changing the signature and editing 25 call sites. It looks tidier and costs the feature its strongest evidence. Contract S4 pins both forms so the wrapper cannot drift into a second mechanism. |
| **D4** — The marker outline's contrast against the fill it encircles (2.73:1 at worst: dark `text.primary` around palette entry 7 olive) is measured but **not asserted** | FR-011 adds the outline because a palette colour is *data* exempted from the recolouring, so the outline's job is to separate that shape **from its background** — which is what FR-012 and FR-018 word a floor for, and which is verified at ≥13.48:1 in every case. No requirement states an outline-vs-fill floor. | Asserting one would require either per-entry outline tuning — which FR-012 explicitly rules out ("never a treatment that could itself fail against a given palette entry") — or changing `MARKER_PALETTE`, which the Scope boundary forbids. Recorded in research R6 and contract §1 so the absent assertion reads as a decision, not an oversight. |
| **D5** — No `pt-BR` catalogue is created, although Constitution X says "English and pt-BR ship first" | `locales/` has held only `en-US/` through 016; this feature adds two keys and is not the right place to found a second catalogue. A `pt-BR/settings.ftl` containing two of ~200 keys would be worse than none — it would present a mostly-untranslated Portuguese locale — and nothing in CI gates it. | Creating a two-key `pt-BR` directory. Recorded here and in research R13 so the gap is visibly this project's standing debt rather than this feature's omission. |

### Assumption carried from the spec, restated

The spec's clarify pass resolved every ambiguity, and Phase 0 confirmed
each resolution against the code. Two of its assumptions are worth
re-stating because the implementation depends on them being *assumptions*
rather than measurements:

- **The focus ring widens to 3 px.** No source states a target width; the
  spec chose 3 px as clearly distinguishable from 2 px. Contract H16 pins
  it as a value, so changing it later is a one-constant edit with a test
  that names it. *Rejected alternative*: 4 px — visibly heavier, and it
  would crowd the unchanged 1 px `FOCUS_RING_GAP` against adjacent
  controls, which FR-014 forbids adjusting.
- **High contrast is manual only.** FR-014 forbids reading an OS
  high-contrast preference. *Rejected alternative*: auto-follow — the
  codebase has no auto-detected appearance axis other than
  `ThemePreference::System`, which egui/winit provide natively; a manual
  setting can later become the value an OS preference *seeds* without
  breaking anything written here.
