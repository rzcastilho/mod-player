# Research: Notification Placement, Severity, and Message Quality

**Feature**: 019-notification-presentation | **Date**: 2026-09-25 | **Plan**: [plan.md](./plan.md)

All product values are fixed by the spec's Clarifications (2026-09-25). This
file resolves the remaining *engineering* unknowns against the code as it
stands on this branch. Each entry: Decision / Rationale / Alternatives.

Code facts this research is grounded in (read on 2026-09-25):

- `crates/modplayer-ui/src/app.rs:315` draws the stack as
  `Area::new("shell-notifications").anchor(Align2::RIGHT_TOP, vec2(-8, 8))`,
  before `CentralPanel`, after `Panel::left("shell-nav-rail")`.
- `crates/modplayer-ui/src/notifications.rs::show` iterates
  `center.visible()` (newest first) and renders every item in a
  `Frame::popup`, one `ui.horizontal` row: severity label, (attribution),
  message, action buttons, Dismiss. No cap, no truncation, no colour.
- `crates/modplayer-core/src/notifications.rs`: `Notification { id, severity,
  message_key, args, action, actions, created_at, dismissed, dedupe_key,
  attribution }`; `tick(now)` dismisses `Info` once
  `now - created_at >= INFO_AUTO_DISMISS` (10 s).
