# Implementation Plan: Shell Navigation and Launch Gates

**Branch**: `feature/020-shell-navigation-and-gates` | **Date**: 2026-09-25 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/020-shell-navigation-and-gates/spec.md`

**Requirement IDs**: FR-001, FR-001a, FR-001b, FR-002–FR-014, SC-001–SC-006; NFR-6.1, NFR-6.2,
NFR-7.1 (constitution X).

## Summary

Today `App::ui` adds the five-item nav rail (`Panel::left("shell-nav-rail")`, `app.rs:314`)
every frame, including during the launch gates, where clicks change `shell.section` with no
visible effect; the rail's selected item is a filled `selectable_label`; section content has no
retained scroll; and the eleven Settings categories are drawn with `horizontal_wrapped`
(`settings/mod.rs:144`), so they wrap at the 960 px minimum width. This feature, confined to
`crates/modplayer-ui` plus `locales/en-US`:

1. **Honest chrome** (US1, P1): a pure per-frame `shell::Chrome::for_frame(step, device_check_open)`
   decides whether the rail exists and which gate step (if any) is current. The rail panel is not
   added at all during Welcome / Sign-in / Device Check or the Settings "Test output device"
   preview; the same predicate gates the action dispatcher, so section shortcuts are inert
   whenever the rail is hidden (research R1).
2. **Step indicator** (US1): `GateStep` (Welcome 1, Sign in 2, Audio output check 3; total fixed
   at 3) drawn in a `Panel::top` only during gates, one `ProgressIndicator` AccessKit node named
   "Step n of 3: label"; position derived solely from `LaunchStep`, so retries and sub-views never
   change it (R2).
3. **Rail as navigation** (US3, P2): new `widgets::controls::nav_item` — leading-edge `accent` bar,
   `text_primary`/`text_secondary` labels, no filled background, `selected = true` in AccessKit,
   ≥ 3:1 indicator contrast in all four role sets (R3).
4. **Section memory** (US3): new `section_memory::SectionMemory` stores each scrollable view's
   vertical offset keyed by section *and* sub-view (library tab/detail, settings category),
   restores it once on return (egui clamps if content shrank), and is reset — together with the
   section sub-view states — on sign-out / revocation via an epoch in every scroll id salt
   (R4, R5).
5. **Non-wrapping settings row** (US2, P1): new `settings/category_row.rs` measures every label,
   runs a pure `partition()` (longest canonical prefix that fits + trailing "More"; selected
   category pinned just before More), and draws on one `ui.horizontal` line; "More" opens a
   `PopupKind::Menu` with explicit Up/Down/Enter/Escape handling (R6, R7).
6. **Library tab counts** (US4, P3): already delivered by 016; regression tests only (R9).

Technical approach and rejected alternatives: [research.md](./research.md) (R1–R10).

**Assumptions recorded (engineering choices where the spec is silent):**
- Sign-out also resets `Shell.section` to Library and `settings.category` to the first category
  (US3-AS4 "every section starts at its default view"). *Rejected*: keeping the last section
  across sign-in — it would restore a view whose data was just cleared.
- Settings keeps its search box and category row fixed; only category content scrolls (the
  selected category must always be visible, FR-010). *Rejected*: scrolling the whole Settings
  screen, which could scroll the row out of view.
- Now Playing has no section-level scroll area (018's fixed responsive layout); it counts as
  "a section with no scrollable content" per the spec edge case (R5).
- pt-BR: no pt-BR bundle or locale switch exists yet (R8). en-US keys ship now, pt-BR drafts are
  recorded in [contracts/fluent-strings.md](./contracts/fluent-strings.md), and the "longer
  translation" test uses the existing 40 % pseudo-expansion hook. See Complexity Tracking.

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, `rust-toolchain.toml`; workspace `rust-version = "1.95"`)

**Primary Dependencies**: eframe/egui 0.36 (`accesskit`; `Panel`, `ScrollArea`, `Popup`),
`fluent-templates` via `modplayer_core::{tr, tr_args}`; consumes `modplayer_account::{LaunchStep, next_step}`
and `modplayer_core::settings_registry::SettingsCategory` unchanged — **no new crates or dependencies**

**Storage**: N/A — all new state is in-memory UI state; nothing persisted (eframe `persistence`
feature stays off), no `settings.toml` key

**Testing**: `cargo test --workspace`; headless egui `Context::run_ui` + AccessKit tree +
`FullOutput.shapes` inspection in `crates/modplayer-ui/tests/` (new `shell_navigation.rs`,
`settings_category_row.rs`, `section_memory.rs`; extended `library_view.rs`, `fluent_keys.rs`,
`accessibility.rs`, `design_token_contrast.rs`, `actions.rs`); `proptest` for `partition()`
invariants; `with_pseudo_expansion(40, …)` for FR-012; manual scenarios M1–M9 ([quickstart.md](./quickstart.md))

**Target Platform**: macOS, Windows, Linux desktop (egui logical points); manual run on macOS

**Project Type**: desktop app (Cargo workspace, one crate per component)

**Performance Goals**: no measurable per-frame cost at ≥ 30 Hz repaint — row partition is
O(11) over layouts egui already caches; section memory is O(1) hash lookups; no disk I/O

**Constraints**: zero changes to audio/real-time code or the engine crate; token-only colours
and metrics (`design_token_literals.rs`); all strings externalized; minimum window 960 × 640
(018); rail and gate content never both interactive; no `unwrap()`/`expect()` outside tests

**Scale/Scope**: 1 crate (`modplayer-ui`: `app.rs`, `shell.rs`, `section_memory.rs` new,
`settings/mod.rs`, `settings/category_row.rs` new, `rows.rs`, `library_view.rs`,
`detail_view.rs`, `widgets/controls.rs`, `theme/controls.rs`, `now_playing.rs` salt only),
2 locale files, 6 new Fluent keys, 3 new test files

No open unknowns remain: product values are fixed by spec Clarifications 1–15;
engineering unknowns are resolved in research.md.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Principles touched: VII, VIII, X (primary); I–VI and IX verified untouched.

- [x] **I. Real-Time Path Is Sacred** — N/A: only UI-thread code in `modplayer-ui` changes; no engine, audio-io, effects or queue code, so no "real-time safety" PR note is required.
- [x] **II. Plugins Are Guests** — N/A: no plugin runtime, Capability Gateway or budget change; the Plugins section only gains an outer scroll area, and plugin panels/overlays are untouched.
- [x] **III. Host Primitives, Plugin Behaviors** — N/A: no markers, loops, effects, actions or transport-focus concepts are added or changed; the existing action registry is only gated by the same predicate as before (now named `Chrome::navigation_enabled`).
- [x] **IV. Audio Source Is Replaceable and Isolated** — Pass: no `audio-source-*` dependency added; every new test uses `FakeBackend` + `SyntheticHost`, like existing `modplayer-ui` tests.
- [x] **V. No Audio Ever Leaves the Engine** — N/A: the feature handles layout, focus and scroll offsets only; no sample buffers, cache or file output.
- [x] **VI. Security and Privacy by Default** — Pass: no credential handling changes; sign-out now clears *more* UI session state (section memory, library/search/settings sub-views), and nothing new is logged or persisted.
- [x] **VII. Rust Quality Gates** — Pass: no new crate or dependency; modules stay in `modplayer-ui` (already `#![forbid(unsafe_code)]`); fmt/clippy `-D warnings`/test/deny gates in quickstart; no `unwrap`/`expect` outside tests; new public items get doc comments with runnable examples where pure (`partition`, `Chrome::for_frame`, `GateStep`).
- [x] **VIII. Test What the NFRs Promise** — Pass: test-first per contract clause (C1–C10, R1–R11, M1–M7, L1–L4); proptest for `partition()` arithmetic invariants; regression test pins FR-001b (no deferred nav action); manual M1–M9 executed by the implementing agent per Manual Scenario Sign-Off.
- [x] **IX. One Plugin API Definition** — N/A: no plugin API surface changes; no schema or manifest touched.
- [x] **X. Simplicity, Portability, User's Override** — Pass with one recorded deviation: no trait, no feature flag, no crate; `SectionMemory`/`partition` are concrete types/functions; identical behaviour on all platforms (pure egui); every new control is keyboard-operable (Tab, Enter/Space, arrows, Escape — NFR-6.1) and named (`nav_item`, "More settings categories", step indicator — NFR-6.2); all strings externalized in en-US, **pt-BR drafts recorded but not shipped because no pt-BR bundle exists yet** (NFR-7.1, see Complexity Tracking); the one-action user override is unaffected.

