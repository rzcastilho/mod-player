# Quickstart: Key & Tempo Bundled Plugin and Getting Started Panel

**Feature**: 013-key-and-tempo-plugin | **Date**: 2026-09-20
Validation guide only — design in [plan.md](plan.md),
[data-model.md](data-model.md) and [contracts/](contracts/);
implementation detail belongs to `tasks.md`.

## Prerequisites

- Rust 1.95.0 (`rust-toolchain.toml`; if the shell exports
  `RUSTUP_TOOLCHAIN`, run with `RUSTUP_TOOLCHAIN=1.95.0`).
- C++ toolchain for the vendored Luau build (unchanged from 009).
- macOS host with a signed-in Premium account for the manual scenarios
  (Constitution Governance › Manual Scenario Sign-Off); two tracks the
  agent can tell apart by ear (call them **A** and **B**), one of them
  a typical mixed-music track.
- No env var is needed to see Key & Tempo or the Getting Started card —
  both are host defaults. `MODPLAYER_CONFIG_DIR=$(mktemp -d)` gives a
  fresh install (M8).

## Automated gates (green before any manual scenario)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
scripts/check-license-headers.sh
MODPLAYER_UPDATE_API_REFERENCE=1 cargo test -p modplayer-capability-gateway --test api_reference && git diff --exit-code docs/plugin-api/v1.md
```

Feature suites (all inside `cargo test --workspace`):

| Suite | Proves | Spec / contract |
|---|---|---|
| `cargo test -p modplayer-capability-gateway --test api_reference` | schema `1.4`; "Node parameters" section; `docs/plugin-api/v1.md` regenerated and identical | FR-020, Constitution IX; api §8 |
| `cargo test -p modplayer-capability-gateway --test manifest` | `api = "1.4"` accepted, `"1.5"` refused | api §1 |
| `cargo test -p modplayer-plugin-runtime --test bindings` | `set_param` name/bool/enum forms and refusals; `list_chain().params`; **`effect_chain_changed` deliverable for plugin-owned nodes (R4 regression)** | FR-020; api §3–4, §8 |
| `cargo test -p modplayer-core --test effects_model` | wire names ⇄ schema; revision bumps only on change; mode id → `set_mode` | FR-020; api §8 |
| `cargo test -p modplayer-core --test controller_effects` | `tempo_step`/panel edit → one coalesced event carrying targets | FR-013, FR-020, SC-009 |
| `cargo test -p modplayer-core --test controller_plugins_permissions` | `apply.rs` name/enum resolution and `invalid_argument` refusals | api §3.1 |
| `cargo test -p modplayer-core --test bundled_key_tempo` | package valid, licence copies, `@key` coverage, schema-only call scan, `packages()` order | FR-001, FR-002, FR-017, SC-008; plugin §1 |
| `cargo test -p modplayer-core --test controller_key_tempo` | every plugin-contract §7 scenario end to end on `FakeBackend` + `ScriptedHost` | US1–US3, FR-003–FR-016, SC-003–SC-005, SC-009 |
| `cargo test -p modplayer-core --test plugins_manifest_discovery` | both bundled plugins Active with grants, no fixtures needed; 1.0–1.3 fixtures still load | FR-019, SC-008; card §5 |
| `cargo test -p modplayer-core --test settings` / `--test persist` | `[onboarding]` round trip, absent ⇒ false, survives sign-out | FR-018; card §4 |
| `cargo test -p modplayer-ui --test library_view` / `--test first_launch` | card shown until Dismiss; persists across relaunch; hidden in detail view; tutorial outcome | FR-018, SC-007; card §1–2 |
| `cargo test -p modplayer-ui --test plugin_panels` / `--test accessibility` / `--test fluent_keys` | Key & Tempo panel and the card keyboard-operable with names; every string key resolves | FR-017, FR-018, SC-006 |
| `cargo test -p modplayer-core --test controller_section_loop` / `--test bundled_section_loop` | Section Loop unchanged except the "two bundled packages" expectation | SC-008 second clause |

Expected: every suite green on ubuntu / macos / windows; the 008 latency
and ramp/crossfade tests and the 009/010/011 fixture suites pass
**unchanged**.

## Launch for manual scenarios

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer &
# M8 only (fresh install):
MODPLAYER_CONFIG_DIR=$(mktemp -d) ./target/debug/modplayer &
```

Drive with the Governance recipe (Quartz `CGEventPost`, `screencapture
-l <windowid>`); helper scripts under `target/manual-walk/`. Evidence:
one screenshot per scenario, the Effect Chain panel visible where the
expected column names it, plus the app log lines tagged
`plugin org.modplayer.key-tempo`.

## Manual scenarios (executed by the implementing agent; results recorded in tasks.md)

