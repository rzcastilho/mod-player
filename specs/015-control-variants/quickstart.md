# Quickstart: Button, Toggle, and Meter Variants

**Feature**: 015-control-variants |
**Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md) |
**Contracts**: [control-variants.md](contracts/control-variants.md),
[interaction-states.md](contracts/interaction-states.md),
[meter-bands.md](contracts/meter-bands.md)

---

## 1. Automated gates

Run with the pinned toolchain — the shell may carry `RUSTUP_TOOLCHAIN`
overriding `rust-toolchain.toml` (Constitution, Manual Scenario Sign-Off):

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo fmt --all --check
RUSTUP_TOOLCHAIN=1.95.0 cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTUP_TOOLCHAIN=1.95.0 cargo test --workspace
RUSTUP_TOOLCHAIN=1.95.0 cargo deny check          # unchanged: no dependency edit
scripts/check-license-headers.sh                  # SPDX on the two new files
```

Suites this feature adds:

| Suite | Covers |
|---|---|
| unit `theme::controls::*` | B1, B2, B9, S1–S3, I1, I2, F1, F2, M1–M5, K3 |
| unit `theme::style::*` (extended) | I4 (FR-011a) — plus 014's existing tests **verbatim**, including `no_geometry_or_interaction_field_changes` |
| `tests/control_variants.rs` | B3–B7, B10, A5, L1, S4 (SC-001, SC-002, SC-009) |
| `tests/interaction_states.rs` | I3, I6, I7, F3–F5 (SC-003, SC-004, SC-009) |
| `tests/meter_bands.rs` | M6–M9, K1, K2, K5–K7 (SC-005, SC-009) |
| `tests/control_inventory.rs` | S5, S6 — the source-level boolean/selection inventory (SC-010) |

Suites that must pass **unmodified** (they are this feature's regression
net, not its output): `accessibility.rs`, `fluent_keys.rs`,
`design_token_literals.rs` (still **0** hits — SC-007),
`design_token_contrast.rs`, `design_token_roles.rs` (SC-006),
`plugins_view.rs`, `plugin_panels.rs`, `settings_plugins.rs`,
`queue_view.rs`, `effects_view.rs`, `markers.rs`, `rows.rs`,
`now_playing.rs`, `controls.rs`, `actions.rs` (SC-008), and
`modplayer-capability-gateway`'s `api_reference.rs` regeneration diff,
which must show **no diff** (Principle IX).

**Test-first (Constitution VIII)**: every suite above lands red, asserting
[data-model.md](data-model.md)'s values, **before** the module and the
call-site conversions that satisfy it. 014's Phase 2 inverted that order
once and had to record the deviation — do not repeat it.

---

## 2. Build and launch

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer
./target/debug/modplayer                     # background
```

Debug-only toggles that need a relaunch with the variable set:
`MODPLAYER_CONFIG_DIR=$(mktemp -d)` (fresh first launch → the Welcome
screen, needed by **M1**), `MODPLAYER_LIBRARY_FIXTURE=large` (populated
rows, needed by **M5**).

Helper scripts live **inside the worktree** at `target/manual-walk/`
(gitignored) — the scope-guard hook denies writes to the scratchpad,
`/tmp` and the memory directory.

Drive and capture per the constitution's recipe: locate the window with
Python `Quartz.CGWindowListCopyWindowInfo` (owner `modplayer`, name
`ModPlayer`), drive with `CGEventPost`, capture with
`screencapture -x -o -l <windowid> <png>`, then read the image back and
judge the scenario from it plus the app log. For M2/M4/M6 a
`target/manual-walk/pixel.py` that samples named points (as 014's
`contrast.py` did) is the evidence, not an impression.

---

## 3. Manual scenarios (executed by the implementing agent)

Each scenario records **pass / deviation with evidence** on its `tasks.md`
task; any deviation is written back into this file and
[research.md](research.md) (Governance › Manual Scenario Sign-Off).

