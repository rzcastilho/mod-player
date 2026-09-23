// SPDX-License-Identifier: MIT OR Apache-2.0

//! Host widgets for the control-variant grammar (015-control-variants,
//! data-model.md §7): `button`, `switch`, `row_frame`,
//! `destructive_gap` and the app-wide `paint_focus_ring` pass. These
//! consume the values defined in `theme::controls` and never write a
//! colour, alpha or stroke width of their own (FR-019).
