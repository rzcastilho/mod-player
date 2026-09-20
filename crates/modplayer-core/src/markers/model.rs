// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core marker/loop-region model: `TrackMarkers` and its mutation API
//! (006, data-model.md §1; owner deltas per 009 data-model.md §1.5,
//! FR-024, contracts/plugin-host-service.md).

use modplayer_audio_source::TrackId;
use modplayer_effects::catalog::PluginId;

/// The most markers (of any kind, counting the lone endpoint of an
/// incomplete region) a single track may carry (I1, FR-002).
pub const MAX_MARKERS: usize = 64;

/// The longest a marker's `name` is kept after `rename` trims/truncates
/// it (data-model.md §1.3).
pub const MAX_NAME_CHARS: usize = 64;

/// A region's default crossfade, in milliseconds (data-model.md §1.4).
const DEFAULT_CROSSFADE_MS: u8 = 5;

/// The shortest a region may be to arm: `max(1, rate / 1000)` frames (~1
/// ms, data-model.md §1.4).
fn min_region_frames(rate: u32) -> u64 {
    (u64::from(rate) / 1000).max(1)
}

/// A marker's stable identity within its track (data-model.md §1.1).
/// Allocated by [`TrackMarkers::next_marker_id`]-equivalent internal
/// counter and persisted so ids stay stable across sessions; a saved
/// file never reuses an id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MarkerId(u32);

impl MarkerId {
    /// Crate-internal: `markers::store::load` reconstructs ids straight
    /// from the persisted file rather than through `TrackMarkers`'s own
    /// allocator (data-model.md §4).
    pub(crate) fn from_raw(id: u32) -> Self {
        Self(id)
    }

    pub(crate) fn raw(self) -> u32 {
        self.0
    }
}

/// A loop region's stable identity within its track (data-model.md §1.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionId(u32);

impl RegionId {
    /// As [`MarkerId::from_raw`], for regions.
    pub(crate) fn from_raw(id: u32) -> Self {
        Self(id)
    }

    pub(crate) fn raw(self) -> u32 {
        self.0
    }
}

/// A cue slot, `1..=8` (data-model.md §1.2, FR-013). Constructed only
/// through [`CueSlot::new`], so an out-of-range value can never exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CueSlot(u8);

impl CueSlot {
    /// `slot` must be `1..=8`; `None` otherwise.
    pub fn new(slot: u8) -> Option<Self> {
        if (1..=8).contains(&slot) {
            Some(Self(slot))
        } else {
            None
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }

    /// Crate-internal: builds a slot from a compile-time-known-valid
    /// value (`1..=8`), for `actions::catalog`'s `const CATALOG` (007,
    /// data-model.md §1.1), which has no `Option`-friendly way to build a
    /// `const` array. Never exposed outside the crate — every call site
    /// passes a literal already checked against the same `1..=8` range
    /// `new` validates at runtime.
    pub(crate) const fn new_const(slot: u8) -> Self {
        Self(slot)
    }
}

/// A palette index, `0..=7` (data-model.md §1.3, research R13). The
/// persisted value is the index; the colour itself lives only in
/// `modplayer-ui::theme::MARKER_PALETTE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PaletteIndex(u8);

impl PaletteIndex {
    /// Clamped into `0..=7`.
    pub fn new(index: u8) -> Self {
        Self(index.min(7))
    }

    pub fn get(self) -> u8 {
        self.0
    }

    /// The next colour in the 8-entry palette, wrapping (`cycle_color`).
    fn next(self) -> Self {
        Self((self.0 + 1) % 8)
    }
}

/// A loop region's repeat count (data-model.md §1.4, FR-011): `Infinite`
/// (the default) or a bounded `Times(1..=1000)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatCount {
    #[default]
    Infinite,
    Times(u16),
}

impl RepeatCount {
    /// Clamp `value` into `1..=1000` and wrap it as `Times`.
    pub fn times_clamped(value: u16) -> Self {
        Self::Times(value.clamp(1, 1_000))
    }
}

/// A marker's owner (data-model.md §1.3, FR-024; 009 data-model.md
/// §1.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Host,
    Plugin(PluginId),
}

/// What role a marker plays (data-model.md §1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    Point,
    RegionStart { region: RegionId },
    RegionEnd { region: RegionId },
    Cue { slot: CueSlot },
}

