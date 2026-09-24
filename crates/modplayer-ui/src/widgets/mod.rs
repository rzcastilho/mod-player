// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reusable Now Playing widgets (contracts/ui-surface.md): the peak meter
//! and the master-volume slider.
//!
//! `initials`/`skeleton` (004-search-and-library-browse) add the shared
//! row primitives every catalog list (Search/Library/detail) renders
//! through: the artwork-failure placeholder and the loading-row shimmer
//! (contracts/ui-surface.md §6-7).

pub mod chain_meters;
pub mod controls;
pub mod initials;
pub mod knob;
pub mod peak_meter;
pub mod skeleton;
pub mod volume;