| # | Steps | Expected | Spec |
|---|---|---|---|
| **M1** Transpose | Play track A. Open Now Playing; the docked "Key & Tempo" panel is present. Click **Key down** twice. Open the Effect Chain panel. | Pitch audibly drops a whole step at once (no perceptible lag), tempo and elapsed-time rate unchanged, no obvious artefacts; the Key slider reads −2, Fine tune 0; the chain lists exactly a Pitch Shift and a Time Stretch node, both labelled "Key & Tempo", pitch node `semitones = −2`. Click **Reset key** → pitch back to normal, slider 0. | US1-1/2, SC-001, FR-005, FR-016 |
| **M2** Fine tune and formant | Drag **Key** to −5, **Fine tune** to +30. Flip **Formant preservation** on. | Effect Chain shows `semitones = −4.70`; only the Pitch Shift node shows Formant on; the panel keeps −5 / +30 (not −4.7 rounded). | US1-3/4, FR-006, FR-008 |
| **M3** Slow down with Section Loop looping | In Section Loop set A/B around a passage and flip Loop on. In Key & Tempo drag **Tempo** to 60. | Loop keeps wrapping gaplessly at the slower tempo, pitch unchanged; Effect Chain still lists only Key & Tempo's two nodes. **Reset tempo** → 100 %, loop continues. | US2-1/2/6, SC-002, FR-015, FR-016 |
| **M4** `+` / `-` and the step note | With Tempo at 100 press physical `+` three times, then `-` once. Set **Tempo step** to 5; press `+`; click **Tempo up**. Set step back to 10. | Keys change tempo by 10 each time (120 after the sequence) and the panel's Tempo slider follows within a frame; Settings › Controls flags Key & Tempo's `+`/`-` rows as conflicting with the host's. At step 5: `+` still moves 10 (130) and the "Shortcut note" row reads "+ / - still steps by the default 10% until rebound in Settings › Controls"; **Tempo up** moves 5 (135). Step 10 → the note row is empty. | US2-3/4/5, FR-004, FR-013, SC-009 |
| **M5** Remember, skip, return | On track A set Key −2; flip **Remember for this track** on. Skip to track B. Skip back to track A. | Immediately after the flip, `~/Library/Application Support/ModPlayer/…/state/org.modplayer.key-tempo/<trackA>` (per 009's paths) holds `settings`. On B: pitch normal, Key 0, Remember off, "Track memory" row empty. Back on A: pitch −2 restored before the first audible second, Remember on, "Track memory" reads "Restored from this track's memory". Nudge Key to −3 → badge stays; stored `key` becomes −3. | US3-1/2/3, SC-003, FR-011, FR-012 |
| **M6** Don't remember; keep across tracks | On track B (never remembered) set Tempo 60; skip to A and back to B. Then on B with Remember off set Key −2 and flip **Keep across tracks** on; skip to a third track C (never remembered). | B returns at 100 % with no badge; Section Loop's markers on B (set some first) are intact. On C: Key stays −2, no badge, Remember off. Turn Keep across tracks off, skip to B and back → C-style carry-over stops; values reset. | US3-4/5, SC-004, FR-011, FR-012 |
| **M7** Remember off, disable/suspend | On A (remembered) flip Remember off; skip away and back. Then set Key −2 again, Remember on; Settings › Plugins → disable Key & Tempo; listen; re-enable. | After Remember off: A returns at 0 / 100 % with no badge (entry removed). Disable: pitch stays shifted (nodes orphaned, Effect Chain shows them "orphaned"), panel gone. Re-enable: **no audible change**, panel shows Key −2, Remember on, "Track memory" empty. | US3-6/7, SC-005, FR-005, FR-014 |
| **M8** Fresh install, Getting Started | Launch with a fresh `MODPLAYER_CONFIG_DIR`; complete Welcome, sign-in, device check. Then: click **Open plugin tutorial**; quit; relaunch; sign out and in; click **Dismiss**; quit; relaunch. | The Library view opens with a non-modal "Getting started" card at the top naming Section Loop (I / O / L / [ / ]) and Key & Tempo (+ / −) with **Open plugin tutorial** and **Dismiss**; the tutorial link opens the GitHub `docs/plugin-tutorial.md` URL in the system browser (404 is acceptable) and the card stays. The card is present again after quit → relaunch and after sign-out → sign-in. After Dismiss it is gone, and stays gone after relaunch. Settings › Plugins shows both bundled plugins installed and enabled; no approval sheet appeared at any point. | US4-1/2/3/4, SC-007, FR-018, FR-019 |
| **M9** Host edits are mirrored | In the Effect Chain panel drag Key & Tempo's pitch node `semitones` to +7, then set its Mode to Performance; with Remember on for the track. | Panel shows Key +7 / Fine tune 0, **Quality mode** toggle turned on by itself (auto-switch) and back off after the host Mode change; stored entry's `key` is 7. Remove the Time Stretch node in the Effect Chain panel → "Track memory" row reads "Effect node removed — disable and re-enable Key & Tempo to recreate it"; Tempo controls do nothing; Key controls still work; disable + re-enable recreates it after the pitch node. | FR-009, FR-020, Edge Cases, SC-009 |
| **M10** Accessibility | VoiceOver on; Tab through the Key & Tempo panel, then (fresh install) the Getting Started card. | Every control is reachable and announced by its label ("Key (semitones)", "Fine tune (cents)", "Tempo (%)", "Tempo step (%)", "Formant preservation", "Quality mode", "Remember for this track", "Keep across tracks", "Track memory", "Shortcut note", six buttons); the card's heading, two lines, link and Dismiss are announced. | SC-006, FR-017, FR-018 |

A deviation from any expected column is recorded on the scenario's
task in `tasks.md`, fixed with a regression test, and — if the
behaviour was mis-specified rather than mis-built — noted in
[research.md](research.md) as a post-walk correction.
