// SPDX-License-Identifier: MIT OR Apache-2.0

//! Waveform pointer and keyboard input: click/drag-to-seek, zoom, and pan
//! (005-now-playing-waveform, contracts/ui-waveform.md §2-3).
//!
//! US1 implemented the seek subset: click, drag preview (committed on
//! release, cancelled on `Esc`), and the keyboard seek rows (`←`/`→`,
//! `Shift+←`/`Shift+→`, `Home`/`End`). US2 (T042) adds zoom/pan: pointer
//! wheel/pinch (`zoom_delta`, anchored on the pointer for the detail view
//! or the playhead for the overview), horizontal scroll/`Shift`+wheel to
//! pan the detail, and the keyboard's `+`/`=`/`-`/`0`/`Alt+←`/`Alt+→`/
//! `Alt+Shift+←`/`Alt+Shift+→` rows. Zoom/pan events carry no `len`/`rate`
//! — the caller (`waveform::mod`/`now_playing.rs`) applies them to its own
//! `DetailWindow` via `zoom_about`/`zoom_step`/`pan`/`reset`, which is
//! where that clamping context already lives.

use egui::{Key, Modifiers, Response, Ui};

use super::coords::TimeSpace;
use crate::actions::{self, Claim};

/// `Left`/`Right` seek step without a modifier (contracts/ui-waveform.md
/// §3).
const SEEK_STEP_SECONDS: f64 = 5.0;
/// `Shift+Left`/`Shift+Right` seek step.
const FINE_SEEK_STEP_SECONDS: f64 = 0.5;
/// `Alt+Left`/`Alt+Right` pan step, as a fraction of the detail window's
/// current width (contracts/ui-waveform.md §3).
const PAN_STEP_FRACTION: f64 = 0.1;
/// `Alt+Shift+Left`/`Alt+Shift+Right` pan step: a full window's width.
const PAN_PAGE_FRACTION: f64 = 1.0;

/// What one frame of pointer/keyboard input on a waveform widget implies
/// (contracts/ui-waveform.md §2-3). The caller (`waveform::mod`) applies
/// `Preview`/`CancelDrag` to its `WaveformState`, `Commit` to
/// `PlaybackController::seek_frames`, and `Zoom`/`Pan`/`ZoomStep`/
/// `PanStep`/`Reset` to its `DetailWindow` — kept out of this module so it
/// stays free of both the controller and `DetailWindow` types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WaveformEvent {
    /// A live drag preview target — no seek yet.
    Preview(u64),
    /// Seek to this frame now (click release, drag release, or a keyboard
    /// row).
    Commit(u64),
    /// `Esc` while dragging: drop the preview, no seek.
    CancelDrag,
    /// Pointer wheel/pinch: `DetailWindow::zoom_about(anchor_frame,
    /// factor, ..)`. `factor` is egui's own `zoom_delta` convention
    /// (`> 1.0` zooms in); `anchor_frame` is the pointer's frame in the
    /// detail view, or the playhead on the overview.
    Zoom { anchor_frame: u64, factor: f64 },
    /// Horizontal scroll / `Shift`+wheel on the detail view:
    /// `DetailWindow::pan(delta_frames, ..)`.
    Pan { delta_frames: i64 },
    /// `+`/`=` (`zoom_in = true`) or `-` (`zoom_in = false`):
    /// `DetailWindow::zoom_step(playhead, zoom_in, ..)`.
    ZoomStep { zoom_in: bool },
    /// `Alt+←`/`Alt+→`/`Alt+Shift+←`/`Alt+Shift+→`: a signed fraction of
    /// the detail window's current width to pass to `DetailWindow::pan`
    /// (negative = backward in time).
    PanStep { fraction: f64 },
    /// `0`: `DetailWindow::reset(len, rate)`.
    Reset,
}

