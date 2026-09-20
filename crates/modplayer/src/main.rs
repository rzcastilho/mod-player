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
    AccountService, AuthorizationService, Clock, SessionCredential, SpotifyAuthorizationService,
    SystemClock,
};
use modplayer_audio_io::CpalBackend;
use modplayer_audio_source_connect::{
    ConnectConfig, ConnectSource, CredentialError, ReceiverCredentials,
};
use modplayer_core::library::LibraryPaths;
use modplayer_core::{PlaybackController, SettingsStore};
use modplayer_secure_store::{EntryName, KeyringSecureStore, SecureStore};
use modplayer_ui::App;

/// A ≤ 30-line stderr `log::Log` at `Info` level (009 research R19): lets
/// the manual scenarios read `log::{info,warn,error}!(target:
/// "plugin:<identifier>", …)` entries (and everything else routed through
/// the `log` façade) on the terminal. `modplayer_core::plugins::PluginLog`
/// is the structured, in-memory record the UI/tests read; this is just
/// the human-readable mirror.
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "[{level}] {target}: {args}",
                level = record.level(),
                target = record.target(),
                args = record.args()
            );
        }
    }

    fn flush(&self) {}
}

static STDERR_LOGGER: StderrLogger = StderrLogger;

/// `ReceiverCredentials` over the OS secure store (design note 5:
/// "the only read path"; contracts/connect-source.md §1). Reads the same
/// `SessionCredential` 002's `AccountService` writes/refreshes — this
/// crate never sees the token beyond this one call per (re)connect.
struct SecureStoreCredentials {
    store: Arc<dyn SecureStore>,
}

impl ReceiverCredentials for SecureStoreCredentials {
    fn access_token(&self) -> Result<String, CredentialError> {
        let bytes = self
            .store
            .get(EntryName::SessionCredential)
            .map_err(|_| CredentialError::Unavailable)?
            .ok_or(CredentialError::Unavailable)?;
        let credential =
            SessionCredential::from_payload(&bytes).map_err(|_| CredentialError::Unavailable)?;
        Ok(credential.access_token)
    }
}

fn main() -> anyhow::Result<()> {
    log::set_logger(&STDERR_LOGGER)
        .map(|()| log::set_max_level(log::LevelFilter::Info))
        .unwrap_or(());

    let settings_store = SettingsStore::new()
        .context("could not resolve a settings directory (no home directory found)")?;
    let config_dir = settings_store
        .path()
        .parent()
        .map(Path::to_path_buf)
        .context("settings path has no parent directory")?;

    let secure_store: Arc<dyn SecureStore> = Arc::new(KeyringSecureStore::new());

    // `device_name`/`device_id` are placeholders here: the real,
    // settings-derived values are sent with `SourceCommand::Initialize`
    // once `PlaybackController::set_playback_permitted(true, _)` decides
    // playback is allowed (contracts/connect-source.md §1, design note 5).
    let connect_source = ConnectSource::new(ConnectConfig {
        device_name: String::new(),
        device_id: String::new(),
        credentials: Arc::new(SecureStoreCredentials {
            store: Arc::clone(&secure_store),
        }),
    });
    // 004-search-and-library-browse (research R6/R13): the library mirror
    // and play log live under the platform data dir; `None` (unresolvable
    // dir) means an in-memory-only library, never a crash. The 2026-09-17
    // manual walk (quickstart M9/M10) found this opt-in missing from the
    // binary — nothing was ever persisted or reloaded.
    let mut controller =
        PlaybackController::new(CpalBackend::new(), connect_source, settings_store)
            .with_library_paths(LibraryPaths::resolve());
    controller.launch();

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
