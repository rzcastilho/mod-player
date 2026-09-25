# Data Model: Window Sizing and Responsive Plugin Dock

**Feature**: 018-window-sizing-and-responsive-dock | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

All sizes are `f32` logical points.

## 1. `WindowSettings` (persisted, `modplayer-core::settings::window`)

Domain value carried in `AudioSettings.window`; wire form is the `[window]`
section of `settings.toml` (see [contracts/window-settings.md](./contracts/window-settings.md)).

| Field | Type | Default | Valid range after load | Source |
|---|---|---|---|---|
| `inner_width` | `f32` | 1200 | ≥ 960, finite | FR-001, FR-003, FR-014 |
| `inner_height` | `f32` | 820 | ≥ 640, finite | FR-001, FR-003, FR-014 |
| `dock_width` | `f32` | 280 | [240, 480], finite | FR-004, FR-005, FR-014 |

Constants (same module): `DEFAULT_INNER_SIZE = (1200, 820)`,
`MIN_INNER_SIZE = (960, 640)`, `DOCK_WIDTH_DEFAULT = 280`,
`DOCK_WIDTH_MIN = 240`, `DOCK_WIDTH_MAX = 480`.

**Validation (load, per field, independent)**:

1. Field absent → default.
2. TOML value not `Float`/`Integer` → default (not a load failure).
3. Value non-finite (`inf`, `nan`) or `≤ 0` → default.
4. Otherwise clamp: width `max(v, 960)`, height `max(v, 640)`, dock
   `clamp(v, 240, 480)`. No upper clamp on window size.

**Invariant**: every `WindowSettings` value the controller holds or the store
writes satisfies the post-load ranges above (the setters clamp too), so
`load(save(x)) == x` for any valid `x` (proptest, Constitution VIII).

**Relationships**: one per `settings.toml`; owned by `AudioSettings`;
shadowed in `PlaybackController.window` (read per frame, no I/O).

**Writes**:
- `dock_width`: on splitter drag end or each keyboard step (FR-005).
- `inner_width/height`: when `WindowSizeTracker` reports a settled size, and
  on `App::on_exit` (FR-003). Never while maximized/fullscreen.
- Write only when the clamped value differs from the shadow.

## 2. `WindowSizeTracker` (session-only, `modplayer-ui::layout`)

| Field | Type | Meaning |
|---|---|---|
| `last_saved` | `Vec2` | size last persisted (initialised to restored size) |
| `pending` | `Option<Vec2>` | latest non-maximized size not yet saved |
| `changed_at` | `Option<Instant>` | when `pending` last changed |
| `last_write` | `Option<Instant>` | last time a save was yielded |

Operations:
- `observe(size, maximized_or_fullscreen, now)` — ignore if maximized/fullscreen;
  if `|size − last_saved| > 0.5` on either axis (and differs from `pending`) set
  `pending = size, changed_at = now`.
- `poll(now) -> Option<Vec2>` — yields `pending` when
  `now − changed_at ≥ 500 ms` and (`last_write` is none or
  `now − last_write ≥ 500 ms`); sets `last_saved`, `last_write`, clears pending.
- `next_deadline() -> Option<Instant>` — for `request_repaint_after`.
- `flush() -> Option<Vec2>` — on exit, yields `pending` unconditionally.

## 3. Dock presentation (derived per frame, `modplayer-ui::layout`)

```text
enum DockPresentation { None, Docked, Hidden, Overlay }

dock_presentation(window_width, docked_count, overlay_open):
  docked_count == 0            → None      (nothing drawn, no toggle)
  window_width >= 1024         → Docked    (overlay_open forced false)
  overlay_open                 → Overlay
  else                         → Hidden
```

State transitions (`overlay_open` is session-only, egui temp memory):

| From | Event | To |
|---|---|---|
| Docked | window narrowed < 1024 | Hidden (overlay_open = false) |
| Hidden | "Panels" toggle on | Overlay |
| Overlay | "Panels" toggle off / Esc with focus inside | Hidden |
| Overlay / Hidden | window widened ≥ 1024 | Docked (overlay_open cleared) |
| Overlay / Hidden / Docked | last docked panel closed or floated | None (overlay_open cleared) |
| None | a panel is docked | Docked or Hidden per width |

"Panels" toggle visible ⇔ presentation ∈ {Hidden, Overlay}; pressed ⇔ Overlay.

Focus rule: if the previous frame's focused widget belonged to the dock/overlay
and this frame's presentation is Hidden or None, focus moves to the "Panels"
toggle (Hidden) or is surrendered (None — toggle absent).

## 4. Effective dock width (derived per frame)

```text
max_eff   = max(240, min(480, content_width − 560))
effective = clamp(live_or_stored_width, 240, max_eff)
```

- `content_width` = Now Playing content `Ui` width (window − nav rail −
  central panel margins), research R7.
- `live_or_stored_width` = the in-progress drag width (egui temp memory) if a
  drag is active, else `WindowSettings.dock_width`.
- Never written back to `WindowSettings` (FR-004).

**Splitter keyboard** (focused): `←` → `stored + 16`, `→` → `stored − 16`,
`Home` → 240, `End` → `max_eff`; result clamped to [240, `max_eff`] then
persisted (clamped again into [240, 480] by the setter).

## 5. Waveform heights (derived per frame)

```text
H        = Now Playing content Ui max_rect().height()   (research R11)
overview = max(64,  round(0.08 × H))
detail   = max(120, round(0.22 × H))
```

No upper bound. Replaces `waveform::OVERVIEW_HEIGHT` (72) and `DETAIL_HEIGHT` (120).

## 6. Docked panel header layout (derived per frame)

| Element | Rule |
|---|---|
| Icon | fixed 16 pt (`ICON_SIZE`, unchanged) |
| Title (`plugin-panel-header` = "{plugin} — {title}") | single line, `truncate()`; tooltip = full text on hover and while focused; AccessKit label = full text |
| Buttons Float/Dock, Close, [gap], Disable | `TextWrapMode::Extend`; never elided |
| Row decision | if `available − icon − spacing − buttons_w ≥ TITLE_MIN_WIDTH` → 1 row; else title row + `horizontal_wrapped` button row(s) |
| Host status/placeholder text | `Label::wrap()` |

## 7. New locale keys (`locales/en-US/plugins.ftl`)

| Key | en-US | Used by |
|---|---|---|
| `plugin-dock-panels-toggle` | Panels | transport row toggle (FR-007) |
| `plugin-dock-resize` | Resize plugin dock | splitter accessible name (FR-004a) |
| `plugin-dock-resize-value` | { $width } points | splitter AccessKit value text |

Title tooltip reuses `plugin-panel-header` (no new key).