**Post-design re-check (after Phase 1)**: unchanged — data-model.md adds only in-memory UI
types; contracts add no plugin/API/engine surface; no new dependency. Gate: **PASS**.

## Project Structure

### Documentation (this feature)

```text
specs/020-shell-navigation-and-gates/
├── plan.md              # This file
├── research.md          # Phase 0 (R1–R10)
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1 (automated + manual M1–M9)
├── contracts/
│   ├── shell-chrome.md          # rail visibility, step indicator, rail selected state (C1–C10)
│   ├── settings-category-row.md # single-line row + overflow + keyboard (R1–R11)
│   ├── section-memory.md        # sub-view + scroll retention (M1–M7)
│   ├── library-tab-counts.md    # regression only (L1–L4)
│   └── fluent-strings.md        # en-US keys + pt-BR drafts (F1–F3)
├── checklists/          # from /speckit.specify / clarify
└── tasks.md             # Phase 2 (/speckit.tasks — not created here)
```

### Source Code (repository root)

```text
crates/modplayer-ui/
├── src/
│   ├── app.rs                    # frame ordering; Chrome-gated dispatch; show_chrome; SectionMemory wiring; sign-out resets
│   ├── shell.rs                  # + GateStep, StepState, Chrome, show_chrome, step indicator; nav_rail → nav_item
│   ├── section_memory.rs         # NEW: SectionMemory, ViewKey, LibraryViewKey
│   ├── rows.rs                   # + virtualized_list_in (virtualized_list becomes a wrapper)
│   ├── library_view.rs           # main tab list through section memory (counts untouched)
│   ├── detail_view.rs            # detail list through section memory
│   ├── now_playing.rs            # effect-chain scroll salt gains session epoch
│   ├── lib.rs                    # pub mod section_memory
│   ├── settings/
│   │   ├── mod.rs                # header/content split; category_row replaces horizontal_wrapped
│   │   └── category_row.rs       # NEW: partition, CategoryRowState, show, More menu
│   ├── widgets/controls.rs       # + nav_item
│   └── theme/controls.rs         # + nav_indicator selector, NAV_INDICATOR_WIDTH{,_HIGH_CONTRAST}
└── tests/
    ├── shell_navigation.rs       # NEW
    ├── settings_category_row.rs  # NEW
    ├── section_memory.rs         # NEW
    ├── library_view.rs           # + L3
    ├── fluent_keys.rs            # + 6 keys
    ├── accessibility.rs          # + nav_item selected / step indicator / More names
    ├── design_token_contrast.rs  # + nav indicator ≥ 3:1
    └── actions.rs                # + no nav action during gates

locales/en-US/
├── app.ftl                       # + gate-step-* keys
└── settings.ftl                  # + settings-more, settings-more-a11y
```