- `device_policy::resolve` builds `DeviceWarning::MissingPreferred {
  device_name: id.as_str() }` — the raw id (UX-38's 120-char string).
- `controller.rs::handle_device_lost` raises `device-lost` with
  `$device = active.name` (human) but never names the fallback;
  `handle_device_list_changed` raises `device-available-again` with
  `found.name`.
- `controller.rs::raise_settings_warning` raises
  `keybindings-invalid-entries` with `$ids = ids.join(", ")`.
- `confirm_device` is the **only** writer of `settings.output_device`.
- Theme roles: `crates/modplayer-ui/src/theme/tokens.rs::Roles` exposes
  `surface_raised`, `positive`, `warning`, `danger`; `tokens::roles(visuals)`
  returns the active table (light/dark/high-contrast).
- egui/eframe 0.36 (workspace `Cargo.toml`); MSRV 1.95.

---

## R1 — Anchoring the stack bottom-right and growing upward

**Decision**: Change the anchor in `app.rs` to
`Align2::RIGHT_BOTTOM, vec2(-8.0, -8.0)`. Keep the same `Area` id, the same
`Order` (default `Middle`) and the same draw position in `update`, so z-order
relative to other overlays is unchanged (spec clarification). Inside the
area, render newest → oldest top → bottom, then the "N more" / "Show fewer"
button last. Because an anchored `Area` is positioned from its measured size
of the previous frame, a bottom anchor makes the stack grow upward as cards
are added.

**Rationale**: One-line placement change, matches FR-001 exactly; egui's
`Area::anchor` already handles the bottom pivot. Keeping the id avoids a
one-frame jump from a new area memory entry.

**Alternatives considered**:
- `Panel::bottom` reserved strip — rejected: it would steal layout height
  from every screen permanently and is modal-ish in effect (reflows content).
- Anchoring inside `CentralPanel`'s rect instead of the window — rejected:
  the rect changes with the dock/nav rail; the spec fixes the offset from the
  window corner.
- Reversing order (newest at bottom, nearest the corner) — rejected: the spec
  and `NotificationCenter` contract say newest at top.

## R2 — Proving "no header / tab strip / category list / table header / nav rail is covered"

**Decision**: Two layers of verification.

1. **Worst-case stack rect (headless)**: no analytic height function; a headless egui test
   (`crates/modplayer-ui/tests/notification_stack.rs`) renders the stack in a
   `960 × 640` `RawInput::screen_rect` with the worst-case collapsed content —
   three `Critical` cards, each with two actions, a `Details` toggle, a
   `Show more` toggle and a 40-character device name — plus the `1 more`
   button, and records the area's `Rect` via `ctx.memory(|m| m.area_rect(id))`.
2. **Screen intersection (headless)**: the same test renders the library
   screen (tab strip), the Settings screen (category list), and the plugins
   table (column headers) via their public `show` functions inside a
   `CentralPanel` + nav rail at `960 × 640` and `1200 × 820`, collects the
   rectangles of the protected widgets from AccessKit node bounds (pattern
   already used by `tests/responsive_dock.rs` / `tests/plugin_panels.rs`),
   and asserts **zero intersection** with the stack rect (SC-001).

Manual scenario M1/M2 captures the screenshot evidence.

**Rationale**: At 640 px height the worst-case collapsed stack is estimated
at ≈ 3 × 104 + 3 × 6 (spacing) + 28 (button) + 8 (offset) ≈ 366 px, leaving
the top ≈ 274 px free; every protected header lives in the top ≈ 120 px of
the central panel, the nav rail is on the left edge, and the settings category
list is a left column (≤ 220 px wide) that cannot reach `x ≥ 960 − 368 = 592`.
A test pins that reasoning so a later change that grows cards is caught.

**Alternatives considered**:
- Computing the stack height analytically — rejected: egui text layout
  depends on font metrics; a headless render is the source of truth.
- Only a manual screenshot — rejected: SC-001 asks for an automated check.

## R3 — Visible cap and overflow partition

**Decision**: Pure function in `modplayer-ui::notifications`:

```text
pub const MAX_COLLAPSED_CARDS: usize = 3;
pub fn partition(visible_len: usize, expanded: bool) -> StackPartition { shown, overflow }
```

`shown = if expanded { visible_len } else { min(visible_len, 3) }`,
`overflow = visible_len − min(visible_len, 3)`. The UI takes
`center.visible().take(shown)`. Because `visible()` is newest-first and is
recomputed every frame, "promote the newest hidden card after a dismissal"
(FR-006) holds by construction; no queue bookkeeping. When `overflow == 0`
the `expanded` flag is forced back to `false` (FR-006 "returns to collapsed").
A `proptest` checks the invariants (`shown ≤ visible_len`, collapsed ⇒
`shown ≤ 3`, `overflow == visible_len.saturating_sub(3)`).

**Rationale**: Keeps `NotificationCenter` presentation-agnostic (FR-018:
render whatever `visible()` yields) and makes the rule unit-testable
without egui.

**Alternatives considered**: A `visible_capped()` on `NotificationCenter` —
rejected: the cap is presentation, and core would then need UI state
(`expanded`).

## R4 — Expanded stack height cap and scrolling

**Decision**: When expanded, wrap the cards in
`egui::ScrollArea::vertical().max_height(0.6 × ctx.screen_rect().height())`
(inner window height in logical points) with `auto_shrink([false, true])`;
the "Show fewer" button sits **outside** the scroll area so it is always
reachable. Collapsed state renders without a `ScrollArea`.

**Rationale**: Matches FR-005; a ScrollArea is keyboard-scrollable and
keeps focus traversal intact (Tab moves into off-screen buttons and egui
scrolls them into view via `scroll_to_me` on focus).

**Alternatives considered**: Paging ("1–3 of 7") — rejected: spec says
"expands the rest" inline.

## R5 — UI-only stack state and where it lives

**Decision**: A plain struct owned by the app (`ModPlayerApp.notification_stack:
notifications::StackState`):

```text
StackState { expanded: bool, show_more: BTreeSet<u64>, details: BTreeSet<u64> }
```

`show` takes `&mut StackState`; each frame it prunes ids no longer in
`center.visible()` (so dismissed cards leave no stale flags) and never
touches flags for ids that merely moved between shown/overflow (edge case
"new arrival while a card is expanded"). Not persisted, not in egui memory.

**Rationale**: Owning the state in the app (not `egui::Memory`) makes it
directly assertable in tests and survives `Area` id changes; keyed by
notification id per spec.

**Alternatives considered**: `ctx.data_mut` temp storage — rejected: harder
to test and to prune deterministically.

## R6 — Two-line truncation and "Show more" detection

**Decision**: Build the message as an `egui::text::LayoutJob` with
`wrap = TextWrapping { max_width: text_width, max_rows: 2, break_anywhere:
false, overflow_character: Some('…') }`, lay it out with
`ui.fonts(|f| f.layout_job(job))`, and read `galley.elided` to decide
whether the "Show more" toggle is needed (FR-010: *if and only if* the full
text exceeds two lines). When expanded, lay out with `max_rows: usize::MAX`.
`text_width = card_width − accent(4) − inner margins − icon column`.
Card width `= min(360, screen_width − 16)` via pure `card_width(screen_w)`.
Text style: `TextStyle::Body` (014 type scale).

The label's AccessKit node gets `WidgetInfo::labeled(Label, true, full_text)`
— the **full, untruncated** message — so truncation never hides text from
assistive technology (FR-015).

For plugin-attributed notifications the attribution icon + name stay on the
first row; only the text after them is laid out with the two-row cap (N3
unchanged).

**Rationale**: egui 0.36's `TextWrapping::max_rows` + `overflow_character`
is the built-in ellipsis mechanism; `Galley::elided` is the exact "needs more
than two lines" signal, locale- and font-independent (edge case "long device
names / other locales").

**Alternatives considered**: Character-count heuristics — rejected: wrong for
proportional fonts and other locales. `Label::truncate()` — rejected: single
line only.

## R7 — Severity accent bar, icon colour, card frame

**Decision**: `fn severity_color(roles: &Roles, s: Severity) -> Color32`
(`Info → positive`, `Warning → warning`, `Critical → danger`) in
`notifications.rs`, reading `theme::tokens::roles(ui.visuals())`. Card =
`Frame::new().fill(roles.surface_raised).inner_margin(..)` using the 014
spacing/radius tokens, with `inner_margin.left` enlarged by 4 px; after the
frame is shown, paint a 4 px full-height filled rect on the left edge of the
frame's response rect (`painter.rect_filled`, left corners rounded to the
card radius). The icon glyph is a `RichText` coloured with the same role.
Icons stay `⛔` / `⚠` / `ℹ`; the severity word (`severity-*`) stays as
visible text next to the icon.

