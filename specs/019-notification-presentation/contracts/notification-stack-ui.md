# Contract: Notification Stack UI

**Feature**: 019-notification-presentation | Supersedes the "Notification area" paragraph of
[001 contracts/ui-surface.md](../../001-walking-skeleton/contracts/ui-surface.md) for placement,
cap, styling and truncation. Everything not listed stays as 001/002/003/009/011 defined it.

Module: `crates/modplayer-ui/src/notifications.rs`. Caller: `crates/modplayer-ui/src/app.rs`.

## S1 Placement (FR-001, FR-002)

- `Area::new(Id::new("shell-notifications"))`, anchor `Align2::RIGHT_BOTTOM`, offset `(-8, -8)` logical px.
- Same `Order` and same draw position in `update` as before (z-order unchanged).
- Non-modal: no `Modal`, no input blocking outside its own cards; AccessKit exposes no `is_modal` node.
- At every inner size ≥ 960 × 640 the **collapsed** stack rect has zero intersection with: nav rail, library tab strip, library first list row (at default size), settings category list, plugin table column headers.

## S2 Order and cap (FR-003, FR-004, FR-006)

- Reading order top → bottom: newest → oldest non-dismissed.
- Collapsed: first `min(n, 3)` of `center.visible()`.
- `n > 3` ⇒ one button below the oldest shown card, label `notification-more` with `$count = n − 3`.
- Recomputed every frame from `visible()`: a dismissal promotes the newest hidden card; `n ≤ 3` ⇒ no button and `expanded = false`.

## S3 Expanded (FR-005)

- Activating `notification-more` ⇒ `expanded = true`: every non-dismissed card, newest first, in a vertical `ScrollArea` whose max height is `0.6 × inner window height`; button relabelled `notification-show-fewer`, rendered outside the scroll area.
- Activating `notification-show-fewer` ⇒ `expanded = false`.
- Never persisted.

## S4 Card (FR-007, FR-008, FR-014)

```text
┌─┬──────────────────────────────────────────────┐
│█│ ⛔ Critical   [plugin icon + name]            │  ← icon in role colour + severity word (text)
│█│ Message text wrapped to at most two lines …   │  ← body style, 2-row cap
│█│ [Show more] [Details]                         │  ← only when applicable
│█│ monospace selectable detail (when expanded)   │
│█│ [Action 1] [Action 2] [Dismiss]               │  ← unchanged set and dispatch
└─┴──────────────────────────────────────────────┘
 ^ 4 px accent bar, role colour, full card height
```

- Width `min(360, inner width − 16)`; fill `Roles::surface_raised`; bar/icon colour per severity: Critical→`danger`, Warning→`warning`, Info→`positive`. No colour literals.
- Actions and Dismiss: identical set, order and `NotificationInteraction` output as before.

## S5 Truncation (FR-010)

- Message laid out at card text width, `max_rows = 2`, overflow `…`.
- "Show more" (`notification-show-more`) present **iff** the 2-row galley is elided; toggles to "Show less" (`notification-show-less`) and full text.
- Plugin-attributed: icon + name stay before the text; only the text is capped.

## S6 Details (FR-013)

- "Details" (`notification-details`) present **iff** `notification.detail.is_some()`; toggles to `notification-hide-details`.
- Expanded: `detail` in a selectable, monospace, wrapping label below the message.
- "Show more" and "Details" are independent.

## S7 Timing (FR-009)

`show` reports, via `NotificationInteraction::hold_changes`, each Info card whose "Show more" or "Details" expansion started (`true`) or ended (`false`) this frame. `app.rs` calls `hold_auto_dismiss(id)` / `release_auto_dismiss(id, Instant::now())`. See [notification-center-api.md](./notification-center-api.md) C3.

## S8 Accessibility (FR-015)

- Every control is an `egui::Button` (Tab-reachable, Enter/Space) with a non-empty externalized name.
- Severity label accessible name = glyph + severity word.
- Message label accessible name = the **full** resolved message (plugin: `"<plugin>: <text>"`, as 011 N3), regardless of truncation.
- Accent bar is painted, not a widget (no AccessKit node).

## S9 Signature

```text
pub fn show(ui: &mut Ui, center: &NotificationCenter, state: &mut StackState) -> NotificationInteraction
pub fn partition(visible_len: usize, expanded: bool) -> StackPartition
pub fn card_width(screen_width: f32) -> f32
pub const MAX_COLLAPSED_CARDS: usize = 3;
```

`NotificationInteraction { dismissed, action_clicked, hold_changes }`.