/// One marker (data-model.md §1.3, DM-6).
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub id: MarkerId,
    pub kind: MarkerKind,
    /// Frames, `0..=len_frames` (FR-019 clamp on every set).
    pub position: u64,
    /// Trimmed, `<= MAX_NAME_CHARS`; a `Point`'s default is externalised
    /// via `marker-default-name` (design note 14); region/cue defaults
    /// are empty.
    pub name: String,
    pub color: PaletteIndex,
    pub owner: Owner,
    /// Always `false` this slice; persisted for forward compatibility.
    pub transient: bool,
    /// Always `true` this slice.
    pub visible: bool,
    /// Session-only (never persisted): set by `store::load` when the
    /// saved position exceeded the current track length (FR-018);
    /// cleared the next time the marker moves.
    pub clamped: bool,
}

/// A loop region: an A/B marker pair plus its crossfade/repeat/arm state
/// (data-model.md §1.4, DM-7).
#[derive(Debug, Clone, PartialEq)]
pub struct LoopRegion {
    pub id: RegionId,
    /// The `RegionStart` marker; `None` while incomplete.
    pub a: Option<MarkerId>,
    /// The `RegionEnd` marker; `None` while incomplete.
    pub b: Option<MarkerId>,
    /// `0..=50`, default `5` (FR-010).
    pub crossfade_ms: u8,
    /// `Infinite` (default) or `Times(1..=1000)` (FR-011).
    pub repeat: RepeatCount,
    /// Session-only: never serialized; every region loads disarmed
    /// (FR-016).
    pub armed: bool,
    /// Session-only mirror of `RtShared::loop_wraps`, reset on arm.
    pub wraps: u32,
}

impl LoopRegion {
    /// Both endpoints are set.
    pub fn is_complete(&self) -> bool {
        self.a.is_some() && self.b.is_some()
    }

    /// `(position_of(a), position_of(b))`, or `None` while incomplete or
    /// if an endpoint id somehow no longer resolves.
    pub fn span(&self, markers: &TrackMarkers) -> Option<(u64, u64)> {
        let a = markers.position_of(self.a?)?;
        let b = markers.position_of(self.b?)?;
        Some((a, b))
    }

    /// `b - a`, or `None` while incomplete.
    pub fn length_frames(&self, markers: &TrackMarkers) -> Option<u64> {
        self.span(markers).map(|(a, b)| b.saturating_sub(a))
    }

    /// Complete and at least `min_region_frames(rate)` (~1 ms) long
    /// (FR-007/FR-008).
    pub fn is_armable(&self, markers: &TrackMarkers, rate: u32) -> bool {
        match self.span(markers) {
            Some((a, b)) => b.saturating_sub(a) >= min_region_frames(rate),
            None => false,
        }
    }

    /// The crossfade this region would actually use at `rate`
    /// (data-model.md §1.4): `loop_math::effective_crossfade(configured,
    /// a, b)`.
    pub fn effective_crossfade_frames(&self, markers: &TrackMarkers, rate: u32) -> u64 {
        match self.span(markers) {
            Some((a, b)) => {
                let configured = u64::from(self.crossfade_ms) * u64::from(rate) / 1_000;
                modplayer_engine::loop_math::effective_crossfade(configured, a, b)
            }
            None => 0,
        }
    }
}

/// A `TrackMarkers` mutation was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MarkerError {
    #[error("the track already has the maximum number of markers")]
    LimitReached,
    #[error("loop region is too short to arm")]
    RegionTooShort,
    #[error("loop region is incomplete")]
    RegionIncomplete,
    #[error("marker not found")]
    NotFound,
    /// A plugin tried to mutate a marker/region/cue it does not own (009
    /// FR-024, C2).
    #[error("this plugin does not own that marker, region or cue")]
    NotOwner,
    /// Controller-level only: no current track (marker-service.md §1).
    #[error("no current track")]
    NoTrack,
}

/// The per-track aggregate of markers, regions and cue points
/// (data-model.md §1.5): the shadow state the controller derives the
/// engine's real-time loop copy from and persists per track identity.
/// Every mutation keeps `markers` sorted by `(position, id)` (I2), clamps
/// positions to `0..=len_frames` (I4), and enforces the 64-marker limit
/// (I1).
///
/// ```
/// use modplayer_audio_source::TrackId;
/// use modplayer_core::markers::TrackMarkers;
///
/// let track = TrackId::new("spotify:track:4uLU6hMCjMI75M1A2tKUQC").unwrap();
/// let mut markers = TrackMarkers::new(track, 44_100, 44_100 * 180);
/// let id = markers.add_point(44_100 * 10).unwrap();
/// assert_eq!(markers.count(), 1);
/// markers.move_marker(id, 44_100 * 20).unwrap();
/// assert_eq!(markers.position_of(id), Some(44_100 * 20));
/// markers.delete(id).unwrap();
/// assert_eq!(markers.count(), 0);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TrackMarkers {
    track: TrackId,
    len_frames: u64,
    sample_rate: u32,
    markers: Vec<Marker>,
    regions: Vec<LoopRegion>,
    current_region: Option<RegionId>,
    next_marker_id: u32,
    next_region_id: u32,
    dirty: bool,
    /// Bumped alongside `dirty` on every mutation (009 C3): the
    /// controller compares this against its own last-seen value once per
    /// `tick()` to decide whether to fan out `marker_changed`/
    /// `loop_*`/etc, independent of whether a save is due.
    revision: u64,
}

