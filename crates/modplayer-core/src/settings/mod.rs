// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings: the persisted `AudioSettings` model and the store that loads
//! and saves it as `settings.toml` (contracts/settings-file.md).

mod model;
mod store;

pub use model::{AudioSettings, InvalidField, RawSettings, SCHEMA_VERSION};
pub use store::{CONFIG_DIR_ENV, LoadOutcome, SaveError, SettingsStore, SettingsWarning};
