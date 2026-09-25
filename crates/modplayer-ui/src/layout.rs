// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pure, per-frame window/dock/waveform layout math
//! (018-window-sizing-and-responsive-dock, data-model.md §2-§5,
//! contracts/ui-responsive-dock.md D1-D10). Every function here is a
//! deterministic, allocation-free computation from egui-observed inputs
//! (window/content width, content height, pointer state) to a layout
//! decision — no widget drawing, no settings I/O. `plugin_panels.rs`,
//! `now_playing.rs`, `app.rs` and `shell.rs` are the callers (see
//! plan.md's Implementation Phasing).

use std::time::{Duration, Instant};

use egui::Vec2;

/// Below this window width the plugin dock auto-hides (data-model.md §3,
/// FR-006).
///
/// # Examples
///
/// ```
/// assert_eq!(modplayer_ui::layout::AUTO_HIDE_THRESHOLD, 1024.0);
/// ```
pub const AUTO_HIDE_THRESHOLD: f32 = 1024.0;

/// The minimum amount of host content width the dock must always leave
/// visible, further limiting the dock's effective maximum width beyond
/// its stored `[240, 480]` range (data-model.md §4, FR-004a).
///
/// # Examples
///
/// ```
/// assert_eq!(modplayer_ui::layout::HOST_CONTENT_FLOOR, 560.0);
/// ```
pub const HOST_CONTENT_FLOOR: f32 = 560.0;

/// The splitter's `←`/`→` keyboard step, in logical points (data-model.md
/// §4, FR-005).
///
/// # Examples
///
/// ```
/// assert_eq!(modplayer_ui::layout::DOCK_KEY_STEP, 16.0);
/// ```
pub const DOCK_KEY_STEP: f32 = 16.0;

/// Waveform overview height's proportion of window content height
/// (data-model.md §5, FR-012): `max(WAVEFORM_OVERVIEW_MIN, round(H *
/// WAVEFORM_OVERVIEW_PROPORTION))`.
///
/// # Examples
///
/// ```
/// assert_eq!(modplayer_ui::layout::WAVEFORM_OVERVIEW_PROPORTION, 0.08);
/// ```
pub const WAVEFORM_OVERVIEW_PROPORTION: f32 = 0.08;

/// Waveform overview height's floor, in logical points (data-model.md §5).
///
/// # Examples
///
/// ```
/// assert_eq!(modplayer_ui::layout::WAVEFORM_OVERVIEW_MIN, 64.0);
/// ```
pub const WAVEFORM_OVERVIEW_MIN: f32 = 64.0;

/// Waveform detail height's proportion of window content height
/// (data-model.md §5, FR-012): `max(WAVEFORM_DETAIL_MIN, round(H *
/// WAVEFORM_DETAIL_PROPORTION))`.
///
/// # Examples
///
/// ```
/// assert_eq!(modplayer_ui::layout::WAVEFORM_DETAIL_PROPORTION, 0.22);
/// ```
pub const WAVEFORM_DETAIL_PROPORTION: f32 = 0.22;

/// Waveform detail height's floor, in logical points (data-model.md §5).
///
/// # Examples
///
/// ```
/// assert_eq!(modplayer_ui::layout::WAVEFORM_DETAIL_MIN, 120.0);
/// ```
pub const WAVEFORM_DETAIL_MIN: f32 = 120.0;

/// The window-size tracker's save debounce (data-model.md §2, research
/// R2): at most one `[window]` write every 500 ms while the user is
/// actively resizing.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// assert_eq!(modplayer_ui::layout::SAVE_DEBOUNCE, Duration::from_millis(500));
/// ```
pub const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Which form the plugin dock takes this frame (data-model.md §3, contract
/// D1), derived from the window width, how many panels are docked, and
/// whether the session-only overlay is open.
///
/// # Examples
///
/// ```
/// use modplayer_ui::layout::{dock_presentation, DockPresentation};
///
/// assert_eq!(dock_presentation(1200.0, 1, false), DockPresentation::Docked);
/// assert_eq!(dock_presentation(800.0, 1, false), DockPresentation::Hidden);
/// assert_eq!(dock_presentation(800.0, 1, true), DockPresentation::Overlay);
/// assert_eq!(dock_presentation(800.0, 0, true), DockPresentation::None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockPresentation {
    /// No panel is docked: nothing is drawn, no "Panels" toggle either.
    None,
    /// The window is wide enough (`>= AUTO_HIDE_THRESHOLD`): the dock
    /// renders as a normal docked column.
    Docked,
    /// The window is narrow and the overlay is closed: the dock is
    /// hidden, and the "Panels" toggle is shown (unpressed).
    Hidden,
    /// The window is narrow and the overlay is open: the dock renders as
    /// a dismissible right-anchored overlay.
    Overlay,
}

