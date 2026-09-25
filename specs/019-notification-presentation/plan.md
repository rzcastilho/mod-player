# Implementation Plan: Notification Placement, Severity, and Message Quality

**Branch**: `feature/019-notification-presentation` | **Date**: 2026-09-25 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/019-notification-presentation/spec.md`

**Requirement IDs**: FR-001–FR-018, SC-001–SC-006; review UX-05, UX-37, UX-38; NFR-6.1, NFR-6.2, NFR-7.1 (constitution X).

## Summary

The notification stack today is an `egui::Area` anchored top-right (`app.rs:315`) that renders
every non-dismissed notification as an identical grey `Frame::popup` row with unbounded text, and
the device-missing message interpolates the raw persisted `DeviceId` (≈ 120 chars, UX-38). This
feature, touching only `modplayer-core` (notifications, device policy, settings, controller raise
sites) and `modplayer-ui` (notification widget, app wiring) plus `locales/en-US`:

1. Re-anchors the stack **bottom-right** (`RIGHT_BOTTOM`, 8 px), same `Area` id and z-order (R1),
   and proves zero overlap with headers/tab strips/category list/table headers/nav rail at
   960×640 and 1200×820 with a headless egui geometry test (R2).
2. Caps the collapsed stack at the **three newest** notifications with a "{N} more" / "Show fewer"
   button; expanded stack scrolls within 60 % of window height (R3, R4). Pure `partition()`
   recomputed from `NotificationCenter::visible()` every frame, so promotion on dismissal is free.
3. Draws each card on `surface_raised` with a **4 px left accent bar** and icon in the severity
   role colour (`danger`/`warning`/`positive`, theme `Roles`, so light/dark/high-contrast work),
   severity word kept as text (R7).
4. **Truncates** messages to two lines via egui `TextWrapping { max_rows: 2, overflow_character:
   '…' }`, showing "Show more" iff the galley is elided; the accessible name stays the full text (R6).
5. Adds optional `Notification.detail` behind a "Details" toggle (device notifications → raw
   `DeviceId`; keybindings warning → dropped ids) and rewrites device strings to human sentences
   naming the fallback device (R9, R11, R12).
6. Persists `[audio] output_device_name` on device confirmation so a missing device can be named;
   legacy files fall back to `name:<name>` then "Your saved output device" (R10).
7. Holds `Info` auto-dismiss while a card is expanded and restarts the 10 s timer on collapse
   (core `hold_auto_dismiss` / `release_auto_dismiss`, R8).

Technical approach and rejected alternatives: [research.md](./research.md) (R1–R15).

**Assumptions recorded (spec was already clarified; these are engineering choices):**
- Generic phrases ("Your saved output device", "the system default output") are resolved via `tr`
  at raise time and passed as args, following `plugin-suspended`'s `$cause` precedent. *Rejected*:
  Fluent selectors on sentinel arg values; duplicate `*-unnamed` keys (R9).
- Expansion state lives in an app-owned `StackState`, not egui memory. *Rejected*: `ctx.data_mut`
  (harder to test/prune) (R5).
- The hold/timer policy lives in core, not the UI. *Rejected*: resetting `created_at`; UI timer (R8).
- `device_policy::resolve` gains a `saved_name` parameter (two callers). *Rejected*: resolving the
  name in the controller after the fact — keeps the pure policy testable end to end.

## Technical Context

**Language/Version**: Rust 1.95.0 (stable, `rust-toolchain.toml`; workspace `rust-version = "1.95"`)

**Primary Dependencies**: eframe/egui 0.36 (`accesskit`), `serde` + `toml` (settings), `fluent-templates` via `modplayer_core::{tr, tr_args}` — all existing; **no new crates**

**Storage**: existing `settings.toml` through `modplayer-core::settings::SettingsStore`; one new optional key `[audio] output_device_name` (additive, `schema_version` unchanged)

**Testing**: `cargo test --workspace`; core unit + integration tests (`crates/modplayer-core/tests/{notifications,device_policy,settings}.rs`), `proptest` for `partition()` invariants, `display_name_for_saved` and settings round-trip; headless egui `Context::run_ui` + AccessKit harness in `crates/modplayer-ui/tests/` (new `notification_stack.rs`, extended `notifications.rs`, `accessibility.rs`, `fluent_keys.rs`); manual scenarios M1–M11 per constitution sign-off

**Target Platform**: macOS, Windows, Linux desktop (egui logical points); manual run on macOS

**Project Type**: desktop app (Cargo workspace, crate per component)

**Performance Goals**: no measurable per-frame cost — at most `n` extra text layouts per frame for visible cards (egui galley cache); no disk I/O added to the frame loop; UI stays at existing ≥ 60 Hz repaint

**Constraints**: zero changes to the audio/real-time path or engine crate; no colour literals (tokens only, enforced by `design_token_literals.rs`); all strings in `locales/en-US`; stack non-modal; 960 × 640 minimum window (018)

**Scale/Scope**: 2 crates (`modplayer-core`: `notifications.rs`, `device_policy.rs`, `settings/model.rs`, `controller.rs`; `modplayer-ui`: `notifications.rs`, `app.rs`), 2 locale files, 4 rewritten + 8 new Fluent keys, 1 new UI test file

No open unknowns remain: product values are fixed in spec Clarifications; engineering unknowns resolved in research.md.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.1. Principles touched: VI (lightly), VII, VIII, IX (verified unchanged), X.

- [x] **I. Real-Time Path Is Sacred** — N/A: no change to `modplayer-engine`, `modplayer-audio-io` or any audio-thread code; all changes run on the UI thread (`tick`, raise sites already on the controller's UI-thread path). No "real-time safety" PR note needed.
- [x] **II. Plugins Are Guests** — Pass: plugin-attributed notifications keep their lifecycle, attribution and non-blocking rendering; no Capability Gateway or budget change; plugins cannot set `detail` (`raise_attributed` unchanged).
- [x] **III. Host Primitives, Plugin Behaviors** — N/A: no markers/loops/effects/transport-focus concepts touched; notifications remain a host primitive.
- [x] **IV. Audio Source Is Replaceable** — Pass: no `audio-source-*` dependency added; all new tests run with `FakeBackend`/`SyntheticHost`.
- [x] **V. No Audio Leaves the Engine** — N/A: feature handles only message text and settings strings, never sample buffers or the cache.
- [x] **VI. Security and Privacy by Default** — Pass: `detail` carries device ids / action ids only — never credentials; existing `tests/credential_leak.rs` extended to scan rendered Details text; `output_device_name` is a device label, no telemetry added.
- [x] **VII. Rust Quality Gates** — Pass: fmt/clippy `-D warnings`/test/deny required; no new crate (so no dependency justification needed); no `unsafe`; no `unwrap`/`expect` outside tests; new public items (`raise_with_detail`, `hold_auto_dismiss`, `release_auto_dismiss`, `display_name_for_saved`, `partition`, `card_width`, `StackState`) get doc comments with runnable examples where pure.
- [x] **VIII. Test What the NFRs Promise** — Pass: test-first for each FR (tasks order tests before impl); proptest for state serialization (`output_device_name` round-trip) as the principle requires, plus `partition`/name-resolution properties; UX-38 regression test with the literal id; manual M1–M11 executed by the agent.
- [x] **IX. One Plugin API Definition** — N/A: plugin API schema (`notify` surface from 011) untouched; `detail` is host-only, so no API version bump or change request.
- [x] **X. Simplicity, Portability, User's Override** — Pass: no new trait, no feature flag, no crate; identical behaviour on all platforms (pure egui); every new control is a focusable `Button` with an externalized accessible name (NFR-6.1/6.2); all strings are Fluent keys (NFR-7.1). pt-BR: the repo ships only `locales/en-US/` today (spec Assumptions) — this feature adds no locale gap beyond the existing one; new keys follow the same files when pt-BR lands. Dismiss/action buttons (incl. Disable plugin) keep one-action user override.

**Post-design re-check (after Phase 1)**: all gates still pass. Design added no crate, trait or
flag; the only cross-crate API growth is additive (`detail`, hold/release, `resolve` parameter).
No violations → Complexity Tracking lists assumptions only.

## Project Structure

### Documentation (this feature)

```text
specs/019-notification-presentation/
├── plan.md              # This file
├── research.md          # Phase 0 (R1–R15)
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1 (automated gates + manual M1–M11)
├── contracts/
│   ├── notification-stack-ui.md         # S1–S9 placement, cap, card, truncation, a11y
│   ├── notification-center-api.md       # C1–C5 core API delta + raise sites
│   ├── settings-output-device-name.md   # K1–K7 new settings key
│   └── fluent-strings.md                # rewritten + new en-US keys
├── checklists/          # from /speckit.specify & /speckit.clarify
└── tasks.md             # Phase 2 (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
crates/modplayer-core/
├── src/
│   ├── notifications.rs        # + detail, auto_dismiss_from, held; raise_with_detail; hold/release; tick
│   ├── device_policy.rs        # + display_name_for_saved; resolve(saved_name); MissingPreferred fields
│   ├── settings/model.rs       # + AudioSettings/RawAudio output_device_name
│   └── controller.rs           # confirm_device persists name; device + keybindings raise sites
└── tests/
    ├── notifications.rs        # detail + hold/release + raise-site assertions
    ├── device_policy.rs        # name resolution + proptest
    └── settings.rs             # output_device_name round-trip, legacy fixture, keybindings detail