/// Resolve this frame's pointer/keyboard input on a waveform widget into at
/// most one [`WaveformEvent`] (contracts/ui-waveform.md §2-3). `previewing`
/// is whether a drag preview is currently active (`state.drag.is_some()`),
/// needed to scope `Esc` to "cancel the drag in progress" rather than any
/// unrelated `Esc` press. `playhead` is the frame keyboard steps (and the
/// overview's zoom anchor) are relative to. `is_detail` distinguishes the
/// detail view from the overview for the pointer rows that behave
/// differently on each (contracts/ui-waveform.md §2): only the detail
/// view's own `space` maps a pointer position to a meaningful zoom anchor
/// or pans by pixels, so pointer-driven panning is scoped to it; the
/// overview still zooms (anchored on the playhead) and both widgets share
/// every keyboard row.
pub fn handle(
    ui: &Ui,
    response: &Response,
    space: &TimeSpace,
    playhead: u64,
    previewing: bool,
    is_detail: bool,
) -> Option<WaveformEvent> {
    // Register this frame's claim (007, contracts/ui-actions.md §2) so
    // next frame's dispatcher leaves this widget's own rows alone while
    // it has focus — `Space` deliberately absent, so a focused waveform
    // still lets it toggle play/pause (US1 AS9).
    actions::register_claim(
        ui.ctx(),
        response.id,
        Claim::Keys(actions::waveform_claims()),
    );

    if previewing && ui.input(|input| input.key_pressed(Key::Escape)) {
        return Some(WaveformEvent::CancelDrag);
    }

    if response.dragged()
        && let Some(pos) = response.interact_pointer_pos()
    {
        return Some(WaveformEvent::Preview(space.frame_at(pos.x)));
    }

    if response.drag_stopped()
        && let Some(pos) = response.interact_pointer_pos()
    {
        return Some(WaveformEvent::Commit(space.frame_at(pos.x)));
    }

    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        return Some(WaveformEvent::Commit(space.frame_at(pos.x)));
    }

    if response.hovered()
        && let Some(event) = pointer_zoom_or_pan(ui, response, space, playhead, is_detail)
    {
        return Some(event);
    }

    if response.has_focus() {
        return keyboard_seek_zoom_pan(ui, space, playhead);
    }

    None
}

/// Wheel/pinch zoom (either waveform) and horizontal scroll/`Shift`+wheel
/// pan (detail only) (contracts/ui-waveform.md §2). At most one of the two
/// is resolved per frame; a frame that both scrolled and pinched is rare
/// and zoom takes priority since it is the widest-reaching change.
fn pointer_zoom_or_pan(
    ui: &Ui,
    response: &Response,
    space: &TimeSpace,
    playhead: u64,
    is_detail: bool,
) -> Option<WaveformEvent> {
    let zoom_delta = ui.input(|input| input.zoom_delta());
    if zoom_delta != 1.0 {
        let anchor_frame = if is_detail {
            response
                .hover_pos()
                .map_or(playhead, |pos| space.frame_at(pos.x))
        } else {
            playhead
        };
        return Some(WaveformEvent::Zoom {
            anchor_frame,
            factor: f64::from(zoom_delta),
        });
    }

    if !is_detail {
        return None;
    }
    let scroll = ui.input(|input| {
        let raw = input.smooth_scroll_delta();
        if input.modifiers.shift && raw.x == 0.0 {
            raw.y
        } else {
            raw.x
        }
    });
    if scroll == 0.0 {
        return None;
    }
    // Scrolling right (positive x) moves the view forward in time; egui's
    // scroll delta is already in screen-pixel units, so the widget's own
    // `frames_per_pixel` converts it directly.
    let delta_frames = (f64::from(-scroll) * space.frames_per_pixel()).round() as i64;
    if delta_frames == 0 {
        None
    } else {
        Some(WaveformEvent::Pan { delta_frames })
    }
}