impl TrackMarkers {
    /// A fresh, empty aggregate for `track` (used both for a brand-new
    /// track and as the "empty state" `store::load` falls back to).
    pub fn new(track: TrackId, sample_rate: u32, len_frames: u64) -> Self {
        Self {
            track,
            len_frames,
            sample_rate,
            markers: Vec::new(),
            regions: Vec::new(),
            current_region: None,
            next_marker_id: 1,
            next_region_id: 1,
            dirty: false,
            revision: 0,
        }
    }

    // -- Identity / bookkeeping ------------------------------------------

    pub fn track(&self) -> &TrackId {
        &self.track
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn len_frames(&self) -> u64 {
        self.len_frames
    }

    /// Set by every mutation; cleared by `mark_clean` once the debounced
    /// save picks it up (controller/`store` territory, not this module).
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Monotonic, bumped on every mutation (009 C3) — never reset by
    /// [`Self::mark_clean`], since it tracks "has anything changed since
    /// I last fanned out an event", not "has anything changed since I
    /// last saved".
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Crate-internal: reconstruct a loaded state directly from its parts
    /// (`markers::store::load`, data-model.md §4), bypassing the mutation
    /// API's per-call invariant enforcement — `store::load` has already
    /// applied every repair rule itself (dangling refs, clamped positions,
    /// duplicate cue slots). `dirty` starts `false`: a freshly loaded state
    /// is not itself unsaved. `markers` is (re-)sorted (I2) since a
    /// hand-edited file is not guaranteed to already be.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        track: TrackId,
        sample_rate: u32,
        len_frames: u64,
        mut markers: Vec<Marker>,
        regions: Vec<LoopRegion>,
        current_region: Option<RegionId>,
        next_marker_id: u32,
        next_region_id: u32,
    ) -> Self {
        markers.sort_by(|a, b| a.position.cmp(&b.position).then(a.id.cmp(&b.id)));
        Self {
            track,
            len_frames,
            sample_rate,
            markers,
            regions,
            current_region,
            next_marker_id,
            next_region_id,
            dirty: false,
            revision: 0,
        }
    }

    /// Crate-internal: the raw counter `markers::store::encode` persists
    /// as `next_marker_id` (data-model.md §4).
    pub(crate) fn next_marker_id_raw(&self) -> u32 {
        self.next_marker_id
    }

    /// As [`Self::next_marker_id_raw`], for regions.
    pub(crate) fn next_region_id_raw(&self) -> u32 {
        self.next_region_id
    }

    fn alloc_marker_id(&mut self) -> MarkerId {
        let id = MarkerId(self.next_marker_id);
        self.next_marker_id += 1;
        id
    }

    fn alloc_region_id(&mut self) -> RegionId {
        let id = RegionId(self.next_region_id);
        self.next_region_id += 1;
        id
    }

    fn resort(&mut self) {
        self.markers
            .sort_by(|a, b| a.position.cmp(&b.position).then(a.id.cmp(&b.id)));
    }

    fn marker_mut(&mut self, id: MarkerId) -> Option<&mut Marker> {
        self.markers.iter_mut().find(|m| m.id == id)
    }

    fn region_mut(&mut self, id: RegionId) -> Option<&mut LoopRegion> {
        self.regions.iter_mut().find(|r| r.id == id)
    }

    /// A fresh `Point`'s default name: `"Marker n"` where `n` is the
    /// track's point-marker count + 1 at creation (data-model.md §1.3),
    /// externalised via `marker-default-name` (design note 14; the key
    /// lands with US3, T083). Region/cue defaults are empty.
    fn default_name_for(&self, kind: MarkerKind) -> String {
        match kind {
            MarkerKind::Point => {
                let n = self
                    .markers
                    .iter()
                    .filter(|m| matches!(m.kind, MarkerKind::Point))
                    .count()
                    + 1;
                crate::i18n::tr_args("marker-default-name", &[("n", n.to_string())])
            }
            MarkerKind::RegionStart { .. }
            | MarkerKind::RegionEnd { .. }
            | MarkerKind::Cue { .. } => String::new(),
        }
    }

