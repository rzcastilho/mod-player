# Contract: Settings file (`settings.toml`)

**Crate**: `modplayer-core::settings` | **Traces**: FR-004, FR-005, FR-010, FR-011, FR-017, FR-020 (DM-20 subset)

## Location

`directories::ProjectDirs::from("", "ModPlayer", "ModPlayer").config_dir().join("settings.toml")`

| Platform | Resolved path |
|---|---|
| macOS | `~/Library/Application Support/ModPlayer/settings.toml` |
| Windows | `%APPDATA%\ModPlayer\ModPlayer\config\settings.toml` |
| Linux | `$XDG_CONFIG_HOME/modplayer/settings.toml` (default `~/.config/modplayer/`) |

Override for tests and portable use: environment variable `MODPLAYER_CONFIG_DIR` (absolute directory path). Every test writes under a temp directory via this variable; no test touches the real location.

## Schema (version 1)

```toml
schema_version = 1

[audio]
output_device = "coreaudio:AppleUSBAudioEngine:..."   # optional; DeviceId display form or "name:<name>"
device_confirmed = false
buffer_preset = "balanced"                            # "performance" | "balanced" | "safe"
limiter_ceiling_db = -1.0                             # -6.0 ..= -0.1, step 0.1
master_volume = 80                                    # 0 ..= 100

[audio.safe_volume]
enabled = true
cap = 50                                              # 0 ..= 100

[appearance]
theme = "system"                                      # "system" | "light" | "dark"
```

## Read rules (FR-020, edge cases)

| Situation | Result |
|---|---|
| File missing | defaults; **no** notification (first launch is normal) |
| File unparseable (TOML error) | defaults; `Warning` notification `settings-unreadable`; file rewritten on next change |
| `schema_version` > 1 | defaults; `Warning` `settings-newer-version`; file **not** rewritten until the user changes a setting |
| Unknown key | ignored (forward compatibility) |
| Out-of-range numeric | clamped by the newtype (`CeilingDb`, `VolumePercent`); no notification |
| Unknown enum string | field default; `Warning` `settings-invalid-value` (one notification per load, listing fields) |

## Write rules

1. Serialize the full struct (never partial updates).
2. Write to `settings.toml.tmp` in the same directory, `File::sync_all()`.
3. `std::fs::rename("settings.toml.tmp", "settings.toml")` (atomic replace on all three platforms).
4. On any error: keep the previous file intact, raise `Warning` `settings-save-failed`; do not retry automatically.
5. Writes happen on the controller thread, never on the real-time path, debounced 250 ms after the last change (a master-volume drag produces one write, not hundreds).

## Defaults (must match spec Assumptions)

`buffer_preset = balanced`, `limiter_ceiling_db = -1.0`, `safe_volume = { enabled = true, cap = 50 }`, `master_volume = 80`, `theme = system`, `output_device = none`, `device_confirmed = false`.

## Tests that pin this contract

- Round-trip: defaults → write → read == defaults.
- Out-of-range file values (`limiter_ceiling_db = 3.0`, `master_volume = 250`, `cap = -5`) load as `−0.1`, `100`, `0` (FR-025(e)).
- Garbage file loads defaults and produces exactly one `Warning`.
- Simulated crash mid-write (write `.tmp`, do not rename) leaves the previous file readable and unchanged.
- Unknown key and newer schema version behave as tabled above.
