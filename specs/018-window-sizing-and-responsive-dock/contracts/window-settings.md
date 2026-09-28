# Contract: `[window]` settings section and controller API

**Feature**: 018-window-sizing-and-responsive-dock | Requirements: FR-001, FR-003, FR-005, FR-014 | Model: [data-model.md §1](../data-model.md)

## W1 — Wire format (`settings.toml`)

Optional, additive table. No `schema_version` bump.

```toml
[window]
inner_width = 1200.0   # logical points, float or integer
inner_height = 820.0
dock_width = 280.0
```

- Every key optional; absent table ≡ all keys absent.
- A key whose value is not a TOML float/integer, is non-finite, or is `≤ 0`
  is treated as absent. This MUST NOT fail deserialization of the file and
  MUST NOT alter any other section's loaded values. No notification is raised.
- Valid values are clamped: `inner_width ≥ 960`, `inner_height ≥ 640`,
  `dock_width ∈ [240, 480]`.
- Unknown keys inside `[window]` are ignored (same as other sections).
- Save always writes all three keys with the current clamped values.

## W2 — Rust surface (`modplayer-core`)

```rust
// modplayer_core::settings (re-exported)
pub struct WindowSettings { pub inner_width: f32, pub inner_height: f32, pub dock_width: f32 }
impl Default for WindowSettings { /* 1200, 820, 280 */ }
pub const DEFAULT_INNER_SIZE: (f32, f32);   // (1200.0, 820.0)
pub const MIN_INNER_SIZE: (f32, f32);       // (960.0, 640.0)
pub const DOCK_WIDTH_DEFAULT: f32;          // 280.0
pub const DOCK_WIDTH_MIN: f32;              // 240.0
pub const DOCK_WIDTH_MAX: f32;              // 480.0

// AudioSettings gains:
pub window: WindowSettings,

// PlaybackController gains:
pub fn window_settings(&self) -> WindowSettings;          // shadow read, no I/O
pub fn set_dock_width(&mut self, width: f32);             // clamp [240,480]; persist iff changed
pub fn set_window_inner_size(&mut self, w: f32, h: f32);  // clamp ≥ 960×640; persist iff changed
```

Setter rules: non-finite / `≤ 0` input is ignored (no write). Persistence goes
through the existing `persist_settings` path; a failed save raises the existing
`settings-save-failed` warning.

## W3 — Launch (`crates/modplayer/src/main.rs`)

`NativeOptions.viewport = ViewportBuilder::default()
.with_inner_size([w.inner_width, w.inner_height])
.with_min_inner_size([960.0, 640.0])` where `w = controller.window_settings()`.
Position, maximized and fullscreen are not set (OS decides).

## W4 — Test obligations

| ID | Test | Location |
|---|---|---|
| W4.1 | absent `[window]` → defaults 1200/820/280 | `crates/modplayer-core/tests/settings.rs` |
| W4.2 | below-min values clamp up; dock > 480 / < 240 clamps | same |
| W4.3 | `"wide"`, `nan`, `inf`, `-5`, `0` per key → default for that key only; other sections intact; no `InvalidField` | same |
| W4.4 | proptest: any valid `WindowSettings` round-trips exactly through save/load | `crates/modplayer-core/tests/settings_window_proptest.rs` |
| W4.5 | proptest: any `f64`/string per key loads to a value within the valid ranges | same |
| W4.6 | controller setters persist only on change and survive a new controller over the same store | `crates/modplayer-core/tests/controller_window.rs` |