    /// After any position change to a region endpoint (I3): if
    /// `pos(a) > pos(b)`, swap the two markers' *kinds* (their names and
    /// colours stay with their own marker ids, FR-007). Equal positions
    /// do not swap.
    fn maybe_swap_region_endpoints(&mut self, region: RegionId) {
        let (a_id, b_id) = match self.regions.iter().find(|r| r.id == region) {
            Some(r) => match (r.a, r.b) {
                (Some(a), Some(b)) => (a, b),
                _ => return,
            },
            None => return,
        };
        let (Some(pos_a), Some(pos_b)) = (self.position_of(a_id), self.position_of(b_id)) else {
            return;
        };
        if pos_a <= pos_b {
            return;
        }
        if let Some(m) = self.marker_mut(a_id) {
            m.kind = MarkerKind::RegionEnd { region };
        }
        if let Some(m) = self.marker_mut(b_id) {
            m.kind = MarkerKind::RegionStart { region };
        }
        if let Some(r) = self.region_mut(region) {
            r.a = Some(b_id);
            r.b = Some(a_id);
        }
    }

    // -- Mutation API (data-model.md §1.5) --------------------------------

    /// `I1`, default name/colour, `I2` (FR-001).
    pub fn add_point(&mut self, pos: u64) -> Result<MarkerId, MarkerError> {
        if self.markers.len() >= MAX_MARKERS {
            return Err(MarkerError::LimitReached);
        }
        let pos = pos.min(self.len_frames);
        let id = self.alloc_marker_id();
        let name = self.default_name_for(MarkerKind::Point);
        self.markers.push(Marker {
            id,
            kind: MarkerKind::Point,
            position: pos,
            name,
            color: PaletteIndex::new(1),
            owner: Owner::Host,
            transient: false,
            visible: true,
            clamped: false,
        });
        self.resort();
        self.dirty = true;
        self.revision += 1;
        Ok(id)
    }

    /// As [`Self::add_point`], for a plugin-created point (009 FR-024):
    /// `owner`/`transient` are caller-supplied instead of always `Owner::
    /// Host`/`false`.
    pub fn add_point_owned(
        &mut self,
        pos: u64,
        owner: Owner,
        transient: bool,
    ) -> Result<MarkerId, MarkerError> {
        if self.markers.len() >= MAX_MARKERS {
            return Err(MarkerError::LimitReached);
        }
        let pos = pos.min(self.len_frames);
        let id = self.alloc_marker_id();
        let name = self.default_name_for(MarkerKind::Point);
        self.markers.push(Marker {
            id,
            kind: MarkerKind::Point,
            position: pos,
            name,
            color: PaletteIndex::new(1),
            owner,
            transient,
            visible: true,
            clamped: false,
        });
        self.resort();
        self.dirty = true;
        self.revision += 1;
        Ok(id)
    }

    /// `CreateLoopRegion` (009 contracts/plugin-api-v1.md §3): both
    /// endpoints are created together, already owned — unlike the host's
    /// two-step `set_loop_a`/`set_loop_b`.
    pub fn new_loop_region_owned(
        &mut self,
        a_ms: u64,
        b_ms: u64,
        owner: Owner,
        transient: bool,
    ) -> Result<RegionId, MarkerError> {
        if self.markers.len() + 2 > MAX_MARKERS {
            return Err(MarkerError::LimitReached);
        }
        let region_id = self.alloc_region_id();
        self.regions.push(LoopRegion {
            id: region_id,
            a: None,
            b: None,
            crossfade_ms: DEFAULT_CROSSFADE_MS,
            repeat: RepeatCount::Infinite,
            armed: false,
            wraps: 0,
        });

        let a_pos = a_ms.min(self.len_frames);
        let a_id = self.alloc_marker_id();
        self.markers.push(Marker {
            id: a_id,
            kind: MarkerKind::RegionStart { region: region_id },
            position: a_pos,
            name: String::new(),
            color: PaletteIndex::new(0),
            owner,
            transient,
            visible: true,
            clamped: false,
        });

        let b_pos = b_ms.min(self.len_frames);
        let b_id = self.alloc_marker_id();
        self.markers.push(Marker {
            id: b_id,
            kind: MarkerKind::RegionEnd { region: region_id },
            position: b_pos,
            name: String::new(),
            color: PaletteIndex::new(0),
            owner,
            transient,
            visible: true,
            clamped: false,
        });

        if let Some(r) = self.region_mut(region_id) {
            r.a = Some(a_id);
            r.b = Some(b_id);
        }
        self.current_region = Some(region_id);
        self.maybe_swap_region_endpoints(region_id);
        self.resort();
        self.dirty = true;
        self.revision += 1;
        Ok(region_id)
    }

