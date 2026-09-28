// SPDX-License-Identifier: MIT OR Apache-2.0

//! `WindowSettings`: the `[window]` section of `settings.toml` — the main
//! window's restored inner size and the plugin dock's width
//! (018-window-sizing-and-responsive-dock, contracts/window-settings.md
//! W1/W2, data-model.md §1).
//!
//! Every field validates independently on load: absent, the wrong TOML
//! type, non-finite, or `<= 0` all fall back to that field's own default;
//! otherwise the value is clamped into its valid range. This mirrors
//! `RawAppearance::high_contrast`'s permissive-`toml::Value` pattern
//! (`settings/model.rs`) so one malformed `[window]` key never fails the
//! whole settings file, and never raises an `InvalidField` warning (the
//! spec treats it as though the key were simply absent).

/// Default restored window inner size, logical points (FR-001).
///
/// # Examples
///
/// ```
/// use modplayer_core::settings::DEFAULT_INNER_SIZE;
///
/// assert_eq!(DEFAULT_INNER_SIZE, (1200.0, 820.0));
/// ```
pub const DEFAULT_INNER_SIZE: (f32, f32) = (1200.0, 820.0);

/// Minimum window inner size, logical points (FR-002). The launch options
/// (contract W3) also pass this to `ViewportBuilder::with_min_inner_size`
/// so the OS itself enforces it.
///
/// # Examples
///
/// ```
/// use modplayer_core::settings::MIN_INNER_SIZE;
///
/// assert_eq!(MIN_INNER_SIZE, (960.0, 640.0));
/// ```
pub const MIN_INNER_SIZE: (f32, f32) = (960.0, 640.0);

/// Default plugin dock width, logical points (FR-004).
///
/// # Examples
///
/// ```
/// use modplayer_core::settings::DOCK_WIDTH_DEFAULT;
///
/// assert_eq!(DOCK_WIDTH_DEFAULT, 280.0);
/// ```
pub const DOCK_WIDTH_DEFAULT: f32 = 280.0;

/// Minimum plugin dock width, logical points (FR-004/FR-005).
///
/// # Examples
///
/// ```
/// use modplayer_core::settings::DOCK_WIDTH_MIN;
///
/// assert_eq!(DOCK_WIDTH_MIN, 240.0);
/// ```
pub const DOCK_WIDTH_MIN: f32 = 240.0;

/// Maximum plugin dock width, logical points (FR-004/FR-005) — further
/// limited at render time (`modplayer-ui::layout::effective_dock_width`)
/// to keep at least 560 pt of host content.
///
/// # Examples
///
/// ```
/// use modplayer_core::settings::DOCK_WIDTH_MAX;
///
/// assert_eq!(DOCK_WIDTH_MAX, 480.0);
/// ```
pub const DOCK_WIDTH_MAX: f32 = 480.0;

/// The `[window]` section's domain shape: the main window's restored inner
/// size and the plugin dock's width, all in logical points
/// (018-window-sizing-and-responsive-dock, data-model.md §1). Every value
/// a `PlaybackController` holds or a `SettingsStore` writes already
/// satisfies the post-load ranges documented on [`DEFAULT_INNER_SIZE`]/
/// [`MIN_INNER_SIZE`]/[`DOCK_WIDTH_MIN`]/[`DOCK_WIDTH_MAX`] — the setters
/// clamp too (contract W2).
///
/// # Examples
///
/// ```
/// use modplayer_core::settings::WindowSettings;
///
/// let w = WindowSettings::default();
/// assert_eq!((w.inner_width, w.inner_height), (1200.0, 820.0));
/// assert_eq!(w.dock_width, 280.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowSettings {
    pub inner_width: f32,
    pub inner_height: f32,
    pub dock_width: f32,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            inner_width: DEFAULT_INNER_SIZE.0,
            inner_height: DEFAULT_INNER_SIZE.1,
            dock_width: DOCK_WIDTH_DEFAULT,
        }
    }
}

/// Extracts a finite, positive `f64` from a raw TOML value — `None` for a
/// missing value (`value` is `None`), a non-numeric TOML type, `NaN`/
/// infinite, or `<= 0` (contract W1: all of these are "treated as
/// absent").
pub(crate) fn positive_finite(value: Option<&toml::Value>) -> Option<f64> {
    let raw = match value? {
        toml::Value::Float(f) => *f,
        toml::Value::Integer(i) => *i as f64,
        _ => return None,
    };
    (raw.is_finite() && raw > 0.0).then_some(raw)
}

