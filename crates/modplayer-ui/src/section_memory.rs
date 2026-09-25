// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sub-view + scroll retention (020-shell-navigation-and-gates, US3,
//! data-model.md §4, contracts/section-memory.md): each section's
//! scrollable view keeps its own vertical offset, keyed by section *and*
//! sub-view (library tab/detail, settings category), restored once on
//! return — never persisted to disk (`M6`; eframe's `persistence` feature
//! stays off), and reset — together with an `epoch` bump that discards
//! every egui-side persisted scroll state keyed by the old salt — on
//! sign-out/revocation (`FR-007`, US3-AS4).
//!
//! `SettingsCategory` (`modplayer_core::settings_registry`) is a read-only
//! dependency (plan.md Project Structure) and does not derive `Hash`, so
//! [`ViewKey`]'s `Hash` impl is written by hand, hashing a category by its
//! own [`SettingsCategory::label_key`] — a `&'static str` that is 1:1 with
//! the variant (`Eq`/`Hash` consistency: `a == b` implies
//! `label_key(a) == label_key(b)`), rather than deriving it.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use egui::ScrollArea;
use modplayer_core::settings_registry::SettingsCategory;

use crate::detail_view::DetailTarget;
use crate::library_view::LibraryTab;

/// Which of Library's own scrollable views (data-model.md §4).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LibraryViewKey {
    Tab(LibraryTab),
    Detail(DetailTarget),
}

/// Identifies one scrollable view across the whole app (data-model.md §4,
/// contracts/section-memory.md "Attachment points"). Now Playing has no
/// entry (research.md R5): it has no section-level scroll area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewKey {
    Library(LibraryViewKey),
    Search,
    Settings(SettingsCategory),
    Plugins,
}

impl Hash for ViewKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            ViewKey::Library(key) => key.hash(state),
            ViewKey::Search | ViewKey::Plugins => {}
            ViewKey::Settings(category) => category.label_key().hash(state),
        }
    }
}

/// Every scrollable view's last observed vertical offset, keyed by
/// [`ViewKey`] (data-model.md §4). Owned by `App`, alongside `shell`/
/// `settings`/`library_view`. No `serde` derive (M6): nothing here is ever
/// written to disk.
#[derive(Debug, Default)]
pub struct SectionMemory {
    /// Bumped by [`Self::reset`]; folded into every scroll area's id salt so
    /// a sign-out discards egui's own persisted scroll state too, not just
    /// this map (research.md R4).
    epoch: u64,
    /// The last observed vertical offset per scrollable view.
    offsets: HashMap<ViewKey, f32>,
    /// Which view's area was drawn (via [`Self::record`]) last frame — lets
    /// [`Self::scroll_area`] apply a stored offset exactly once, the frame a
    /// view becomes visible again, rather than fighting the user's own
    /// live scrolling every frame.
    shown_last_frame: Option<ViewKey>,
}

impl SectionMemory {
    /// A vertical [`ScrollArea`] for `key`: `auto_shrink([false, false])`,
    /// `id_salt(("section-scroll", key, epoch))`, and — only the first frame
    /// `key` is shown again after being away — `.vertical_scroll_offset`
    /// seeded from this view's last recorded offset (egui clamps it to the
    /// new content height on show, satisfying M3).
    pub fn scroll_area(&mut self, key: &ViewKey) -> ScrollArea {
        let mut area = ScrollArea::vertical()
            .id_salt(("section-scroll", key.clone(), self.epoch))
            .auto_shrink([false, false]);
        if self.shown_last_frame.as_ref() != Some(key)
            && let Some(stored) = self.offsets.get(key)
        {
            area = area.vertical_scroll_offset(*stored);
        }
        area
    }

    /// Record `key`'s vertical offset after drawing it this frame (the
    /// `ScrollAreaOutput::state.offset.y` [`Self::scroll_area`]'s
    /// `ScrollArea::show`/`show_rows` returned), and mark it as this frame's
    /// shown view.
    pub fn record(&mut self, key: ViewKey, offset_y: f32) {
        self.shown_last_frame = Some(key.clone());
        self.offsets.insert(key, offset_y);
    }

    /// Call once per frame, after every section that might have drawn a
    /// memory-backed view: `drawn` is that view's key, or `None` when no
    /// such view was on screen this frame (a launch gate, Now Playing, a
    /// Settings-triggered Device Check preview). `None` clears
    /// `shown_last_frame`, so the next frame a view *is* drawn, it is
    /// correctly treated as "shown again after an absence" and its stored
    /// offset is re-applied.
    pub fn end_frame(&mut self, drawn: Option<&ViewKey>) {
        if drawn.is_none() {
            self.shown_last_frame = None;
        }
    }

    /// `key`'s last recorded offset, if any (test/diagnostic read).
    #[must_use]
    pub fn offset(&self, key: &ViewKey) -> Option<f32> {
        self.offsets.get(key).copied()
    }

    /// The current epoch (test/diagnostic read).
    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Clear every recorded offset and bump the epoch (US3-AS4, FR-007):
    /// the next draw of any key starts at the top, and every scroll area's
    /// id salt changes, so no stale egui-persisted offset survives either.
    pub fn reset(&mut self) {
        self.offsets.clear();
        self.shown_last_frame = None;
        self.epoch += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_memory_has_no_offsets_and_epoch_zero() {
        let memory = SectionMemory::default();
        assert_eq!(memory.epoch(), 0);
        assert_eq!(memory.offset(&ViewKey::Search), None);
    }

    #[test]
    fn record_then_offset_round_trips() {
        let mut memory = SectionMemory::default();
        memory.record(ViewKey::Search, 42.0);
        assert_eq!(memory.offset(&ViewKey::Search), Some(42.0));
    }

    #[test]
    fn reset_clears_offsets_and_bumps_epoch() {
        let mut memory = SectionMemory::default();
        memory.record(ViewKey::Search, 42.0);
        memory.record(ViewKey::Plugins, 7.0);
        memory.reset();
        assert_eq!(memory.epoch(), 1);
        assert_eq!(memory.offset(&ViewKey::Search), None);
        assert_eq!(memory.offset(&ViewKey::Plugins), None);
    }

    #[test]
    fn end_frame_with_none_clears_shown_last_frame() {
        let mut memory = SectionMemory::default();
        memory.record(ViewKey::Search, 10.0);
        assert_eq!(memory.shown_last_frame, Some(ViewKey::Search));
        memory.end_frame(None);
        assert_eq!(memory.shown_last_frame, None);
    }

    #[test]
    fn end_frame_with_some_leaves_shown_last_frame() {
        let mut memory = SectionMemory::default();
        memory.record(ViewKey::Search, 10.0);
        memory.end_frame(Some(&ViewKey::Search));
        assert_eq!(memory.shown_last_frame, Some(ViewKey::Search));
    }

    /// Two `ViewKey::Settings` values over the same category are equal and
    /// hash equal (the hand-written `Hash` impl's whole reason for being).
    #[test]
    fn settings_view_key_equal_categories_hash_equal() {
        use std::collections::hash_map::DefaultHasher;

        fn hash_of(key: &ViewKey) -> u64 {
            let mut hasher = DefaultHasher::new();
            key.hash(&mut hasher);
            hasher.finish()
        }

        let a = ViewKey::Settings(SettingsCategory::Audio);
        let b = ViewKey::Settings(SettingsCategory::Audio);
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));

        let c = ViewKey::Settings(SettingsCategory::About);
        assert_ne!(a, c);
    }
}
