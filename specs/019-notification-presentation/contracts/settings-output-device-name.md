# Contract: `[audio] output_device_name` settings key

**Feature**: 019-notification-presentation | Extends
[001 contracts/settings-file.md](../../001-walking-skeleton/contracts/settings-file.md) `[audio]`.

```toml
[audio]
output_device = "coreaudio:AppleUSBAudioEngine:..."   # unchanged
output_device_name = "Scarlett 2i2 USB"               # NEW, optional
device_confirmed = true
```

| Rule | Behaviour |
|------|-----------|
| K1 | Optional string. Absent ⇒ `None`. Files without it load byte-for-byte equivalent state and raise no warning (FR-012). |
| K2 | Empty or whitespace-only ⇒ `None`; no `InvalidField`, no `settings-invalid-value` notification. |
| K3 | Written only by `confirm_device`, set to the confirmed device's `OutputDeviceInfo::name` in the same save as `output_device`. |
| K4 | Serialized only when `Some` (`skip_serializing_if`); a pre-019 file re-saved without a confirm stays without the key. |
| K5 | Read only to name a missing device in `device-missing-at-launch`; never used to match or select a device (matching stays id-then-name per 001 FR-004). |
| K6 | `schema_version` unchanged; older builds ignore the key. |
| K7 | Never logged beyond existing settings logging; contains no credential (Principle VI). |
