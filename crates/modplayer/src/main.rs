// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]

//! Binary entrypoint: constructs the production `CpalBackend`, the
//! `PlaybackController` and the `modplayer-ui` app, launches the
//! controller (device resolution + auto test tone per
//! `PlaybackController::launch`, data-model.md §6.2), and runs the
//! `eframe` event loop.
//!
//! Settings resolve through `SettingsStore::new()`, which honours the
//! `MODPLAYER_CONFIG_DIR` environment variable override before falling
//! back to the platform config directory (contracts/settings-file.md).
//! Top-level error handling uses `anyhow`: any failure to resolve a
//! settings directory or run the `eframe` event loop is reported and
//! exits non-zero rather than panicking.

use anyhow::Context;
use modplayer_audio_io::CpalBackend;
use modplayer_core::{PlaybackController, SettingsStore};
use modplayer_ui::App;

fn main() -> anyhow::Result<()> {
    let settings_store = SettingsStore::new()
        .context("could not resolve a settings directory (no home directory found)")?;

    let mut controller = PlaybackController::new(CpalBackend::new(), settings_store);
    controller.launch();

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "ModPlayer",
        native_options,
        Box::new(|cc| Ok(Box::new(App::new(cc, controller)))),
    )
    .context("eframe event loop failed")?;

    Ok(())
}