**Rationale**: FR-007/FR-008; no colour literal (enforced by the existing
`tests/design_token_literals.rs` scan), high contrast (017) automatic because
`roles()` returns the HC table. The text keeps carrying severity for
greyscale/colour-blind users.

**Alternatives considered**: `Frame::stroke` on the left only — not supported
per side in egui; a full-card tinted background — rejected: harms text
contrast (014 contrast tests).

## R8 — Info auto-dismiss hold while expanded (WCAG 2.2.1)

**Decision**: Add to `Notification` a private-to-core `auto_dismiss_from:
Instant` (initially `created_at`) and `held: bool`. New `NotificationCenter`
methods:

- `hold_auto_dismiss(id)` → `held = true`.
- `release_auto_dismiss(id, now)` → `held = false; auto_dismiss_from = now`.

`tick(now)` dismisses `Info` only when `!held && now − auto_dismiss_from ≥
INFO_AUTO_DISMISS`. The UI emits hold/release transitions in
`NotificationInteraction::hold_changes` whenever an Info card goes from "no
expansion" to "Show more or Details expanded" and back; `app.rs` applies
them. Pruned ids (dismissed) need no release. `Warning`/`Critical` ignore
holds (never auto-dismiss anyway).

**Rationale**: Keeps timing policy in core (testable with injected
`Instant`s, like the existing `info_auto_dismisses_after_ten_seconds`), and
FR-017/FR-009 compliant: the only behaviour change is the hold. Collapsed
overflow Info items keep ageing (edge case) because nothing holds them.

