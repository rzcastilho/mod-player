# Contract: UI surface, accessibility names, and Fluent keys

**Crate**: `modplayer-ui` (egui/eframe) | **Traces**: FR-002, FR-003, FR-005, FR-015–FR-019, FR-021, FR-022

Every interactive widget below has (1) a Fluent key for its visible label, (2) an accessible name (same string, via egui's AccessKit integration), (3) a keyboard path. Colour never carries meaning alone (severity icons + text).

## Screens

### Device Check (modal-free full-window view; shown on first launch or from Settings › Audio)

| Widget | Kind | Fluent key | Keyboard |
|---|---|---|---|
| Device list | radio group, system default preselected | `device-check-device-list` | ↑/↓ moves selection → tone restarts |
| Buffer preset | combo: Performance / Balanced / Safe with `(~N ms)` suffix | `buffer-preset-performance` … | Enter/Space opens, ↑/↓, Enter |
| Play test tone | button | `device-check-play-tone` | Enter/Space |
| Prompt | text "Did you hear that?" | `device-check-question` | — |
| Yes | button (default action) | `device-check-yes` | Enter |
| No, try another | button | `device-check-no` | |
| Skip for now | button | `device-check-skip` | Esc |
| Empty state (zero devices) | text + only Skip | `device-check-no-devices` | |

Latency label format: `{preset} (~{ms} ms)` with `ms` rounded to one decimal; raw frame counts never appear here (FR-005).

### Main window

| Region | Contents |
|---|---|
| Navigation (left rail) | Library · Now Playing · Plugins · Settings — `nav-library`, `nav-now-playing`, `nav-plugins`, `nav-settings`; keys `Ctrl/Cmd+1..4`; Tab focus order = visual order |
| Notification area (top-right stack, newest first) | each item: severity icon + severity text (`severity-critical` / `severity-warning` / `severity-info`) + message + Dismiss button (`notification-dismiss`); Info auto-dismisses after 10 s |
| Content | selected section |

### Now Playing

| Widget | Kind | Fluent key | Notes |
|---|---|---|---|
| Track title | text | `synthetic-track-title` = "Synthetic test track" | |
| Play/Pause | toggle button | `transport-play` / `transport-pause` | Space |
| Stop | button | `transport-stop` | |
| Master volume | slider 0–100 % with dB readout | `master-volume` | ←/→ ±1, PgUp/PgDn ±10; accessible value = "{pct} %, {db} dB" |
| Peak meter | custom bar, dBFS scale −60..0 with ceiling tick | `peak-meter` | accessible value = last peak in dBFS; reads `RtShared::peak_bits` each frame |
| Disabled reason (zero devices) | inline text | `transport-disabled-no-device` | transport buttons disabled |

### Library, Plugins

Placeholder text only: `placeholder-library`, `placeholder-plugins`.

### Settings

Category list (fixed order, `settings-cat-account` … `settings-cat-about`), search box (`settings-search`, focused with `Ctrl/Cmd+F`), results list showing `{category} › {title}`; Enter on a result navigates to the category and focuses the setting.

Working settings (descriptor id → widget):

| Category | id | Widget | Fluent title / description keys |
|---|---|---|---|
| Audio | `audio.output_device` | combo of devices | `setting-output-device`, `setting-output-device-desc` |
| Audio | `audio.buffer_preset` | combo with latency suffix | `setting-buffer-preset`, `-desc` |
| Audio | `audio.limiter_ceiling` | slider −6.0..−0.1 step 0.1 | `setting-limiter-ceiling`, `-desc` (description contains "ceiling" so SC-008 search works) |
| Audio | `audio.safe_volume_enabled` | checkbox | `setting-safe-volume`, `-desc` |
| Audio | `audio.safe_volume_cap` | slider 0–100 | `setting-safe-volume-cap`, `-desc` |
| Audio | `audio.test_output_device` | button → Device Check | `setting-test-output-device`, `-desc` |
| Appearance | `appearance.theme` | combo System / Light / Dark | `setting-theme`, `-desc` |
| Language | `language.locale` | combo (English only) | `setting-locale`, `-desc` |
| Developer | `developer.buffer_frames` | read-only text "requested N / negotiated M frames" | `setting-buffer-frames`, `-desc` |
| Developer | `developer.raise_notification` | three buttons Critical / Warning / Info | `setting-raise-notification`, `-desc` |

Other categories render `placeholder-settings-category` and contribute no descriptors to search.

## Notification message keys

`device-lost` (`{ $device }`), `device-missing-at-launch` (`{ $device }`), `device-available-again` (`{ $device }`), `no-output-devices`, `device-appeared` (`{ $device }`), `settings-unreadable`, `settings-newer-version`, `settings-invalid-value` (`{ $fields }`), `settings-save-failed`, `sample-notification-critical` / `-warning` / `-info`.

## Theme

`Theme::System` → `ctx.set_theme(ThemePreference::System)` (egui tracks OS changes live); `Light`/`Dark` → fixed. Applied at startup before the first frame and immediately on change.

## Tests that pin this contract

- `modplayer-ui`: every Fluent key listed here resolves in `locales/en-US/*.ftl` (test iterates a static key table); the settings search returns `audio.limiter_ceiling` for query `"ceiling"` and `"CEIL"`, and nothing for `"zzz"`; search over the full table completes in < 1 ms (comfortably under the 50 ms budget, SC-008).
- Manual (quickstart): keyboard-only walkthrough and AT inspector check on each platform.