/// Sanitizes `[window] inner_width` (data-model.md §1): absent/invalid ->
/// [`DEFAULT_INNER_SIZE`]`.0`; otherwise `max(v, `[`MIN_INNER_SIZE`]`.0)` —
/// no upper clamp. A finite `f64` that overflows `f32` on cast (e.g. a
/// hand-edited `1e300`) is treated the same as any other out-of-range
/// value the domain type cannot represent: the default, not `inf`.
pub(crate) fn sanitize_inner_width(value: Option<&toml::Value>) -> f32 {
    match positive_finite(value).map(|v| v as f32) {
        Some(v) if v.is_finite() => v.max(MIN_INNER_SIZE.0),
        _ => DEFAULT_INNER_SIZE.0,
    }
}

/// Sanitizes `[window] inner_height` (data-model.md §1): absent/invalid ->
/// [`DEFAULT_INNER_SIZE`]`.1`; otherwise `max(v, `[`MIN_INNER_SIZE`]`.1)` —
/// no upper clamp. See [`sanitize_inner_width`] on the `f32`-overflow case.
pub(crate) fn sanitize_inner_height(value: Option<&toml::Value>) -> f32 {
    match positive_finite(value).map(|v| v as f32) {
        Some(v) if v.is_finite() => v.max(MIN_INNER_SIZE.1),
        _ => DEFAULT_INNER_SIZE.1,
    }
}

/// Sanitizes `[window] dock_width` (data-model.md §1): absent/invalid ->
/// [`DOCK_WIDTH_DEFAULT`]; otherwise clamped into [`DOCK_WIDTH_MIN`,
/// `DOCK_WIDTH_MAX`].
pub(crate) fn sanitize_dock_width(value: Option<&toml::Value>) -> f32 {
    positive_finite(value).map_or(DOCK_WIDTH_DEFAULT, |v| {
        (v as f32).clamp(DOCK_WIDTH_MIN, DOCK_WIDTH_MAX)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_contract() {
        let w = WindowSettings::default();
        assert_eq!(w.inner_width, 1200.0);
        assert_eq!(w.inner_height, 820.0);
        assert_eq!(w.dock_width, 280.0);
    }

    #[test]
    fn sanitize_absent_is_default() {
        assert_eq!(sanitize_inner_width(None), DEFAULT_INNER_SIZE.0);
        assert_eq!(sanitize_inner_height(None), DEFAULT_INNER_SIZE.1);
        assert_eq!(sanitize_dock_width(None), DOCK_WIDTH_DEFAULT);
    }

    #[test]
    fn sanitize_wrong_type_is_default() {
        let s = toml::Value::String("wide".to_string());
        assert_eq!(sanitize_inner_width(Some(&s)), DEFAULT_INNER_SIZE.0);
        assert_eq!(sanitize_dock_width(Some(&s)), DOCK_WIDTH_DEFAULT);
    }

    #[test]
    fn sanitize_non_finite_or_non_positive_is_default() {
        for raw in [f64::NAN, f64::INFINITY, -5.0, 0.0] {
            let v = toml::Value::Float(raw);
            assert_eq!(sanitize_inner_width(Some(&v)), DEFAULT_INNER_SIZE.0);
            assert_eq!(sanitize_inner_height(Some(&v)), DEFAULT_INNER_SIZE.1);
            assert_eq!(sanitize_dock_width(Some(&v)), DOCK_WIDTH_DEFAULT);
        }
    }

    #[test]
    fn sanitize_f32_overflow_is_default_not_infinite() {
        // Finite in f64, but overflows f32 on cast — must not leak an
        // infinite (non-finite) domain value.
        let huge = toml::Value::Float(3.416_487_913_381_366e67);
        assert_eq!(sanitize_inner_width(Some(&huge)), DEFAULT_INNER_SIZE.0);
        assert_eq!(sanitize_inner_height(Some(&huge)), DEFAULT_INNER_SIZE.1);
    }

    #[test]
    fn sanitize_clamps_valid_values() {
        let below = toml::Value::Float(100.0);
        assert_eq!(sanitize_inner_width(Some(&below)), MIN_INNER_SIZE.0);
        assert_eq!(sanitize_inner_height(Some(&below)), MIN_INNER_SIZE.1);

        let low_dock = toml::Value::Float(10.0);
        assert_eq!(sanitize_dock_width(Some(&low_dock)), DOCK_WIDTH_MIN);
        let high_dock = toml::Value::Float(1_000.0);
        assert_eq!(sanitize_dock_width(Some(&high_dock)), DOCK_WIDTH_MAX);

        let mid_dock = toml::Value::Integer(300);
        assert_eq!(sanitize_dock_width(Some(&mid_dock)), 300.0);

        let above_min = toml::Value::Float(2_000.0);
        assert_eq!(sanitize_inner_width(Some(&above_min)), 2_000.0);
    }
}
