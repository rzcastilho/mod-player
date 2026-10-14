// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pure geometry for the Search results page (026-search-results-structure,
//! data-model §3, research R2/R3): one flat, scrollable column of group
//! blocks (header, fixed-height rows, optional "Show more" footer) whose
//! extents are computed without a `Ui`, so virtualisation and the pinned
//! header are unit- and property-testable.

use std::borrow::Cow;
use std::ops::Range;

use modplayer_audio_source::SearchKind;

/// One shown group's measurements, input to [`ResultsLayout::new`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupBlock {
    /// Which group this block is.
    pub kind: SearchKind,
    /// Height of the group header.
    pub header_h: f32,
    /// Height of every row (real or skeleton).
    pub row_h: f32,
    /// Number of rows (3 for a skeleton group).
    pub rows: usize,
    /// Height of the "Show more" footer, when the group has one.
    pub footer_h: Option<f32>,
}

/// A [`GroupBlock`] placed in the column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGroup {
    /// Which group this is.
    pub kind: SearchKind,
    /// Top of the header.
    pub header_top: f32,
    /// Height of the header.
    pub header_h: f32,
    /// Top of the first row.
    pub rows_top: f32,
    /// Row height.
    pub row_h: f32,
    /// Row count.
    pub rows: usize,
    /// Top of the footer, when present.
    pub footer_top: Option<f32>,
    /// Bottom of the last row or footer.
    pub content_end: f32,
}

/// The whole column's geometry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResultsLayout {
    /// Placed groups in input order.
    pub groups: Vec<PlacedGroup>,
    /// Total content height.
    pub total_height: f32,
}

impl ResultsLayout {
    /// Place `blocks` top to bottom separated by `gap`.
    ///
    /// ```
    /// use modplayer_audio_source::SearchKind;
    /// use modplayer_ui::search_layout::{GroupBlock, ResultsLayout};
    ///
    /// let block = GroupBlock {
    ///     kind: SearchKind::Track,
    ///     header_h: 20.0,
    ///     row_h: 50.0,
    ///     rows: 2,
    ///     footer_h: Some(30.0),
    /// };
    /// let layout = ResultsLayout::new(&[block, block], 10.0);
    /// assert_eq!(layout.total_height, 2.0 * 150.0 + 10.0);
    /// assert_eq!(layout.groups[1].header_top, 160.0);
    /// ```
    #[must_use]
    pub fn new(blocks: &[GroupBlock], gap: f32) -> Self {
        let mut y = 0.0_f32;
        let mut groups = Vec::with_capacity(blocks.len());
        for (i, b) in blocks.iter().enumerate() {
            if i > 0 {
                y += gap;
            }
            let header_top = y;
            let rows_top = header_top + b.header_h;
            let rows_bottom = rows_top + b.rows as f32 * b.row_h;
            let footer_top = b.footer_h.map(|_| rows_bottom);
            let content_end = rows_bottom + b.footer_h.unwrap_or(0.0);
            groups.push(PlacedGroup {
                kind: b.kind,
                header_top,
                header_h: b.header_h,
                rows_top,
                row_h: b.row_h,
                rows: b.rows,
                footer_top,
                content_end,
            });
            y = content_end;
        }
        Self {
            groups,
            total_height: y,
        }
    }

    /// Rows of group `g` to draw for the viewport `[top, bottom)`: those
    /// intersecting it, plus one either side, clamped to the group.
    ///
    /// ```
    /// use modplayer_audio_source::SearchKind;
    /// use modplayer_ui::search_layout::{GroupBlock, ResultsLayout};
    ///
    /// let block = GroupBlock {
    ///     kind: SearchKind::Track,
    ///     header_h: 20.0,
    ///     row_h: 50.0,
    ///     rows: 100,
    ///     footer_h: None,
    /// };
    /// let layout = ResultsLayout::new(&[block], 0.0);
    /// let r = layout.visible_rows(0, 520.0, 720.0);
    /// assert!(r.contains(&10) && r.len() < 10);
    /// ```
    #[must_use]
    pub fn visible_rows(&self, g: usize, top: f32, bottom: f32) -> Range<usize> {
        let Some(p) = self.groups.get(g) else {
            return 0..0;
        };
        if p.rows == 0 || p.row_h <= 0.0 || bottom <= p.rows_top {
            return 0..0;
        }
        let first = ((top - p.rows_top) / p.row_h).floor();
        let last = ((bottom - p.rows_top) / p.row_h).ceil();
        let start = (first - 1.0).max(0.0) as usize;
        let end = ((last + 1.0).max(0.0) as usize).min(p.rows);
        start.min(end)..end
    }

