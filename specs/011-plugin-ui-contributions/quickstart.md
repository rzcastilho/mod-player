# Quickstart: Plugin UI Contributions

**Feature**: 011-plugin-ui-contributions | **Date**: 2026-09-20
Validation guide only — design in [plan.md](plan.md),
[data-model.md](data-model.md) and [contracts/](contracts/);
implementation detail belongs to `tasks.md`.

## Prerequisites

- Rust 1.95.0 (`rust-toolchain.toml`; if the shell exports
  `RUSTUP_TOOLCHAIN`, run with `RUSTUP_TOOLCHAIN=1.95.0`).
- C++ toolchain for the vendored Luau build (unchanged from 009).
- macOS host with a signed-in Premium account and a VoiceOver-capable
  session for M1 (Constitution Governance › Manual Scenario Sign-Off).

## Automated gates (green before any manual scenario)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
scripts/check-license-headers.sh
```

Feature suites (all inside `cargo test --workspace`):

| Suite | Proves | Spec |
|---|---|---|
| `cargo test -p modplayer-capability-gateway --test ui_validation` | every `invalid_state` reason and path message; id-grammar and widget-list proptests; caps at the boundary | FR-001, FR-002a, FR-003, FR-014, FR-017, FR-019, FR-024 |
| `cargo test -p modplayer-capability-gateway --test gateway` | `notify` 6/60 s window, validation-refused calls consume a slot, `ui` 101st/s | FR-020, FR-020a |
| `cargo test -p modplayer-capability-gateway --test api_reference` | `docs/plugin-api/v1.md` regenerated for 1.2 | Constitution IX |
| `cargo test -p modplayer-capability-gateway --test state_store` | `Scope::Settings` round trip and cap accounting | FR-018 |
| `cargo test -p modplayer-plugin-runtime` | `api.ui.*` → RPC; local `get_settings` with default substitution; `settings_changed` after the write; the three Lua payloads; interaction flag on `request_focus` | contracts/plugin-api-v1.2.md §3–§4, FR-026 |
| `cargo test -p modplayer-core --test actions` | tiered conflicts (host > bundled, bundled = bundled), dormant overrides, 65th action, unbound-on-rejected-default, `is_invocable` | FR-010, FR-010a, FR-011, FR-013 |
| `cargo test -p modplayer-core --test plugin_ui_registry` | panel/overlay/settings registries' state machines | FR-003, FR-007, FR-015, FR-016, FR-018, FR-025 |
| `cargo test -p modplayer-core --test controller_plugin_ui` | US1–US5 end to end on `FakeBackend` + `SyntheticSource` with the six `ui-*` fixtures via `debug_probe` | US1–US5; SC-002, SC-005, SC-007 |
| `cargo test -p modplayer-core --test settings` | `[plugin_panels]` round trip (proptest); non-`host.` keybindings retained dormant | FR-005, FR-010a |
| `cargo test -p modplayer-core --test controller_plugins_lifecycle` | 009 teardown order + `PluginUi::on_stop`/`on_ready` | FR-025 |
| `cargo test -p modplayer-ui --test plugin_panels` | roles/names per kind ("Tempo, slider"), key precedence, one event per drag, selection follows focus, theme switch with zero events, dock order, float clamp, placeholder | US1; SC-001, SC-004, SC-008 |
| `cargo test -p modplayer-ui --test plugin_overlays` | re-projection under zoom/scroll with zero plugin calls, draw order, detail-only labels | US3; SC-003 |
| `cargo test -p modplayer-ui --test settings_plugins` / `controls` / `notifications` / `plugins_view` | settings pages + search path; plugin shortcut groups + tier conflict text; attributed non-modal notifications; per-panel controls | US2, US4, US5; SC-006 |
| `cargo test -p modplayer-ui --test accessibility` / `fluent_keys` | every new element named; every new key present in `plugins.ftl` | NFR-6.2, Constitution X |
| `cargo test -p modplayer --test single_dependent` / `decoded_store_boundary` | dependency and audio-boundary rules unchanged | Constitution IV, V |

Expected: green on ubuntu / macos / windows.

## Running the app with the UI fixtures

```bash
cargo build -p modplayer
MODPLAYER_PLUGIN_FIXTURES=1 ./target/debug/modplayer                      # 16 fixtures incl. ui-panel … ui-icons
MODPLAYER_CONFIG_DIR=$(mktemp -d) MODPLAYER_PLUGIN_FIXTURES=1 ./target/debug/modplayer   # fresh settings.toml
```

Fixture console lines look like `[plugin:org.modplayer.fixture.ui-panel]
info panel_interaction widget=tempo value=121`. Probes are driven from a
test (`Control::Probe`) or, manually, by the fixture's own timer
(`ui-notify`'s own `arm`/`counts` probes are test-only — a real
`Control::Probe` dispatch blocks the host on a bounded wait for this
plugin's reply, and a nested `notify` RPC inside that same wait would
deadlock the drain loop against itself, so it never posts more than one
call per probe in a way a human could drive live; manual M9 instead uses
the `ui-panel` fixture's own "Notify ×7" button, whose `panel_interaction`
handler — fire-and-forget from the host's side, so no such deadlock —
calls `api.ui.notify` directly, seven times).

## Manual scenarios (executed by the implementing agent; results recorded in tasks.md)

Drive with the constitution's Quartz recipe; one screenshot per step;
read stderr for the fixtures' event logs. Now Playing = `Cmd+3`;
Settings = `Cmd+5`.

| # | Scenario | Steps | Expected |
|---|---|---|---|
| M1 | Accessible, themed panel (US1 AS1, AS3, AS4) | Enable `ui-panel`; open Now Playing; `Tab` into the dock to "Tempo"; VoiceOver on; press `→` ×3; click "Snap to beat"; Settings › Appearance → Dark | VoiceOver says "Tempo, slider, 120" then 121/122/123; three `panel_interaction` log lines with `value` one step apart; toggle announces on and logs `value=true`; panel colours change on the next frame with **no** new plugin log line |
| M2 | Unlabeled widget refused (US1 AS2) | From `ui-panel`'s "Register bad" button (probe `register_unlabeled`) | Console: `register_panel: invalid_state/unlabeled_widget widgets[1] (snap)`; the good panel is unchanged |
| M3 | Float / dock / close / disable persistence (US1 AS5, AS6) | Float the panel, drag it, restart; re-dock; Close; Plugins list → Show; Disable from the list; restart; Enable | Floated position survives restart; header always shows icon + "UI panel fixture — Controls"; Close hides until Show; Disabled stays hidden across restart until Enable |
| M4 | Suspension placeholder (US1 AS8) | Enable `ui-panel`; probe `hang` (button "Hang") | Panel frame stays with "UI panel fixture, suspended: handler exceeded 4 ms" + Restart/Disable; Restart → live panel returns after `ready_ack`; Disable → frame removed |
| M5 | Host wins `L` (US2 AS1, AS2) | Enable `ui-shortcuts`; Settings › Controls; press `L` in Now Playing; rebind "Take over transport" to `Shift+T`; press it | Map flags both `L` rows, host row un-greyed, plugin row "conflicts with the host's Toggle loop"; `L` still toggles the loop; no `action_invoked`; after rebind: flag clears, `action_invoked source=keyboard` logged |
| M6 | Two bundled plugins collide (US2 AS3, AS8) | Enable `ui-shortcuts` and `ui-panel` (whose manifest registers `ui-panel.focus_me` with default `Shift+K`, the same chord as `ui-shortcuts.nudge`) | Both rows flagged, neither fires on `Shift+K`; remove `ui-panel.focus_me`'s binding → `nudge` fires with `action_invoked source=keyboard value=1.0` (continuous) |
| M7 | Overlays re-project (US3 AS1, AS2, AS5) | Enable `ui-overlay`; play a track; zoom the detail view (`+`), scroll it; skip track | Line/region/glyph on both views, label on detail only, all pinned to the same m:ss under zoom/scroll; no plugin log during zoom; overlays vanish on track change and reappear at the new track's positions |
| M8 | Settings page + search (US4 AS1–AS4) | Enable `ui-settings`; Settings › Plugins › "UI settings fixture"; toggle "Snap to beat"; set "Semitone shift" to 3 + Enter; search "semitone"; restart; probe `get` | Fields keyboard-operable with names; two `settings_changed` log lines; search hit "Plugins › UI settings fixture › Semitone shift" opens the page; after restart `get_settings` logs `shift=3` |
| M9 | Notification rate limit (US5 AS1, AS2) | Enable `ui-panel`; open its "Controls" panel; click "Notify ×7" | Six attributed, non-blocking notifications with the fixture's icon + name; seventh logged `rate_limited`; nothing modal |
| M10 | Bad assets degrade (FR-014a) | Enable `ui-icons` | Plugin loads; console warning names `glyphs/big.png`; glyph `ok` renders, glyph `big` shows the generic glyph; the manifest `icon` shows in the header |

Deviations found while executing M1–M10 are recorded on the scenario
task in `tasks.md` and, where behaviour differs from the spec, appended
to `research.md` with a regression test.
