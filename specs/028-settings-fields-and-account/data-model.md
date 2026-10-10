# Data Model: Settings Fields, Placeholders, and Account Summary

**Feature**: 028-settings-fields-and-account | **Date**: 2026-09-30

Presentation-only feature: **no persisted format, value domain or default changes** (FR-017). Everything below is either an existing model read as-is, one new core constant/predicate, or UI-local frame/session state. Decisions referenced as R# live in [research.md](./research.md).

## 1. Existing models read (unchanged)

| Model | Crate / path | Fields used | Notes |
|---|---|---|---|
| `AudioSettings` | `modplayer-core/src/settings/model.rs` | `buffer_preset`, `limiter_ceiling_db`, `safe_volume{enabled,cap}`, `theme`, `high_contrast`, `device_name`, `nudge_step_ms` | `AudioSettings::default()` is the **single source of defaults** for reset (R4). |
| `CeilingDb` | `modplayer-engine/src/types.rs` | `MIN = -6.0`, `MAX = -0.1`, `Default = -1.0` | Read-only; engine crate not modified. |
| `SafeVolume` / `VolumePercent` | engine types | default `{enabled: true, cap: 50}` | Read-only. |
| `BufferPreset` | engine types | `#[default] Balanced` | Read-only. |
| `Theme` | engine types | default `System` | Read-only. |
| `SettingsCategory`, `SettingDescriptor`, `DESCRIPTORS` | `modplayer-core/src/settings_registry.rs` | category, id, title_key | Descriptor ids are the field ids used for focus/highlight/reset. |
| `AccountSession` | `modplayer-account/src/session.rs` | `display_name`, `tier`, `last_validated_at: Option<OffsetDateTime>`, `state` | Read-only; no new data collected. |
| `AccountService::signout_categories()` | `modplayer-account` | `Vec<&'static str>` Fluent keys | Consequence list for the dialog. |

## 2. New core items (additive, non-persisted)

### 2.1 `NUDGE_STEP_MS_RANGE` (modplayer-core, settings model)

```text
pub const NUDGE_STEP_MS_RANGE: RangeInclusive<u16> = 1..=1000;
```
- Used by: existing `clamp_nudge_step_ms` (replaces its literal), UI `DragValue::range`, range caption (R3).
- Validation: unchanged — load-time clamp into the range.

### 2.2 `SettingsCategory::is_available(self) -> bool` (modplayer-core, settings_registry)

| Variant | is_available |
|---|---|
| `Offline`, `PrivacyDiagnostics` | `false` |
| every other variant | `true` |

- Invariant (tested): `!c.is_available() ⇒ DESCRIPTORS has no entry with category == c`.
- Lifecycle: later features (003-offline-and-library/001, 007-operations-and-polish/001) flip their variant to `true` when they add settings; the badge and not-available body disappear automatically.

## 3. UI-local models (`crates/modplayer-ui/src/settings/`)

### 3.1 `FieldSpec` (field.rs) — per-frame, borrowed

| Field | Type | Meaning / rule |
|---|---|---|
| `id` | `&'static str` | Descriptor id (`"audio.limiter_ceiling"`…); must exist in `DESCRIPTORS` for search-reachable fields. |
| `label` | `String` | `tr(title key)`; also the reset accessible-name `{field}`. |
| `help` | `Option<String>` | Drawn indented `space::SM`, `text::SECONDARY`, `roles.text_secondary`, max width `body_measure`. |
| `range` | `Option<String>` | Pre-formatted caption (`setting-range-*`), drawn under the control, secondary style. |
| `reset` | `ResetState` | `Hidden` \| `Offered` — `Offered` iff current ≠ default and a write path exists (R4). |
| `highlight` | `bool` | This field is the active search highlight target. |

### 3.2 `FieldOutput` (field.rs)

| Field | Type | Meaning |
|---|---|---|
| `rect` | `egui::Rect` | Whole field (label + help + control + caption); used for highlight paint and `scroll_to_rect`. |
| `control` | `egui::Response` | The control's response (focus requests, `changed()`). |
| `reset_clicked` | `bool` | Reset activated this frame (click or keyboard). |

### 3.3 `ResettableField` catalogue (field.rs) — static table