    /// `SetCue` (009 contracts/plugin-api-v1.md §3): an empty slot
    /// creates a plugin-owned cue; a slot the same owner already holds
    /// moves it; a slot another owner holds is refused (`NotOwner`).
    pub fn set_cue_owned(
        &mut self,
        slot: CueSlot,
        pos: u64,
        owner: Owner,
    ) -> Result<MarkerId, MarkerError> {
        let pos = pos.min(self.len_frames);
        if let Some(existing) = self
            .markers
            .iter_mut()
            .find(|m| matches!(m.kind, MarkerKind::Cue { slot: s } if s == slot))
        {
            if existing.owner != owner {
                return Err(MarkerError::NotOwner);
            }
            existing.position = pos;
            existing.clamped = false;
            let id = existing.id;
            self.resort();
            self.dirty = true;
            self.revision += 1;
            return Ok(id);
        }
        if self.markers.len() >= MAX_MARKERS {
            return Err(MarkerError::LimitReached);
        }
        let id = self.alloc_marker_id();
        self.markers.push(Marker {
            id,
            kind: MarkerKind::Cue { slot },
            position: pos,
            name: String::new(),
            color: PaletteIndex::new(2),
            owner,
            transient: false,
            visible: true,
            clamped: false,
        });
        self.resort();
        self.dirty = true;
        self.revision += 1;
        Ok(id)
    }

    /// The single body behind the host's `set_loop_a`/`set_loop_b` *and*
    /// 012-section-loop-plugin's `markers.set_loop_endpoint` (data-model.md
    /// §2.1, research R2): on `region` (or a fresh one when `None`, I8),
    /// creates the named endpoint (I1) if absent, subject to the
    /// [`MAX_MARKERS`] limit, or moves it if present (I3, I4); a new
    /// marker's owner is `owner`, its `transient` is always `false`
    /// (FR-013), and it is named by [`Self::default_name_for`] like every
    /// other host-created endpoint. Runs the same swap (I3) and
    /// `current_region` (I8) side effects as the host path, so the host's
    /// own `L` can arm a plugin-owned region (Constitution X user
    /// override).
    ///
    /// `region = Some(id)` for a region this call does not already own
    /// (empty) is `NotFound` if `id` names no region on this track; an
    /// owner mismatch is the caller's job to check first (`apply.rs::
    /// require_owner`) — this method never inspects the existing
    /// endpoints' owner.
    ///
    /// ```
    /// use modplayer_audio_source::TrackId;
    /// use modplayer_core::markers::{Owner, TrackMarkers};
    ///
    /// let track = TrackId::new("spotify:track:4uLU6hMCjMI75M1A2tKUQC").unwrap();
    /// let mut markers = TrackMarkers::new(track, 44_100, 44_100 * 180);
    /// // A caller-owned, A-only region (`region = None` allocates one).
    /// let (region, a) = markers
    ///     .set_loop_endpoint_owned(None, true, 44_100 * 10, Owner::Host)
    ///     .unwrap();
    /// assert_eq!(markers.regions()[0].a, Some(a));
    /// assert_eq!(markers.regions()[0].b, None);
    /// // Completing it: `b` created on the same region.
    /// let (region2, _b) = markers
    ///     .set_loop_endpoint_owned(Some(region), false, 44_100 * 20, Owner::Host)
    ///     .unwrap();
    /// assert_eq!(region, region2);
    /// ```
    pub fn set_loop_endpoint_owned(
        &mut self,
        region: Option<RegionId>,
        which_a: bool,
        pos: u64,
        owner: Owner,
    ) -> Result<(RegionId, MarkerId), MarkerError> {
        let pos = pos.min(self.len_frames);
        let region_id = match region {
            Some(id) => {
                if !self.regions.iter().any(|r| r.id == id) {
                    return Err(MarkerError::NotFound);
                }
                id
            }
            None => self.new_loop_region(),
        };
        let existing = self
            .regions
            .iter()
            .find(|r| r.id == region_id)
            .and_then(|r| if which_a { r.a } else { r.b });

        let marker_id = if let Some(id) = existing {
            if let Some(m) = self.marker_mut(id) {
                m.position = pos;
                m.clamped = false;
            }
            id
        } else {
            if self.markers.len() >= MAX_MARKERS {
                return Err(MarkerError::LimitReached);
            }
            let id = self.alloc_marker_id();
            let kind = if which_a {
                MarkerKind::RegionStart { region: region_id }
            } else {
                MarkerKind::RegionEnd { region: region_id }
            };
            let name = self.default_name_for(kind);
            self.markers.push(Marker {
                id,
                kind,
                position: pos,
                name,
                color: PaletteIndex::new(0),
                owner,
                transient: false,
                visible: true,
                clamped: false,
            });
            if let Some(r) = self.region_mut(region_id) {
                if which_a {
                    r.a = Some(id);
                } else {
                    r.b = Some(id);
                }
            }
            id
        };

        self.current_region = Some(region_id);
        self.maybe_swap_region_endpoints(region_id);
        self.resort();
        self.dirty = true;
        self.revision += 1;
        Ok((region_id, marker_id))
    }

