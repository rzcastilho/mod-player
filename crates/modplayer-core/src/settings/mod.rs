// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings: the persisted `AudioSettings` model and the store that loads
//! and saves it as `settings.toml` (contracts/settings-file.md).

mod model;
mod store;
mod window;

pub use model::{
    AudioSettings, DeviceName, DeviceNameError, DisclosureAcknowledgement, InvalidField,
    NowPlayingPanels, PanelPersisted, PanelPlacement, RawNowPlayingPanels, RawPanel, RawSettings,
    RawWindow, SCHEMA_VERSION, generate_connect_device_id,
};
pub use store::{CONFIG_DIR_ENV, LoadOutcome, SaveError, SettingsStore, SettingsWarning};
pub use window::{
    DEFAULT_INNER_SIZE, DOCK_WIDTH_DEFAULT, DOCK_WIDTH_MAX, DOCK_WIDTH_MIN, MIN_INNER_SIZE,
    WindowSettings,
};
