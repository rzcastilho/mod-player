// SPDX-License-Identifier: MIT OR Apache-2.0

//! `WaveformState` (drag preview, detail window, last-track tracking) and
//! `DetailWindow` (zoom/pan/follow, pure and unit-tested without egui)
//! (005-now-playing-waveform, data-model.md §5.1-5.2).
//!
//! `DetailWindow`'s pure zoom/pan/follow methods (`initial`, `zoom_about`,
//! `zoom_step`, `reset`, `pan`, `follow_playhead`, `recenter`,
//! `suspend_follow_if_outside`, `contains`) land here in US2 (T041); they
//! take `len`/`rate` explicitly wherever clamping needs them since the
//! struct itself only carries the window's own bounds and follow flag
//! (data-model.md §5.1 names the shape, not literal Rust signatures).

use modplayer_audio_source::TrackId;
use modplayer_core::markers::MarkerId;

/// The floor on `width_frames` (contracts/ui-waveform.md §3, FR-011): the
/// most zoomed-in the detail view ever gets.
pub const DETAIL_MIN_WINDOW_MS: u64 = 200;
/// The default width a fresh `DetailWindow` opens at (data-model.md §5.1,
/// contracts/ui-waveform.md's M5/M6 scenarios).
pub const DETAIL_INITIAL_WINDOW_SECONDS: u64 = 30;

/// A zoomed time window into the waveform (data-model.md §5.1): `[200 ms,
/// len]` wide, clamped so `start_frame + width_frames <= len`, following
/// the playhead by default. Every mutator is pure (returns a new value) so
/// it is unit-testable without egui and safely called every frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetailWindow {
    pub start_frame: u64,
    pub width_frames: u64,
    pub follow: bool,
}

impl DetailWindow {
    /// The narrowest width allowed at `rate` (200 ms, at least one frame).
    fn min_width_frames(rate: u32) -> u64 {
        (DETAIL_MIN_WINDOW_MS * u64::from(rate.max(1)) / 1000).max(1)
    }

    /// The default (30 s) width, capped at the whole track for a shorter
    /// one (FR-011's ceiling).
    fn default_width_frames(len: u64, rate: u32) -> u64 {
        (DETAIL_INITIAL_WINDOW_SECONDS * u64::from(rate.max(1)))
            .min(len.max(1))
            .max(1)
    }

    /// `start_frame` clamped so the window of `width` never runs past
    /// `len` (FR-011/012's end clamp), for a track at least `width` long;
    /// for a track shorter than `width` the only valid start is `0`.
    fn clamped_start(start: u64, width: u64, len: u64) -> u64 {
        let max_start = len.saturating_sub(width);
        start.min(max_start)
    }

    /// A fresh, 30-second (or whole-track, if shorter) window centred on
    /// `playhead`, following (data-model.md §5.1). The entry point for a
    /// track that has never shown a detail view before; a track change
    /// with a pre-existing window instead calls [`Self::recenter`] to keep
    /// the user's chosen width (contracts/ui-waveform.md §5).
    pub fn initial(playhead: u64, len: u64, rate: u32) -> Self {
        let width = Self::default_width_frames(len, rate);
        let start = Self::clamped_start(playhead.saturating_sub(width / 2), width, len);
        Self {
            start_frame: start,
            width_frames: width,
            follow: true,
        }
    }

    /// Whether `frame` currently falls inside this window (inclusive of
    /// both edges).
    pub fn contains(&self, frame: u64) -> bool {
        frame >= self.start_frame && frame <= self.start_frame + self.width_frames
    }