    /// On `current_region` (creating one if none: I8); creates the `A`
    /// endpoint (I1) or moves it; I3, I4 (FR-006). Delegates to
    /// [`Self::set_loop_endpoint_owned`] with `Owner::Host` so the host's
    /// own behaviour is byte-for-byte what it was before 012-section-
    /// loop-plugin generalised this body (research R2).
    pub fn set_loop_a(&mut self, pos: u64) -> Result<(RegionId, MarkerId), MarkerError> {
        self.set_loop_endpoint_owned(self.current_region, true, pos, Owner::Host)
    }

    /// As [`Self::set_loop_a`], for the `B` endpoint.
    pub fn set_loop_b(&mut self, pos: u64) -> Result<(RegionId, MarkerId), MarkerError> {
        self.set_loop_endpoint_owned(self.current_region, false, pos, Owner::Host)
    }

    /// An empty, incomplete region becomes current; no marker is created
    /// until `set_loop_a`/`set_loop_b` (no limit check here).
    pub fn new_loop_region(&mut self) -> RegionId {
        let id = self.alloc_region_id();
        self.regions.push(LoopRegion {
            id,
            a: None,
            b: None,
            crossfade_ms: DEFAULT_CROSSFADE_MS,
            repeat: RepeatCount::Infinite,
            armed: false,
            wraps: 0,
        });
        self.current_region = Some(id);
        self.dirty = true;
        self.revision += 1;
        id
    }

    /// `I5`, `I1` only when creating (FR-013): moves the existing cue in
    /// `slot`, if any, otherwise creates one.
    pub fn set_cue(&mut self, slot: CueSlot, pos: u64) -> Result<MarkerId, MarkerError> {
        let pos = pos.min(self.len_frames);
        if let Some(existing) = self
            .markers
            .iter_mut()
            .find(|m| matches!(m.kind, MarkerKind::Cue { slot: s } if s == slot))
        {
            existing.position = pos;
            existing.clamped = false;
            let id = existing.id;
            self.resort();
            self.dirty = true;
            self.revision += 1;
            return Ok(id);
        }
        if self.markers.len() >= MAX_MARKERS {
            return Err(MarkerError::LimitReached);
        }
        let id = self.alloc_marker_id();
        let name = self.default_name_for(MarkerKind::Cue { slot });
        self.markers.push(Marker {
            id,
            kind: MarkerKind::Cue { slot },
            position: pos,
            name,
            color: PaletteIndex::new(2),
            owner: Owner::Host,
            transient: false,
            visible: true,
            clamped: false,
        });
        self.resort();
        self.dirty = true;
        self.revision += 1;
        Ok(id)
    }

    /// Drag/nudge commit: `I3`, `I4`, clears `clamped`.
    pub fn move_marker(&mut self, id: MarkerId, pos: u64) -> Result<(), MarkerError> {
        let pos = pos.min(self.len_frames);
        let kind = {
            let marker = self.marker_mut(id).ok_or(MarkerError::NotFound)?;
            marker.position = pos;
            marker.clamped = false;
            marker.kind
        };
        if let MarkerKind::RegionStart { region } | MarkerKind::RegionEnd { region } = kind {
            self.maybe_swap_region_endpoints(region);
        }
        self.resort();
        self.dirty = true;
        self.revision += 1;
        Ok(())
    }

