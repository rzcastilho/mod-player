// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Settings › Developer screen (contracts/ui-surface.md's
//! `developer.buffer_frames` / `developer.raise_notification` /
//! `developer.play_from_account` descriptors): a read-only buffer-frame
//! readout, three buttons that raise a sample notification of each
//! severity (so the notification area can be exercised without waiting for
//! a real device event), and "Play from account" (FR-022, T063): fetches
//! up to 20 recently-played tracks (falling back to saved tracks when
//! empty, `AccountService::request_recent_tracks`), replaces the queue and
//! starts playback. Hidden when playback is not permitted (not signed in,
//! or not Premium).

use egui::Ui;
use modplayer_account::{AccountService, SessionState};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{AccountReadError, SourceHost, TrackRef};
use modplayer_core::{PlaybackController, Severity, tr};

/// "Play from account"'s own UI state (contracts/ui-surface.md §4):
/// constructed once and reused across frames like every other Settings
/// sub-screen state, so a request started on one frame is still tracked
/// when its `AccountEvent::ReadResult` arrives on a later one.
#[derive(Debug, Default)]
pub enum PlayFromAccountState {
    #[default]
    Idle,
    /// Awaiting the source's `AccountTracks` reply for this request id
    /// (Stage 2: sourced through the session, not the Web API).
    Loading(u64),
    Empty,
}

/// What `handle_read_result` learned from an `AccountEvent::ReadResult`,
/// for `App` to act on (queue + play, or raise a notification) — the inline
/// loading/empty/forbidden state itself is already reflected in
/// `PlayFromAccountState` by the time this returns.
pub enum PlayFromAccountOutcome {
    /// Not our request, or an event this screen doesn't handle.
    Ignored,
    /// Tracks are ready to queue and play.
    Ready(Vec<TrackRef>),
    /// A definitive failure that isn't `Forbidden`/empty (contracts/
    /// ui-surface.md §5 `play-from-account-failed`).
    Failed,
}

impl PlayFromAccountState {
    /// React to a `PlaybackController::take_account_tracks` reply (Stage 2,
    /// spec Amendment 2026-09-16). A no-op — returning `Ignored` — for any
    /// reply that isn't the request this screen has pending.
    pub fn handle_account_tracks(
        &mut self,
        request_id: u64,
        result: &Result<Vec<TrackRef>, AccountReadError>,
    ) -> PlayFromAccountOutcome {
        if !matches!(self, PlayFromAccountState::Loading(pending) if *pending == request_id) {
            return PlayFromAccountOutcome::Ignored;
        }
        match result {
            Ok(tracks) if tracks.is_empty() => {
                *self = PlayFromAccountState::Empty;
                PlayFromAccountOutcome::Ignored
            }
            Ok(tracks) => {
                *self = PlayFromAccountState::Idle;
                PlayFromAccountOutcome::Ready(tracks.clone())
            }
            Err(AccountReadError::Unavailable) => {
                *self = PlayFromAccountState::Idle;
                PlayFromAccountOutcome::Failed
            }
        }
    }
}

/// Draw the raw buffer-frame readout, the "raise sample notification"
/// buttons, and (when playback is permitted) "Play from account", applying
/// any click directly to `controller`'s notification center or `account`.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    account: &mut AccountService,
    play_from_account: &mut PlayFromAccountState,
) {
    ui.label(tr("setting-buffer-frames"));
    ui.label(tr("setting-buffer-frames-desc"));
    ui.label(buffer_frames_readout(controller));

    ui.label(tr("setting-raise-notification"));
    ui.label(tr("setting-raise-notification-desc"));
    ui.horizontal(|ui| {
        if ui.button(tr("severity-critical")).clicked() {
            controller
                .notifications_mut()
                .raise(Severity::Critical, "sample-notification-critical");
        }
        if ui.button(tr("severity-warning")).clicked() {
            controller
                .notifications_mut()
                .raise(Severity::Warning, "sample-notification-warning");
        }
        if ui.button(tr("severity-info")).clicked() {
            controller
                .notifications_mut()
                .raise(Severity::Info, "sample-notification-info");
        }
    });

    // Shown once signed in (spec Amendment 2026-09-16: tier is now sourced
    // from the session, so it can't gate this — a real Premium user reads as
    // `Unknown`). Tracks are fetched through the source's own session, not
    // the Web API.
    if !matches!(
        account.state(),
        SessionState::Active | SessionState::Expired
    ) {
        return;
    }
    ui.separator();
    ui.label(tr("setting-play-from-account"));
    ui.label(tr("setting-play-from-account-desc"));
    let loading = matches!(play_from_account, PlayFromAccountState::Loading(_));
    if ui
        .add_enabled(!loading, egui::Button::new(tr("setting-play-from-account")))
        .clicked()
    {
        *play_from_account = PlayFromAccountState::Loading(controller.request_account_tracks(20));
    }
    match play_from_account {
        PlayFromAccountState::Loading(_) => {
            ui.label(tr("play-from-account-loading"));
        }
        PlayFromAccountState::Empty => {
            ui.label(tr("play-from-account-empty"));
        }
        PlayFromAccountState::Idle => {}
    }
}

/// `"requested N / negotiated M frames"` (contracts/ui-surface.md) — the
/// one place in the UI raw frame counts are shown (developer-only, unlike
/// Device Check's rounded-ms preset labels).
fn buffer_frames_readout<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
) -> String {
    let requested = controller.preset().requested_frames().frames();
    let negotiated = controller.shared().negotiated_frames();
    format!("requested {requested} / negotiated {negotiated} frames")
}

#[cfg(test)]
mod tests {
    use super::*;
    use modplayer_audio_io::FakeBackend;
    use modplayer_core::SettingsStore;

    fn fresh_store() -> SettingsStore {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-developer-test-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = std::fs::create_dir_all(&dir);
        SettingsStore::with_path(dir.join("settings.toml"))
    }

    #[test]
    fn readout_shows_requested_and_zero_negotiated_before_any_stream() {
        let controller = PlaybackController::new(
            FakeBackend::new(vec![]),
            modplayer_audio_source_synthetic::SyntheticHost::new(44_100),
            fresh_store(),
        );
        assert_eq!(
            buffer_frames_readout(&controller),
            "requested 256 / negotiated 0 frames"
        );
    }
}