/// Derive this frame's [`DockPresentation`] (data-model.md §3 truth
/// table, contract D1):
///
/// ```text
/// docked_count == 0    -> None
/// window_width >= 1024 -> Docked  (overlay_open is forced false by the caller)
/// overlay_open          -> Overlay
/// else                  -> Hidden
/// ```
#[must_use]
pub fn dock_presentation(
    window_width: f32,
    docked_count: usize,
    overlay_open: bool,
) -> DockPresentation {
    if docked_count == 0 {
        DockPresentation::None
    } else if window_width >= AUTO_HIDE_THRESHOLD {
        DockPresentation::Docked
    } else if overlay_open {
        DockPresentation::Overlay
    } else {
        DockPresentation::Hidden
    }
}

/// The dock's effective render-time width (data-model.md §4, contract
/// D2): the stored (or in-progress drag) width, clamped to `[240,
/// max(240, min(480, content_width - HOST_CONTENT_FLOOR))]`. Never written
/// back to `WindowSettings` (FR-004) — this is a render-time clamp only.
///
/// # Examples
///
/// ```
/// use modplayer_ui::layout::effective_dock_width;
///
/// // Plenty of content width: the stored 280 passes through unclamped.
/// assert_eq!(effective_dock_width(280.0, 1200.0), 280.0);
/// // Narrow content: further limited below the stored width.
/// assert_eq!(effective_dock_width(280.0, 700.0), 240.0);
/// ```
#[must_use]
pub fn effective_dock_width(stored_or_live: f32, content_width: f32) -> f32 {
    // Not `.clamp(240.0, ..)`: `content_width - HOST_CONTENT_FLOOR` can be
    // below 240 (a very narrow window), which would make `f32::clamp`'s
    // `min > max` and panic. `.min(480.0).max(240.0)` is the deliberately
    // panic-safe equivalent (data-model.md §4).
    #[allow(clippy::manual_clamp)]
    let max_eff = (content_width - HOST_CONTENT_FLOOR).min(480.0).max(240.0);
    stored_or_live.clamp(240.0, max_eff)
}

/// Waveform overview/detail heights (data-model.md §5, contract D8):
/// `(max(64, round(0.08 H)), max(120, round(0.22 H)))`. No upper bound.
///
/// # Examples
///
/// ```
/// use modplayer_ui::layout::waveform_heights;
///
/// assert_eq!(waveform_heights(800.0), (64.0, 176.0));
/// assert_eq!(waveform_heights(100.0), (64.0, 120.0));
/// ```
#[must_use]
pub fn waveform_heights(h: f32) -> (f32, f32) {
    let overview = (WAVEFORM_OVERVIEW_PROPORTION * h)
        .round()
        .max(WAVEFORM_OVERVIEW_MIN);
    let detail = (WAVEFORM_DETAIL_PROPORTION * h)
        .round()
        .max(WAVEFORM_DETAIL_MIN);
    (overview, detail)
}

/// Whether an outer window size effectively fills its monitor, i.e. the
/// window is maximized ("zoomed") even though the platform didn't say so
/// (research R2 addendum): egui-winit only queries `is_maximized()` at
/// viewport creation on macOS (egui#3494), so a window zoomed after launch
/// still reports `maximized == Some(false)`. A zoomed macOS window spans
/// the monitor's visible frame — the full width with the menu bar and a
/// bottom Dock taken off its height, or the full height minus the menu bar
/// with a side Dock taken off its width — so it counts as filling the
/// monitor when one axis spans edge to edge (height allowing
/// [`MENU_BAR_ALLOWANCE`]) and the other covers at least
/// [`FILL_PROPORTION`] of the monitor.
///
/// # Examples
///
/// ```
/// use egui::vec2;
/// use modplayer_ui::layout::fills_monitor;
///
/// let monitor = vec2(1680.0, 1050.0);
/// assert!(fills_monitor(vec2(1680.0, 1025.0), monitor)); // zoomed, Dock hidden
/// assert!(fills_monitor(vec2(1680.0, 955.0), monitor)); // zoomed, Dock at bottom
/// assert!(fills_monitor(vec2(1600.0, 1025.0), monitor)); // zoomed, Dock at side
/// assert!(!fills_monitor(vec2(1400.0, 928.0), monitor)); // a restored size
/// ```
#[must_use]
pub fn fills_monitor(outer: Vec2, monitor: Vec2) -> bool {
    let spans_width = outer.x >= monitor.x - 2.0;
    let spans_height = outer.y >= monitor.y - MENU_BAR_ALLOWANCE;
    (spans_width && outer.y >= FILL_PROPORTION * monitor.y)
        || (spans_height && outer.x >= FILL_PROPORTION * monitor.x)
}

