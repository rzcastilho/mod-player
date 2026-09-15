// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]

//! Binary entrypoint: constructs the production `CpalBackend`, the
//! `PlaybackController`, the `AccountService` (002-first-launch-and-
//! sign-in), and the `modplayer-ui` app, launches both controller and
//! account service (device resolution + auto test tone per
//! `PlaybackController::launch`, data-model.md §6.2; session resolution
//! per `AccountService::launch`, contracts/account-session.md), and runs
//! the `eframe` event loop.
//!
//! Settings resolve through `SettingsStore::new()`, which honours the
//! `MODPLAYER_CONFIG_DIR` environment variable override before falling
//! back to the platform config directory (contracts/settings-file.md);
//! `account.toml` lives in the same directory
//! (contracts/account-session.md). Top-level error handling uses
//! `anyhow`: any failure to resolve a settings directory or run the
//! `eframe` event loop is reported and exits non-zero rather than
//! panicking.

use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use modplayer_account::{
    AccountService, AuthorizationService, Clock, SpotifyAuthorizationService, SystemClock,
};
use modplayer_audio_io::CpalBackend;
use modplayer_core::{PlaybackController, SettingsStore};
use modplayer_secure_store::{KeyringSecureStore, SecureStore};
use modplayer_ui::App;

fn main() -> anyhow::Result<()> {
    let settings_store = SettingsStore::new()
        .context("could not resolve a settings directory (no home directory found)")?;
    let config_dir = settings_store
        .path()
        .parent()
        .map(Path::to_path_buf)
        .context("settings path has no parent directory")?;

    let mut controller = PlaybackController::new(CpalBackend::new(), settings_store);
    controller.launch();

    let secure_store: Arc<dyn SecureStore> = Arc::new(KeyringSecureStore::new());
    let auth_service: Arc<dyn AuthorizationService> = Arc::new(SpotifyAuthorizationService::new());
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
    let mut account = AccountService::new(secure_store, auth_service, clock, config_dir);
    account.launch();

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "ModPlayer",
        native_options,
        Box::new(|cc| Ok(Box::new(App::new(cc, controller, account)))),
    )
    .context("eframe event loop failed")?;

    Ok(())
}
