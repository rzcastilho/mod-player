// SPDX-License-Identifier: MIT OR Apache-2.0

//! Control variant values (015-control-variants, data-model.md §2–§7):
//! button variants and their paint (§2), interaction-state fills and the
//! focus ring (§3), the widget-state slot map (§4), the switch's metrics
//! and state table (§5), meter bands and scale marks (§6), and the
//! constants the host widgets in `widgets/controls.rs` consume (§7).
//! Every colour, alpha, stroke width and metric this feature introduces
//! lives here (FR-019) — no call site writes a literal.