    /// The group whose header should be pinned for a viewport starting at
    /// `top`, with the y to draw it at: `min(top, content_end - header_h)`,
    /// so the next group's header pushes it out.
    ///
    /// ```
    /// use modplayer_audio_source::SearchKind;
    /// use modplayer_ui::search_layout::{GroupBlock, ResultsLayout};
    ///
    /// let block = GroupBlock {
    ///     kind: SearchKind::Track,
    ///     header_h: 20.0,
    ///     row_h: 50.0,
    ///     rows: 10,
    ///     footer_h: None,
    /// };
    /// let layout = ResultsLayout::new(&[block], 0.0);
    /// assert_eq!(layout.pinned_header(100.0), Some((0, 100.0)));
    /// assert_eq!(layout.pinned_header(0.0), None);
    /// ```
    #[must_use]
    pub fn pinned_header(&self, top: f32) -> Option<(usize, f32)> {
        self.groups
            .iter()
            .position(|p| p.header_top < top && top < p.content_end)
            .map(|g| {
                let p = &self.groups[g];
                (g, top.min(p.content_end - p.header_h))
            })
    }
}

/// Longest query (in Unicode scalar values) shown verbatim in the empty
/// state (FR-012, research R11).
pub const QUERY_DISPLAY_MAX: usize = 60;

