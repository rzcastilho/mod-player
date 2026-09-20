# PR body draft: 011-plugin-ui-contributions

Drafted by the implementing agent per T122 (Phase 8 Polish). Paste this
into the pull request description when
`feature/011-plugin-ui-contributions` is opened against `main`; it
follows `.github/PULL_REQUEST_TEMPLATE.md`.

---

## Summary

Lets a plugin show the user something and give them controls — without
ever injecting markup, styles or scripts into the host (spec:
[011-plugin-ui-contributions](specs/011-plugin-ui-contributions/spec.md)).
Five `ui.*` surfaces become operable in plugin API **1.2** (additive
minor): `ui.panel` registers a flat list of host widgets the host
renders, themes and makes keyboard-operable with an accessible name from
the plugin's own label (an unlabeled widget is refused with its
`widgets[<i>] (<id>)` path); panels dock or float inside Now Playing,
close/disable/persist, and report `panel_interaction`. `ui.overlay` draws
lines/regions/labels/glyphs in track-time ms that the host re-projects on
every zoom/scroll with zero plugin code. `ui.shortcuts` registers
`<identifier>.<name>` actions into 007's Action & Binding registry with
an ownership tier (host > bundled > community): a host-vs-plugin
collision leaves only the plugin's binding inactive, a same-tier
collision leaves both inactive until the user resolves it.
`ui.settings` renders a declarative schema under Settings › Plugins,
persisted by the host into a new host-owned `Scope::Settings` and
reported as `settings_changed`. `ui.notify` posts attributed,
non-blocking notifications rate-limited to 6 per rolling 60 s. A
`request_focus()` made inside an interaction handler counts as a user
interaction for 010's auto-on-interaction policy.

## Real-time safety

N/A — no real-time path changes. `crates/modplayer-engine` and
`crates/modplayer-effects` are untouched; every registry mutation runs on
the controller (UI) thread inside `tick`/`drain_plugin_requests`;
painting overlays and widgets is pure egui on the UI thread; the only
transport effect this feature can cause (a `marker list` row seeking)
reuses the existing user-command `seek` path. `tests/realtime.rs` is
unchanged and green.

## Area-maintainer sign-off (GOV-3.2)

This PR modifies `crates/modplayer-capability-gateway/` (schema, DTOs,
validators, limiter, a new store scope, manifest fields) and
`crates/modplayer-plugin-runtime/` (`ui.*` bindings, scheduler payloads,
`Control::SettingsWrite`, the interaction-origin flag). Per the
constitution's Governance section, changes to the Capability Gateway or
the Plugin Runtime require review and sign-off by the designated area
maintainer *in addition to* normal code review.

- **Gateway files touched**: `api/v1.toml`, `build.rs`, `src/lib.rs`,
  `src/ui.rs`, `src/ui/limits.rs`, `src/api.rs`, `src/request.rs`,
  `src/event.rs`, `src/manifest.rs`, `src/limiter.rs`,
  `src/state/{store,paths,writer}.rs`, `tests/{ui_validation.rs,
  gateway.rs, state_store.rs, manifest.rs, api_reference.rs}`.
- **Plugin-runtime files touched**: `src/bindings/mod.rs`,
  `src/bindings/ui.rs`, `src/bindings/transport.rs` (doc comment only),
  `src/bindings/state.rs` (doc comment only), `src/handle.rs`,
  `src/context.rs`, `src/budget.rs`, `src/scheduler.rs`,
  `tests/{bindings.rs, scheduler.rs}`.
- **Designated area maintainer**: `@rzcastilho` (repository owner; see
  `CODEOWNERS` — currently scoped to `modplayer-engine`,
  `modplayer-secure-store` and `modplayer-audio-source-connect` only).
  `CODEOWNERS` still does not list `modplayer-capability-gateway/` or
  `modplayer-plugin-runtime/` even though the constitution's GOV-3.2
  names both — the same gap 010's own PR body flagged; unchanged by this
  feature (a `CODEOWNERS` change is a repo-governance change, not a
  011-plugin-ui-contributions deliverable), but flagged here again so the
  sign-off is requested explicitly rather than silently skipped by the
  absence of an automatic reviewer assignment.
- [ ] Gateway/runtime maintainer (`@rzcastilho`) has reviewed and
      approved the two crates' changes above.

## Constitution IX change request

> **Plugin API change request — 1.1 → 1.2 (011-plugin-ui-contributions).**
> *Change*: adds the `ui` namespace (9 requests), 3 events, 2 rate
> buckets, 4 manifest fields; flips 5 permissions (`ui.panel`,
> `ui.overlay`, `ui.shortcuts`, `ui.settings`, `ui.notify`) to operable.
> *Compatibility*: additive; no existing request, event, payload field or
> refusal changes meaning; every `api = "1.0"`/`"1.1"` manifest still
> loads; `ready_ack.capabilities` grows.
> *Deprecations*: none. *Migration*: none required.
> *Reference*: regenerated `docs/plugin-api/v1.md`
> (`tests/api_reference.rs::reference_is_current`).

(Verbatim from
[contracts/plugin-api-v1.2.md §6](specs/011-plugin-ui-contributions/contracts/plugin-api-v1.2.md#6-change-request-constitution-ix--copied-into-the-pr-body).)

## Test plan

- `cargo fmt --all --check`: clean (one pre-existing formatting drift in
  `crates/modplayer-ui/src/settings/plugins.rs` fixed along the way, see
  tasks.md T119).
- `cargo clippy --workspace --all-targets --all-features -- -D
  warnings`: no issues found.
- `cargo test --workspace`: **1488 passed, 11 ignored, 0 failed** (129
  suites, 149.45 s; macOS, `RUSTUP_TOOLCHAIN` unset per
  `rust-toolchain.toml`'s 1.95.0 pin). See
  [quickstart.md](specs/011-plugin-ui-contributions/quickstart.md)'s
  suite table for what each feature suite proves, and tasks.md's Phase 8
  for the per-story sessions' own defects-found-and-fixed notes. Two
  flakes surfaced once, under the heavy parallelism of a full
  `cargo test --workspace` run, and were confirmed environmental (not
  regressions) by re-running each alone, green: `modplayer-ui/tests/
  plugin_panels.rs::placeholder_on_suspend` (a 1 ms CPU-share budget is
  inherently load-sensitive) and `modplayer-account/src/
  listener.rs::error_callback_with_matching_state_resolves_as_error` (a
  loopback HTTP listener test, pre-existing, untouched by this feature).
- `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok (no
  new licence surface from the `image` crate's `png` feature — see
  tasks.md T123).
- `scripts/check-license-headers.sh`: every `*.rs` file carries the SPDX
  header.
- Manual scenarios M1–M10: automated proxies all green in this sandbox
  (no interactive/native-window-driving tool available here); the live
  Quartz-recipe walk — screenshots, VoiceOver for M1 specifically —
  still owes a maintainer session on real hardware before final sign-off,
  the same gap 009's and 010's own manual passes recorded. See
  tasks.md's Manual Scenario Log (T061, T080, T093, T107, T116, and the
  T121 consolidation) for the full per-scenario breakdown and the
  automated proxy each PASS cites.

## Checklist

- [x] `cargo fmt --all --check`
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [x] `cargo test --workspace`
- [x] `cargo deny check`
- [x] `scripts/check-license-headers.sh`
- [x] This PR changes `crates/modplayer-capability-gateway/api/v1.toml`;
      the written change request is included above (Constitution IX).
