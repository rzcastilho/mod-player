# PR body draft: 012-section-loop-plugin

Drafted by the implementing agent per T062 (Phase 6 Polish). Paste this
into the pull request description when
`feature/012-section-loop-plugin` is opened against `main`; it follows
`.github/PULL_REQUEST_TEMPLATE.md`.

---

## Summary

Ships the first bundled plugin, **Section Loop**
(`org.modplayer.section-loop`): a Luau package under `plugins/bundled/`
that a musician uses to drop A and B around a passage and drill it
hands-free, gaplessly, until it's solid (spec:
[012-section-loop-plugin](specs/012-section-loop-plugin/spec.md)). It
is written only against the public plugin API — the same `api.*`
surface a third-party author sees — so it doubles as living
documentation, and it ships under the host's `MIT OR Apache-2.0`
licence with its own licence copies. It declares
`playback.observe`, `transport.control`, `markers.read`,
`markers.write`, `ui.panel`, `ui.overlay`, `ui.shortcuts` (required)
and `analysis.read` (optional, unused); registers 23 trigger actions
(`I`/`O`/`L`/`[`/`]` defaults); registers one panel (Set A, Set B,
Loop, Repeat count slider, marker list, an inert Snap-to-beat +
explanation, Status); draws A/B lines, an accent region, labels and cue
dots as overlays; and keeps every marker in the host's per-track store,
so markers survive restarts and the plugin being disabled, and the
host's own marker UI keeps editing them.

Because the shipped `markers` API could not express an A-only region
or a repeat count, this feature also ships plugin API **1.3** — an
additive minor bump of exactly `markers.set_loop_endpoint`,
`markers.set_loop_repeat` and a `regions` array in `markers.list()`.

## Real-time safety

N/A — no real-time path changes. `crates/modplayer-engine` and
`crates/modplayer-effects` are untouched; both new requests reach the
engine only through the controller's existing `recommit_if_armed` →
buffer-boundary `Command` path; seam and wrap counting stay host-side
(FR-009). `tests/realtime.rs` is unchanged and green.

## Area-maintainer sign-off (GOV-3.2)

This PR modifies `crates/modplayer-capability-gateway/` (schema, DTOs)
and `crates/modplayer-plugin-runtime/` (bindings, snapshot). Per the
constitution's Governance section, changes to the Capability Gateway or
the Plugin Runtime require review and sign-off by the designated area
maintainer *in addition to* normal code review.

- **Gateway files touched**: `api/v1.toml`, `src/request.rs`,
  `tests/{api_reference.rs, gateway.rs}`.
- **Plugin-runtime files touched**: `src/handle.rs`,
  `src/bindings/markers.rs`, `src/bindings/mod.rs`, `tests/bindings.rs`.
- **Designated area maintainer**: `@rzcastilho` (repository owner; see
  `CODEOWNERS` — `CODEOWNERS` still does not list
  `modplayer-capability-gateway/` or `modplayer-plugin-runtime/` even
  though the constitution's GOV-3.2 names both — the same gap
  009/010/011's own PR bodies flagged; unchanged by this feature, but
  flagged here again so the sign-off is requested explicitly rather
  than silently skipped by the absence of an automatic reviewer
  assignment).
- [ ] Gateway/runtime maintainer (`@rzcastilho`) has reviewed and
      approved the two crates' changes above.

## Constitution IX change request

> **Plugin API change request — 1.2 → 1.3 (012-section-loop-plugin).**
> *Change*: adds `markers.set_loop_endpoint` and
> `markers.set_loop_repeat` (both `markers.write`, `markers` rate
> bucket, no focus) and a `regions` array in `markers.list()`'s result.
> *Why*: FR-13.1 obliges the bundled Section Loop plugin to drive two
> host primitives — an incomplete (A-only/B-only) loop region and a
> region's repeat count (DM-7) — that the 1.2 `markers` surface cannot
> express; reimplementing them in script would violate Constitution
> III. *Compatibility*: additive; no existing request, event, payload
> field or refusal changes meaning; `create_loop` remains the one-shot
> two-endpoint constructor. *Deprecations*: none. *Migration*: none.
> *Reference*: regenerated `docs/plugin-api/v1.md`. *Real-time safety*:
> N/A — both calls reach the engine only through the existing
> buffer-boundary recommit path.

(Verbatim from
[contracts/plugin-api-v1.3.md §6](specs/012-section-loop-plugin/contracts/plugin-api-v1.3.md#6-change-request-constitution-ix--copied-into-the-pr-body).)

## Test plan

- `cargo fmt --all --check`: clean (fixed pre-existing formatting drift
  in `crates/modplayer-core/tests/controller_section_loop.rs` and
  `crates/modplayer-plugin-runtime/tests/bindings.rs`, plus this
  phase's own new file, along the way — tasks.md T060).
- `cargo clippy --workspace --all-targets --all-features -- -D
  warnings`: no issues found (fixed one pre-existing `collapsible_if`
  in `crates/modplayer-core/src/plugins/apply.rs`'s `UpdateWidget` arm
  and one in `crates/modplayer-core/tests/markers_model.rs`, plus one
  in this phase's own `bundled_section_loop.rs`, along the way —
  tasks.md T060).
- `cargo test --workspace` (`--no-fail-fast`, `RUSTUP_TOOLCHAIN=1.95.0`
  per `rust-toolchain.toml`'s pin): **1540 passed, 11 ignored, 0
  failed** (macOS). Task **T022** (Phase 2) fixed the two
  `modplayer-ui/tests/accessibility.rs` assertions that assumed a
  zero-bundled-package/empty-focus-holder baseline — they now expect
  Section Loop as the always-discovered bundled package and, for the
  Transport-panel empty-state tests, wait for Section Loop `Active`
  then `plugin_disable` it before asserting the empty state. One flake
  class is known and non-blocking: `modplayer-core/tests/
  controller_plugin_ui.rs::button_action_source_ui` can fail under the
  thread contention of a fully parallel `cargo test --workspace` run;
  it is consistently green under `--test-threads=1` or run alone
  (confirmed this session), and 011's own PR body recorded the same
  class of flake.
- `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok
  (no new dependency).
- `scripts/check-license-headers.sh`: every `*.rs` file carries the
  SPDX header.
- Manual scenarios M1–M9: automated proxies all green in this sandbox
  (no interactive/native-window-driving or VoiceOver tool available
  here — the same gap 009/010/011's own sessions recorded); a real
  `cargo build -p modplayer` and a timed launch produced no panic. The
  literal Quartz-recipe walk (VoiceOver traversal, screenshot per step)
  still owes a maintainer session on real hardware before final
  Governance sign-off. See tasks.md's Manual Scenario Log (T061) for
  the full per-scenario breakdown and the automated proxy each PASS
  cites.

## Checklist

- [x] `cargo fmt --all --check`
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [x] `cargo test --workspace` — 1540 passed, 11 ignored, 0 failed
- [x] `cargo deny check`
- [x] `scripts/check-license-headers.sh`
- [x] This PR changes `crates/modplayer-capability-gateway/api/v1.toml`;
      the written change request is included above (Constitution IX).