    /// Zoom so that `factor` more track-time fits on screen per unit width
    /// (`factor > 1` zooms in — narrower window — `0 < factor < 1` zooms
    /// out), keeping `anchor_frame` at the same fractional position within
    /// the window (contracts/ui-waveform.md §2: the pointer position on
    /// the detail view, the playhead on the overview). `follow` is carried
    /// over unchanged; the caller applies
    /// [`Self::suspend_follow_if_outside`] afterwards (contracts/
    /// ui-waveform.md §5).
    pub fn zoom_about(&self, anchor_frame: u64, factor: f64, len: u64, rate: u32) -> Self {
        let min_width = Self::min_width_frames(rate);
        let max_width = len.max(min_width);
        let factor = if factor.is_finite() && factor > 0.0 {
            factor
        } else {
            1.0
        };
        let new_width = ((self.width_frames.max(1) as f64) / factor)
            .round()
            .clamp(min_width as f64, max_width as f64) as u64;

        let relative = if self.width_frames == 0 {
            0.5
        } else {
            (anchor_frame.saturating_sub(self.start_frame)) as f64 / self.width_frames as f64
        };
        let new_start_signed = anchor_frame as f64 - relative * new_width as f64;
        let new_start =
            Self::clamped_start(new_start_signed.max(0.0).round() as u64, new_width, len);

        Self {
            start_frame: new_start,
            width_frames: new_width,
            follow: self.follow,
        }
    }

    /// The keyboard zoom row (contracts/ui-waveform.md §3): `+`/`=` (`zoom_in
    /// = true`) halves the window, `-` (`zoom_in = false`) doubles it,
    /// both anchored on `playhead` (never the pointer, since there is
    /// none).
    pub fn zoom_step(&self, playhead: u64, zoom_in: bool, len: u64, rate: u32) -> Self {
        let factor = if zoom_in { 2.0 } else { 0.5 };
        self.zoom_about(playhead, factor, len, rate)
    }

    /// `0` (contracts/ui-waveform.md §3): back to the default width
    /// centred on the window's current middle, following again.
    pub fn reset(&self, len: u64, rate: u32) -> Self {
        let width = Self::default_width_frames(len, rate);
        let center = self.start_frame + self.width_frames / 2;
        let start = Self::clamped_start(center.saturating_sub(width / 2), width, len);
        Self {
            start_frame: start,
            width_frames: width,
            follow: true,
        }
    }

    /// Pan by `delta_frames` (positive = forward in time), clamped at both
    /// track ends (FR-012). `follow` is carried over unchanged; the caller
    /// applies [`Self::suspend_follow_if_outside`] afterwards.
    pub fn pan(&self, delta_frames: i64, len: u64) -> Self {
        let start = if delta_frames >= 0 {
            self.start_frame.saturating_add(delta_frames.unsigned_abs())
        } else {
            self.start_frame.saturating_sub(delta_frames.unsigned_abs())
        };
        Self {
            start_frame: Self::clamped_start(start, self.width_frames, len),
            follow: self.follow,
            ..*self
        }
    }

    /// Applied once per frame while `playing` (contracts/ui-waveform.md
    /// §5): a no-op unless `follow` is set and the playhead has left the
    /// window, in which case the window pages forward — jumps so the
    /// playhead lands back at its left edge — rather than scrolling
    /// smoothly.
    pub fn follow_playhead(&self, playhead: u64, playing: bool, len: u64) -> Self {
        if !playing || !self.follow || self.contains(playhead) {
            return *self;
        }
        Self {
            start_frame: Self::clamped_start(playhead, self.width_frames, len),
            follow: true,
            ..*self
        }
    }

    /// A seek landed outside the current window (contracts/ui-waveform.md
    /// §5): keep the width, centre on `frame`, and resume following.
    pub fn recenter(&self, frame: u64, len: u64) -> Self {
        let start = Self::clamped_start(
            frame.saturating_sub(self.width_frames / 2),
            self.width_frames,
            len,
        );
        Self {
            start_frame: start,
            follow: true,
            ..*self
        }
    }