/// Height (points) [`fills_monitor`] allows for the menu bar when deciding
/// that a window spans the monitor vertically.
pub const MENU_BAR_ALLOWANCE: f32 = 40.0;

/// Share of the monitor the non-spanning axis must cover for
/// [`fills_monitor`] to treat the window as maximized.
pub const FILL_PROPORTION: f32 = 0.85;

/// Debounced, maximized/fullscreen-ignoring tracker for the main window's
/// restored inner size (data-model.md §2, research R2): observes the
/// live viewport size every frame, and yields a size to persist at most
/// once per [`SAVE_DEBOUNCE`] once it has settled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowSizeTracker {
    last_saved: Vec2,
    pending: Option<Vec2>,
    changed_at: Option<Instant>,
    last_write: Option<Instant>,
}

impl WindowSizeTracker {
    /// Start a tracker whose baseline is the size the window was just
    /// restored to (or the default, on first launch) — nothing pending.
    ///
    /// # Examples
    ///
    /// ```
    /// use egui::vec2;
    /// use modplayer_ui::layout::WindowSizeTracker;
    ///
    /// let tracker = WindowSizeTracker::new(vec2(1200.0, 820.0));
    /// assert_eq!(tracker.last_saved(), vec2(1200.0, 820.0));
    /// ```
    #[must_use]
    pub fn new(initial: Vec2) -> Self {
        Self {
            last_saved: initial,
            pending: None,
            changed_at: None,
            last_write: None,
        }
    }

    /// The size last handed back by [`Self::poll`] or [`Self::flush`] (or
    /// the restored size the tracker was constructed with, if neither has
    /// fired yet).
    #[must_use]
    pub fn last_saved(&self) -> Vec2 {
        self.last_saved
    }

    /// Observe this frame's live viewport `size`. While
    /// `maximized_or_fullscreen` the size is ignored and any pending size
    /// is discarded (FR-003: never persist a maximized/fullscreen geometry,
    /// nor the zoom animation's in-between frames, as the restored size).
    /// Otherwise, if `size`
    /// differs from the last-saved size by more than 0.5 pt on either
    /// axis, and differs from whatever is already pending, it becomes the
    /// new pending size and resets the debounce clock.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    ///
    /// use egui::vec2;
    /// use modplayer_ui::layout::WindowSizeTracker;
    ///
    /// let mut tracker = WindowSizeTracker::new(vec2(1200.0, 820.0));
    /// tracker.observe(vec2(1300.0, 820.0), false, Instant::now());
    /// assert!(tracker.next_deadline().is_some());
    /// ```
    pub fn observe(&mut self, size: Vec2, maximized_or_fullscreen: bool, now: Instant) {
        if maximized_or_fullscreen {
            // Drop anything still settling: those are the frames of the
            // zoom/fullscreen animation itself, not a size the user chose.
            self.pending = None;
            self.changed_at = None;
            return;
        }
        let differs_from_saved =
            (size.x - self.last_saved.x).abs() > 0.5 || (size.y - self.last_saved.y).abs() > 0.5;
        if !differs_from_saved {
            return;
        }
        if self.pending != Some(size) {
            self.pending = Some(size);
            self.changed_at = Some(now);
        }
    }

    /// Yield the pending size once it has settled: `now - changed_at >=
    /// SAVE_DEBOUNCE`, and either no size has ever been written or `now -
    /// last_write >= SAVE_DEBOUNCE`. Updates `last_saved`/`last_write` and
    /// clears `pending` when it fires.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    ///
    /// use egui::vec2;
    /// use modplayer_ui::layout::{SAVE_DEBOUNCE, WindowSizeTracker};
    ///
    /// let mut tracker = WindowSizeTracker::new(vec2(1200.0, 820.0));
    /// let t0 = Instant::now();
    /// tracker.observe(vec2(1300.0, 820.0), false, t0);
    /// assert_eq!(tracker.poll(t0), None, "debounce has not elapsed yet");
    /// assert_eq!(tracker.poll(t0 + SAVE_DEBOUNCE), Some(vec2(1300.0, 820.0)));
    /// ```
    pub fn poll(&mut self, now: Instant) -> Option<Vec2> {
        let pending = self.pending?;
        let changed_at = self.changed_at?;
        if now.duration_since(changed_at) < SAVE_DEBOUNCE {
            return None;
        }
        if let Some(last_write) = self.last_write
            && now.duration_since(last_write) < SAVE_DEBOUNCE
        {
            return None;
        }
        self.last_saved = pending;
        self.last_write = Some(now);
        self.pending = None;
        self.changed_at = None;
        Some(pending)
    }

