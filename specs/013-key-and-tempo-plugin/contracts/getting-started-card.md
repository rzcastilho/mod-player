# Getting Started card (host-native, FR-018 / FR-019)

UI contract for the dismissible onboarding card and the default-enabled
bundled plugins. Host code only — no plugin API involvement.

## 1. Placement and visibility (V)

- **V1** Drawn by `App::show_library` (`crates/modplayer-ui/src/app.rs`)
  at the top of the Library view's content, above the tab row, only
  when no detail target (album/artist/playlist) is open.
- **V2** Shown iff `controller.getting_started_dismissed() == false`.
  The flag is `[onboarding] getting_started_dismissed` in the per-user
  settings file (device-scoped; `false` when absent).
- **V3** First appears the first time the main window is reached on
  this install — after 002's sign-in and 003's device check, or after a
  non-Premium account's **Continue** — for any account and tier; shown
  again on every later launch and every return to the Library view
  until dismissed.
- **V4** Non-modal: an ordinary framed group inside the scroll area; it
  never blocks input to the rest of the window and gates no other
  first-launch step.

## 2. Content (C) — all strings Fluent keys in `locales/en-US/app.ftl`

| Element | Key | en-US text |
|---|---|---|
| Heading | `getting-started-title` | Getting started |
| Section Loop line | `getting-started-section-loop` | Section Loop — drop A and B around a passage and drill it hands-free. Shortcuts: I set A, O set B, L loop, [ / ] nudge. |
| Key & Tempo line | `getting-started-key-tempo` | Key & Tempo — transpose a song or slow it down without changing the rest. Shortcuts: + / - tempo step; key controls in the panel. |
| Link-button | `getting-started-tutorial` | Open plugin tutorial |
| Button | `getting-started-dismiss` | Dismiss |

- **C1** The link-button opens `modplayer_core::links::GETTING_STARTED_TUTORIAL_URL`
  (`https://github.com/rzcastilho/mod-player/blob/main/docs/plugin-tutorial.md`)
  via `ctx.open_url(OpenUrl::new_tab(..))` from `App` only (002 design
  note 10); the card stays visible afterwards.
- **C2** Dismiss calls `controller.dismiss_getting_started()`: sets the
  flag, persists through `persist_settings` (atomic replace;
  `settings-save-failed` warning on error, the in-memory flag still set
  so the card disappears this session), and the card is not drawn again.
- **C3** Only Dismiss writes the flag; quitting with the card visible
  leaves it unset.

## 3. Accessibility (X)

- **X1** Both controls are egui buttons reachable by Tab and activated
  by Enter/Space; their accessible names are their labels; the tutorial
  control carries `Role::Link` through `accesskit_node_builder`.
- **X2** The heading is a `Heading`-styled label so screen readers
  announce the section.

## 4. Persistence and scope (S)

- **S1** `AudioSettings.getting_started_dismissed` round-trips through
  `SettingsStore::save`/`load`; absent section ⇒ `false`.
- **S2** Sign-out (002) and revocation (003) do not modify the flag
  (same file section handling as `[disclosure]`).
- **S3** A fresh `MODPLAYER_CONFIG_DIR` shows the card again (device-scoped).

## 5. Bundled plugins on first run (B) — FR-019

- **B1** `bundled::packages()` returns Section Loop then Key & Tempo;
  both are discovered on every launch, granted their declared
  permissions without an approval sheet, and `enabled` by default (009
  FR-024 mechanics, unchanged).
- **B2** The Plugins section lists both as installed and enabled with
  `can_uninstall == false`; either can be disabled in one action.

## 6. Named tests

| Test | File | Proves |
|---|---|---|
| `getting_started_card_shown_until_dismissed` | `modplayer-ui/tests/library_view.rs` | V1, V2, C2 — card present; after Dismiss absent in the same session |
| `getting_started_dismiss_persists_across_relaunch` | `modplayer-ui/tests/first_launch.rs` | S1 — new `App` on the same store: card absent |
| `getting_started_flag_survives_sign_out` | `modplayer-core/tests/settings.rs` | S2 — sign-out path leaves `[onboarding]` intact |
| `getting_started_flag_round_trip` | `modplayer-core/tests/settings.rs` (+ proptest in `persist.rs`) | S1 — save/load equality, absent ⇒ false |
| `getting_started_tutorial_opens_url` | `modplayer-ui/tests/library_view.rs` | C1 — outcome `OpenTutorial`; `App` issues `open_url` with the constant |
| `getting_started_not_shown_in_detail_view` | `modplayer-ui/tests/library_view.rs` | V1 |
| `getting_started_strings_exist` | `modplayer-ui/tests/fluent_keys.rs` | C — every key resolves |
| `getting_started_controls_accessible` | `modplayer-ui/tests/accessibility.rs` | X1, X2 |
| `both_bundled_plugins_enabled_with_grants` | `modplayer-core/tests/plugins_manifest_discovery.rs` | B1, B2 (shared with key-tempo-plugin.md P4) |
