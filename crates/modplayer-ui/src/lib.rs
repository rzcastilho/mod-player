// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]

//! The egui/eframe application shell: navigation, Device Check, Now
//! Playing, Settings, notifications and theme.
//!
//! Device Check (T050-T051), Now Playing (T063-T066), and the app shell
//! (navigation, notifications, theme, placeholders, Settings › Developer —
//! T078-T084) are implemented so far; the searchable Settings shell and its
//! remaining category screens land in US5 (see
//! `specs/001-walking-skeleton/tasks.md`, T087-T095).

pub mod app;
pub mod device_check;
pub mod notifications;
pub mod now_playing;
pub mod privacy_notice;
pub mod settings;
pub mod shell;
pub mod sign_in;
pub mod theme;
pub mod welcome;
pub mod widgets;

pub use app::App;
pub use device_check::DeviceCheckScreen;
pub use shell::{Section, Shell};
pub use sign_in::SignInScreen;
pub use welcome::WelcomeScreen;