    /// The earliest instant at which [`Self::poll`] might next yield a
    /// size — for `ctx.request_repaint_after(..)`, so a settled resize is
    /// still saved even if no further input arrives to trigger a repaint.
    /// `None` when nothing is pending.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    ///
    /// use egui::vec2;
    /// use modplayer_ui::layout::WindowSizeTracker;
    ///
    /// let tracker = WindowSizeTracker::new(vec2(1200.0, 820.0));
    /// assert_eq!(tracker.next_deadline(), None);
    /// ```
    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        let changed_at = self.changed_at?;
        let mut deadline = changed_at + SAVE_DEBOUNCE;
        if let Some(last_write) = self.last_write {
            deadline = deadline.max(last_write + SAVE_DEBOUNCE);
        }
        Some(deadline)
    }

    /// Unconditionally yield the pending size (if any), ignoring the
    /// debounce — `App::on_exit` calls this so a resize that never
    /// settled is still persisted (FR-003).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    ///
    /// use egui::vec2;
    /// use modplayer_ui::layout::WindowSizeTracker;
    ///
    /// let mut tracker = WindowSizeTracker::new(vec2(1200.0, 820.0));
    /// tracker.observe(vec2(1300.0, 820.0), false, Instant::now());
    /// assert_eq!(tracker.flush(), Some(vec2(1300.0, 820.0)));
    /// assert_eq!(tracker.flush(), None, "nothing left pending");
    /// ```
    pub fn flush(&mut self) -> Option<Vec2> {
        let pending = self.pending.take()?;
        self.changed_at = None;
        self.last_saved = pending;
        Some(pending)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    // ---------------------------------------------------------------
    // D10.1: dock_presentation truth table, including the 1023.9/1024.0
    // boundary and the zero-panels case.
    // ---------------------------------------------------------------

    #[test]
    fn fills_monitor_detects_zoom_but_not_restored_sizes() {
        let monitor = Vec2::new(1680.0, 1050.0);
        assert!(fills_monitor(Vec2::new(1680.0, 1025.0), monitor));
        assert!(fills_monitor(monitor, monitor));
        assert!(!fills_monitor(Vec2::new(1400.0, 928.0), monitor));
        assert!(!fills_monitor(Vec2::new(1680.0, 700.0), monitor));
        assert!(!fills_monitor(Vec2::new(1000.0, 1025.0), monitor));
        assert!(!fills_monitor(Vec2::new(960.0, 668.0), monitor));
    }

    #[test]
    fn dock_presentation_truth_table() {
        assert_eq!(dock_presentation(1200.0, 0, false), DockPresentation::None);
        assert_eq!(dock_presentation(1200.0, 0, true), DockPresentation::None);
        assert_eq!(
            dock_presentation(1024.0, 1, false),
            DockPresentation::Docked
        );
        assert_eq!(
            dock_presentation(1023.9, 1, false),
            DockPresentation::Hidden
        );
        assert_eq!(
            dock_presentation(1023.9, 1, true),
            DockPresentation::Overlay
        );
        // At/above threshold, Docked wins even if overlay_open is (stale)
        // true — the caller is expected to force-clear it, but the pure
        // function itself must not surface Overlay above the threshold.
        assert_eq!(dock_presentation(1024.0, 1, true), DockPresentation::Docked);
    }

    // ---------------------------------------------------------------
    // D10.2: effective_dock_width proptest — always in [240, 480], never
    // above max(240, content - 560), monotone in the stored width.
    // ---------------------------------------------------------------

    proptest! {
        #[test]
        fn effective_dock_width_stays_in_bounds(
            stored in 0.0f32..2_000.0,
            content in 0.0f32..4_000.0,
        ) {
            let effective = effective_dock_width(stored, content);
            prop_assert!(effective >= 240.0);
            prop_assert!(effective <= 480.0);
            prop_assert!(effective <= (content - HOST_CONTENT_FLOOR).max(240.0));
        }

        #[test]
        fn effective_dock_width_is_monotone_in_stored_width(
            content in 0.0f32..4_000.0,
            a in 0.0f32..2_000.0,
            b in 0.0f32..2_000.0,
        ) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            prop_assert!(effective_dock_width(lo, content) <= effective_dock_width(hi, content));
        }
    }

    // ---------------------------------------------------------------
    // D10.3: waveform_heights formula and monotonicity.
    // ---------------------------------------------------------------

    proptest! {
        #[test]
        fn waveform_heights_matches_formula(h in 0.0f32..10_000.0) {
            let (overview, detail) = waveform_heights(h);
            prop_assert_eq!(overview, (0.08 * h).round().max(64.0));
            prop_assert_eq!(detail, (0.22 * h).round().max(120.0));
        }

        #[test]
        fn waveform_heights_is_monotone(a in 0.0f32..10_000.0, b in 0.0f32..10_000.0) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let (lo_overview, lo_detail) = waveform_heights(lo);
            let (hi_overview, hi_detail) = waveform_heights(hi);
            prop_assert!(lo_overview <= hi_overview);
            prop_assert!(lo_detail <= hi_detail);
        }
    }

    // ---------------------------------------------------------------
    // D10.4: WindowSizeTracker debounce/maximized-ignore/exit-flush.
    // ---------------------------------------------------------------

    #[test]
    fn tracker_ignores_maximized_or_fullscreen() {
        let mut tracker = WindowSizeTracker::new(Vec2::new(1200.0, 820.0));
        let now = Instant::now();
        tracker.observe(Vec2::new(1900.0, 1200.0), true, now);
        assert_eq!(tracker.next_deadline(), None);
        assert_eq!(tracker.poll(now + SAVE_DEBOUNCE), None);
    }

    #[test]
    fn tracker_discards_zoom_animation_frames_once_maximized() {
        let mut tracker = WindowSizeTracker::new(Vec2::new(1400.0, 900.0));
        let now = Instant::now();
        tracker.observe(Vec2::new(1550.0, 960.0), false, now);
        tracker.observe(Vec2::new(1680.0, 997.0), true, now);
        assert_eq!(tracker.next_deadline(), None);
        assert_eq!(tracker.poll(now + SAVE_DEBOUNCE), None);
        assert_eq!(tracker.flush(), None);
    }

    #[test]
    fn tracker_ignores_sub_pixel_noise() {
        let mut tracker = WindowSizeTracker::new(Vec2::new(1200.0, 820.0));
        let now = Instant::now();
        tracker.observe(Vec2::new(1200.2, 820.2), false, now);
        assert_eq!(tracker.next_deadline(), None);
    }

    #[test]
    fn tracker_debounces_until_settled() {
        let mut tracker = WindowSizeTracker::new(Vec2::new(1200.0, 820.0));
        let t0 = Instant::now();
        tracker.observe(Vec2::new(1300.0, 820.0), false, t0);
        // Not yet settled.
        assert_eq!(tracker.poll(t0 + Duration::from_millis(100)), None);
        // A further resize before settling resets the debounce clock.
        let t1 = t0 + Duration::from_millis(200);
        tracker.observe(Vec2::new(1400.0, 820.0), false, t1);
        assert_eq!(tracker.poll(t1 + Duration::from_millis(400)), None);
        // Now it settles.
        let settled_at = t1 + SAVE_DEBOUNCE;
        assert_eq!(tracker.poll(settled_at), Some(Vec2::new(1400.0, 820.0)));
        assert_eq!(tracker.last_saved(), Vec2::new(1400.0, 820.0));

        // A second, later resize goes through its own independent debounce.
        let t2 = settled_at + Duration::from_millis(10);
        tracker.observe(Vec2::new(1450.0, 820.0), false, t2);
        assert_eq!(tracker.poll(t2 + Duration::from_millis(100)), None);
        assert_eq!(
            tracker.poll(t2 + SAVE_DEBOUNCE),
            Some(Vec2::new(1450.0, 820.0))
        );
    }

    #[test]
    fn tracker_flush_is_unconditional() {
        let mut tracker = WindowSizeTracker::new(Vec2::new(1200.0, 820.0));
        assert_eq!(tracker.flush(), None);
        let now = Instant::now();
        tracker.observe(Vec2::new(1300.0, 820.0), false, now);
        // Flush fires immediately, no debounce wait.
        assert_eq!(tracker.flush(), Some(Vec2::new(1300.0, 820.0)));
        assert_eq!(tracker.flush(), None);
    }
}
