# Contract: UI surface delta (extends 001/002 contracts/ui-surface.md)

**Crate**: `modplayer-ui` | Locale: en-US only; every string below is a
Fluent key in the new `locales/en-US/playback.ftl` (or `settings.ftl` for
descriptors). Every control is keyboard-operable with an accessible name,
role and state (FR-023; the `accessibility` test enumerates them).

## 1. Now Playing (replaces 001's minimal transport)

| Element | Accessible name key | Behaviour |
|---|---|---|
| Track title / artists heading | `now-playing-title` (`{ $title }`), `now-playing-artist` | from `current_track()`; `now-playing-empty` when none |
| Status line | `status-buffering`, `status-reconnecting`, `status-playing-on` (`{ $device }`), `status-disabled-*` | one line under the heading; empty when `Playing` and healthy |
| Banner "Playing on <device>" + **Play here** button | `banner-playing-elsewhere`, `banner-play-here` | shown while `ActiveState::Inactive`; `Play here` → `play_here()` |
| Play/Pause toggle | `transport-play` / `transport-pause` | `Space` (when the Now Playing view has focus) |
| Stop | `transport-stop` | |
| Skip back | `transport-skip-back` | FR-004 |
| Skip forward | `transport-skip-forward` | |
| Seek slider | `transport-seek` (`aria-valuetext` = `mm:ss / mm:ss`) | `Left/Right` = ±5 s, `Home/End`; commits `seek()` on release or key |
| Position / duration readout | `transport-position` | updated every frame from `PositionClock` (≥ 60 Hz while playing: `request_repaint_after(16 ms)`) |
| Master volume + peak meter | unchanged from 001 | |
| Inline disabled reason | `transport-disabled-no-device` (001), `transport-disabled-premium-required`, `transport-disabled-not-verified`, `transport-disabled-source-unavailable`, `transport-disabled-update-required` | controls disabled; navigation, Settings and Queue stay usable |

## 2. Queue view

Reached from Now Playing via a **Queue** toggle button (`queue-toggle`,
`Ctrl/Cmd+Q`) that shows the queue panel beside/below the transport (no new
nav-rail section — 001's four sections are unchanged).

| Element | Key | Behaviour |
|---|---|---|
| Header with shuffle toggle and repeat cycle button | `queue-shuffle` (state on/off), `queue-repeat-off` / `-one` / `-all` | |
| Row (title, artist, origin badge `queue-badge-play-next`, `queue-badge-unavailable`, current marker `queue-current`) | `queue-row` (`{ $title }`) | current row visibly distinguished; history never shown |
| Row actions: **Move up**, **Move down**, **Play next**, **Remove** | `queue-move-up`, `queue-move-down`, `queue-play-next`, `queue-remove` | keyboard-operable buttons (also `Alt+Up/Down`, `Delete` on a focused row); drag-and-drop is additive |
| Empty state | `queue-empty` | |

## 3. Settings › Playback (new screen `settings/playback.rs`)

| Descriptor id | Title key | Control |
|---|---|---|
| `playback.device_name` | `setting-device-name` (hint `setting-device-name-hint` "as seen by other apps on your account") | single-line text field; commits on Enter/blur via `set_device_name`; inline error `setting-device-name-too-long`; empty restores the default and the field shows the default as placeholder |

## 4. Settings › Developer (extended)

| Descriptor id | Title key | Control |
|---|---|---|
| `developer.play_from_account` | `setting-play-from-account` (desc `setting-play-from-account-desc`) | button; hidden when `!playback_permitted`; runs `request_recent_tracks(20)`, then `queue_replace(tracks)` + `play()`; inline result `play-from-account-loading` / `play-from-account-empty` / `play-from-account-forbidden` ("Sign in again to grant access to your library") |

## 5. Notifications (keys, severities, actions)

| Key | Severity | Action button key |
|---|---|---|
| `stream-reconnect-warning` | Warning | — |
| `stream-source-unavailable` | Critical | `action-status-page`, `action-retry` |
| `stream-source-update-required` | Critical | `action-status-page` |
| `transfer-request-failed` | Warning | — |
| `queue-item-skipped-unavailable` (`{ $title }`) | Info | — |
| `subscription-downgraded` | Warning | `action-open-upgrade-page` |
| `play-from-account-failed` (`{ $reason }`) | Warning | — |

`NotificationAction::{OpenStatusPage, RetrySource, OpenUpgradePage}` are
rendered by 002's action button; URLs open via `ctx.open_url` only.
Notifications may carry two actions: `Notification.action` becomes
`actions: Vec<NotificationAction>` (≤ 2) — `raise_with_action` keeps its
signature (wraps into a one-element vec).

## 6. App wiring

- `App` maps 002 `AccountEvent`s to `controller.set_playback_permitted(..)`
  / `clear_for_sign_out()` / `on_tier_free()`, and `AccountEvent::ReadResult`
  to the developer action and the transfer banner name.
- `App::on_exit` → `controller.shutdown()` (FR-008: closing the window quits;
  audio stops, buffers discarded, device deregistered).
- `Ticker` thread requests repaints every 33 ms while intent is `Playing`
  (research R12) so `tick()` keeps running while minimized/hidden.
- Launch: after 002 reports `Active`/`Premium`, `App` calls
  `set_playback_permitted(true)` and `request_playback_state()`; if another
  device is active, the banner is shown before any local transport (FR-019).

## 7. Tests pinning this contract (`modplayer-ui`)

- `fluent_keys.rs`: every key above exists in `playback.ftl`/`settings.ftl`
  and no unused keys.
- `accessibility.rs`: every control in §1–§4 exposes a non-empty
  accessible name and correct role/state through egui's AccessKit tree with
  a `ScriptedHost` and `FakeBackend`.
- `now_playing.rs`: disabled reasons per `ActiveState`/health; banner +
  Play here path; seek slider commits once per release.
- `queue_view.rs`: rows in effective order, badges, keyboard actions mutate
  the controller's queue.
- `trademark.rs` (002) unchanged: no service marks in the new strings.
