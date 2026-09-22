# Implementation Plan: Button, Toggle, and Meter Variants

**Branch**: `015-control-variants` | **Date**: 2026-09-22 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/015-control-variants/spec.md`

## Summary

Give ModPlayer's controls a grammar. On top of 014's ten semantic roles,
add **four button variants** (`primary` filled accent, `default` today's
baseline, `quiet` text-only, `destructive` danger-outlined), **one toggle
form** — a pill switch whose thumb moves, replacing every persistent
boolean that today looks like a highlighted button — **three interaction
states** the app has never had (hover 4 %, pressed 8 %, a 2 px offset
`accent` focus ring), and **three-band meters** (`positive`/`warning`/
`danger` split at −6 dBFS and the danger boundary) with scale marks at
−6 and 0 dB. Every new value is derived *inside* `theme/`, from the ten
roles; this feature adds **no eleventh role** and writes no colour, alpha
or stroke width at any call site (FR-019).

Two toolkit findings shape the whole design, and both were verified in
egui 0.36.2 rather than assumed:

1. **There is no style hook that repaints a `Checkbox`** (research R1).
   `Style::checkbox_style` is an inherent method, and `Checkbox::ui`
   hard-codes a square icon and a check-mark polyline. So the switch is a
   host widget used at call sites, not a style install — FR-008c's
   *mechanism* changes while its *guarantee* survives intact, because the
   two lines that draw every plugin-contributed boolean
   (`plugin_panels.rs:363`, `settings/plugins.rs:167`) are host lines.
   `crates/modplayer-capability-gateway/api/v1.toml` is byte-identical; Principle IX stays untriggered.
2. **`Response::widget_state()` folds focus into `Active`** (research R2),
   so focus and pressed cannot both live in the widget-state slots. The
   pressed fill goes into the slots as FR-011a requires; the ring is drawn
   by **one app-level pass** at the end of `App::ui`, reading
   `Memory::focused()` + `Context::read_response` and stroking into a
   foreground layer (research R3). One site, app-wide coverage, zero
   call-site colour — and the offset (`rect.expand(1.0)` +
   `StrokeKind::Outside`) is what keeps the ring identifiable on an
   `accent`-filled selection, structurally rather than chromatically.

The call-site payload is small and fully enumerated (data-model §9): one
`primary` (`welcome.rs:108`), seven `destructive` sites — including the
`plugin-panel-disable` copy in `plugin_panels.rs:291` that the spec's
site list omits — four `quiet` Queue actions, fifteen switch conversions,
four row-hover wrappers, two meters, and five lines in `theme/style.rs`.
Everything else inherits: `default` *is* what `recolor_widget` already
installs, so FR-005's ~60 buttons are not edited and cannot drift.

Nothing touches the real-time path, the plugin API, the Fluent
catalogues, or any persisted value (research R15).

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, pinned by `rust-toolchain.toml`;
edition 2024) — unchanged from 001–014.

**Primary Dependencies**: existing only — egui/eframe **0.36.2**
(+`accesskit`), `epaint`/`ecolor` 0.36.2 (transitive). The APIs this
feature relies on were each read in the vendored 0.36.2 source:
`Context::read_response` (`context.rs:1355`), `Memory::focused`
(`memory/mod.rs:566`), `Context::layer_painter` (`context.rs:1587`),
`Painter::add`/`Painter::set` (`painter.rs:213`/`:242`),
`Button::fill`/`stroke`/`frame`/`corner_radius` (`button.rs:143`–`200`),
`WidgetInfo::selected`, `StrokeKind::Outside`, `Color32::gamma_multiply`,
`egui::__run_test_ctx`. **No new crate, no new dependency, no new feature
flag, no dev-dependency** (Constitution X); `crates/modplayer-ui/Cargo.toml`
is unchanged. Explicitly rejected: `egui_kittest` / any screenshot-diff
harness (research R16), and vendoring a forked `egui::Checkbox` (R1).

**Storage**: none. No persisted value changes — a variant is a static
property of a call site (spec § Key Entities), the settings file gains no
field, and there is no migration to write. All new values are
compile-time `const`/`static` data or pure functions over the 014 role
tables.

**Testing**: `cargo test --workspace` — new
`crates/modplayer-ui/tests/{control_variants, interaction_states,
meter_bands, control_inventory}.rs` plus `#[cfg(test)]` suites in
`src/theme/controls.rs` and the extended one in `src/theme/style.rs`.
Everything is asserted from **values**, headlessly, through
`egui::__run_test_ctx` — the same route 014 used, no rendering harness.
The regression net is the *unmodified* existing suites: `accessibility.rs`
(role/`Toggled` assertions — SC-008), `fluent_keys.rs`,
`design_token_literals.rs` (must still report **0** — SC-007),
`design_token_contrast.rs`, `design_token_roles.rs` (SC-006), and every
view suite (FR-017 forbids behaviour change). Manual scenarios M1–M10 in
[quickstart.md](quickstart.md), executed by the implementing agent
(Governance › Manual Scenario Sign-Off) with `screencapture` plus a
point-sampling `target/manual-walk/pixel.py` as evidence. CI gates
unchanged (fmt, clippy `-D warnings`, test, deny, licence headers, API
reference regeneration diff) on ubuntu / macos / windows.