    /// Trim, truncate to `MAX_NAME_CHARS`; empty on a `Point` keeps the
    /// old name (non-`Point` kinds accept an empty name, clearing it back
    /// to "no custom name").
    pub fn rename(&mut self, id: MarkerId, name: &str) -> Result<(), MarkerError> {
        let trimmed = name.trim();
        let marker = self.marker_mut(id).ok_or(MarkerError::NotFound)?;
        if trimmed.is_empty() {
            if !matches!(marker.kind, MarkerKind::Point) {
                marker.name = String::new();
            }
            // A Point's empty rename keeps the old name (no-op).
        } else {
            marker.name = trimmed.chars().take(MAX_NAME_CHARS).collect();
        }
        self.dirty = true;
        self.revision += 1;
        Ok(())
    }

    pub fn recolor(&mut self, id: MarkerId, color: PaletteIndex) -> Result<(), MarkerError> {
        let marker = self.marker_mut(id).ok_or(MarkerError::NotFound)?;
        marker.color = color;
        self.dirty = true;
        self.revision += 1;
        Ok(())
    }

    pub fn cycle_color(&mut self, id: MarkerId) -> Result<(), MarkerError> {
        let marker = self.marker_mut(id).ok_or(MarkerError::NotFound)?;
        marker.color = marker.color.next();
        self.dirty = true;
        self.revision += 1;
        Ok(())
    }

    /// `I9`: deleting a region endpoint clears that side and disarms the
    /// region; deleting the last endpoint removes it, falling
    /// `current_region` back to the most recently created remaining
    /// region (the highest surviving id) or `None`. A cue frees its slot.
    pub fn delete(&mut self, id: MarkerId) -> Result<(), MarkerError> {
        let index = self
            .markers
            .iter()
            .position(|m| m.id == id)
            .ok_or(MarkerError::NotFound)?;
        let kind = self.markers[index].kind;
        self.markers.remove(index);

        if let MarkerKind::RegionStart { region } | MarkerKind::RegionEnd { region } = kind {
            let was_a = matches!(kind, MarkerKind::RegionStart { .. });
            if let Some(r) = self.region_mut(region) {
                if was_a {
                    r.a = None;
                } else {
                    r.b = None;
                }
                r.armed = false;
                let now_empty = r.a.is_none() && r.b.is_none();
                if now_empty {
                    self.regions.retain(|reg| reg.id != region);
                    if self.current_region == Some(region) {
                        self.current_region = self.regions.iter().map(|reg| reg.id).max();
                    }
                }
            }
        }

        self.dirty = true;
        self.revision += 1;
        Ok(())
    }

    /// `I8` only (no engine effect): a region endpoint sets
    /// `current_region`.
    pub fn select_marker(&mut self, id: MarkerId) -> Result<(), MarkerError> {
        let marker = self
            .markers
            .iter()
            .find(|m| m.id == id)
            .ok_or(MarkerError::NotFound)?;
        if let MarkerKind::RegionStart { region } | MarkerKind::RegionEnd { region } = marker.kind {
            self.current_region = Some(region);
        }
        self.dirty = true;
        self.revision += 1;
        Ok(())
    }

    /// `I6`, `I7` (FR-007/FR-008): disarms any other region, resets
    /// `wraps`. `RegionIncomplete`/`RegionTooShort` per the region's
    /// `is_armable`.
    pub fn arm(&mut self, region: RegionId) -> Result<(), MarkerError> {
        let (a, b) = {
            let r = self
                .regions
                .iter()
                .find(|r| r.id == region)
                .ok_or(MarkerError::NotFound)?;
            (r.a, r.b)
        };
        let a = a.ok_or(MarkerError::RegionIncomplete)?;
        let b = b.ok_or(MarkerError::RegionIncomplete)?;
        let pos_a = self.position_of(a).ok_or(MarkerError::NotFound)?;
        let pos_b = self.position_of(b).ok_or(MarkerError::NotFound)?;
        if pos_b.saturating_sub(pos_a) < min_region_frames(self.sample_rate) {
            return Err(MarkerError::RegionTooShort);
        }
        for r in &mut self.regions {
            if r.id == region {
                r.armed = true;
                r.wraps = 0;
            } else {
                r.armed = false;
            }
        }
        self.current_region = Some(region);
        self.dirty = true;
        self.revision += 1;
        Ok(())
    }

    /// L7b: disarm the armed region only if `owner` owns it (its `a`
    /// endpoint's owner — both endpoints of a plugin-created region share
    /// the same owner); a no-op (and `false`) otherwise.
    pub fn disarm_if_owned_by(&mut self, owner: Owner) -> bool {
        if self.armed_region_owner() == Some(owner) {
            self.disarm();
            true
        } else {
            false
        }
    }