    /// Called after a user pan/zoom this frame (contracts/ui-waveform.md
    /// §5): following stops only if that action left the playhead outside
    /// the window (e.g. panning away, or a pointer-anchored zoom whose
    /// anchor wasn't the playhead) — a zoom centred on the playhead stays
    /// within it and keeps following.
    pub fn suspend_follow_if_outside(&self, playhead: u64) -> Self {
        if self.follow && !self.contains(playhead) {
            Self {
                follow: false,
                ..*self
            }
        } else {
            *self
        }
    }

    /// One convergence step of the marker-drag zoom-assist (006, research
    /// R14, contracts/ui-markers.md §5): moves the current width 80% of
    /// the way toward `target_width` (clamped to `[200 ms, len]`, like
    /// every other zoom) and recentres on `live` — the marker's current
    /// drag position — every frame, rather than snapping straight to it.
    /// Called once per frame while a marker drag is in progress, this
    /// converges within about ten frames (SC-003's <= 5 ms landing
    /// margin), and never maps the pointer's absolute position through a
    /// window that is itself recentring on it (design note 11). `follow`
    /// is left suspended: a drag is not the playhead moving.
    pub fn zoom_assist(&self, live: u64, target_width: u64, len: u64, rate: u32) -> Self {
        let min_width = Self::min_width_frames(rate);
        let max_width = len.max(min_width);
        let target = target_width.clamp(min_width, max_width);
        let current = self.width_frames.max(1) as f64;
        let new_width = (current + (target as f64 - current) * 0.8)
            .round()
            .clamp(min_width as f64, max_width as f64) as u64;
        let start = Self::clamped_start(live.saturating_sub(new_width / 2), new_width, len);
        Self {
            start_frame: start,
            width_frames: new_width,
            follow: false,
        }
    }
}

/// Which waveform widget a live drag preview originated on (data-model.md
/// §5.2). `Detail` is unreachable until US2 wires the detail widget
/// (T045); the variant exists now so `DragPreview` doesn't need revisiting
/// then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOrigin {
    Overview,
    Detail,
}

/// A live (uncommitted) seek target while the pointer is down on a
/// waveform (data-model.md §5.2): shown as the playhead/label preview,
/// cleared on release (commit) or `Esc` (cancel) — never itself a seek.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragPreview {
    pub target_frame: u64,
    pub origin: DragOrigin,
}

/// A live (uncommitted) marker drag (006, data-model.md §3, research
/// R14): `live` accumulates the pointer's relative delta in detail-space
/// frames every frame, never the pointer's absolute position mapped
/// through a recentring window (design note 11). `origin_position` and
/// `origin_detail` are restored verbatim on `Esc`; `live` is committed
/// via `move_marker` on release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerDrag {
    pub marker: MarkerId,
    pub origin_position: u64,
    pub live: u64,
    pub origin_detail: DetailWindow,
}

/// Session waveform state, owned by `App` for as long as it runs
/// (data-model.md §5.2, extended by 006 data-model.md §3).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WaveformState {
    /// `None` until the detail widget exists (US2); the overview alone
    /// never populates it.
    pub detail: Option<DetailWindow>,
    pub drag: Option<DragPreview>,
    /// The track `detail`/`drag` were last computed against, so a track
    /// change can re-centre the detail window (US2) rather than keep
    /// showing stale bounds. Also used this phase to know when a drag
    /// preview must be dropped because its track went away.
    pub last_track: Option<TrackId>,
    /// A marker glyph/row drag in progress (006, contracts/ui-markers.md
    /// §3), distinct from `drag` (005's waveform-rect seek preview).
    pub marker_drag: Option<MarkerDrag>,
    /// Mirror of egui's own focus, for the marker keyboard table (arrow
    /// nudge, Delete, F2, C, Esc — contracts/ui-markers.md §3).
    pub focused_marker: Option<MarkerId>,
    /// An in-progress inline rename's draft text, keyed by the marker
    /// being renamed (contracts/ui-markers.md §4).
    pub rename: Option<(MarkerId, String)>,
    /// "Clear N markers?" two-step confirmation state (contracts/
    /// ui-markers.md §4).
    pub clear_confirm: bool,
    /// This view's own text-field widget ids, rebuilt every frame — the
    /// view-level shortcut guard checks egui's focus against this list
    /// rather than `wants_keyboard_input` (research R17).
    pub text_field_ids: Vec<egui::Id>,
    /// The current inline refusal reason for a view-level marker/loop
    /// shortcut (contracts/ui-markers.md §2, §4: shown as `markers-status`
    /// under the panel header), a Fluent key or `None`. Cleared on the
    /// next successful `I`/`O`/`L`/`M`/`1-8`/`Shift+1-8` action.
    pub marker_status: Option<&'static str>,
}

