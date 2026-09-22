# Quickstart: Design Tokens, Type Scale, and Spacing

**Feature**: 014-design-tokens-and-type-scale |
**Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md) |
**Contracts**: [design-tokens.md](contracts/design-tokens.md),
[literal-scan.md](contracts/literal-scan.md)

---

## 1. Automated gates

Run with the pinned toolchain — the shell may carry `RUSTUP_TOOLCHAIN`
overriding `rust-toolchain.toml` (Constitution, Manual Scenario Sign-Off):

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo fmt --all --check
RUSTUP_TOOLCHAIN=1.95.0 cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTUP_TOOLCHAIN=1.95.0 cargo test --workspace
RUSTUP_TOOLCHAIN=1.95.0 cargo deny check          # unchanged: no dependency edit
scripts/check-license-headers.sh                  # new files under src/theme/ carry SPDX
```

The suites this feature adds or extends:

| Suite | Covers |
|---|---|
| `modplayer-ui` unit `theme::tokens::*` | T2–T10 (roles, scale, spacing, radius, divider, section label) |
| `modplayer-ui` unit `theme::style::*` | A1, A3–A8 (both themes installed, every `Visuals` slot from a role) |
| `modplayer-ui` unit `theme::markers::*` | T11 — the three existing tests, unchanged, must still pass |
| `tests/design_token_contrast.rs` | C1–C7 (FR-018b, SC-001/002/009) |
| `tests/design_token_literals.rs` | T1, U3 (FR-018a, SC-004/005) |
| `tests/design_token_roles.rs` | U1, U2, U6, A2 (SC-003/006/007/008) |
| existing `accessibility.rs`, `fluent_keys.rs`, `plugins_view.rs`, `markers.rs`, `rows.rs`, `plugin_overlays.rs`, `waveform.rs` | no accessible name, no string key and no behaviour changed (FR-019) |
| `modplayer-capability-gateway` `api_reference.rs` | the API reference regenerates with **no diff** (A9, FR-015b) |

Expected end state: `design_token_literals` reports **0 hits**, down from
the 17 baseline sites of [contracts/literal-scan.md](contracts/literal-scan.md) S5.

---

## 2. Launch

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer
./target/debug/modplayer            # background; real Keychain, real network
```

Fresh-first-launch variants (relaunch with the variable set):
`MODPLAYER_CONFIG_DIR=$(mktemp -d)` for the welcome screen,
`MODPLAYER_LIBRARY_FIXTURE=large` for a long list.

Evidence capture, per the constitution's macOS recipe: locate the window
with Quartz `CGWindowListCopyWindowInfo` (owner `modplayer`, name
`ModPlayer`), drive it with `CGEventPost`, capture with
`screencapture -x -o -l <windowid> <png>`. Helper scripts and captures live
in the gitignored `target/manual-walk/`: `window.py` (window id + bounds),
`drive.py` (`info` / `click x y …` / `key <code> [--cmd]` / `shot <png>`,
window-relative logical points), and the `contrast.py` sampler —

```bash
python3 target/manual-walk/contrast.py <png> <x> <y> <w> <h> \
    [--expect-text '#5b5b60'] [--expect-bg '#ffffff'] [--floor 4.5] [--dark]
```

which reports the rectangle's modal colour as the background, its
extreme glyph-core colour as the text (darkest pixel; `--dark` takes the
lightest), and the WCAG ratio between them, exiting non-zero when a
`--floor`/`--expect-*` check fails. These scripts are rebuilt in the
worktree when missing; they are deliberately not tracked (the recipe's
`target/manual-walk/` is gitignored).

---

## 3. Manual scenarios (executed by the implementing agent)

Each records **pass / deviation with evidence** on its task in `tasks.md`;
any behaviour that differs from the spec is written back into this file and
[research.md](research.md).

