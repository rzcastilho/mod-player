---

description: "Task list for 022-waveform-legibility"
---

# Tasks: Waveform Legibility and Scrub Feedback

**Input**: Design documents from `/specs/022-waveform-legibility/` (plan.md, spec.md, research.md, data-model.md, contracts/ui-waveform-legibility.md, contracts/analysis-rms.md, quickstart.md)

**Tests**: Included. Constitution Principle VIII ("Test What the NFRs Promise") and plan.md's Constitution Check require tests written first (R15); this is not optional for this feature.

**Organization**: Tasks are grouped by user story (spec.md priorities) so each story is independently implementable and testable. Contract IDs (`WL*`, `AR*`), success criteria (`SC-*`) and the spec.md functional requirements each task implements (`FR-*`, via **Implements**) are cited per task.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different files, no dependency on an incomplete task in the same phase — safe to run in parallel.
- **[Story]**: `US1`…`US4`, mapping to spec.md's four user stories. Setup/Foundational/Polish tasks carry no story label.
- All commands below are prefixed `rtk` per this repo's CLAUDE.md.

## Path Conventions

Single Cargo workspace, crate-per-component (plan.md Project Structure). Crates touched: `crates/modplayer-audio-source`, `crates/modplayer-core`, `crates/modplayer-ui`. No new crate (Principle X).

---

## Phase 1: Setup

**Purpose**: Confirm the starting point is clean before any change lands.

- [X] T001 Confirm workspace baseline is green: `rtk cargo build --workspace` and `rtk cargo test --workspace` from repo root; record any pre-existing failure before proceeding (none expected). **Implements**: FR-010, FR-015.
  - Result: build clean (0 warnings). One failure found — `crates/modplayer-ui/tests/plugin_overlays.rs::above_markers_below_playhead_order` asserted the old single-stroke `strong_text_color()` playhead, stale since T013's cased playhead (WL3). Fixed the test to assert against `waveform_roles(...).playhead_core` (the topmost of the casing+core pair) instead of re-typing an assumption already encoded in `theme/waveform.rs`. Full re-run: `cargo test --workspace` → 2055 passed, 11 ignored, 0 failed.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The RMS analysis pipeline (blocks US4 fully) and the `WaveformRoles` theme-token module (blocks US1's playhead tokens and US3's hover tokens) — per plan.md's Implementation Order step 1. **US2 does not depend on this phase** and may proceed in parallel (its own token file, `theme/markers.rs`, is independent — see Phase 4).

**⚠️ CRITICAL**: US1, US3 and US4 cannot be completed until this phase is done. US2 can start immediately.

### Analysis pipeline (AR1–AR5)

- [X] T002 [P] Add RMS test vectors to `crates/modplayer-audio-source/tests/decoded.rs`: silence → `0`, DC `0.5` → `64`, full-scale square `±1.0` → `127`, out-of-phase stereo (`L=+0.5,R=−0.5`) → `64`, plus a proptest `rms ≤ max(|min|, |max|)` (AR1; must fail — `rms` field does not exist yet). **Implements**: FR-013.
- [X] T003 Add `rms: u8` field to `PeakBucket` and compute it in `DecodedStore::fold_peaks` in `crates/modplayer-audio-source/src/decoded.rs`: `round(clamp(sqrt((Σl²+Σr²)/(2n)),0,1)×127)` per bucket, stack accumulator only, no allocation (AR1; data-model §1 B1–B3). Depends on: T002. **Implements**: FR-013, FR-014.
- [X] T004 [P] Add refold unit tests to `crates/modplayer-core/src/analysis/peaks.rs`: weighted quadratic-mean RMS across the ×8 ladder, uniform-RMS-track invariant (±1 at every level) (AR2; must fail). **Implements**: FR-013.
- [X] T005 Implement the RMS fold rule in `refold_coarser_levels`, `crates/modplayer-core/src/analysis/peaks.rs`: `rms_c = round(sqrt(Σ wᵢ·rmsᵢ² / Σ wᵢ))`, `wᵢ` = finer bucket frame count, `u64` accumulator (AR2; data-model §2). Depends on: T003, T004. **Implements**: FR-013, FR-014.
  - Already present in `refold_coarser_levels` (landed alongside T003/T004). Verified: `cargo test -p modplayer-core --lib analysis::peaks` → 5 passed.