impl WaveformState {
    /// The live preview frame, if a drag from `origin` is in progress —
    /// what the playhead/elapsed/remaining labels show instead of the
    /// real position while dragging (data-model.md §5.4).
    pub fn preview_frame(&self) -> Option<u64> {
        self.drag.map(|drag| drag.target_frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_state_has_no_drag_preview() {
        let state = WaveformState::default();
        assert_eq!(state.preview_frame(), None);
    }

    #[test]
    fn preview_frame_reflects_the_active_drag() {
        let state = WaveformState {
            drag: Some(DragPreview {
                target_frame: 1_234,
                origin: DragOrigin::Overview,
            }),
            ..WaveformState::default()
        };
        assert_eq!(state.preview_frame(), Some(1_234));
    }

    // T039 (US2, contracts/ui-waveform.md §5, data-model.md §5.1):
    // `DetailWindow`'s pure zoom/pan/follow methods, unit-tested without
    // egui.

    const RATE: u32 = 44_100;

    fn seconds(s: u64) -> u64 {
        s * u64::from(RATE)
    }

    #[test]
    fn detail_window_initial_is_30s_centered() {
        let len = seconds(300);
        let playhead = seconds(100);
        let window = DetailWindow::initial(playhead, len, RATE);

        assert_eq!(window.width_frames, seconds(30));
        assert_eq!(window.start_frame, playhead - seconds(15));
        assert!(window.follow);
        assert!(window.contains(playhead));
    }

    #[test]
    fn detail_window_zoom_floor_200ms() {
        let len = seconds(300);
        let mut window = DetailWindow::initial(seconds(100), len, RATE);
        // Zoom in hard, far past any single step's floor.
        for _ in 0..40 {
            window = window.zoom_step(seconds(100), true, len, RATE);
        }
        let min_width = (200 * u64::from(RATE)) / 1000;
        assert_eq!(window.width_frames, min_width);
    }

    #[test]
    fn detail_window_zoom_ceiling_whole_track() {
        let len = seconds(300);
        let mut window = DetailWindow::initial(seconds(100), len, RATE);
        for _ in 0..40 {
            window = window.zoom_step(seconds(100), false, len, RATE);
        }
        assert_eq!(window.width_frames, len);
        assert_eq!(window.start_frame, 0);
    }

    #[test]
    fn detail_window_follow_pages_forward() {
        let len = seconds(300);
        let window = DetailWindow::initial(seconds(10), len, RATE);
        // Advance the playhead past the window's right edge.
        let playhead = window.start_frame + window.width_frames + seconds(1);
        let paged = window.follow_playhead(playhead, true, len);

        assert_eq!(paged.start_frame, playhead, "must page, not scroll");
        assert_eq!(paged.width_frames, window.width_frames);
        assert!(paged.follow);
    }

    #[test]
    fn detail_window_clamps_at_track_ends() {
        let len = seconds(300);
        let window = DetailWindow::initial(seconds(1), len, RATE);

        let panned_before_start = window.pan(-1_000_000_000, len);
        assert_eq!(panned_before_start.start_frame, 0);

        let panned_past_end = window.pan(1_000_000_000, len);
        assert_eq!(
            panned_past_end.start_frame,
            len - panned_past_end.width_frames
        );
    }

    #[test]
    fn detail_window_recenters_on_outside_seek() {
        let len = seconds(300);
        let window = DetailWindow::initial(seconds(10), len, RATE).suspend_follow_if_outside(0);
        let far_target = seconds(250);
        assert!(!window.contains(far_target), "sanity: target is outside");

        let recentred = window.recenter(far_target, len);
        assert!(recentred.follow);
        assert!(recentred.contains(far_target));
        assert_eq!(recentred.width_frames, window.width_frames);
    }

    #[test]
    fn detail_window_suspends_follow_on_pan_or_zoom() {
        let len = seconds(300);
        let playhead = seconds(100);
        let window = DetailWindow::initial(playhead, len, RATE);
        assert!(window.follow);

        // Pan far enough away that the playhead falls outside the window.
        let panned = window.pan(seconds(60) as i64, len);
        let after_pan = panned.suspend_follow_if_outside(playhead);
        assert!(!after_pan.follow);

        // A zoom anchored on the playhead itself keeps it in view, so
        // following is not suspended.
        let zoomed_on_playhead = window.zoom_about(playhead, 2.0, len, RATE);
        let after_zoom = zoomed_on_playhead.suspend_follow_if_outside(playhead);
        assert!(after_zoom.follow);

        // A zoom anchored elsewhere can push the playhead out of view.
        let anchor = window.start_frame;
        let zoomed_away = window.zoom_about(anchor, 100.0, len, RATE);
        let after_zoom_away = zoomed_away.suspend_follow_if_outside(playhead);
        assert!(!after_zoom_away.follow);
    }

    #[test]
    fn detail_window_reset_reenables_follow() {
        let len = seconds(300);
        let playhead = seconds(100);
        let window = DetailWindow::initial(playhead, len, RATE);
        let narrowed = window
            .zoom_about(playhead, 1000.0, len, RATE)
            .suspend_follow_if_outside(playhead);
        // Zooming in hard while anchored on the playhead should not itself
        // suspend follow, but simulate a subsequent pan that does, to
        // exercise `reset` un-suspending it.
        let panned = narrowed.pan(seconds(5) as i64, len);
        let suspended = panned.suspend_follow_if_outside(playhead);

        let reset = suspended.reset(len, RATE);
        assert!(reset.follow, "reset must re-enable follow");
        assert_eq!(reset.width_frames, seconds(30));
    }

    // T081 (006 US3, research R14, contracts/ui-markers.md §5): the
    // marker-drag zoom-assist converges toward its target width within
    // ~10 frames (SC-003's <= 5ms landing margin) and stays centred on
    // `live`, with `follow` left suspended throughout.
    #[test]
    fn detail_window_zoom_assist_converges_to_target() {
        let len = seconds(300);
        let live = seconds(100);
        let mut window = DetailWindow::initial(live, len, RATE);
        let target_width = 200 * u64::from(RATE) / 1000; // 200ms, the floor
        assert_ne!(
            window.width_frames, target_width,
            "sanity: starts far from the target"
        );

        for _ in 0..10 {
            window = window.zoom_assist(live, target_width, len, RATE);
            assert!(!window.follow, "zoom_assist always suspends follow");
            assert!(
                window.contains(live),
                "must stay centred on the live drag position every step"
            );
        }
        assert_eq!(
            window.width_frames, target_width,
            "must converge to the target width within 10 steps"
        );
    }

    /// `zoom_assist` never overshoots past `[200ms, len]`, same floor/
    /// ceiling as every other zoom.
    #[test]
    fn detail_window_zoom_assist_respects_floor_and_ceiling() {
        let len = seconds(300);
        let live = seconds(50);
        let window = DetailWindow::initial(live, len, RATE);

        let clamped_low = window.zoom_assist(live, 0, len, RATE);
        let min_width = (200 * u64::from(RATE)) / 1000;
        assert!(clamped_low.width_frames >= min_width);

        let clamped_high = window.zoom_assist(live, len * 10, len, RATE);
        assert!(clamped_high.width_frames <= len);
    }
}