**Target Platform**: Desktop macOS, Windows 10+, Linux. Every value is
identical on all three — the variants, the switch metrics, the state
alphas, the ring and the bands are computed from the same token tables and
the same 4 px spacing scale everywhere (Constitution X).

**Project Type**: Desktop application — the 13-crate Cargo workspace is
unchanged. One crate is modified (`crates/modplayer-ui`).

**Performance Goals**: zero work on the audio thread — no engine or
effects file is touched. Per-frame UI cost added: one
`Memory::focused()` read plus at most one `read_response` and one stroked
rect for the focus pass; one `read_response` per host-drawn control
(egui's own `Checkbox` already pays this); two or three extra
`rect_filled` calls per meter for the band segments and marks; one
reserved-and-set shape per interactive row. The 30 Hz repaint budget
(`REPAINT_INTERVAL` 33 ms) is unaffected, and `theme::apply_tokens` still
builds its two `Style`s once into a `OnceLock`.

**Constraints**: Constitution I — no real-time path edit; PR note "N/A".
II/III — no plugin-facing change; plugin booleans inherit the switch
through host code (R1). VI — no credential, network, file, asset or
telemetry surface. VII — `forbid(unsafe_code)` holds, no
`unwrap`/`expect` outside tests, SPDX headers on the two new files,
public items documented. VIII — the NFRs this feature promises (NFR-6.1
keyboard, NFR-6.2 roles/states, NFR-6.4 colour never sole carrier) are
pinned by tests that already exist and must pass unmodified, plus the new
value suites. IX — untriggered: `crates/modplayer-capability-gateway/api/v1.toml` and `docs/plugin-api/v1.md`
byte-identical. X — no crate, trait or flag; identical on three
platforms; every string keeps its Fluent key. **Out of scope** (FR-017,
FR-018, FR-021): layout, row geometry, tab/chip appearance, click targets,
shortcuts, confirmation steps, animation, and the `spectrum` widget.

**Scale/Scope**: `src/theme/controls.rs` ≈ 300 LOC + ≈ 200 LOC unit tests;
`src/widgets/controls.rs` ≈ 220 LOC; `theme/style.rs` ≈ 20 LOC changed;
meters ≈ 120 LOC changed across two files; call-site edits ≈ 90 LOC across
14 view files (1 primary + 7 destructive + 4 quiet + 15 switches + 4 row
wrappers + 4 gaps); new integration tests ≈ 500 LOC. New semantic roles:
**0**. Locales: **0** keys. Plugin API: **0** changes. Dependencies:
**0** changes. Persisted fields: **0**.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Touched? | Status | How this plan complies |
|---|---|---|---|
| I. Real-Time Path Is Sacred (non-negotiable) | No | ✅ PASS | No file under `crates/modplayer-engine/` or `crates/modplayer-effects/` is modified. The two meters keep reading the values their caller already reads per frame (`peak_meter.rs:20-23`, `chain_meters.rs` module doc); no queue, atomic, allocation or buffer callback is added, and the band selector is a pure `f32 -> enum` function on the UI thread. PR real-time note: "N/A — no real-time path changes." |
| II. Plugins Are Guests (non-negotiable) | No | N/A | No gateway, runtime, permission, budget or refusal path is touched. Plugin-contributed booleans change appearance only because the **host** line that draws them (`plugin_panels.rs:363`, `settings/plugins.rs:167`) draws a switch instead of a checkbox; no plugin gains or loses a capability and no plugin code runs differently (research R1). |
| III. Host Primitives, Plugin Behaviors | **Yes** (preserved) | ✅ PASS | Markers, loop regions, effect nodes and transport stay host primitives; this feature changes only how their controls are painted. No DSP, no threshold logic: the meters' dB maths (`to_db`, `fraction_of`) and the plugin health thresholds (`health_color`) are untouched — only the colour each resolves to moves onto the band/role tables (FR-015, contract R3). |
| IV. Audio Source Is Replaceable and Isolated | No | N/A | No source crate, trait or implementation is touched; every automated test here is a pure value test needing no audio source at all. |
| V. No Audio Ever Leaves the Engine (non-negotiable) | No | N/A | No sample data, buffer, cache or file path appears anywhere in this feature. The meters render a dB figure the UI already holds. |
| VI. Security and Privacy by Default | No | ✅ PASS | No credential, network call, file write, bundled asset or telemetry surface. No font or image is added, so `cargo deny`'s licence surface is unchanged. The quickstart's live-token line is quoted from the constitution for completeness and is used by no scenario here. |
| VII. Rust Quality Gates | **Yes** | ✅ PASS | No new crate; `#![forbid(unsafe_code)]` unaffected; no `unwrap`/`expect` outside tests — the band selector is total over `f32` (NaN falls through to `Positive`, asserted), `radius::full` already clamps, and both new helpers return early on `None` rather than unwrapping `focused()`/`read_response`. SPDX headers on `src/theme/controls.rs` and `src/widgets/controls.rs` (`scripts/check-license-headers.sh` gates it); public items carry doc comments, and `band` carries a doc example that runs under `cargo test --doc`. `cargo deny` unaffected (zero dependency change). |
| VIII. Test What the NFRs Promise | **Yes** | ✅ PASS | The NFRs this feature touches are pinned by tests, not by review: **NFR-6.2** by the existing `accessibility.rs`/`plugins_view.rs`/`plugin_panels.rs`/`settings_plugins.rs` role and `Toggled` assertions, which must pass **unmodified** after every switch conversion (contract A2, SC-008); **NFR-6.1** by the untouched `controls.rs`/`actions.rs` suites (FR-017); **NFR-6.4** by the switch's moving thumb (contract S3), the destructive label-plus-gap pair (B11) and the meters' retained `mono` readouts and accessible values (R1, R2). The new value suites are named per rule in the three contracts. **Test-first (the MUST, in full)**: every suite lands red against data-model.md's values before the module and the call-site conversions — the order 014's Phase 2 inverted once and had to record; tasks.md must sequence it correctly from the start. |
| IX. One Plugin API Definition | No | ✅ PASS | **Untriggered by construction**: the switch reaches plugin-contributed booleans through host code that already draws them under `contracts/ui-panels.md` A4, so no request, event, DTO, permission or schema line changes. `crates/modplayer-capability-gateway/api/v1.toml` and `docs/plugin-api/v1.md` must be byte-identical; the existing `api_reference.rs` regeneration-diff test in CI is the check (contract S7). No written change request is required or included. |
| X. Simplicity, Portability, User's Override | **Yes** | ✅ PASS | No new trait, feature flag, crate or dependency; two new files in the crate that already owns every pixel. **YAGNI**: no animation (FR-021), no screenshot harness (R16), no scanner widening (R7), no forked toolkit widget (R1). **Portability**: identical values and identical embedded font stack on all three platforms. **Externalized strings**: zero Fluent keys added, removed or re-cased — a switch renders the same string its checkbox did (contract A3). **Keyboard/accessible names**: unchanged and test-gated (A2, A4). **User override**: every disable/bypass/enable control this feature restyles keeps its one-action behaviour; the user can still take transport focus back and disable any plugin in one action (FR-017). |
| Governance: area-maintainer sign-off | No | N/A | No change under `crates/modplayer-engine/`, `crates/modplayer-capability-gateway/` or `crates/modplayer-plugin-runtime/` (GOV-3.2), so no additional sign-off beyond normal review. Requirement ids (FR-, SC-, US-, NFR-, C-, GOV-) are referenced throughout spec, plan, contracts, data model and test names. |
| Governance: Manual Scenario Sign-Off | **Yes** | ⚠️ PLANNED, AT RISK | [quickstart.md](quickstart.md) § 3 defines M1–M10, to be executed by the implementing agent against the real build on macOS with `screencapture` + point-sampling evidence; each result is recorded on its `tasks.md` task and any deviation written back into quickstart.md/research.md. **Known host risk**: 014's M1/M2 could not run because `App::launch_step`'s sign-in gate hides Library and Settings and the host account session was revoked. The same gate stands between this feature and M2–M6 and M8–M10. M1 (Welcome screen, US5) is reachable regardless. If the gate blocks the walk, the required outcome is to record the scenarios as **not executed** with the reason and mark the affected checkpoints **not reached** — never to sign off on automated evidence alone, and never to fabricate a signed-in state (quickstart § 4). |

**Pre-Phase-0 result**: PASS (no violations).

**Post-Phase-1 re-check**: PASS. The design adds no crate, trait, flag,
dependency or `unsafe`; the real-time path, the plugin API, the locales
and the persisted data are untouched; the only deviation from the spec's
letter is FR-008c's stated *mechanism*, which egui 0.36.2 makes
impossible and whose *guarantee* the design preserves exactly
(Complexity Tracking D1). Everything else the re-check might have
flagged — the second `plugin-panel-disable` site, the off-thumb colour the
spec leaves unnamed, the focus/pressed conflation in the toolkit, the
rounded-end compromise in the segmented fill — is recorded below.

## Project Structure

### Documentation (this feature)

```text
specs/015-control-variants/
├── plan.md                      # This file
├── spec.md                      # Feature specification (input)
├── research.md                  # Phase 0: decisions R1–R16, verified against egui 0.36.2 and this worktree
├── data-model.md                # Phase 1: the value set — variants, state fills, ring, switch, bands, marks, call-site map
├── quickstart.md                # Phase 1: automated gates (named suites) + manual scenarios M1–M10
├── contracts/
│   ├── control-variants.md      # B1–B11, S1–S7, A1–A5, L1–L3 with the test that pins each
│   ├── interaction-states.md    # I1–I7, F1–F6, N1–N2, Y1
│   └── meter-bands.md           # M1–M9, K1–K7, R1–R4
├── checklists/
│   └── requirements.md          # (created by /speckit-checklist)
└── tasks.md                     # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

Existing layout (001–014) is kept; `+` marks new files, `~` modified.

```text
crates/
├── modplayer-ui/
│   ├── Cargo.toml                        (unchanged — no new dependency)
│   ├── src/
│   │   ├── theme/
│   │   │   ├── controls.rs               + Variant/VariantPaint, hover/pressed fills, focus ring + gap,
│   │   │   │                               switch metrics & state table, DESTRUCTIVE_GAP, Band/band()/
│   │   │   │                               band_color, mark widths + mark_color  (data-model §2,3,5,6)
│   │   │   ├── mod.rs                    ~ `pub mod controls;` + re-exports
│   │   │   ├── style.rs                  ~ recolor_widget re-differentiated per slot (FR-011a, data-model §4)
│   │   │   ├── tokens.rs                   (unchanged — the ten roles, the scales)
│   │   │   ├── contrast.rs                 (unchanged — `composite` reused for the state fills)
│   │   │   └── markers.rs                  (unchanged)
│   │   ├── widgets/
│   │   │   ├── controls.rs               + button(), switch(), row_frame(), destructive_gap(),
│   │   │   │                               paint_focus_ring()                     (data-model §7)
│   │   │   ├── mod.rs                    ~ `pub mod controls;`
│   │   │   ├── peak_meter.rs             ~ segmented bands, −6/0 dB marks, ceiling-tick rule (M6–M8, K1–K7)
│   │   │   └── chain_meters.rs           ~ level_pair bands both sub-bars, 0.7 dim removed (M9); spectrum untouched
│   │   ├── app.rs                        ~ paint_focus_ring(ui.ctx()) as the last statement of `App::ui`
│   │   ├── welcome.rs                    ~ welcome-acknowledge → primary (:108)
│   │   ├── markers.rs                    ~ clear-all/clear-yes → destructive + gap (:843, :853); loop-arm → switch (:757)
│   │   ├── effects_view.rs               ~ effects-remove → destructive + gap (:163); bypass + 5 param booleans → switch
│   │   ├── queue_view.rs                 ~ 4 row actions → quiet (:84-93); shuffle → switch (:27); row hover
│   │   ├── plugins_view.rs               ~ plugin-panel-disable → destructive + gap (:161); enable → switch (:183); row hover
│   │   ├── plugin_panels.rs              ~ plugin-panel-disable → destructive (:291); plugin checkbox → switch (:363)
│   │   ├── now_playing.rs                ~ queue/effects/transport disclosure → switch (:145, :152, :159)
│   │   ├── rows.rs                       ~ row hover/pressed via the existing rect+response (:502-510)
│   │   └── settings/{account,audio,plugins}.rs
│   │                                     ~ account-sign-out + modal confirm → destructive (:60, :146);
│   │                                       safe-volume and boolean plugin fields → switch (:166, :167)
│   └── tests/
│       ├── control_variants.rs           + B3–B7, B10, A5, L1, S4            (SC-001/002/009)
│       ├── interaction_states.rs         + I3, I6, I7, F3–F5                 (SC-003/004/009)
│       ├── meter_bands.rs                + M6–M9, K1, K2, K5–K7              (SC-005/009)
│       ├── control_inventory.rs          + S5, S6 — the source-level boolean/selection sweep (SC-010)
│       └── {accessibility,fluent_keys,design_token_*,plugins_view,plugin_panels,settings_plugins,
│           queue_view,effects_view,markers,rows,now_playing,controls,actions}.rs
│                                           (unchanged — they gate FR-016/FR-017, SC-006/007/008)
└── modplayer-capability-gateway/tests/api_reference.rs
                                            (unchanged — must regenerate with no diff, Principle IX)
```

**Structure Decision**: keep the single 13-crate Cargo workspace under
`crates/` with one crate per architectural component (Constitution VII)
and add **no crate**. The whole feature lives in `crates/modplayer-ui`,
the crate that already owns every pixel, and it splits along the seam 014
established: **values in `crates/modplayer-ui/src/theme/`, painting in
`crates/modplayer-ui/src/widgets/`**. `theme/` gains one file,
`controls.rs`, because FR-019 requires every colour, alpha, stroke width
and metric to live inside the token module — the module that 014's literal
scan excludes — and because the variant table, the band selector and the
mark rule are pure functions worth unit-testing without a `Ui`;
`theme/style.rs`'s `recolor_widget` is edited in place, since FR-011a
demands the slot re-differentiation happen at the one `Style` construction
site 014 built (design note 3, "one construction site"). `widgets/` gains
one file, `controls.rs`, beside the existing `peak_meter`/`chain_meters`/
`knob`/`volume`/`skeleton`/`initials` widgets, because a switch and a
variant button are widgets in exactly the sense those already are, and
because the row-hover and focus-ring helpers need `Ui`/`Context` access
that does not belong in a value module. The focus pass is wired in
`crates/modplayer-ui/src/app.rs`, the only place that runs after every
view on every frame — the same file where `theme::apply_tokens` already
runs before every view. Verification lives beside the other UI
integration suites in `crates/modplayer-ui/tests/`, from where the SC-010
inventory can walk `crates/modplayer-ui/src/**` without a CI-configuration
change. `crates/modplayer` (the binary), `crates/modplayer-engine`,
`crates/modplayer-effects`, `crates/modplayer-core`,
`crates/modplayer-capability-gateway`, `crates/modplayer-plugin-runtime`,
`plugins/`, `locales/` and `docs/` are untouched; the workspace
dependency graph is unchanged.

## Design notes that tasks must respect

1. **Tests before the code they pin** (Constitution VIII): the four new
   integration suites and the `#[cfg(test)]` modules in
   `theme/controls.rs` land first, asserting data-model.md's values, so
   the red→green transition is observed. 014's Phase 2 inverted this once
   and had to record a deviation — sequence it correctly from the start.
2. **Values in `theme/`, painting in `widgets/`** (R7): if a task finds
   itself typing a number into `widgets/controls.rs`, a view file or a
   meter, the number belongs in `theme/controls.rs` instead. This is what
   keeps `design_token_literals.rs` at 0 with its exclusion list
   unchanged — no new scan pattern is added (L2).
3. **`default` is the do-nothing variant**: never edit an FR-005 call
   site to "make it default". It already is, via `recolor_widget`.
4. **One `Style` construction site** (014 design note 3): the slot
   re-differentiation goes inside `style::build_style`'s `visuals()`, once
   per frame, and nowhere else. `expansion`, `striped`, `handle_shape`,
   `interact_cursor`, `animation_time` and the `Spacing` component sizes
   stay untouched — 014's `no_geometry_or_interaction_field_changes` must
   pass verbatim (V6, Y1).
5. **Never call `Response::widget_state()` from host code** (R2, I7): it
   reports `Active` for a merely-focused widget. Host controls read
   `hovered()` and `is_pointer_button_down_on()` directly.
6. **`next_auto_id()` and the widget must be adjacent** (R4): the helper
   reads last pass's response by the id the widget is about to take;
   allocating anything between the two breaks the match. A unit test
   simulates two passes to prove the state tracks.
7. **Row hover is reserve-then-set, never a new layout container** (R5,
   FR-018): `Painter::add(Shape::Noop)` before the content,
   `Painter::set` after. `rows.rs` already has the rect and the response —
   use them, do not re-allocate.
8. **The focus ring is painted once, last, app-wide** (R3, F4): in
   `App::ui`, after every panel. No view file paints a ring; no helper is
   sprinkled per call site.
9. **The switch carries its call site's role, not one role** (R9, A1):
   `SwitchKind::Checkbox` for anything that is a `Checkbox` today,
   `SwitchKind::Toggle` for anything that is a `selectable_label`/
   `toggle_value` today. `plugins_view.rs:290/313` counts and asserts the
   absence of `Role::CheckBox` nodes — a uniformly-typed switch fails two
   assertions at once.
10. **Boolean vs. selection is the conversion line** (FR-008a/FR-008b):
    convert persistent booleans app-wide; leave tabs, the nav rail,
    Settings categories, combo options and list-item selection exactly as
    they are — they belong to 003-list-row-and-panel-components. The
    SC-010 inventory test enforces both directions.
11. **Both `plugin-panel-disable` sites** (R6): `plugins_view.rs:161` and
    `plugin_panels.rs:291`. The spec's list names only the first; SC-001
    samples the dock too.
12. **`queue-remove` is quiet, not destructive** (FR-004): reversible
    action; the destructive variant's value is its scarcity.
13. **The destructive gap is a ratio, not a number** (B9): assert
    `DESTRUCTIVE_GAP >= 2 × item_spacing.x`, so a later spacing change
    cannot void the rule silently. Where the destructive control has no
    neighbour, add nothing.
14. **Band order is fixed and boundaries belong upward** (M2, M3): write
    the selector exactly as data-model §6 states it, so a ceiling at or
    below −6 dBFS empties the warning band instead of inverting it.
15. **The meters lose two things on purpose** (K5, M9): the
    `warn_fg_color` ceiling tick and the RMS `gamma_multiply(0.7)`. Both
    removals are requirements, not cleanups.
16. **Threshold logic is not this feature's** (FR-015, R3): `health_color`
    and the meters' dB maths keep their comparisons; only the colour they
    resolve to moves.
17. **No animation** (FR-021): a state changes on the frame its input
    changes.
18. **Manual scenarios M1–M10 are executed by the implementing agent**
    (Governance); if the sign-in gate blocks them, record "not executed"
    with the reason and mark the checkpoint not reached — do not sign off
    on automated evidence and do not fabricate a session
    (quickstart § 4).

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

No constitution violations. The one spec deviation forced by the toolkit,
the assumptions this plan had to make where the spec named no value, and
the accepted residual behaviours are recorded here for traceability.

| Decision / deviation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| **D1 — Spec deviation: FR-008c's mechanism. The switch is a host widget used at call sites, not "installed in the shared style once per frame"** | egui 0.36.2 offers no hook to replace a widget's painting: `Style::checkbox_style` is an inherent method (`widget_style.rs:174-190`), and `Checkbox::ui` hard-codes a square icon rect and a check-mark polyline (`widgets/checkbox.rs:120-160`). A pill track with a positioned thumb is unreachable at any field value. FR-008c's *guarantee* is preserved exactly: plugin-contributed booleans reach the screen through **host** lines (`plugin_panels.rs:363`, `settings/plugins.rs:167`) under `contracts/ui-panels.md` A4, so they inherit the switch with no plugin API surface change and Principle IX stays untriggered — verified by the API-reference regeneration diff (research R1, contract S7). | Vendoring a forked `egui::Checkbox` — a toolkit widget to maintain across every bump, for a shape we can draw in 40 lines. Waiting for an upstream style hook — blocks the feature on a third party. `Classes`/`SELECTED_CLASS` styling — classes only select among values `checkbox_style` already computes; they cannot replace its painting. |
| **D2 — Accepted residual: built-in widgets show the pressed fill while merely focused** | `Response::widget_state()` maps `has_focus()` to `WidgetState::Active` (`widget_style.rs:104-116`), so any widget this feature does not replace — the FR-008b selection controls, `ComboBox`, `Slider`, `DragValue`, `default`-variant buttons — reads the `active` slot when focused. FR-011a requires that slot to carry the pressed fill. Mitigation: those controls also receive the app-wide focus ring (F1–F4), so focus is never ambiguous, and host-drawn controls bypass `widget_state()` entirely (I7). The cost is a one-step-stronger fill while focused (research R2, contract N1). | Leaving `active` equal to `inactive` to dodge the conflation — deletes FR-011/FR-011a outright, a strictly larger loss. Patching every built-in call site to draw a host control — hundreds of edits to work around one toolkit line. |
| **D3 — Assumption: the switch's off-thumb is `text.secondary`** | FR-007 names the off track (`surface.raised` + `divider`) and both on-state colours but not the off thumb. `text.secondary` is chosen because 014 FR-011 already verifies it at ≥ 4.5:1 against `surface.raised`, so the thumb is visible in both themes with no new contrast floor to establish (data-model §5). | `text.primary` — visually heavier than the on state's thumb, inverting the emphasis. `divider` — 8 % alpha, invisible against the track it sits on. Adding a role for it — FR-019 forbids an eleventh role. |
| **D4 — Spec-list gap: `plugin-panel-disable` has two host call sites** | FR-003 names `plugins_view.rs` only, but `plugin_panels.rs:291` draws the plugin dock's own Enable/Disable button with the same Fluent key. SC-001 samples "every screen containing a destructive action", and the dock is visible with the Main screen, so converting only one site fails the criterion on the dock (research R6). | Following the spec's list literally — ships a view where the same action is destructive in one place and neutral in another, which is the exact confusion FR-001a exists to prevent. |
| **D5 — Segmented fill: the lowest band keeps the rounded end, higher bands are drawn square on top** | egui has no rounded clip rect, and painting three rounded rects leaves 4 px notches at the interior seams. Band seams are vertical cuts mid-bar, where a square edge is correct; only the bar's ends want the radius, and the left end is always the lowest band. The right end loses its rounding once the fill passes −6 dB — imperceptible at a 14–18 px bar with a 4 px radius (research R10). | Three rounded rects — visible notches at every seam. A single-colour fill chosen by the current level — contradicts FR-012's "segmented" and US4's own Independent Test ("the fill progresses positive → warning → danger"). |
| **D6 — The level pair's danger band is one column wide** | `level_pair` clamps to `SCALE_MAX_DB = 0.0` (`chain_meters.rs:41-42`) and FR-012 fixes its danger boundary at 0 dBFS, so `danger` is reached only at exactly 0.0 dBFS. This is what the spec says and what SC-005 tests; it is recorded so a later reader does not file the thin red sliver as a bug (research R10). | Giving the level pair a synthetic ceiling below 0 dBFS — inventing a threshold no source states, and diverging the two meters' semantics. |
| **D7 — `pressed_fill` is numerically identical to 014's `divider`** | Both are `text.primary` at 8 %. 014's precedent is that a derived value is named for its job, not its formula; a 1 px rule and a control fill are different jobs that happen to share a derivation. No test asserts they differ (research R7). | Choosing a different pressed alpha purely to avoid the coincidence — the 4 %/8 % doubling is FR-011's stated relationship, and the normative property is the ordering, not the numbers. |
| **D8 — FR-019's alpha/stroke-width clause is met by construction, not by widening 014's scanner** | 014 deliberately scans colour and font-size literals only, because radius/spacing/stroke patterns cannot be told from legitimate geometry maths without false failures (014 Complexity Tracking). This feature instead puts every number in the scanner-excluded token module, so there is no literal to find, and pins that with a contract test (L1) rather than a regex (research R7). | Adding stroke/alpha patterns to the scan — false positives on `rect.height() * 0.5`-style maths would push implementers toward `#[allow]`-shaped escapes, weakening the rule that matters. |
| **D9 — Verification is values (automated) plus pixels (manual), with no screenshot harness** | SC-001–SC-005 are stated as visual judgements. Everything derivable from values is asserted headlessly through `egui::__run_test_ctx`; the rendered result is evidenced by the quickstart's M1–M10 under Governance › Manual Scenario Sign-Off — the split the spec's own Clarifications chose (research R16). | `egui_kittest` or an image-diff gate — a new dev-dependency, a new CI surface and a new class of flake, for a feature whose entire payload is already expressible as values (Constitution X). 014 rejected the same dependency for the same reason. |
| **D10 — Branch/spec header mismatch, recorded** | `spec.md`'s header reads **Feature Branch: `016-control-variants`** while the feature directory, the worktree and the git branch are all `015-control-variants`. This plan, its artifacts and every path above use **015**. No content depends on the number; the header line is a typo in the spec and is left as-is rather than edited during planning. | Renaming the directory or the branch to match the typo — breaks the worktree, the spec-kit prerequisites script and every relative link in 014's and this feature's documents. |