| id | Scenario | Expected | Gates |
|---|---|---|---|
| **M1** | Launch with a fresh `MODPLAYER_CONFIG_DIR`. Capture the Welcome screen. | "I understand, continue" is filled `accent` with an `text.on-accent` label; "Decline" is the default surface with a `divider` outline; **exactly one** primary button in the window. | US5, B5, SC-002 |
| **M2** | Open Now Playing → Markers panel with at least one marker. Capture the header, then press "Clear all markers" and capture the confirmation. | "Clear all markers" carries the `danger` outline and `danger` label and is separated from "New loop region" by ≥ 16 px; in the confirmation, "Yes" is destructive, "No" is default, with the gap between them. | US1, B6, B10, SC-001 |
| **M3** | Open the Effect Chain with ≥ 1 node; open a plugin panel row; open Settings › Account. Capture each. | "Remove", "Disable" and "Sign out" each render destructive and each is visually distinct from every other button in the same view; the sign-out modal's confirming button is destructive too. | US1, B6, SC-001 |
| **M4** | Tab keyboard focus onto the **selected** library row, then onto a switch that is on. Capture both. | The 2 px `accent` ring is outside the control with a 1 px gap of surface between; the interior `accent` selection fill remains visible as a separate signal. Sample the gap pixel — it must be the surface colour, not accent. | US3, F1–F3, SC-004 |
| **M5** | With `MODPLAYER_LIBRARY_FIXTURE=large`, rest the pointer on a library row, a queue row, a plugin row, a marker row and a switch. Capture each hovered and unhovered. | Each surface visibly changes; the switch's hover is distinct from its on/off colour; no row moves or resizes between the two captures. | US3, I1, I6, SC-003 |
| **M6** | Toggle Queue, Effects and Transport on and off; toggle a plugin's Enabled; toggle an effect's Bypass. Capture on and off for each. | Each renders as a pill with a thumb; the thumb is at the opposite end in the two states; the on state is unmistakable and unlike any button. | US2, S1–S4, SC-010 |
| **M7** | Press and hold the pointer on a button, then on a switch. Capture mid-press. | Pressed is distinct from both resting and hovered, and stronger than hovered. | US3, I3 |
| **M8** | Play audio and drive the output level from silence up through −6 dBFS and past the limiter ceiling. Capture the Now Playing peak meter at three levels. | Fill progresses `positive` → `warning` → `danger`; at or above the ceiling the **rightmost** filled column is `danger`; the −6 dB and 0 dB ticks are visible, the ceiling tick is the wider one, and each tick's colour follows the fill (gap through the band, tick on the empty track). | US4, M6–M8, K1–K3, SC-005 |
| **M9** | Capture the Effect Chain's pre-/post-chain level pair under signal. | Peak **and** RMS sub-bars band identically, the RMS bar is not dimmed, both marks are present, and both `mono` readouts stay digit-aligned with the readouts above them. | US4, M9, R1, SC-006 |
| **M10** | Switch Settings › Appearance between Light and Dark with a destructive button, a switch and a meter on screen. Capture both themes. | Every variant, switch state, interaction fill, ring and band renders from the theme's own roles in both themes; nothing is unreadable and nothing keeps a light-theme colour in dark. | FR-019, L3 |

---

## 4. Execution notes and known host risk

**Carried forward from 014 — read before starting.** 014's M1/M2 could not
be executed on the 2026-09-22 host: `App::launch_step`'s sign-in gate
hides Library and Settings until an account session exists, and the host's
session was revoked. The same gate stands between this feature and
**M2–M6, M8–M10**. Before starting the manual walk, verify the app
reaches `LaunchStep::Main`; if it does not, the correct outcome is to
record the scenarios as **not executed** with the reason, mark the
affected user story's checkpoint **not reached**, and write it into
plan.md § Complexity Tracking — **not** to declare the stories verified on
automated evidence, and **not** to fabricate a signed-in state or add a
demo mode to reach the rows (a source change outside this feature's
scope, FR-017).

**M1 is reachable regardless** — the Welcome screen precedes the sign-in
gate, so US5's primary-variant evidence can always be captured — **unless
the window itself never forms** (see the 2026-09-23 update below, where it
did not).

**Update, 2026-09-23 (this feature's own manual walk, tasks.md T058)**: on
that day's host, the block was strictly worse than 014's revoked session.
The app process never reached `eframe::run_native` at all: its main thread
parked indefinitely in `modplayer::main` → `AccountService::launch` →
`AccountService::launch_resolve_session` → `KeyringSecureStore::get` →
`SecKeychainFindGenericPassword` → `mach_msg_trap` (confirmed with
`sample <pid> 1`), for 60+ seconds of active `%CPU` (not an idle/asleep
process) across two independent launches — one plain background launch,
one via `launchctl asuser 501` to rule out a foreign security-session
cause. `Quartz.CGWindowListCopyWindowInfo(kCGWindowListOptionAll, …)`
showed **zero** windows for the process's PID at any point in either run.
Calling the same Keychain lookup directly (`security
find-generic-password -s ModPlayer -a session-credential -w`) returned in
under a second with "item not found" (exit 44), so the hang is specific to
the in-process `SecKeychainFindGenericPassword` call — most likely an
unattended keychain-access-consent prompt from the ad-hoc-signed dev
binary that has no session to render or dismiss it in a non-interactive
launch. Because window creation is downstream of this call, **M1 was not
reachable either** that day, contrary to the paragraph above. All of
M1–M10 were recorded **not executed**, with no keychain state modified and
no signed-in state fabricated to route around it (Governance). See
plan.md § Complexity Tracking (D11) and research.md § R16 addendum.

**Live token for `#[ignore = "manual"]` probes** (not needed by any
scenario here, listed for completeness):
`security find-generic-password -s ModPlayer -a session-credential -w | jq -r .access_token`
— never print it; filter `bearer|access_token` from captured output.