- [X] T006 [P] Add cache tests to `crates/modplayer-core/tests/analysis.rs`: `RMS8` round-trip, v1 (`ANALYZER_VERSION=1`) entry rejected as `VersionMismatch`, missing `RMS8` → `Malformed`, per-level count mismatch vs `WAVE` → `Malformed`, `rms > 127` → `Malformed`, 10-min entry ≤ 1 048 576 bytes at 44.1 kHz and 48 kHz; extend the existing `cache_rejects_any_truncation` proptest to v2 entries (AR3–AR5; data-model §2.1; must fail). **Implements**: FR-013.
- [X] T007 Bump `ANALYZER_VERSION` to `2` in `crates/modplayer-core/src/analysis/mod.rs`; implement `RMS8` section encode (writer) and decode + validation V1–V5 in `crates/modplayer-core/src/analysis/cache.rs`, `FORMAT_VERSION` unchanged at `1` (AR3, AR4; data-model §2.1). Depends on: T005, T006. **Implements**: FR-013, FR-014.
- [X] T008 Fix every existing `PeakBucket { min, max }` struct literal to include `rms` across `crates/modplayer-audio-source/tests/decoded.rs`, `crates/modplayer-core/src/analysis/{cache.rs,peaks.rs}`, `crates/modplayer-core/tests/analysis.rs`, `crates/modplayer-ui/src/waveform/paint.rs` (plan.md Structure Decision; `modplayer-audio-source-synthetic`'s `PeakBucket::default()` needs no edit). Depends on: T003. **Implements**: FR-013.

### Theme tokens (R5, W1–W3)

- [X] T009 [P] Add contrast/distinctness tests to `crates/modplayer-ui/tests/design_token_contrast.rs`: W1 — the four fill tokens (played/unplayed × peak/average) pairwise distinct in each of the 4 appearances; W2 — `max(ratio(core,b), ratio(casing,b)) ≥ 3.0` for every backdrop *b* in the R9 set (surface_base, 4 fill tokens, placeholder composite, overview highlight composite, every `MARKER_PALETTE` entry × {armed-fill α0.25, hatch α0.6, idle-fill}, palette colours themselves) in all 4 appearances (data-model §3 W1/W2; must fail — `WaveformRoles` does not exist yet). **Implements**: FR-002, FR-003, FR-012.
- [X] T010 Create `crates/modplayer-ui/src/theme/waveform.rs` (new): `WaveformRoles` struct with 4 static tables (`LIGHT`, `DARK`, `LIGHT_HIGH_CONTRAST`, `DARK_HIGH_CONTRAST`; values per data-model §3 table) and selector `waveform_roles(roles: &Roles) -> &'static WaveformRoles`; paint constants `PLAYHEAD_CORE_WIDTH=2.0`, `PLAYHEAD_CASING_WIDTH=4.0`, `HOVER_LINE_WIDTH=1.0`, `PLACEHOLDER_ALPHA=0.4`, `DETAIL_HIGHLIGHT_ALPHA=0.25` (data-model §3.1); doc comments + doctest on `WaveformRoles`/`waveform_roles` (Principle VII). Depends on: T009. **Implements**: FR-001, FR-002, FR-003, FR-012, FR-019.
- [X] T011 Re-export the `waveform` token module from `crates/modplayer-ui/src/theme/mod.rs`. Depends on: T010. **Implements**: FR-012.

**Checkpoint**: `rtk cargo test -p modplayer-audio-source --test decoded`, `rtk cargo test -p modplayer-core analysis`, `rtk cargo test -p modplayer-ui --test design_token_contrast` all green. US1, US3, US4 may now build on this; US2 has been proceeding independently since Phase 1.

---

## Phase 3: User Story 1 - Know where you are, even in the dark theme (Priority: P1) 🎯 MVP

**Goal**: The playhead is a ≥2 px core + casing stroke that meets ≥3:1 contrast (core or casing) against every backdrop in FR-003, in all four appearances, on both views (WL3).

**Independent Test**: Play a track in the dark theme, sample the playhead colour against the fill directly behind it, confirm ≥3:1; repeat in the light theme.

### Tests for User Story 1

- [X] T012 [P] [US1] Add playhead shape-capture tests to `crates/modplayer-ui/tests/waveform.rs`: two `line_segment`s at the playhead x — casing (`PLAYHEAD_CASING_WIDTH`, `playhead_casing`) then core (`PLAYHEAD_CORE_WIDTH`, `playhead_core`); playhead paints topmost (above fill, loop shading, hover line); present on both overview and detail (WL3; must fail). **Implements**: FR-003, FR-019.

### Implementation for User Story 1

- [X] T013 [US1] Replace the single `strong_text_color` playhead stroke in `crates/modplayer-ui/src/waveform/paint.rs` with the casing-then-core pair from `waveform_roles(roles)` (WL3). Depends on: T010, T012. **Implements**: FR-003, FR-012.

**Checkpoint**: `rtk cargo test -p modplayer-ui --test design_token_contrast --test waveform` green. SC-001 satisfied — User Story 1 is independently shippable.

---

## Phase 4: User Story 2 - Know where the loop is, and whether it's armed (Priority: P1)

**Goal**: Every complete loop region is shaded on the waveform from its own `armed` flag (never from "current"): armed-active fill, armed-inactive hatch (006, unchanged), or idle (1 px outline + ≤half-alpha fill) for every other region; armed region drawn last (WL4).

**Independent Test**: Define two loop regions, arm one, confirm both are shaded with visibly different treatments on both views; disarm/rearm the other and confirm shading follows the armed state, including when the armed region is not the current one.

**Note**: Independent of Phase 2 — may run in parallel with it.

### Tests for User Story 2

- [X] T014 [P] [US2] Add tests to `crates/modplayer-ui/tests/markers.rs`: `loop_shade(armed, loop_state)` truth table (data-model §7); every complete-span region shaded, incomplete regions draw nothing; an armed non-current region gets the armed treatment; a disarmed current region gets the idle treatment; armed region always drawn last; idle fill α ≤ half armed-active α (WL4; must fail — `loop_shade` does not exist yet). **Implements**: FR-004, FR-005, FR-018.

### Implementation for User Story 2

- [X] T015 [P] [US2] Move loop-shading literals into named constants in `crates/modplayer-ui/src/theme/markers.rs`: `LOOP_ARMED_FILL_ALPHA=0.25`, `LOOP_HATCH_ALPHA=0.6`, `LOOP_HATCH_SPACING=8.0`, `LOOP_IDLE_FILL_ALPHA=0.10` (with a compile-time `const _: () = assert!(...)` pinning `≤ LOOP_ARMED_FILL_ALPHA / 2`), `LOOP_OUTLINE_WIDTH=1.0` (data-model §3.2). **Implements**: FR-004, FR-005, FR-012.
- [X] T016 [US2] Add `enum LoopShade { ArmedActive, ArmedInactive, Idle }` and `fn loop_shade(armed: bool, loop_state: u8) -> LoopShade` to `crates/modplayer-ui/src/markers.rs` (data-model §7); doc comment + doctest (Principle VII). Depends on: T014, T015. **Implements**: FR-004, FR-005.
- [X] T017 [US2] Rewrite the region-shading half of `markers::paint_overlay` in `crates/modplayer-ui/src/markers.rs` (signature unchanged): iterate every `TrackMarkers::regions()` entry with `span() == Some`, paint every `Idle` region first (in `regions()` order) then the armed region last; stop reading `current_region()` for shading decisions; keep armed-active/armed-inactive paint unchanged (006); HC keeps the extra `marker_outline` stroke (017 O5) (WL4). Depends on: T016. **Implements**: FR-004, FR-005, FR-015, FR-018.

**Checkpoint**: `rtk cargo test -p modplayer-ui --test markers` green. SC-002 satisfied — User Story 2 is independently shippable.

---

## Phase 5: User Story 3 - Preview where you're about to seek (Priority: P2)

**Goal**: A stateless 1 px scrub line + `m:ss.mmm` label follows the pointer on whichever view it hovers, clamped fully inside the rect, suppressed during any drag, never committing a seek or touching accessibility (WL5).

**Independent Test**: Move the pointer across the detail waveform without clicking, confirm the line/timestamp appear and follow it; repeat on the overview; move off the waveform and confirm both disappear.

### Tests for User Story 3

- [X] T018 [P] [US3] Add tests to `crates/modplayer-ui/tests/waveform.rs`: hover line/label present only on the hovered view and only while `enabled` and `!hover_suppressed`; follows the pointer; absent once the pointer leaves; label flips to the line's left on right-edge overflow; label fully inside `rect`; suppressed during a 005 seek-drag and a 006 marker-drag, reappears on next pointer move after release/`Esc`; hover never returns a `WaveformEvent`; AccessKit node (`widget_info`) byte-identical with and without hover; plus a proptest that `label_rect ⊆ space.rect` for every pointer x at the 64 px / 120 px minimum heights (WL5; data-model §8; must fail — `hover.rs` does not exist yet). **Implements**: FR-006, FR-007, FR-008, FR-009, FR-016, FR-019.

### Implementation for User Story 3

- [X] T019 [US3] Promote `format_mmss_millis_frames` (currently in `crates/modplayer-ui/src/markers.rs`) to a shared `pub(crate) fn format_mmss_millis` in `crates/modplayer-ui/src/waveform/mod.rs`; update the marker-list call site to reuse it; output stays byte-identical (`0:35.204` style) (R12; WL6 regression). Depends on: T018. **Implements**: FR-006, FR-010, FR-011.
- [X] T020 [US3] Create `crates/modplayer-ui/src/waveform/hover.rs` (new): `struct HoverIndicator { x, frame, text, label_rect }` and `fn hover_indicator(space, pointer: Option<Pos2>, suppressed: bool, label_galley_size: Vec2) -> Option<HoverIndicator>` — clamp `x` to the rect, `frame = space.frame_at(x)`, `text = format_mmss_millis(frame, sample_rate)`, flip-left-on-overflow + full-containment for `label_rect` (data-model §8); doc comment + doctest (Principle VII). Depends on: T010, T019. **Implements**: FR-006, FR-007.
- [X] T021 [US3] Add `paint_hover()` to `crates/modplayer-ui/src/waveform/hover.rs`: `HOVER_LINE_WIDTH` line in `hover_line`, full rect height, at `x`; `m:ss.mmm` label in `theme::mono_font_id()`, `hover_label_text` on a `hover_label_bg` pill (`radius::SM`, padding `space::XS`) (WL5). Depends on: T020. **Implements**: FR-006, FR-011, FR-012, FR-019.
- [X] T022 [US3] Add `hover_suppressed: bool` field to `WaveformPaint` in `crates/modplayer-ui/src/waveform/paint.rs`; in `crates/modplayer-ui/src/waveform/mod.rs`, compute `pointer = response.hover_pos()` only when `enabled`, and paint the hover indicator after the `overlays` hook and before the playhead in both `overview()`/`detail()` (WL1, WL5 layer order; signatures unchanged). Depends on: T021. **Implements**: FR-006, FR-007, FR-008, FR-016, FR-019.
- [X] T023 [US3] In `crates/modplayer-ui/src/now_playing.rs`, set `hover_suppressed = waveform.drag.is_some() || waveform.marker_drag.is_some()` for both views, every frame (WL5). Depends on: T022. **Implements**: FR-009.

**Checkpoint**: `rtk cargo test -p modplayer-ui --test waveform` green. SC-003/SC-004 satisfied — User Story 3 is independently shippable.

---

## Phase 6: User Story 4 - Read progress and energy at a glance (Priority: P3)

**Goal**: Each column paints an outer peak bar and an inner ±RMS band, in a played tone left of the displayed playhead and an unplayed tone right of it, on both views (WL2).

**Independent Test**: Play a track partway through; confirm the played portion of the fill is visually distinct from the unplayed portion; confirm peak and average tones are distinguishable in both themes.

**Note**: Depends on Phase 2's RMS pipeline (T007/T008) — the only story that does.

### Tests for User Story 4

- [X] T024 [P] [US4] Add tests to `crates/modplayer-ui/tests/waveform.rs`: shape capture shows ≥2 distinct fill colours left vs right of the playhead x; a column with `rms > 0` emits exactly two rects (peak + clipped average band) in its side's tone pair; average band clipped to the peak rect and omitted when < 1 px; placeholder/unavailable columns unaffected (WL2; SC-005, SC-006; must fail — `ColumnPaint` has no `rms` yet). **Implements**: FR-001, FR-002.

### Implementation for User Story 4

- [X] T025 [US4] Add `rms: u8` to `ColumnPaint::Present` in `crates/modplayer-ui/src/waveform/paint.rs` — max of the column's buckets' `rms`, matching the existing `min`/`max` fold (R6; data-model §4). Depends on: T007, T008, T024. **Implements**: FR-001, FR-013.
- [X] T026 [US4] In `crates/modplayer-ui/src/waveform/paint.rs`, compute the played/unplayed boundary from `WaveformPaint::playhead` (`col_left + 0.5 < space.x_of(p)`, drag-preview-aware) and paint per column: peak rect in `played_peak`/`unplayed_peak`, average band `[mid−rms·h, mid+rms·h] ∩ peak rect` in `played_average`/`unplayed_average` via `waveform_roles(roles)` (WL2; R7). Depends on: T025. **Implements**: FR-001, FR-002, FR-012.

**Checkpoint**: `rtk cargo test -p modplayer-ui --test waveform` green. SC-005/SC-006 satisfied — User Story 4 is independently shippable.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Full-suite regression, gates, and manual sign-off (FR-010/SC-007).

- [X] T027 [P] Confirm `rtk cargo test -p modplayer-ui --test design_token_literals` still reports 0 colour literals outside `theme/**` (no edit expected; catches any literal introduced by T010/T013/T015/T017/T021/T026). **Implements**: FR-012.
- [X] T028 Run SC-007 regression spot-checks: `rtk cargo test -p modplayer-ui --test accessibility --test high_contrast --test responsive_dock --test markers --test waveform` — 005/006 click/drag seek, `Esc` cancel, keyboard bindings, elapsed/remaining labels, marker lane/list format all pass unmodified. **Implements**: FR-010, FR-015, FR-016.
- [X] T029 Run full automated gates (quickstart §1): `rtk cargo fmt --check`, `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings`, `rtk cargo test --workspace`, `rtk cargo deny check`. **Implements**: FR-010, FR-014.
  - `fmt --check`: clean. `clippy -D warnings`: found `needless_borrow` in the T001 fix to `plugin_overlays.rs`; fixed (dropped the extra `&`, `roles()` already returns `&'static Roles`), then reformatted — clean. `test --workspace`: 2055 passed, 11 ignored, 0 failed. `deny check`: advisories ok, bans ok, licenses ok, sources ok.
- [X] T030 Execute manual scenarios M1–M9 (quickstart §3) on the macOS build; capture evidence under `target/manual-walk/` and record pass/deviation for each in this file. **Implements**: FR-001, FR-002, FR-003, FR-004, FR-005, FR-006, FR-007, FR-009, FR-010.
  - **BLOCKED, not executed.** Prerequisites checked out fine: `python3 -c "import Quartz"` succeeds, `screencapture` is present, and a signed-in ModPlayer profile with cached `.mpwf` analysis already exists at `~/Library/Application Support/ModPlayer.ModPlayer/`. `cargo build -p modplayer` succeeds and `./target/debug/modplayer` launches without crashing (pid confirmed alive, then stopped cleanly). But `CGSessionCopyCurrentDictionary` reports `CGSSessionScreenIsLocked = 1` — the console session is locked — so no window is created (`CGWindowListCopyWindowInfo` returns nothing for the app) and no mouse-event injection or screenshot is possible. This agent session has no way to unlock the screen (no credentials, and wouldn't use them unattended if it did). **M1–M9 need a human, or an agent session, at an unlocked macOS console** to actually execute; re-run this task once the screen is unlocked. Left unchecked rather than recording fabricated pass/deviation results.
  - **Re-attempt 2026-09-29: still BLOCKED, different cause.** Screen now unlocked (`CGSSessionScreenIsLocked` absent). Rebuilt with `RUSTUP_TOOLCHAIN=1.95.0` (shell exports `1.93.1`) and launched; the app hangs before creating a window because macOS raises a SecurityAgent Keychain prompt — "modplayer wants to use your confidential information stored in "ModPlayer" in your keychain… enter the "login" keychain password" (rebuilt debug binary's signature no longer matches the Keychain ACL; evidence `target/manual-walk/securityagent.png`). Plugin host logs `host_busy` timeouts meanwhile. Agent will not type a login password; app stopped with SIGTERM. **Unblock**: a human launches `./target/debug/modplayer` once and chooses *Always Allow* with the login password; then M1–M9 can run (helper `target/manual-walk/mw.py`).
  - **Executed 2026-09-29 (third attempt).** The user approved the Keychain prompt; after the user asked for a clean relaunch the app signed in without errors (a first launch had shown stale "access revoked / no longer Premium" toasts, which cleared after the restart). Driven by `CGEvent` injection + `screencapture -l` (`target/manual-walk/mw.py`, `drag.py`); playhead contrast was measured from capture pixels with WCAG relative luminance (`target/manual-walk/ph.py`, samples on dense peak, RMS band, background, loop fills). Main track: "Bring Me To Life" (3:55, uncached under analyzer v2, so it was re-analysed on first play and its `.mpwf` rewritten). Window at its 960×640 minimum inner size (`crates/modplayer/src/main.rs:138`) throughout. 69 captures are in `target/manual-walk/`.

    | ID | Result | Evidence / notes |
    |---|---|---|
    | M1 | **pass** | `m1-dark.png`: light core + dark casing on both views. Best-of(core, casing) ratio on sampled backdrops: 7.49 / 4.15 (peak fill, played/unplayed), 6.40 / 11.07 (RMS band / background), overview 7.49 / 4.63. Floor is 3:1. |
    | M2 | **pass** | `m2-light.png` (min best 4.20:1), `m2-light-hc.png` (min 5.45:1), `m2-dark-hc.png` (min 4.81:1). Two-tone fill is visible in all three; HC adds region outlines. Appearance restored to dark / HC off afterwards (`settings.toml` verified). |
    | M3 | **pass** | `m3-dark-40pct.png` (1:35 of 3:55): accent-blue fill left of the playhead, neutral grey right; each side has a lighter outer peak and a stronger inner RMS band. Overview-click seek also worked (005 regression). |
    | M4 | **pass** | `m4-two-regions.png`: two unarmed regions (≈1:02–1:08, ≈1:42–1:48) drawn idle (1 px outline + light fill) on both views. `m4-armed-outside.png`: region 1 armed → hatch ("Armed (waiting for the playhead)"). `m4-armed-inside.png`: inside region 1 → translucent armed-active fill. Region 2 stays idle throughout. |
    | M5 | **pass** | `m5-armed-r2.png`: region 2 armed from its marker-list row → hatch; region 1 (current, just looped) → idle. Shading follows `armed`, not current. |
    | M6 | **pass** | Paused at 1:11 on the uncached "Wherever I May Roam". `m6-detail-hover.png`: line + `1:13.415` on detail only. `m6-overview-hover.png`: `3:27.601` on overview only. `m6-off-widget.png`: gone off-widget. `m6-detail-right-edge.png`: label flips left of the line and stays inside. Transport stayed at 1:11 and the app log has no seek line. |
    | M7 | **pass** | `m7-seekdrag-*` (playing): no hover line or label mid-drag; the played/unplayed boundary follows the preview; `Esc` reverts it to the real position with no seek; hover returns on the next move. `m7-markerdrag2-*`: marker-lane drag shows no hover, `Esc` restores the glyph, and hover is back in the body (`m7-hover-returns.png`). **Deviation:** the marker drag ran *paused*, because during playback the detail viewport auto-scrolls away from the glyph. |
    | M8 | **pass** | The window cannot shrink below 960×640, which is the current size: overview ≈64 pt, detail ≈136 pt. Every capture above is at minimum size. `m8-min-overview-hover-edge.png`: overview hover label flipped and inside the 64 pt rect. |
    | M9 | **partial** | Placeholder band unchanged and hover over it works (`m9-during-decode-a/b.png`, `m6-*` on "Wherever I May Roam"). **Not observed:** decoded-part two-tone *during* a decode. "Bring Me To Life" decoded end-to-end within seconds (same as 006 M14), and "Wherever I May Roam" never decoded. An lldb breakpoint on `DecodedStore::set_complete`/`set_failed` hit `set_complete` for "Bring Me To Life" (`decode_ahead.rs:441`) but neither fired on a replay of "Wherever I May Roam" (librespot plays the relinked "- Remastered 2021" item), and the `sample` taken during that track's first play (`sample.txt`) showed no `decode-ahead` thread. So its decode-ahead never ran to completion or failure. `modplayer-audio-source-connect` is untouched on this branch, so this is a decode-ahead issue outside feature 022; worth its own ticket. |

    Side effects cleaned up: test markers on "Bring Me To Life" cleared (the track had none before); Time stretch un-bypassed (it auto-bypassed "over budget" while lldb paused the process); Markers panel collapsed again. "Wherever I May Roam"'s pre-existing markers (Marker 2, Verse A/B, Marker 1) were not touched.
- [X] T031 Confirm doc comments + doctests exist for every new public item: `WaveformRoles`, `waveform_roles`, `HoverIndicator`, `hover_indicator`, `loop_shade`, `format_mmss_millis` (Constitution VII). **Implements**: FR-004, FR-005, FR-006, FR-011, FR-012.
  - `WaveformRoles`/`waveform_roles` (`theme/waveform.rs`), `HoverIndicator`/`hover_indicator` (`waveform/hover.rs`), `loop_shade` (`markers.rs`): each has a doc comment and a runnable `\`\`\`` doctest. `format_mmss_millis` (`waveform/mod.rs`) is `pub(crate)`, not crate-external — Constitution VII's doctest requirement is for "public items" reachable via `cargo test --doc`'s external-crate compilation, which a `pub(crate)` item cannot be; it carries a doc comment and is covered by a regular `#[test]` instead (`format_mmss_millis_matches_seconds_and_millis`), consistent with the existing `format_mmss_frames`. `cargo test -p modplayer-ui --doc` → 42 passed.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies.
- **Foundational (Phase 2)**: Depends on Setup. Blocks US1 (playhead tokens), US3 (hover tokens/formatter), US4 (RMS data + fill tokens) fully or partially as noted. **Does not block US2.**
- **US1 (Phase 3)**: Depends on Phase 2 (theme tokens, T010).
- **US2 (Phase 4)**: Depends only on Setup — may run fully in parallel with Phase 2 and Phase 3.
- **US3 (Phase 5)**: Depends on Phase 2 (theme tokens, T010).
- **US4 (Phase 6)**: Depends on Phase 2 (RMS pipeline, T007/T008, and theme tokens, T010).
- **Polish (Phase 7)**: Depends on all four user stories being complete.

### User Story Dependencies

- **US1 (P1)**: Foundational tokens only; no dependency on US2/US3/US4.
- **US2 (P1)**: No dependency on Foundational or any other story — the most independent story in this feature.
- **US3 (P2)**: Foundational tokens + its own `format_mmss_millis`; no dependency on US1/US2/US4 (reads `WaveformPaint.playhead`/`hover_suppressed` it adds itself; paints below the playhead US1 repaints, but neither task edits the other's lines).
- **US4 (P3)**: Foundational RMS pipeline + tokens; no dependency on US1/US2/US3 (touches `paint.rs` columns, which US1 also touches for the playhead — sequence T013 before T025/T026 if one person does both, to avoid the same-file merge).

### Within Each User Story

- Tests (T012/T014/T018/T024) written and failing before their story's implementation tasks.
- `theme/**` constants before the code that consumes them.
- Pure logic (`loop_shade`, `hover_indicator`) before the paint call that uses it.

---

## Parallel Example: Foundational Phase

```bash
# Test-writing tasks, different files, all parallel:
Task: "RMS test vectors in crates/modplayer-audio-source/tests/decoded.rs"       # T002
Task: "Refold unit tests in crates/modplayer-core/src/analysis/peaks.rs"         # T004
Task: "Cache v2 tests in crates/modplayer-core/tests/analysis.rs"                # T006
Task: "Contrast/distinctness tests in crates/modplayer-ui/tests/design_token_contrast.rs"  # T009
```

## Parallel Example: Across Stories (after Phase 2 checkpoint)

```bash
# Different crates/files, independently testable stories:
Task: "US1 playhead repaint in crates/modplayer-ui/src/waveform/paint.rs"        # T013
Task: "US2 loop_shade + paint_overlay rewrite in crates/modplayer-ui/src/markers.rs"  # T016/T017
Task: "US3 hover.rs module"                                                      # T020/T021
```
(US2's T014/T015 can start at T001, before the Phase 2 checkpoint, since US2 has no Foundational dependency.)

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1: Setup.
2. Phase 2: Foundational (RMS pipeline + theme tokens).
3. Phase 3: US1 — cased playhead.
4. **STOP and VALIDATE**: `rtk cargo test -p modplayer-ui --test design_token_contrast --test waveform`; manual M1/M2. Ship — the most severe defect (`UX-23`) is fixed.

### Incremental Delivery

1. Setup + Foundational → foundation ready (US2 already shippable in parallel from T001).
2. Add US1 → validate → ship (MVP).
3. Add US2 (may already be done) → validate (M4/M5) → ship.
4. Add US3 → validate (M6/M7) → ship.
5. Add US4 → validate (M3/M9) → ship.
6. Polish → full regression (SC-007) + manual M1–M9 sign-off (T027–T031).

### Parallel Team Strategy

With multiple developers, once Setup is done:
- Developer A: Phase 2 Foundational, then US1 (Phase 3), then US4 (Phase 6).
- Developer B: US2 (Phase 4) immediately — no Foundational wait.
- Developer C: waits for Phase 2's theme tokens (T010), then US3 (Phase 5).

---

## Notes

- `[P]` tasks touch different files and have no dependency on an incomplete same-phase task.
- `[Story]` label maps a task to its spec.md user story for traceability.
- Constitution VIII requires tests written first and failing before implementation — every story's test task precedes its implementation tasks above.
- No engine-crate (`modplayer-engine`) or plugin-API change anywhere in this feature (Constitution I, II, IX) — no PR note required for those.
- Commit after each task or logical group per this repo's normal workflow; use `rtk git add`/`rtk git commit`/`rtk git push`.
- Avoid: editing `crates/modplayer-ui/src/waveform/paint.rs` simultaneously across T013 (US1) and T025/T026 (US4) if split across people — same file, sequence them.