/// The keyboard rows of contracts/ui-waveform.md §3: plain/`Shift` arrows
/// and `Home`/`End` resolve through `seek_frames` (never a raw
/// `Command::Seek`) so they share the exact same clamping the controller
/// already applies to a pointer seek; `Alt`-modified arrows pan the detail
/// window instead of seeking; `+`/`=`/`-`/`0` zoom/reset it.
fn keyboard_seek_zoom_pan(ui: &Ui, space: &TimeSpace, playhead: u64) -> Option<WaveformEvent> {
    let sample_rate = f64::from(space.sample_rate.max(1));
    let coarse_step = (SEEK_STEP_SECONDS * sample_rate).round() as u64;
    let fine_step = (FINE_SEEK_STEP_SECONDS * sample_rate).round() as u64;
    let track_end = space.window.end;

    ui.input(|input| {
        let shift = input.modifiers.shift;
        let alt = input.modifiers.alt;

        if alt && input.key_pressed(Key::ArrowLeft) {
            let fraction = if shift {
                -PAN_PAGE_FRACTION
            } else {
                -PAN_STEP_FRACTION
            };
            return Some(WaveformEvent::PanStep { fraction });
        }
        if alt && input.key_pressed(Key::ArrowRight) {
            let fraction = if shift {
                PAN_PAGE_FRACTION
            } else {
                PAN_STEP_FRACTION
            };
            return Some(WaveformEvent::PanStep { fraction });
        }
        if input.key_pressed(Key::ArrowLeft) {
            let step = if shift { fine_step } else { coarse_step };
            return Some(WaveformEvent::Commit(playhead.saturating_sub(step)));
        }
        if input.key_pressed(Key::ArrowRight) {
            let step = if shift { fine_step } else { coarse_step };
            return Some(WaveformEvent::Commit(
                playhead.saturating_add(step).min(track_end),
            ));
        }
        if input.key_pressed(Key::Home) {
            return Some(WaveformEvent::Commit(0));
        }
        if input.key_pressed(Key::End) {
            return Some(WaveformEvent::Commit(track_end));
        }
        if input.key_pressed(Key::Plus) || input.key_pressed(Key::Equals) {
            return Some(WaveformEvent::ZoomStep { zoom_in: true });
        }
        if input.key_pressed(Key::Minus) {
            return Some(WaveformEvent::ZoomStep { zoom_in: false });
        }
        if input.key_pressed(Key::Num0) {
            return Some(WaveformEvent::Reset);
        }
        None
    })
}

/// What one frame of keyboard input implies for whichever marker glyph/row
/// currently has focus (006, contracts/ui-markers.md §3) — resolved the
/// same pure input -> event way as [`WaveformEvent`]; the caller
/// (`markers::handle_focused_marker_keys`) applies it against
/// `PlaybackController`/`WaveformState`, since this module stays free of
/// both types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKeyAction {
    /// `Delete`/`Backspace`: `delete_marker(id)`.
    Delete,
    /// `F2`/`Enter`: open the row's inline rename.
    OpenRename,
    /// `C`: `cycle_marker_color(id)`.
    CycleColor,
    /// `Esc` (not dragging, not renaming): focus returns to the detail
    /// waveform.
    ReturnFocus,
}

/// Resolve this frame's focused-marker keyboard table (contracts/
/// ui-markers.md §3, minus the four nudge arrows — those are now
/// `HostAction::NudgeEarlier`/`NudgeLater`/`…X10`, owned by the 007
/// dispatcher under `Scope::MarkerFocused`, contracts/ui-actions.md §3).
/// The caller is responsible for only calling this while a glyph/row
/// actually has focus and no text field of the view does.
pub fn focused_marker_key(ui: &Ui) -> Option<MarkerKeyAction> {
    ui.input_mut(|input| {
        if input.consume_key(Modifiers::NONE, Key::Delete)
            || input.consume_key(Modifiers::NONE, Key::Backspace)
        {
            Some(MarkerKeyAction::Delete)
        } else if input.consume_key(Modifiers::NONE, Key::F2)
            || input.consume_key(Modifiers::NONE, Key::Enter)
        {
            Some(MarkerKeyAction::OpenRename)
        } else if input.consume_key(Modifiers::NONE, Key::C) {
            Some(MarkerKeyAction::CycleColor)
        } else if input.consume_key(Modifiers::NONE, Key::Escape) {
            Some(MarkerKeyAction::ReturnFocus)
        } else {
            None
        }
    })
}
