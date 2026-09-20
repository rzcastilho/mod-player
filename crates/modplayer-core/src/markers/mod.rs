// SPDX-License-Identifier: MIT OR Apache-2.0

//! Markers, loop regions and cue points: the core model (`model`) and its
//! on-disk persistence (`store`) (006, data-model.md).

mod model;
pub mod store;

pub use model::{
    CueSlot, LoopRegion, MAX_MARKERS, MAX_NAME_CHARS, Marker, MarkerError, MarkerId, MarkerKind,
    Owner, PaletteIndex, RegionId, RepeatCount, TrackMarkers,
};