    /// Disarms whichever region is armed, if any (a no-op otherwise).
    pub fn disarm(&mut self) {
        for r in &mut self.regions {
            r.armed = false;
        }
        self.dirty = true;
        self.revision += 1;
    }

    /// Arm `current_region` if it is disarmed, else disarm it.
    pub fn toggle_current(&mut self) -> Result<(), MarkerError> {
        let region = self.current_region.ok_or(MarkerError::RegionIncomplete)?;
        let armed_now = self
            .regions
            .iter()
            .find(|r| r.id == region)
            .map(|r| r.armed)
            .unwrap_or(false);
        if armed_now {
            self.disarm();
            Ok(())
        } else {
            self.arm(region)
        }
    }

    pub fn set_crossfade_ms(&mut self, region: RegionId, ms: u8) -> Result<(), MarkerError> {
        let r = self.region_mut(region).ok_or(MarkerError::NotFound)?;
        r.crossfade_ms = ms.min(50);
        self.dirty = true;
        self.revision += 1;
        Ok(())
    }

    pub fn set_repeat(&mut self, region: RegionId, repeat: RepeatCount) -> Result<(), MarkerError> {
        let r = self.region_mut(region).ok_or(MarkerError::NotFound)?;
        r.repeat = match repeat {
            RepeatCount::Infinite => RepeatCount::Infinite,
            RepeatCount::Times(n) => RepeatCount::times_clamped(n),
        };
        self.dirty = true;
        self.revision += 1;
        Ok(())
    }

    /// Empties everything; `current_region = None`.
    pub fn clear_all(&mut self) {
        self.markers.clear();
        self.regions.clear();
        self.current_region = None;
        self.dirty = true;
        self.revision += 1;
    }

    /// Re-clamps every marker to the new length, flagging `clamped` on
    /// any that were beyond it (FR-018).
    pub fn set_len_frames(&mut self, len: u64) {
        self.len_frames = len;
        for m in &mut self.markers {
            if m.position > len {
                m.position = len;
                m.clamped = true;
            }
        }
        self.dirty = true;
        self.revision += 1;
    }

    // -- Queries -----------------------------------------------------------

    pub fn markers(&self) -> &[Marker] {
        &self.markers
    }

    pub fn regions(&self) -> &[LoopRegion] {
        &self.regions
    }

    pub fn region(&self, id: RegionId) -> Option<&LoopRegion> {
        self.regions.iter().find(|r| r.id == id)
    }

    pub fn marker(&self, id: MarkerId) -> Option<&Marker> {
        self.markers.iter().find(|m| m.id == id)
    }

    pub fn cue(&self, slot: CueSlot) -> Option<&Marker> {
        self.markers
            .iter()
            .find(|m| matches!(m.kind, MarkerKind::Cue { slot: s } if s == slot))
    }

    pub fn current_region(&self) -> Option<RegionId> {
        self.current_region
    }

    pub fn armed_region(&self) -> Option<&LoopRegion> {
        self.regions.iter().find(|r| r.armed)
    }

    pub fn count(&self) -> usize {
        self.markers.len()
    }

    pub fn position_of(&self, id: MarkerId) -> Option<u64> {
        self.marker(id).map(|m| m.position)
    }

    /// FR-024/C2: who owns `id`, or `None` if it does not exist
    /// (`not_found` is the caller's job — existence and ownership are
    /// deliberately separate checks).
    pub fn owner_of(&self, id: MarkerId) -> Option<Owner> {
        self.marker(id).map(|m| m.owner)
    }

    /// The owner of the currently armed region (its `a` endpoint), if
    /// any is armed.
    pub fn armed_region_owner(&self) -> Option<Owner> {
        let region = self.armed_region()?;
        region.a.and_then(|id| self.owner_of(id))
    }

    /// L7c: remove every transient marker/region-endpoint owned by
    /// `owner` (a plugin's unload/disable/suspend teardown, or a track
    /// change). Reuses [`Self::delete`]'s own region-cleanup rule, so a
    /// transient region loses both endpoints atomically.
    pub fn remove_transient_owned_by(&mut self, owner: Owner) -> usize {
        let ids: Vec<MarkerId> = self
            .markers
            .iter()
            .filter(|m| m.transient && m.owner == owner)
            .map(|m| m.id)
            .collect();
        let count = ids.len();
        for id in ids {
            let _ = self.delete(id);
        }
        count
    }
}