/// The query as shown in the empty-state message: unchanged when at most
/// [`QUERY_DISPLAY_MAX`] scalar values, else the first 60 followed by `…`.
///
/// ```
/// use modplayer_ui::search_layout::truncate_query;
///
/// assert_eq!(truncate_query("abba"), "abba");
/// assert_eq!(truncate_query(&"x".repeat(61)), format!("{}…", "x".repeat(60)));
/// ```
pub fn truncate_query(query: &str) -> Cow<'_, str> {
    match query.char_indices().nth(QUERY_DISPLAY_MAX) {
        Some((cut, _)) => Cow::Owned(format!("{}…", &query[..cut])),
        None => Cow::Borrowed(query),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const KINDS: [SearchKind; 4] = [
        SearchKind::Track,
        SearchKind::Album,
        SearchKind::Artist,
        SearchKind::Playlist,
    ];
    const GAP: f32 = 16.0;

    fn block_strategy() -> impl Strategy<Value = (usize, f32, usize, Option<f32>)> {
        (
            0usize..=500,
            prop_oneof![Just(56.0f32), Just(72.0f32)],
            16usize..40,
            prop::option::of(Just(32.0f32)),
        )
            .prop_map(|(rows, row_h, header_h, footer)| (rows, row_h, header_h, footer))
    }

    fn blocks_strategy() -> impl Strategy<Value = Vec<GroupBlock>> {
        prop::collection::vec((block_strategy(), 16usize..40), 0..=4).prop_map(|v| {
            v.into_iter()
                .enumerate()
                .map(|(i, ((rows, row_h, _h, footer_h), header_h))| GroupBlock {
                    kind: KINDS[i],
                    header_h: header_h as f32,
                    row_h,
                    rows,
                    footer_h,
                })
                .collect()
        })
    }

    proptest! {
        /// P1: header tops strictly increase; total matches the L1 sum.
        #[test]
        fn p1_tops_increase_and_total_is_sum(blocks in blocks_strategy()) {
            let layout = ResultsLayout::new(&blocks, GAP);
            prop_assert_eq!(layout.groups.len(), blocks.len());
            for w in layout.groups.windows(2) {
                prop_assert!(w[0].header_top < w[1].header_top);
            }
            let sum: f32 = blocks
                .iter()
                .map(|b| b.header_h + b.rows as f32 * b.row_h + b.footer_h.unwrap_or(0.0))
                .sum::<f32>()
                + GAP * blocks.len().saturating_sub(1) as f32;
            prop_assert!((layout.total_height - sum).abs() < 0.5);
        }

        /// P2: visible rows are in range, cover the viewport, and are bounded.
        #[test]
        fn p2_visible_rows_cover_and_bounded(
            blocks in blocks_strategy(),
            top_frac in 0.0f32..1.0,
            vh in 100.0f32..1000.0,
        ) {
            let layout = ResultsLayout::new(&blocks, GAP);
            let top = top_frac * layout.total_height;
            let bottom = top + vh;
            for (g, p) in layout.groups.iter().enumerate() {
                let r = layout.visible_rows(g, top, bottom);
                prop_assert!(r.end <= p.rows && r.start <= r.end);
                prop_assert!(r.len() <= (vh / p.row_h).ceil() as usize + 3);
                for i in 0..p.rows {
                    let rt = p.rows_top + i as f32 * p.row_h;
                    if rt < bottom && rt + p.row_h > top {
                        prop_assert!(r.contains(&i), "row {i} intersects but not in {r:?}");
                    }
                }
            }
        }

        /// P3: pinned iff header_top < top < content_end; never two.
        #[test]
        fn p3_pinned_iff_inside(blocks in blocks_strategy(), top_frac in 0.0f32..1.0) {
            let layout = ResultsLayout::new(&blocks, GAP);
            let top = top_frac * layout.total_height;
            let expected: Vec<usize> = layout
                .groups
                .iter()
                .enumerate()
                .filter(|(_, p)| p.header_top < top && top < p.content_end)
                .map(|(i, _)| i)
                .collect();
            prop_assert!(expected.len() <= 1);
            prop_assert_eq!(layout.pinned_header(top).map(|(g, _)| g), expected.first().copied());
        }

        /// P4: pinned y stays within the group's header..next header bounds.
        #[test]
        fn p4_pinned_y_bounds(blocks in blocks_strategy(), top_frac in 0.0f32..1.0) {
            let layout = ResultsLayout::new(&blocks, GAP);
            let top = top_frac * layout.total_height;
            if let Some((g, y)) = layout.pinned_header(top) {
                let p = &layout.groups[g];
                prop_assert!(y <= top + 0.001);
                prop_assert!(y >= p.header_top);
                if let Some(next) = layout.groups.get(g + 1) {
                    prop_assert!(y + p.header_h <= next.header_top + 0.001);
                }
            }
        }
    }

    /// R11: identity at or under the limit.
    #[test]
    fn truncate_query_identity_up_to_limit() {
        let exact = "é".repeat(QUERY_DISPLAY_MAX);
        assert_eq!(truncate_query(&exact), exact.as_str());
        assert!(matches!(truncate_query(&exact), Cow::Borrowed(_)));
        assert_eq!(truncate_query(""), "");
    }

    /// R11: 61+ scalars truncate to 60 + "…" without splitting scalars.
    #[test]
    fn truncate_query_cuts_at_60_scalars() {
        let long = "日本語".repeat(30);
        let out = truncate_query(&long);
        assert_eq!(out.chars().count(), QUERY_DISPLAY_MAX + 1);
        assert!(out.ends_with('…'));
        assert!(long.starts_with(out.trim_end_matches('…')));
        let two_hundred = "a".repeat(200);
        assert_eq!(
            truncate_query(&two_hundred),
            format!("{}…", "a".repeat(QUERY_DISPLAY_MAX))
        );
    }

    proptest! {
        /// R11: ≤ 61 scalars, identity for ≤ 60, prefix-preserving.
        #[test]
        fn p5_truncate_query_bounds(q in ".{0,200}") {
            let out = truncate_query(&q);
            prop_assert!(out.chars().count() <= QUERY_DISPLAY_MAX + 1);
            if q.chars().count() <= QUERY_DISPLAY_MAX {
                prop_assert_eq!(out.as_ref(), q.as_str());
            } else {
                let body = out.strip_suffix('…').unwrap();
                prop_assert!(q.starts_with(body));
                prop_assert_eq!(body.chars().count(), QUERY_DISPLAY_MAX);
            }
        }
    }
}