| Field id | Default source | Reset offered when | Write path |
|---|---|---|---|
| `audio.buffer_preset` | `defaults.buffer_preset` (Balanced) | `controller.preset() != default && controller.preferred_device().is_some()` | `confirm_device(preferred, default)` |
| `audio.limiter_ceiling` | `defaults.limiter_ceiling_db` (-1.0) | `controller.ceiling() != default` | `set_ceiling` |
| `audio.safe_volume_enabled` | `defaults.safe_volume.enabled` (true) | `cached.safe_volume.enabled != default` | `persist_safe_volume` |
| `audio.safe_volume_cap` | `defaults.safe_volume.cap` (50) | `cached.safe_volume.cap != default` (even if switch off) | `persist_safe_volume` |
| `appearance.theme` | `defaults.theme` (System) | `cached.theme != default` | `persist_theme` + `theme::apply` |
| `appearance.high_contrast` | `defaults.high_contrast` (false) | `cached.high_contrast != default` | `set_high_contrast` + persist |
| `playback.device_name` | `defaults.device_name` (None) | `cached.device_name.is_some()` | `set_device_name("")` |
| `markers.nudge_step_ms` | `defaults.nudge_step_ms` (10) | `controller.nudge_step_ms() != default` | `set_nudge_step_ms` |

Explicitly **not** resettable: `audio.output_device`, `language.locale`, `audio.test_output_device`, `account.*`, `controls.*`, `developer.*`, plugin fields.

### 3.4 `FieldHighlight` (mod.rs, on `SettingsScreen`) — session state

| Field | Type | Rule |
|---|---|---|
| `id` | `&'static str` | Descriptor id of the chosen search result. |
| `started_at` | `f64` | `ctx.input(|i| i.time)` when armed. |
| `armed` | `bool` | `false` on the selecting frame; becomes `true` next frame, after which key/pointer presses clear it. |

State transitions:

```text
None ──(descriptor result chosen; category ∈ {Audio, Playback, Appearance, Language, Account})──▶ Some{armed:false}
Some{armed:false} ──(next frame)──▶ Some{armed:true}
Some{armed:true} ──(time − started_at ≥ 3.0 s) | (any Key pressed) | (any PointerButton pressed) | (category changed)──▶ None
Some ──(new result chosen)──▶ Some{new id, armed:false}
```
Controls/Developer/Plugins results never create a highlight (focus only, unchanged).

### 3.5 `SettingsScreen` additions (mod.rs)

| Field | Type | Purpose |
|---|---|---|
| `defaults` | `AudioSettings` | `AudioSettings::default()` snapshot (reset comparisons). |
| `highlight` | `Option<FieldHighlight>` | §3.4. |
| `pending_focus` | `Option<&'static str>` | Deferred focus after a reset (next frame), merged with `focus_target`. |

### 3.6 Account summary view model (account.rs, per frame)

| Line | Source | Fallback |
|---|---|---|
| Identity (strong) | `session.display_name` | `trim().is_empty()` → `account-identity-unavailable` "Name unavailable" |
| Tier | `Tier::{Premium,Free}` → `tier-premium` / `tier-free` | `Tier::Unknown` → `account-tier-unverified` "Not verified yet" |
| Last verified | `format_last_verified(when, local_offset_at(when))` | `None` → `account-never-verified` "Never verified online" |

`format_last_verified` (pure): `(OffsetDateTime, UtcOffset) -> String` via `account-last-verified-at` with `$day` (no pad), `$month` (`date-month-short-N`), `$year`, `$time` (`HH:MM`, 24 h, zero-padded).

### 3.7 Sign-out dialog geometry (account.rs)

- `signout_dialog_width(viewport_w) = clamp(viewport_w − 2·space::XXL, 420, 560)`.
- Content: title (heading), intro (wrapped), one wrapped bullet per `signout_categories()` entry, buttons `[Cancel][Sign out(Destructive)]`.
- Invariants: no `Label::truncate`; modal rect ⊆ viewport at 960×640 with 40 % expansion; Cancel focused once on open.

### 3.8 Category row item text (category_row.rs)

- `category_item_job(ui, category) -> LayoutJob`: `section_label(label)`; if `!is_available()`, append `space::XS` + `settings-coming-soon` in secondary style/role.
- Same job used for measuring (`measure_category_width`), inline/pinned drawing and "More" menu items.
- Accessible name: available → `label`; unavailable → `settings-category-coming-soon-a11y { $category }`.