crates/modplayer-ui/
├── src/
│   ├── notifications.rs        # StackState, partition, card_width, card rendering, truncation, Details
│   └── app.rs                  # RIGHT_BOTTOM anchor, StackState ownership, hold_changes → core
└── tests/
    ├── notification_stack.rs   # NEW: SC-001 geometry, SC-002 cap/expand, 60 % scroll cap
    ├── notifications.rs        # SC-003 colours ×3 themes, SC-004, toggles, SC-006 actions
    ├── accessibility.rs        # new buttons named; full-text message name
    ├── credential_leak.rs      # Details text scanned
    └── fluent_keys.rs          # new/rewritten keys, SC-005

locales/en-US/
├── app.ftl                     # device-* rewritten; notification-* new keys
└── controls.ftl                # keybindings-invalid-entries rewritten
```

**Structure Decision**: Existing Cargo workspace, no new crate or module directory. Core policy
and data (`crates/modplayer-core/src/{notifications.rs,device_policy.rs,settings/model.rs,controller.rs}`)
stay presentation-agnostic; all placement/cap/styling/truncation lives in
`crates/modplayer-ui/src/notifications.rs` with wiring in `crates/modplayer-ui/src/app.rs`;
strings in `locales/en-US/{app,controls}.ftl`. Tests extend the existing
`crates/modplayer-core/tests/` and `crates/modplayer-ui/tests/` suites plus one new
`crates/modplayer-ui/tests/notification_stack.rs`.

## Complexity Tracking

No constitution violations. Recorded assumptions/decisions with rejected alternatives:

| Decision | Why | Rejected alternative |
|----------|-----|----------------------|
| Generic device phrases resolved via `tr` at raise time and passed as args | Matches `plugin-suspended` `$cause` precedent; one key per phrase | Fluent selector on sentinel arg; duplicate `*-unnamed` message keys |
| Hold/release timer in `NotificationCenter` | Lifecycle stays in one crate, testable with injected `Instant` | Resetting `created_at`; UI-side timer |
| `StackState` owned by the app | Deterministic pruning and direct test assertions | egui `Memory`/`data_mut` storage |
| `resolve()` takes `saved_name` | Keeps name resolution inside the pure, tested policy | Controller patches the warning after `resolve` |
| Z-order unchanged; stack may overlap the 018 narrow-window dock overlay's lower edge | Spec forbids z-order change; FR-002 allows lower content overlap | `Order::Foreground` (would cover modals) |