**Alternatives considered**: Resetting `created_at` — rejected: `created_at`
has other readers (ordering semantics, tests) and "raised at" must stay
truthful. UI-side timer — rejected: splits the lifecycle across crates.

## R9 — Human device names and the fallback name

**Decision**:

- **device-lost**: `handle_device_lost` already has `lost_device_name` (human)
  and `DeviceLostOutcome::FellBackTo { device }`; add `$fallback =
  device.name` and `detail = id.to_string()` (lost `DeviceId` display form).
- **device-missing-at-launch**: extend `DeviceWarning::MissingPreferred` to
  `{ device_name: Option<String>, device_id: Option<DeviceId>, fallback_name: String }`.
  Name resolution is a new pure `device_policy::display_name_for_saved(id:
  Option<&DeviceId>, saved_name: Option<&str>) -> Option<String>`:
  `saved_name` (non-empty) → `<name>` of a `name:<name>` id → `None`. `resolve`
  gains a `saved_name: Option<&str>` parameter (its only callers are
  `controller.rs` and `tests/device_policy.rs`). The controller maps `None`
  to the localized generic phrase `tr("notification-device-unknown")` ("Your
  saved output device") at raise time, and an empty fallback name to
  `tr("notification-device-fallback-default")` ("the system default output").
  `detail = id.to_string()` when an id exists.
- **device-available-again**: unchanged `$device = found.name`; `detail =
  found.id.to_string()`.

Resolving the generic phrases at raise time follows the existing precedent
of `plugin-suspended`'s `$cause` (localized prose passed as an arg).

**Rationale**: FR-011/FR-012; never uses the raw id as a display name; the
id survives only in `detail` (FR-013).

**Alternatives considered**: Fluent selector on a sentinel arg value
(`$device -> [unknown] …`) — rejected: sentinel strings leak into args and
complicate the key test harness; separate `*-unnamed` message keys — rejected:
doubles three keys for one phrase.

## R10 — Persisting `output_device_name`

**Decision**: `AudioSettings.output_device_name: Option<String>` and
`RawAudio.output_device_name` with `#[serde(default, skip_serializing_if =
"Option::is_none")]` under `[audio]`. `confirm_device` looks up the
`OutputDeviceInfo` **before** saving (today it looks it up after) and stores
`info.name` alongside `output_device`; when the device is not in the list,
the name is left unchanged (`None` on first confirm). Controller keeps a
`preferred_device_name` shadow next to `preferred_device` for `launch()`.
Round-trip covered by the existing settings `proptest` strategy (extended with
the new field) plus a fixture test: a pre-019 file (no key) loads with
`output_device_name == None` and re-saves without adding the key.
Empty/whitespace-only strings load as `None`, no `InvalidField` (a display
hint, not a validated value).

**Rationale**: Additive, backward-compatible (FR-012); schema_version not
bumped (optional key; older builds ignore unknown keys — confirmed: no
`deny_unknown_fields` anywhere in `settings/model.rs`). `DeviceId` implements
`Display` (`crates/modplayer-engine/src/types.rs:231`), used for `detail`.

**Alternatives considered**: Querying the backend for a name by id at
launch — impossible when the device is absent, which is exactly the case.

## R11 — `keybindings-invalid-entries` detail

**Decision**: `raise_settings_warning` raises with no args and `detail =
ids.join(", ")`. Fluent: `keybindings-invalid-entries = Some saved keyboard
shortcuts couldn't be read and were reset to their defaults.` Move the key
from `CONTROLS_WARNING_ARG_KEYS` to the plain-keys list in
`tests/fluent_keys.rs`; `tests/settings.rs:581` asserts on `detail` instead of
`args`.

**Rationale**: FR-013 names this notification explicitly.

**Alternatives considered**: Keeping `$ids` inline — rejected by spec.

## R12 — `detail` carrying API

**Decision**: `Notification.detail: Option<String>` (public field, like
`dedupe_key`). New builder-style raise: `raise_with_detail(severity, key, args,
detail: String) -> u64`; and a `set_detail(id, detail)` is **not** added
(YAGNI). All existing `raise*` methods set `detail: None`; the plugin API
(`raise_attributed`) never sets it (plugin API unchanged, Principle IX).

**Rationale**: Minimal surface, no change to existing call sites.

## R13 — Keyboard and accessibility of the new controls

**Decision**: "N more"/"Show fewer", "Show more"/"Show less", "Details"/"Hide
details", Dismiss and actions are `ui.button(tr(key))` (egui `Button` ⇒
focusable, Enter/Space activate, AccessKit `Role::Button` with the label as
name). The accent bar is painted (no widget); the icon + severity word label
keeps its combined accessible name (existing `severity_label`). The Details
payload is `Label::new(RichText::new(detail).monospace()).selectable(true).wrap()`.
Covered by extending `tests/accessibility.rs` / `tests/notifications.rs`
(every button has a non-empty name; message node name equals the full
resolved text).

