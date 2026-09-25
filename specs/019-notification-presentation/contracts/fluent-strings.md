# Contract: Fluent Strings (en-US)

**Feature**: 019-notification-presentation | FR-011, FR-016. All keys live in `locales/en-US/`.
Wording below is the shipped en-US text; any polish must keep the listed facts and contain no raw id.

## Rewritten (`locales/en-US/app.ftl`)

```ftl
device-lost = { $device } disconnected. Now playing through { $fallback }.
device-missing-at-launch = { $device } isn't connected. Playing through { $fallback } instead.
device-available-again = { $device } is available again. You can switch back in Settings › Audio.
```

## Rewritten (`locales/en-US/controls.ftl`)

```ftl
keybindings-invalid-entries = Some saved keyboard shortcuts couldn't be read and were reset to their defaults.
```

(`$ids` removed — ids move to `detail`.)

## New (`locales/en-US/app.ftl`)

```ftl
notification-more = { $count ->
    [one] { $count } more
   *[other] { $count } more
}
notification-show-fewer = Show fewer
notification-show-more = Show more
notification-show-less = Show less
notification-details = Details
notification-hide-details = Hide details
notification-device-unknown = Your saved output device
notification-device-fallback-default = the system default output
```

## Tests

- `crates/modplayer-ui/tests/fluent_keys.rs`: every new key resolves non-empty; device keys resolve with `device` + `fallback` args (`DEVICE_NAMED_KEYS` updated); `keybindings-invalid-entries` moves from `CONTROLS_WARNING_ARG_KEYS` to the plain list.
- SC-005: resolved `device-lost` / `device-missing-at-launch` contain both device names.
- SC-004: with the UX-38 id `coreaudio:AppleGFXHDAEngineOutputDP:10001:0:{6D1E-7715-00097FED}` as `detail`, no substring of length ≥ 4 of the id appears in the collapsed rendered text.
