# Contract: Plugin API 1.2 — the `ui` namespace

**Feature**: 011-plugin-ui-contributions | **Supersedes**: [009 plugin-api-v1.md](../../009-plugin-runtime-and-permissions/contracts/plugin-api-v1.md) §3/§4 (additively), [010 plugin-api-v1.1.md](../../010-transport-focus/contracts/plugin-api-v1.1.md) (additively) | **Schema**: `crates/modplayer-capability-gateway/api/v1.toml` (the generated `docs/plugin-api/v1.md` is the authoritative rendering; this file is the human-authored design it must match)

## 1. Version and capabilities

- `api_version` = **1.2** (additive minor; every `api = "1.0"`/`"1.1"` manifest still loads).
- `ready_ack.capabilities` and `api.capabilities` gain `"ui.panel"`, `"ui.overlay"`, `"ui.shortcuts"`, `"ui.settings"`, `"ui.notify"`.
- Permissions `ui.panel`, `ui.overlay`, `ui.shortcuts`, `ui.settings`, `ui.notify` become `operable = true`.

## 2. Result convention and check order (unchanged, FR-024)

Every call returns `value, nil` or `nil, { code, reason, message }`.
Order: **permission → rate limit → validation → capacity**. `no_focus`
and `budget_exceeded` are never produced by a `ui.*` call.

| Rate bucket | Requests | Limit |
|---|---|---|
| `ui` | `register_panel`, `update_widget`, `add_overlays`, `remove_overlays`, `clear_overlays`, `register_action`, `register_settings`, `get_settings` | 100 / rolling 1 s |
| `notify` | `notify` | 6 / rolling 60 s (PL-8.1) |

## 3. Requests

### 3.1 `api.ui.register_panel(panel_id, title, widgets)` — `ui.panel`

`widgets` = array of widget tables, rendered top-to-bottom:

```lua
{ id = "tempo", kind = "slider", label = "Tempo", min = 60, max = 200, step = 1, value = 120 }
{ id = "snap",  kind = "toggle", label = "Snap to beat" }
{ id = "go",    kind = "button", label = "Take over", action = "take_over" }   -- action: this plugin's own action name (FR-008)
{ id = "mode",  kind = "list",   label = "Mode", items = { {id="a",label="A"}, {id="b",label="B"} }, selected = "a" }
{ id = "hdr",   kind = "label",  label = "Section", text = "Section" }
{ id = "status",kind = "text",   label = "Status", text = "idle" }
{ id = "lvl",   kind = "meter",  label = "Input level" }
{ id = "marks", kind = "marker_list", label = "My markers" }
{ id = "gain",  kind = "knob",   label = "Gain", min = 0, max = 1, step = 0.01 }
```

Returns `true`. Replaces an existing `panel_id` atomically (pending
`panel_interaction`s for the old layout are dropped).

| Refusal | reason | when |
|---|---|---|
| `invalid_state` | `panel_limit` | 17th distinct panel id |
| `invalid_state` | `panel_widget_limit` | > 100 widgets |
| `invalid_state` | `invalid_widget_id` | missing/malformed/duplicate `id` — message `widgets[<i>]` |
| `invalid_state` | `unlabeled_widget` | missing/empty `label` (after `@key` resolution) — message `widgets[<i>] (<id>)` |
| `invalid_state` | `invalid_value` | `min >= max`, `step <= 0`, `value` out of range, > 256 items, duplicate item ids, `selected` unknown, title > 64, label > 256, text > 1,024 |

### 3.2 `api.ui.update_widget(panel_id, widget_id, value)` — `ui.panel`

| kind | accepted `value` |
|---|---|
| `toggle` | boolean |
| `slider` / `knob` | number in `[min, max]` (never clamped) |
| `list` | item id string (`not_found` if unknown) **or** `{ items = {...}, selected = "id"? }` |
| `text` | string ≤ 1,024 |
| `meter` | number, clamped to `[0, 1]` |
| `label` / `button` / `marker_list` | — `invalid_state`/`not_updatable` |

Returns `true`. `not_found` for an unknown panel or widget id. Never
produces an event.

### 3.3 `api.ui.add_overlays(primitives)` / `remove_overlays(ids)` / `clear_overlays()` — `ui.overlay`

```lua
{ id = "c1", kind = "line",   at = 12000 }                                   -- track-time ms (integer ≥ 0)
{ id = "v1", kind = "region", from = 12000, to = 24000, color = "secondary" }
{ id = "l1", kind = "label",  at = 12000, text = "Verse 1" }                 -- ≤ 64 chars, detail view only
{ id = "g1", kind = "glyph",  at = 12000, icon = "chord" }                   -- host glyph or manifest glyphs key
```

`color` ∈ `accent` (default) | `secondary` | `positive` | `warning` | `neutral`.
`add_overlays` is additive; re-adding an id replaces it. Refusals:
`invalid_state`/`overlay_limit` (post-merge count > 500, whole call),
`invalid_state`/`invalid_value` (bad id, `from >= to`, negative or
non-integer position, unknown icon, label too long, unknown color),
`not_found` (`remove_overlays` with an unknown id — nothing removed).
Overlays are cleared on every track change and on the plugin's
disable/suspend/unload.

### 3.4 `api.ui.register_action(spec)` — `ui.shortcuts`