**Rationale**: FR-015 and Constitution X with no custom widgets.

## R14 — Draw order caveat (011/018)

**Decision**: Out of scope. `shell.rs` documents that the notification Area is
drawn before `CentralPanel`, so a floated plugin panel drawn the same frame
can paint over it. The spec fixes "z-order relative to other overlays is
unchanged"; the bottom-right placement overlaps the 018 narrow-window dock
overlay's lower region (right-anchored, `Order::Middle`) exactly as the
top-right stack overlapped its upper region — acceptable per FR-002 ("lower
content may be overlapped").

**Alternatives considered**: Moving the stack to `Order::Foreground` —
rejected: would put it above modal dialogs and change z-order, which the spec
forbids.

## R15 — Performance Mode

**Decision**: No code. The presentation layer renders `center.visible()`
(FR-018). Recorded so tasks do not invent a filter.

## R16 — M1–M11 blocked by a locked GUI session (2026-09-25)

**Observation**: `tasks.md` T040 attempted all eleven manual scenarios.
`modplayer` was built fresh and launched in the background against the
real config dir (no override — the recipe's live-account requirement).
The process stayed alive (`ps` showed `SN`, 29 s elapsed, no crash).
`Quartz.CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly,
kCGNullWindowID)`, polled at ~3 s and ~23 s post-launch, never listed a
`modplayer`-owned window among the on-screen windows (25, then 14) —
only `SecurityAgent` (the lock prompt), `loginwindow`, `Dock`, `Window
Server` and other background/session-chrome owners were present.
`Quartz.CGSessionCopyCurrentDictionary()` confirmed
`CGSSessionScreenIsLocked = 1`.

**Same blocker as 017 (R18/R19/R21)**: a locked macOS GUI session never
forms a window for a newly launched GUI app — `screencapture -x -o -l
<windowid>` and `CGEventPost` both need a windowid that never exists.
This is a machine-state condition, not anything this feature's code
changed; nothing in `notifications.rs`, `device_policy.rs`,
`settings/model.rs`, `controller.rs` or the locale files affects window
creation.

**Action taken**: killed the launched process rather than route around
the lock (Governance › Manual Scenario Sign-Off forbids modifying the
host's real session/Keychain state to work past it). No evidence PNGs
exist for this attempt — there was no window to capture.

**Coverage in lieu of the visual walk**: every mechanism M1–M11 would
confirm is exercised by this feature's own automated suites, all green
under T039's full `cargo test --workspace` (1923 passed, 0 failed) — see
the M1–M11 sign-off table in quickstart.md. This substitutes machine
verification of the underlying values/behaviour, not the real-renderer,
real-OS visual confirmation Principle VIII asks manual scenarios to
provide; M1–M11 remain **not reached** and should be re-attempted once
the GUI session is unlocked.