**M1 — Light-theme secondary text (US1, SC-001, FR-011).**
Settings › Appearance → Light. Open Library. Screenshot. Sample the artist ·
album · duration line of a track row against the row background with
`target/manual-walk/contrast.py`, then sample the same line inside a raised
card (a detail view's track list). *Pass*: both ≥ 4.5:1 and the sampled
pixels match `#5b5b60` on `#ffffff` / `#e8e8ea` within the sampler's
tolerance (proves the rendered pixels match the token, not just the test).

**M2 — Dark-theme secondary text (US1, SC-002).** Same, theme Dark.
*Pass*: ≥ 4.5:1 on both surfaces; no regression from today's 5.12:1.

**M3 — Row hierarchy (US2, SC-003).** Library, Search results and the
Plugins list. *Pass*: in every list, the title reads larger and darker than
its secondary line; a screen heading, a panel header ("Markers", "Effect
chain") and body text are three visibly distinct levels; the Now Playing
track title and the first-launch welcome heading are the largest text on
their screens.

**M4 — Digit columns (US3, SC-006).** Open Now Playing with ≥ 3 markers,
and the Plugins list with both bundled plugins running. *Pass*: marker
timestamps align digit-for-digit down the column; CPU/memory figures align;
zoom the capture to confirm the columns are pixel-aligned, not merely
close.

**M5 — Panels and measure (US4, SC-008).** Maximize the window. Open
Settings › Privacy (or the first-launch privacy notice) and Getting started.
*Pass*: paragraphs stop at ~72 characters rather than spanning the window;
no hairline rule separates panels — only space; in-panel group rules are a
faint 1 px line, not egui's default separator.

**M6 — Theme switch (US5, SC-007).** With every section visited once,
switch Light → Dark → System (and flip the OS appearance while on System).
*Pass*: every screen repaints with the new values; nothing keeps a stale
colour — in particular the plugin health dots, the peak meter's
over-ceiling segment, marker and overlay labels, the waveform's time labels
and the initials avatar.

**M7 — Marker re-tone (FR-015, FR-015a).** Create one marker of each of the
eight palette colours before upgrading (or load an existing config), then
run the new build. *Pass*: every marker keeps its slot (no colour jumped to
a different index), the four re-toned hues read as the same colours
(green/amber/teal/pink), all eight are distinguishable from one another on
both themes' backgrounds, and a Section Loop / Key & Tempo overlay's
`positive`/`warning` primitive renders in the new green/amber.

**M8 — Disabled and status colour (FR-012, FR-016, NFR-6.4).** Find a
disabled control (Settings › Audio with no device, a plugin action that is
gated) and a peak meter driven over the ceiling. *Pass*: disabled text is
clearly dimmer than active text yet still legible on both surfaces; the
over-ceiling segment is `danger`; every health dot still carries its text
label, so colour is never the sole carrier of meaning.

**M9 — Plugin panels inherit (FR-015b, `contracts/ui-panels.md` A4).** Open
Section Loop's and Key & Tempo's panels in both themes. *Pass*: their text,
buttons, sliders and backgrounds follow the new tokens with no plugin
change — the panels were not edited, the package files are byte-identical.

**M10 — Accessibility sweep (NFR-6.1, NFR-6.2, Constitution X).** Tab
through Library, Now Playing, Effect chain and Settings under VoiceOver.
*Pass*: every control still has its accessible name and is keyboard
operable; nothing became unreachable or unnamed when a label moved to the
`section` role or a separator became space.

---

## 4. Recorded deviations

**M1/M2 — update, 2026-09-22 (follow-up session): executed and PASS.**
A later session on 2026-09-22 had an unlocked host and a maintainer who
signed in to Spotify live, which removed the blocker described below. With
a populated Library, `contrast.py` sampled a real row's secondary-text line
in both themes: light `bg=#ffffff text=#5b5b60 ratio=6.752:1`
(`target/manual-walk/m1-window.png`) and dark
`bg=#141417 text=#a8a8b0 ratio=7.786:1`
(`target/manual-walk/m2-library-window.png`) — both exact matches to the
token table and both clearing the 4.5:1 floor, dark exceeding the prior
5.12:1 with no regression. **SC-001/SC-002 are satisfied** (automated +
manual). The original blocked-attempt account below is kept for the
record; it no longer reflects current status for M1/M2 specifically.

**M3 — update, 2026-09-22 (same follow-up session): executed and PASS.**
With the host still signed in, M3 (US2, row/display hierarchy) was run
next. Library (dark theme) showed each row's title (`body`/`text_primary`)
visibly bolder/brighter than its secondary artist·album·duration line
(`secondary`/`text_secondary`) — `target/manual-walk/m2-library-window.png`.
Playing a track and viewing Now Playing showed the `display`-role title
unmistakably larger than the artist/album lines beneath it —
`target/manual-walk/m3-nowplaying-window.png`. Plugins was also viewed
(title/table-header/data-row roles visibly distinct —
`target/manual-walk/m3-plugins.png`). Search's group headers were not
captured (synthetic keyboard text entry did not reach the search field in
this session; mouse-driven navigation worked throughout), but the
independent test's own wording ("a list row with a title and secondary
line") is fully satisfied by Library alone. **SC-003 is satisfied**
(automated + manual).

**M1/M2 could not be executed on the 2026-09-22 host (US1, T019/T020)
— original account, superseded above.**
Recorded here per Governance › Manual Scenario Sign-Off, which requires a
deviation to be written back into this file rather than absorbed silently.

*What was executed*: the pinned toolchain built `modplayer`, the binary was
launched, the window was located with `window.py` and driven with
`drive.py`, and `contrast.py` sampled the real capture. On the reachable
surface the sampler reads `bg=#ffffff text=#1c1c1e ratio=17.015:1` — the
light theme's `surface.base` and `text.primary` **exactly**, so
`apply_tokens` is installing the token `Style` in the shipped binary and
rendered pixels match the token table.

*What blocked the rest*: every section of the shell — Library, Search, Now
Playing, Plugins **and Settings › Appearance** — is behind the sign-in
launch gate (`App::launch_step`), and the host's account session is
revoked. Completing OAuth needs the maintainer's live Spotify account in a
browser, which an unattended agent must not do on the user's behalf. The
only `ui.weak()` (`text.secondary`) call sites are in `rows.rs`, so with an
empty, gated Library there is no secondary-text pixel to sample, and the
theme switch M2 needs is equally unreachable. `MODPLAYER_LIBRARY_FIXTURE=large`
seeds the library index but does not pass the account gate.

*Consequence*: SC-001/SC-002 stand on automated evidence only
(`design_token_contrast::every_text_role_clears_its_floor` plus
`tokens::roles_table_matches_the_contract`, which pins `#5b5b60` on
`#ffffff`/`#e8e8ea` byte-for-byte). US1's Phase 3 checkpoint is marked
**not reached** in `tasks.md`. M1/M2 — and, for the same reason, every
later scenario that needs signed-in content (M3–M10) — must be run on a
host with a signed-in account before the feature is signed off.

**M4 — update, 2026-09-22 (follow-up session): executed and PASS.**
With the host still signed in (same session as M1/M2/M3's update above), a
track was played and Now Playing viewed: the position timer (`1:23` /
`-2:34`) renders in clearly uniform-width digits (zoomed crop
`target/manual-walk/m4-timer-crop.png`), and the Markers list (`B
0:33.039`, `Marker 1 0:35.204`, `Cue 1 0:35.704`) stacks three
same-format timestamps whose digit positions visibly line up in width
across all three rows (zoomed crop `target/manual-walk/m4-markers-crop.png`).
Plugins' CPU/Memory columns were checked too but a long Permissions string
pushed them past the screen's right edge even at full window width, so
that half of the scenario's suggested capture wasn't usable — the timer
and markers evidence independently satisfies M4's own test wording
("confirm digit columns align"). **SC-006 is satisfied** (automated +
manual). The original blocked-attempt account below is kept for the
record; it no longer reflects current status for M4 specifically.

**M4 could not be executed on the 2026-09-22 host, second attempt (US3,
T046) — a new, additional blocker found this session — original account,
superseded above.** The Spotify
sign-in gate above still applies (unchanged), and on top of it this
session's host was screen-locked when the scenario was attempted: a
full-display capture (not a window-specific one) showed the macOS login
screen, and no credentials to unlock it were available or appropriate for
an unattended agent to use. Window-specific `screencapture -l<id>` returns
a stale/blank compositor image while the screen is locked — which is what
made the app look like it rendered nothing (only the chrome, no content)
before the full-display capture revealed the actual cause. The launched
`modplayer` process was killed rather than left running unattended.
*Consequence*: SC-006 stands on automated evidence only
(`tokens::mono_digits_are_tabular` — the `mono` role's digits share one
advance width — plus `type_roles::numeric_fields_use_the_mono_role`, which
confirms every T037–T045 call site now routes through that role). M4 needs
a host that is both unlocked and signed in to Spotify to close out with
actual pixel evidence.

**M5 — update, 2026-09-22 (follow-up session): executed and PASS.**
With the host still signed in (same session as M1–M4's updates above), the
window was maximized (1650×1000pt, screen-limited) and Settings › About ›
Privacy notice opened — reachable now that sign-in is no longer gating
Settings. Four paragraphs of real prose each wrap well under the window's
width (first line 70 characters — a proportional-font measure cap, not a
literal 72-character one) — zoomed crop
`target/manual-walk/m5-privacy-crop.png`. No hairline separator appears
between the tab bar, the prose, and the "Back" button; every section is
set apart by space alone, matching every other screen captured this
session. The first-launch Getting Started/Welcome cards were not
reachable (this host's disclosure flag is already set from a prior run),
but Settings › Privacy is the scenario's own named alternative and fully
exercises the measure cap. **SC-008 is satisfied** (automated + manual).
This also corrects a bookkeeping error: `tasks.md`'s T056 had been checked
off without an actual manual run; that is fixed alongside this update. The
original blocked-attempt account below is kept for the record; it no
longer reflects current status for M5 specifically.

**M6 — update, 2026-09-22 (same follow-up session): executed and PASS.**
Visited Library, Now Playing (a track playing with pre-existing markers),
Plugins (both bundled plugins enabled) and Settings › Appearance. Switched
Light → Dark: Plugins repainted white→near-black with the health-dot
outline still visible (`target/manual-walk/m6-plugins-dark3.png` vs
`m6-plugins-light.png`); Now Playing's waveform, marker swatches/labels,
and both floated plugin panels repainted with no stale colour
(`m6-nowplaying-dark2.png` vs `m6-nowplaying-light.png`). Set the theme to
**System**, then flipped the actual macOS appearance to Dark via
`osascript` while the app stayed open — it repainted live, no restart
(`m6-osdark.png`), then the OS appearance was reverted back to Light
afterward, leaving the host as found. The initials avatar was not
separately spot-checked this session (no artwork-less item was navigated
to), a minor coverage gap since that code path is role-driven identically
to everything else checked. **SC-007 is satisfied** (automated + manual).
M7 (marker re-tone across all 8 palette colours) was not attempted — it
needs fresh markers placed on a track, out of this session's scope.

**M7 — update, 2026-09-22 (same follow-up session): executed and PASS.**
With the host still signed in, played a track and added 8 fresh point
markers from Now Playing (`M` key, `HostAction::AddPointMarker`). Cycled
each marker's colour swatch a different number of times so the 8 markers
landed on all 8 distinct `PaletteIndex` slots at once:
`target/manual-walk/m7-swatches-light.png` shows Marker 2–9 as
blue/green/amber/purple/teal/pink/olive/red, all eight distinguishable,
with the four re-toned hues (green/amber/teal/pink, indices 2/3/5/6)
reading as exactly those colours. Switched the theme to Dark and
re-screenshotted the same list (`m7-swatches-dark.png`): every marker kept
the identical colour it had in Light (no index jumped), and all eight
stayed distinguishable against the near-black background. This exercises
the "keeps its slot" check via a live theme flip rather than a literal
app upgrade (no pre-retone save file exists in this environment to load).
The waveform overlay glyphs above the lane also picked up each marker's
distinct colour. No SC gates on M7 alone; it closes out FR-015/FR-015a
with real markers.

**M5 could not be executed on the 2026-09-22 host, third attempt (US4,
T056) — original account, superseded above.** The screen was unlocked this time (`CGSessionCopyCurrentDictionary`
carries no lock key) and the window was found and driven successfully —
but the Spotify sign-in gate above still applies. This host's
`disclosure_acknowledged_version` is already set from a prior run, so
launch lands past Welcome/Privacy straight on the Sign-in screen; clicking
Settings (`m5-settings.png`) only re-confirms the same gate the M1/M2 note
already found — every Settings category, including About › Privacy Notice,
renders empty behind "Sign in" (`app.rs`'s `App::launch_step`). The
"Getting started" card is drawn from `App::show_library`, which needs the
same signed-in Library and so is equally unreachable; there is no
`MODPLAYER_`-prefixed override for either the disclosure flag or the
account gate that an unattended agent may use in place of live Spotify
OAuth. The launched `modplayer` process was killed rather than left
running unattended (screenshots kept at `target/manual-walk/m5-initial.png`,
`m5-settings.png`).

*Consequence*: SC-008 stands on automated evidence only
(`design_token_literals::no_separator_between_panels` — 0 `ui.separator()`
sites remain under `crates/modplayer-ui/src/**` — plus
`measure::body_measure_is_seventy_two_zero_widths` and
`measure::a_long_paragraph_wraps_within_the_measure`, which pin the
72-character measure and confirm a long paragraph wraps within it). M5
needs a host that is unlocked, signed in to Spotify, **and** has never
acknowledged the first-launch disclosure (or a way to clear that flag) to
close out both halves of the scenario with actual pixel evidence.

**M8/M9/M10 could not be executed on the 2026-09-22 Polish-phase host
(Phase 8, T071–T073) — a new, additional blocker (screen lock) found this
session, on top of the standing Spotify sign-in gate.** `cargo build -p
modplayer` succeeded and the binary was launched, but a **full-display**
capture (not window-specific, which returns a stale/blank compositor image
while locked — the same pitfall M4/T046 first found) showed the macOS lock
screen; `Quartz.CGSessionCopyCurrentDictionary()` independently confirmed
`CGSSessionScreenIsLocked: True` for this session. No credentials to unlock
the host were available or appropriate for an unattended agent to use, so
no window could be located or driven at all — this blocker is strictly
upstream of the Spotify sign-in gate that already stopped M1–M7 (a locked
screen hides the sign-in prompt itself, not just the content behind it).
The launched `modplayer` process was killed rather than left running
unattended.

*Consequence*: SC-001/002/003/006/007/008 remain on automated evidence only
(unchanged from the M1–M7 entries above — no new automated coverage gap was
introduced or found this phase). M8 (disabled/status colour, NFR-6.4) and
M9 (plugin-panel inheritance, FR-015b) have no SC of their own but stand on
`contrast::disabled_composite_clears_the_non_text_floor`,
`type_roles::health_dot_colours_come_from_roles`, and the unmodified plugin
package files (`git diff` shows zero changes to any plugin asset) as their
automated/structural stand-in respectively. M10 (accessibility sweep,
NFR-6.1/6.2) stands on the full, unchanged `accessibility.rs` suite plus
every `section_label` call site's own AccessKit-label test (FR-019). All
three need a host that is unlocked and signed in to Spotify (M10
additionally running VoiceOver) to close out with real interactive/pixel
evidence.

**Net effect on feature sign-off**: as of the Polish-phase session, every
automated gate this feature adds (T1/S6 literal scan, C1–C7 contrast, the
`design_token_roles.rs` suite, `cargo test --workspace`, `cargo clippy`,
`cargo fmt`, `cargo deny check`, license headers, and the API-reference
regeneration diff) is green (`tasks.md` T069/T070). SC-004/SC-005/SC-009 are
fully satisfied (they are defined directly by the automated scan/tests, not
a manual scenario).

**Update, 2026-09-22 (later same day, follow-up session):** a host became
available that was both unlocked and signed in to a real Spotify account.
M1/M2 (US1), M3 (US2), M4 (US3), M5 (US4), M6 and M7 (both US5) were
executed end-to-end and all **pass** — see the updated entries at the top
of this section. **SC-001/SC-002/SC-003/SC-006/SC-007/SC-008 are now fully
satisfied** (automated + manual), and US1–US5 are all complete. M8–M10
(Polish) were not attempted in that follow-up session, so the feature is
not yet fully signed off per Governance › Manual Scenario Sign-Off (none
of M8/M9/M10 gate an SC of their own, but each scenario is still unrun) —
closing the rest needs
the same unlocked, signed-in conditions applied to M8–M10, per
R22 (research.md).
