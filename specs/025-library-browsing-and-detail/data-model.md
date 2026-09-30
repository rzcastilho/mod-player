# Data Model: Library Browsing and Detail View (025)

UI-local types only (`crates/modplayer-ui`). Nothing is persisted; no core/catalog type changes. Source types read: `AlbumRef`, `PlaylistRef`, `ArtistRef`, `TrackRef` (`modplayer-audio-source::catalog`), `TrackListState` (`modplayer-core`).

## 1. Collection Header (presentation, `detail_view.rs`)

| Field | Source | Rule |
|-------|--------|------|
| back | `tr("detail-back")` | Quiet button, top-left, first in tab order; live in skeleton too |
| artwork | `artwork_url` of the ref; fallback initials of `name` | `DETAIL_ARTWORK_SIZE = 128.0` square; `ArtworkCache` (same as rows) |
| title | ref `name` | `text::DISPLAY`, 1 line, truncated; = group accessible name + hover tooltip |
| facts | `HeaderFacts` (§2) | `text::SECONDARY`, `text_secondary` role, 1 line, truncated |
| play | `RowAction::PlayNow` on `RowEntity::{Album,Playlist,Artist}` | `Variant::Primary`; disabled iff `track_count == 0` (§3) |
| more | `rows::actions_menu` | same six `RowAction::ORDER` items |

Constants: `DETAIL_ARTWORK_SIZE: f32 = 128.0` (≥ 3 × `ARTWORK_SIZE` 40). `HEADER_HEIGHT` derived = `interact_size.y + space::SM + DETAIL_ARTWORK_SIZE` (research R1), identical for skeleton and loaded.

**States**:

```text
ref missing (library not synced for id) ──► HeaderSkeleton (Back live)
ref present, list Loading/Failed        ──► Header, facts without runtime / top-track count
ref present, list Cached                ──► Header, full facts
```

`HeaderEvent` (returned to `detail_view::show`): `None | Back | Action(RowAction)`. `Back` merges with Backspace / Alt+Left into `DetailOutcome::Back`; `Action(a)` → `apply_row_action(controller, &entity, &tracks_or_empty, a)`.

## 2. HeaderFacts (pure)

```text
enum FactsInput<'a> {
    Playlist { owner: &str, track_count: u32, tracks: Option<&[TrackRef]> },
    Album    { artists: &[String], year: Option<u16>, track_count: u32, tracks: Option<&[TrackRef]> },
    Artist   { top_tracks: Option<&[TrackRef]> },
}
fn header_facts(input: FactsInput) -> Vec<String>   // present fields only, in order
fn facts_line(facts: &[String]) -> String            // join with FACTS_SEPARATOR " · "
```

`tracks`/`top_tracks` = `Some` only for `TrackListState::Cached`.

| Kind | Ordered fields (omitted when absent) |
|------|--------------------------------------|
| Playlist | `detail-owner{name}` · `playlist-track-count{count}` · runtime |
| Album | artists joined ", " (omit if empty) · year · `playlist-track-count{count}` · runtime |
| Artist | `detail-kind-artist` · `detail-top-track-count{count = len}` |

**Validation**: result contains no empty strings; `facts_line` never starts/ends with the separator nor contains it twice consecutively (proptest over all subsets). Owner shown for every playlist regardless of `editable`.

## 3. Runtime

```text
fn total_runtime_ms(tracks: &[TrackRef]) -> u64            // Σ u64::from(duration_ms), incl. unavailable
fn format_runtime(total_ms: u64, non_empty: bool) -> Option<String>
```

| Input | Output |
|-------|--------|
| empty list | `None` (field omitted) |
| 0 < total < 60 000 ms | `detail-runtime-minutes{minutes=1}` ("1 min") |
| < 1 h | `detail-runtime-minutes{minutes=floor(total/60 000)}` |
| ≥ 1 h | `detail-runtime-hours{hours, minutes = mins % 60}` ("H hr M min") |

## 4. SkeletonShape (`widgets/skeleton.rs`)

```text
struct SkeletonShape { height: f32, text_lines: u8 }
const TRACK:    SkeletonShape = { ROW_HEIGHT 56,      2 }
const ALBUM:    SkeletonShape = { WIDE_ROW_HEIGHT 72, 2 }
const PLAYLIST: SkeletonShape = { WIDE_ROW_HEIGHT 72, 2 }
const ARTIST:   SkeletonShape = { WIDE_ROW_HEIGHT 72, 1 }
```

`LibraryTab::skeleton_shape()`: SavedTracks/RecentlyPlayed → TRACK; SavedAlbums → ALBUM; FollowedArtists → ARTIST; Playlists → PLAYLIST. Detail track list → TRACK. Count stays 3. Each skeleton = one `Role::Status` "loading" node, no `ListItem`.

`header_skeleton(ui) -> bool /*back clicked*/`: rect height == `HEADER_HEIGHT`.

## 5. Row actions menu navigation (`rows.rs`)

```text
struct MenuNav { focused: Option<usize>, return_to: egui::Id }   // egui temp memory, keyed by menu_id
fn step(focused: Option<usize>, key: NavKey, len: usize) -> usize
  Down: None→0, i→(i+1) mod len;  Up: None→len-1, i→(i+len-1) mod len
  Home→0;  End→len-1
```

Lifecycle: opened (row Shift+F10 / Menu key / secondary click → `return_to = row id`; opener click or Enter/Space on opener → `return_to = opener id`; header → header opener id) → navigating → closed (activate or Escape/click-away) → `request_focus(return_to)`; `MenuNav` removed from memory.

## 6. Library View Position (existing, unchanged)

`SectionMemory.offsets[ViewKey::Library(LibraryViewKey::Tab(tab))] : f32` + `LibraryViewState.tab`. Re-applied once when the tab is re-shown after the detail (research R3); egui clamps to `[0, max_scroll]`. Reset on sign-out (020 M4). Never on disk.