**Structure Decision**: Existing Cargo workspace, single crate touched — `crates/modplayer-ui`
(all directories above exist today: `src/`, `src/settings/`, `src/widgets/`, `src/theme/`,
`tests/`) plus `locales/en-US/`. `modplayer-account` (`launch_flow.rs`) and `modplayer-core`
(`settings_registry.rs`, `i18n.rs`) are consumed read-only. `crates/modplayer/src/main.rs` is
unchanged (minimum size already 960 × 640). New code is two modules
(`section_memory.rs`, `settings/category_row.rs`) inside the existing crate, not new crates.

## Complexity Tracking

| Deviation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| FR-002/FR-009/FR-012 require pt-BR strings and a pt-BR test run; this plan ships en-US only + pt-BR drafts in `contracts/fluent-strings.md`, and substitutes the 40 % pseudo-expansion run (NFR-7.1, constitution X) | The repository has no `locales/pt-BR/` bundle and `tr` resolves `en-US` only (`i18n.rs:40–45`); pt-BR is an explicitly deferred slice (`settings/language.rs` header, 001 FR-021) | Building a locale switch now touches every screen and belongs to the pt-BR slice; a partial `locales/pt-BR/` holding only six keys would be loaded by `static_loader!` and misrepresent pt-BR as shipped. The pseudo-expansion (⌈1.4·len⌉) is a stricter length test than the pt-BR drafts. |
| Explicit `SectionMemory` instead of relying on egui's persisted `ScrollArea` state | Needs reset on sign-out, clamped ±1 px restoration that tests can assert, and section-level areas that do not exist today (Settings, Plugins, Search) | egui's `ScrollArea` state type is private (cannot be cleared selectively) and survives sign-out; ids silently change with parent layout. |
