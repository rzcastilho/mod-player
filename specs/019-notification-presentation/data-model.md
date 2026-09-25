# Data Model: Notification Placement, Severity, and Message Quality

**Feature**: 019-notification-presentation | **Date**: 2026-09-25 | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Deltas only; every field not listed is unchanged from 001/002/003/009/011.

## 1. `Notification` (core, `crates/modplayer-core/src/notifications.rs`)

| Field | Type | Status | Notes |
|-------|------|--------|-------|
| `id` | `u64` | unchanged | Key for all UI expansion state. |
| `severity` | `Severity` | unchanged | `Critical` / `Warning` / `Info`. |
| `message_key` | `&'static str` | unchanged | Fluent key (FR-016). |
| `args` | `Vec<(&'static str, String)>` | unchanged type | Device keys gain `fallback`; `keybindings-invalid-entries` loses `ids`. |
| `action`, `actions` | unchanged | unchanged | ≤ 2 (`MAX_NOTIFICATION_ACTIONS`), FR-014. |
| `created_at` | `Instant` | unchanged | Still "raised at"; no longer the auto-dismiss origin. |
| `dismissed` | `bool` | unchanged | |
| `dedupe_key` | `Option<String>` | unchanged | |
| `attribution` | `Option<PluginAttribution>` | unchanged | |
| **`detail`** | `Option<String>` | **new, pub** | Non-localised technical text shown behind "Details". `None` unless raised via `raise_with_detail`. Never set by `raise_attributed` (plugins cannot set it). |
| **`auto_dismiss_from`** | `Instant` | **new, pub(crate)** | Origin of the 10 s `Info` timer. `= created_at` at raise; `= now` on `release_auto_dismiss`. |
| **`held`** | `bool` | **new, pub(crate)** | `true` while the UI reports the card's "Show more" or "Details" expanded. |

**Validation / invariants**

- `detail`, when `Some`, is non-empty (raise sites never pass `""`).
- `tick(now)`: `Info ∧ ¬dismissed ∧ ¬held ∧ now − auto_dismiss_from ≥ 10 s ⇒ dismissed = true`. `Warning`/`Critical`: never auto-dismissed (unchanged).
- Hold/release on unknown or dismissed ids is a no-op.

**State transitions (Info only)**

```text
Raised(held=false, from=created_at)
  --10 s elapsed--> Dismissed
  --hold(id)--> Held            (no ageing)
Held --release(id, now)--> Raised(held=false, from=now)
any --dismiss(id) / dismiss_by_key / dismiss_by_dedupe--> Dismissed
```

## 2. `detail` values per notification (FR-013)

| Message key | Severity | `args` | `detail` |
|-------------|----------|--------|----------|
| `device-lost` | Critical | `device` = lost device's `OutputDeviceInfo::name`; **`fallback`** = fallback's `name` (or `tr("notification-device-fallback-default")` if empty) | lost `DeviceId` display form |
| `device-missing-at-launch` | Warning | `device` = resolved display name (§4) or `tr("notification-device-unknown")`; **`fallback`** as above | preferred `DeviceId` display form (omitted if no id) |
| `device-available-again` | Info | `device` = `found.name` | `found.id` display form |
| `keybindings-invalid-entries` | Warning | *(none — `ids` removed)* | dropped ids joined by `", "` |
| every other key | — | unchanged | `None` |

## 3. `DeviceWarning::MissingPreferred` (core, `device_policy.rs`)

```text
MissingPreferred {
    device_name: Option<String>,   // resolved display name; None ⇒ generic phrase
    device_id: Option<DeviceId>,   // for `detail`
    fallback_name: String,         // default device's OutputDeviceInfo::name
}
```

`resolve(devices, preferred, saved_name: Option<&str>, confirmed)` — new
`saved_name` parameter. Pure; no I/O.

## 4. Display-name resolution (`device_policy::display_name_for_saved`)

Input: `id: Option<&DeviceId>`, `saved_name: Option<&str>`. Output
`Option<String>`, first match wins:

1. `saved_name` trimmed, non-empty → it.
2. `id.as_str()` starts with `name:` and the remainder trimmed is non-empty → remainder.
3. otherwise → `None` (caller substitutes "Your saved output device").

Never returns any other substring of the raw id. Property test: for arbitrary
`id` not in `name:` form and `saved_name = None`, output is `None`.

## 5. Settings: `output_device_name` (core, `settings/model.rs`)

| Layer | Field | Type | Default | Serialization |
|-------|-------|------|---------|---------------|
| `AudioSettings` | `output_device_name` | `Option<String>` | `None` | — |
| `RawAudio` (`[audio]`) | `output_device_name` | `Option<String>` | `None` (`#[serde(default)]`) | `skip_serializing_if = "Option::is_none"` |

- Load: absent ⇒ `None`; empty/whitespace ⇒ `None` (no `InvalidField`, no warning).
- Written only by `PlaybackController::confirm_device` (with `output_device`), from the confirmed device's `OutputDeviceInfo::name`.
- `schema_version` unchanged. Contract: [contracts/settings-output-device-name.md](./contracts/settings-output-device-name.md).

## 6. Notification stack UI state (ui, `crates/modplayer-ui/src/notifications.rs`)

```text
StackState {
    expanded: bool,              // "{N} more" activated; forced false when overflow == 0
    show_more: BTreeSet<u64>,    // ids whose full message is shown
    details:   BTreeSet<u64>,    // ids whose Details payload is shown
}
```

Owned by `ModPlayerApp`; never persisted; not in egui memory.

- Each frame: remove from `show_more`/`details` every id not in `center.visible()`.
- Moving between shown and overflow never changes an id's flags.

**Derived per frame**

| Name | Rule |
|------|------|
| `card_width(screen_w)` | `min(360.0, screen_w − 16.0)` logical px |
| `partition(visible_len, expanded)` | `shown = expanded ? visible_len : min(visible_len, 3)`; `overflow = visible_len.saturating_sub(3)` |
| overflow button label | `overflow > 0 ∧ ¬expanded` ⇒ `notification-more` (`$count = overflow`); `expanded` ⇒ `notification-show-fewer`; `overflow == 0` ⇒ no button |
| expanded max height | `0.6 × screen_rect.height()` then vertical scroll |
| "Show more" toggle present | galley at 2 rows is `elided` |
| "Details" toggle present | `notification.detail.is_some()` |
| Info hold wanted | `severity == Info ∧ (id ∈ show_more ∨ id ∈ details)` |

**`NotificationInteraction`** (returned by `show`) gains
`hold_changes: Vec<(u64, bool)>` — `(id, true)` when an Info card's "hold
wanted" turns on this frame, `(id, false)` when it turns off. `dismissed` and
`action_clicked` unchanged.

## 7. Severity presentation mapping

| Severity | Role (`theme::tokens::Roles`) | Glyph | Word key |
|----------|-------------------------------|-------|----------|
| Critical | `danger` | `⛔` | `severity-critical` |
| Warning | `warning` | `⚠` | `severity-warning` |
| Info | `positive` | `ℹ` | `severity-info` |

Card fill `surface_raised`; accent bar 4 px, full card height, left edge.