```lua
api.ui.register_action({ id = "take_over", label = "Take over transport", kind = "trigger",
                         default_binding = "L", repeats_while_held = false })
```

Registered id: `<plugin identifier>.<id>`. `kind` ∈ `trigger` | `continuous`.
`default_binding` uses 007's platform-neutral encoding (`Primary+Shift+K`,
`L`, …); a binding 007 rejects or cannot parse registers the action
**unbound** with a console warning — the call still returns `true`.
Refusals: `invalid_state`/`action_limit` (65th), `invalid_state`/`invalid_value`
(bad id/label/kind). Re-registering replaces label/kind/flag/default,
keeps the user's override.

### 3.5 `api.ui.register_settings(fields)` / `api.ui.get_settings()` — `ui.settings`

```lua
api.ui.register_settings({
  { id = "snap",   kind = "boolean", label = "Snap to beat", default = true, description = "…" },
  { id = "shift",  kind = "number",  label = "Semitone shift", min = -12, max = 12, step = 1, default = 0 },
  { id = "name",   kind = "string",  label = "Preset name", default = "" },
  { id = "mode",   kind = "choice",  label = "Mode", options = { {id="a",label="A"}, {id="b",label="B"} }, default = "a" },
})
local values = api.ui.get_settings()   -- { snap = true, shift = 0, name = "", mode = "a" }
```

Refusals: `invalid_state`/`settings_field_limit` (> 100),
`invalid_state`/`invalid_schema` (message `fields[<i>]`). `get_settings`
substitutes each field's `default` for a missing or invalid stored
value (console warning; storage untouched) and is `permission_denied`
without `ui.settings`; with no schema registered it returns `{}`.
There is no plugin write call.

### 3.6 `api.ui.notify(level, text)` — `ui.notify`

`level` ∈ `critical` | `warning` | `info`; `text` ≤ 200 chars. Returns
`true`. `rate_limited` for the 7th call in a rolling 60 s (nothing
shown, never queued); `invalid_state`/`invalid_value` for a bad level or
over-length text (the call **did** consume a slot).

### 3.7 `api.transport.request_focus()` (~, 010)

Unchanged signature. When called synchronously inside a
`panel_interaction` or `action_invoked` handler, the request is
user-interaction-originated: under **auto on interaction** it is granted
immediately (FR-026); other policies unchanged.

## 4. Events

| Event | Requires | Payload | Delivery |
|---|---|---|---|
| `panel_interaction` | `ui.panel` | `{ panel = "<panel_id>", widget = "<widget_id>", value = <bool|number|string> }` | direct to the owning plugin, once per committed change |
| `action_invoked` | `ui.shortcuts` | `{ action = "<id>", source = "keyboard" | "ui", value = 1.0 }` (`value` absent for `trigger`) | direct; never for an inactive action |
| `settings_changed` | `ui.settings` | `{ changes = { ["<field-id>"] = <value>, … } }` | direct; user edits only, **after** the value is readable through `get_settings()` |

No event is delivered for close/disable/float/dock, theme change,
zoom/scroll, or track-change overlay clearing.

## 5. Manifest additions (`plugin.toml`)

```toml
icon           = "icon.png"                  # square PNG ≤ 128 px, ≤ 256 KiB (optional)
default_locale = "en-US"                     # optional; default "en-US"

[glyphs]                                     # ≤ 32 entries; PNG ≤ 32 px, ≤ 64 KiB each
chord = "glyphs/chord.png"

[strings.en-US]
tempo = "Tempo"
```

A missing/non-PNG/over-limit asset is **not** a manifest error (generic
glyph + console warning). A `glyphs` key failing `[a-z][a-z0-9_]{0,31}`
or > 32 entries **is** (`MalformedField{"glyphs"}`). Any user-facing
string parameter may be `"@tempo"` → resolved from `[strings.<active
locale>]`, then `[strings.<default_locale>]`, else rendered literally
with a console warning.

## 6. Change request (Constitution IX — copied into the PR body)

**Change**: API 1.1 → 1.2. Adds the `ui` namespace (9 requests), 3
events, 2 rate buckets, 4 manifest fields; flips 5 permissions to
operable. **Compatibility**: additive; no existing request, event,
payload field or refusal changes meaning; `ready_ack.capabilities`
grows. **Deprecations**: none. **Migration**: none required.
**Reference**: regenerated `docs/plugin-api/v1.md`.

## 7. Named tests

- `modplayer-capability-gateway/tests/api_reference.rs::reference_is_current` (regenerated 1.2)
- `tests/gateway.rs::{notify_window_six_per_minute, notify_refused_at_validation_still_counts, ui_category_101st_in_window}`
- `tests/ui_validation.rs::{unlabeled_widget_names_path, duplicate_widget_id, panel_widget_limit_101, slider_bounds, list_item_bounds, overlay_region_order, overlay_unknown_icon, schema_default_out_of_range, notify_text_201, id_grammar_proptest}`
- `modplayer-plugin-runtime/tests/bindings.rs::{ui_calls_are_rpcs, get_settings_is_local_and_substitutes_defaults, settings_scope_unreachable_from_lua}`
- `modplayer-plugin-runtime/tests/scheduler.rs::{panel_interaction_payload, action_invoked_payload, settings_changed_after_write, request_focus_flag_inside_interaction_handler}`
