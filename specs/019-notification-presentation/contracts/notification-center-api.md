# Contract: NotificationCenter API Delta

**Feature**: 019-notification-presentation | Module: `crates/modplayer-core/src/notifications.rs`
(re-exported from `modplayer_core`). Additive; no existing method changes signature or behaviour
except `tick` (C3).

## C1 `Notification.detail`

```text
pub detail: Option<String>
```

`None` for every notification raised through existing `raise*` methods,
`raise_keyed` and `raise_attributed` (plugin API unchanged — Principle IX).

## C2 `raise_with_detail`

```text
pub fn raise_with_detail(
    &mut self,
    severity: Severity,
    message_key: &'static str,
    args: Vec<(&'static str, String)>,
    detail: String,
) -> u64
```

Newest-first insert like every other raise; no actions, no dedupe key, no attribution.
Precondition: `!detail.is_empty()` (debug-asserted; release stores `None` for empty).

## C3 Auto-dismiss hold (FR-009)

```text
pub fn hold_auto_dismiss(&mut self, id: u64)
pub fn release_auto_dismiss(&mut self, id: u64, now: Instant)
pub fn tick(&mut self, now: Instant)   // behaviour refined
```

| Rule | Behaviour |
|------|-----------|
| H1 | `tick` dismisses `Info` iff `!held && now − auto_dismiss_from ≥ INFO_AUTO_DISMISS`. |
| H2 | `hold_auto_dismiss` sets `held = true`; idempotent; no-op for unknown/dismissed ids. |
| H3 | `release_auto_dismiss` sets `held = false`, `auto_dismiss_from = now`; no-op if not held. |
| H4 | `Warning`/`Critical` unaffected (never auto-dismissed). |
| H5 | Explicit `dismiss*` still dismisses a held notification. |

Existing test `info_auto_dismisses_after_ten_seconds` must pass unchanged.

## C4 Raise-site changes (FR-011, FR-013, FR-017)

| Site | Before | After |
|------|--------|-------|
| `controller.rs::handle_device_lost` | `raise_with_args(Critical, device-lost, [device])` | `raise_with_detail(Critical, device-lost, [device, fallback], lost_id)` |
| `controller.rs::raise_device_warning(MissingPreferred)` | `raise_with_args(Warning, device-missing-at-launch, [device=<raw id>])` | `raise_with_detail(…, [device=<human or generic>, fallback], id)`; `raise_with_args` with the same args when no id |
| `controller.rs::handle_device_list_changed` | `raise_with_args(Info, device-available-again, [device])` | `raise_with_detail(…, [device], found.id)` |
| `controller.rs::raise_settings_warning(InvalidKeybindings)` | `raise_with_args(Warning, keybindings-invalid-entries, [ids])` | `raise_with_detail(Warning, keybindings-invalid-entries, [], ids.join(", "))` |

Severity, trigger and action set of every notification: unchanged (FR-017).

## C5 `device_policy`

```text
pub fn display_name_for_saved(id: Option<&DeviceId>, saved_name: Option<&str>) -> Option<String>
pub fn resolve(devices, preferred: Option<&DeviceId>, saved_name: Option<&str>, confirmed: bool)
    -> (DeviceResolution, Option<DeviceWarning>)
DeviceWarning::MissingPreferred { device_name: Option<String>, device_id: Option<DeviceId>, fallback_name: String }
```

Resolution order per [data-model.md §4](../data-model.md#4-display-name-resolution-device_policydisplay_name_for_saved).
