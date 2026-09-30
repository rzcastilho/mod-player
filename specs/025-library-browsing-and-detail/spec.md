# Feature Specification: Library Browsing and Detail View

**Feature Branch**: `feature/025-library-browsing-and-detail`

**Created**: 2026-09-30

**Status**: Draft

**Input**: User description: "Implement the feature specified in specs/autonomous/breakdown/011-browse-and-manage-polish/001-library-browsing-and-detail.md (id 001, library-browsing-and-detail)." Source: UI/UX Review § 3.3 Library (UX-14, UX-17), § 5.4 Component rules, § 4.5 Feedback and affordance. Prerequisites: 016-list-row-and-panel-components (shared row and tab components); 004-search-and-library-browse (library and detail views).

## Clarifications

### Session 2026-09-30 (clarify reviewer)

Sources: breakdown `011-browse-and-manage-polish/001-library-browsing-and-detail.md` (BD), constitution v1.1.1 (CON), existing code in `crates/modplayer-ui` (CODE). Items marked **default** were not settled by a source; the conventional choice was taken.

- Q: What facts appear in each kind's facts line, and in what order? → A (BD + CODE + **default** for kind-specific parts): one line, fields joined by a " · " separator, empty fields dropped with no leading/trailing/double separator. **Playlist**: owner ("by <owner_name>", shown for *every* playlist, editable or not — BD lists owner as a fact; the current `!editable`-only owner line is retired) · track count · total running time. **Album**: artists (comma-joined) · release year · track count · total running time. **Artist**: the kind label "Artist" · number of top tracks shown · total running time is **not** shown (top-tracks runtime is not a meaningful artist fact). All strings are Fluent keys with en + pt-BR (CON X).
- Q: Where does "total running time" come from? → A (CODE + **default**): `PlaylistRef`/`AlbumRef` carry no duration, so it is the sum of `duration_ms` over the loaded track list (`TrackListState::Cached`), including unavailable tracks. While the list is `Loading`/`Failed` the running-time field is omitted (no placeholder, no stray separator). Track count comes from the ref's `track_count` (available before the list loads). Format: under 1 hour "N min"; otherwise "H hr M min" (minutes floored; "0 min" never shown for a non-empty list — lists totalling < 1 min show "1 min"); pt-BR via Fluent.
- Q: Header artwork size? → A (**default**): a single named constant `DETAIL_ARTWORK_SIZE = 128.0` px square (≥ 3× the 40 px row artwork `ARTWORK_SIZE`), same `ArtworkCache` fetch and same initials placeholder fallback as rows. Header content height is fixed to the artwork size so the header never changes height between skeleton, loaded, or long-title states.
- Q: Title style and long titles? → A (CODE + **default**): title uses `theme::text::DISPLAY` (014), single line, truncated with an ellipsis; the full title is the header's accessible name and hover tooltip. Facts line uses `theme::text::SECONDARY`, single line, truncated. Neither ever pushes the play action or back control out of the header at 960 px.
- Q: What does the header's action area contain? → A (CODE + **default**): a **Primary**-variant "Play" button (015 `Variant::Primary`) followed by the same "…" six-action menu rows use (`RowAction::ORDER`, identical items, order and behaviour, including placeholder "coming soon" feedback for Add to playlist / Save to library / Pin for offline). Header "Play" dispatches exactly `RowAction::PlayNow` on the collection entity through `apply_row_action`. No actions are hidden or disabled by ownership — today every row shows all six for every playlist; that stays.
- Q: Zero-track collection? → A (**default**): header renders fully; "Play" is disabled (not hidden) with accessible description/tooltip "This collection has no tracks" (Fluent key); the "…" menu stays enabled (unchanged behaviour). Existing `playlist-no-tracks` message stays; albums/artists with zero tracks show no additional message (unchanged).
- Q: Back control placement, label, keys? → A (CODE + BD): the existing `detail-back` text button moves into the header's top-left, above/before the artwork, as a Quiet-variant button with visible text label (e.g. "‹ Library"; icon alone forbidden). Existing shortcuts Backspace and Alt+Left keep working; the button is in the tab order before the Play button.
- Q: How is library scroll position restored? → A (CODE): reuse `SectionMemory` (020) — the active library tab's `ViewKey::Library(LibraryViewKey::Tab(tab))` offset is recorded while the tab is shown and re-applied once when the tab is shown again after Back; no new persistence (never to disk, CON/020 M6). Active tab is `LibraryViewState.tab`, which is untouched while the detail is open, so the section is restored too. The stored offset is clamped to `[0, max_scroll]` for the current content. Because list rows have fixed per-kind heights (56 / 72 px) that do not depend on window width, a restored pixel offset lands on the same first visible item after a width-only resize; "same item region" therefore means same first-visible row index when the list content is unchanged.
- Q: Detail opened from Search? → A (CODE): Search does not open detail views in the current product (`search_view.rs` ignores `RowEvent::Open`). The "from search" edge case is out of scope; Back always returns to the Library tab that opened the detail.
- Q: What is "minimum width"? → A (CODE + 018): the window at its minimum inner size, 960 × 640 px (`main.rs` `with_min_inner_size`), with every dock panel that 018 allows open at that width in its default/expanded state — i.e. the narrowest list area the app can produce. The overflow "…" control must lie fully within its row rect there (existing `ACTIONS_RESERVED_WIDTH` + `content_column_width` contract), and its popup anchors to that button.
- Q: What does "quiet styling" mean for the "…" control? → A (015 + **default**): the "…" opener uses 015 `Variant::Quiet` (transparent fill, no outline, `text_primary` label) and is **always visible** (not hover-revealed — hover-only controls fail keyboard/touch discoverability, CON X NFR-6.1); hover and focus use 015's standard Quiet interaction states and focus ring.
- Q: Keyboard path for row actions? → A (CODE + **default**, standard menu-button convention): Tab reaches the row; from a focused row Shift+F10 (existing) or the Menu/Context key opens the menu; Tab from the row also reaches the "…" button where Enter/Space opens it. In the open menu Up/Down move between the six items (wrapping), Home/End jump to first/last, Enter/Space activates, Escape dismisses. On activate or dismiss, focus returns to the row that opened the menu. The same applies to the header "…" menu (focus returns to its "…" button).
- Q: Skeleton shape/count? → A (CODE + BD): a skeleton row uses the height of the row kind it replaces — `ROW_HEIGHT` (56) for track lists (Saved Tracks, Recently Played, detail track lists) and `WIDE_ROW_HEIGHT` (72) for Saved Albums, Followed Artists, Playlists (today the library loading state wrongly uses 56 px for every tab). Skeleton shape: artwork square at `ARTWORK_SIZE` plus one bar per text line of that kind, inside the same row rect. Skeleton count stays as today (3); only per-row height/shape must match. Detail-header skeleton: back control rendered live (usable during load), artwork square at `DETAIL_ARTWORK_SIZE`, title bar and facts bar, occupying exactly the loaded header's height.
- Q: Empty states? → A (CODE): unchanged copy (`library-empty*` keys, `playlist-no-tracks`) and their existing single action (Search / Retry). This feature adds no second action to any empty state.
- Q: Accessibility of the header? → A (CON X + **default**): header is a group whose accessible name is the collection title; artwork is decorative (no separate node, as in rows); facts line is exposed as text; Play button name "Play <title>"; "…" named via existing `row-actions` key with the title. All header colours come from theme roles so high-contrast (017) needs no special casing; header is included in the high-contrast legibility test.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Detail page that looks like what it describes (Priority: P1)

A user opens an album, playlist or artist from the library. The page opens with a header showing the artwork at a generous size, the title in the display style, and one secondary line of supporting facts (owner, track count, total running time). The header carries the primary "play from the start" action alongside the collection-level actions already offered in the overflow menu.

**Why this priority**: The detail page is the destination of every browse; a flat stack of equal-weight lines is the main visible weakness this feature fixes, and Play-from-header is the most common intent on arrival.

**Independent Test**: Open a playlist with known owner, track count and duration; verify header contents and that the header play action starts the collection from its first track.

**Acceptance Scenarios**:

1. **Given** a playlist, **When** it is opened, **Then** the header shows its artwork, its title in the display style, a single line of facts (owner, track count, total running time), and a play action.
2. **Given** an album or artist page, **When** it is opened, **Then** the header follows the same structure with facts appropriate to that kind (no equal-weight stacked fact lines).
3. **Given** the header is shown, **When** the user activates the play action, **Then** the collection plays from its first track, using the same play behaviour as the row-level "Play now" on a collection.
4. **Given** the header is shown, **When** the user opens its collection-level actions, **Then** they offer every collection action the overflow menu already offers, with unchanged behaviour.
5. **Given** a collection with a missing fact (e.g. no release year, running time not yet known because the track list is still loading) or missing artwork, **When** the header renders, **Then** the missing fact is omitted from the line without leaving stray separators, and artwork falls back to the existing initials placeholder.

---

### User Story 2 - Back to the library where I left off (Priority: P1)

From a detail page, a clearly labelled back control in the header returns the user to the library, restored to the scroll position (and section) it had before the detail page was opened.

**Why this priority**: Losing one's place in a long library after every detail visit makes browsing painful; equal importance to the header as they form one browse loop.

**Independent Test**: Scroll a long library list, open an item, use back, and confirm the same item region is in view.

**Acceptance Scenarios**:

1. **Given** the library scrolled partway down, **When** the user opens a detail page and then uses the back control, **Then** the library is shown at the scroll position it had.
2. **Given** a detail page, **When** it is displayed, **Then** the back control is visible at the header's top-left with a text label (not icon alone), is reachable by Tab before the Play button, and Backspace / Alt+Left also go back.
3. **Given** the library content changed while away (e.g. a sync added items), **When** the user goes back, **Then** the position is restored as closely as possible and never beyond the end of the list.

---

### User Story 3 - Row actions stay with their row, quietly, by keyboard (Priority: P2)

In every list, each row's overflow control stays inside its own row at every window width, including the minimum width. The six row actions keep their existing behaviour but adopt quiet styling and can be reached and operated with the keyboard alone.

**Why this priority**: Fixes a layout defect (overflow control detached from its row at narrow widths) and an accessibility gap, but lists remain usable without it.

**Independent Test**: Shrink the window to minimum width, confirm every row's overflow control sits within that row; then reach, open and operate all six actions via keyboard only.

**Acceptance Scenarios**:

1. **Given** the window at its minimum width, **When** a list is shown, **Then** each row's overflow control is fully inside its own row and does not overlap or wrap into a neighbouring row.
2. **Given** any width, **When** the overflow menu opens, **Then** it is anchored to the row that opened it.
3. **Given** a focused row, **When** the user presses Shift+F10 or the Menu key (or Tabs to the "…" button and presses Enter/Space), **Then** the menu opens; Up/Down (wrapping) and Home/End move between the six actions, Enter/Space activates, Escape dismisses, and focus returns to the row.
4. **Given** the six actions, **When** any is activated, **Then** it behaves exactly as before this feature (functional actions act, placeholder actions show their existing placeholder feedback).
5. **Given** rows at rest, **When** displayed, **Then** the overflow control is always visible in the Quiet variant (transparent fill, no outline) and shows the Quiet variant's hover state and the standard focus ring on hover/focus.

---

### User Story 4 - Stable loading and meaningful empty states (Priority: P3)

While a list loads, skeleton rows have the same height and shape as the loaded rows, so nothing shifts when data arrives. Existing empty states keep their explanatory sentence and exactly one primary action.

**Why this priority**: Polish that prevents visual jumps; existing behaviour is retained.

**Independent Test**: Load a list with throttled data, compare skeleton row height with loaded row height; trigger each empty state and confirm sentence and single primary action.

**Acceptance Scenarios**:

1. **Given** a list is loading, **When** skeleton rows are shown, **Then** each occupies the same height as a loaded row of that list (56 px for track lists, 72 px for Saved Albums / Followed Artists / Playlists), and the list does not reflow when real rows replace them.
2. **Given** an empty list, **When** its empty state is shown, **Then** it displays its existing explanatory sentence and a single primary action (unchanged copy and action).
3. **Given** a detail page loading, **When** the header and track list are pending, **Then** the header area and track rows show skeletons matching their loaded shapes.

### Edge Cases

- Playlist/album/artist with zero tracks: header still shows; Play is disabled (not hidden) with accessible description "This collection has no tracks"; the "…" menu stays enabled; existing `playlist-no-tracks` message stays.
- Very long titles: title stays on one line, truncated with an ellipsis; full title is the accessible name and tooltip; header height does not change.
- Playlists the user does not own: all six collection actions are shown exactly as on rows today (none hidden or disabled by ownership); the owner appears in the facts line.
- Back navigation when the library section/tab was changed before opening the detail: returns to that same section.
- Window resized between opening a detail page and going back: row heights are width-independent, so restoring the stored offset (clamped to the current max) lands on the same first visible row.
- Detail opened from search: not possible in the current product (Search does not open detail views); out of scope.
- Library content changed while away: stored offset clamped to `[0, max_scroll]`; never scrolls beyond the end.
- Unavailable tracks (greyed rows) keep their appearance and all six actions.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: An album, playlist or artist detail page MUST open with a fixed-height header showing artwork at `DETAIL_ARTWORK_SIZE` (128 px square, initials placeholder fallback), the title in `theme::text::DISPLAY` on one truncated line, and the facts line.
- **FR-002**: The facts MUST appear as one `theme::text::SECONDARY` line joined by " · ": Playlist = owner · track count · running time; Album = artists · release year · track count · running time; Artist = "Artist" · top-track count (length of the loaded top-tracks list, omitted until loaded). Running time is the sum of loaded tracks' `duration_ms` ("N min" / "H hr M min"), omitted until the track list is loaded. Absent facts MUST be omitted with no stray separators.
- **FR-003**: The header MUST carry a Primary-variant Play button that dispatches `RowAction::PlayNow` on the collection entity (identical to a row's Play now); it MUST be disabled with an accessible explanation when the collection has zero tracks.
- **FR-004**: The header MUST carry the same "…" six-action menu rows use (same items, order and behaviour, placeholders included), with no ownership-based hiding.
- **FR-005**: The detail header MUST contain, at its top-left and first in tab order, a Quiet-variant back control with a visible text label that returns to the library tab that opened the detail; Backspace and Alt+Left MUST keep working.
- **FR-006**: Going back MUST restore the library's active tab and its scroll offset via the existing `SectionMemory` (in-session only), clamped to the current content's scroll range.
- **FR-007**: Each row's overflow control MUST remain fully inside its own row rect at every supported window width, including 960 × 640 with all 018 dock panels open, and its menu MUST anchor to that row's "…" button.
- **FR-008**: The six row actions MUST keep their existing behaviour, and MUST be keyboard operable: Shift+F10 / Menu key on a focused row (or Enter/Space on the "…" button) opens; Up/Down (wrapping), Home/End navigate; Enter/Space activates; Escape dismisses; focus then returns to the originating row (or header "…" button).
- **FR-009**: The overflow control MUST be always visible, rendered in 015 `Variant::Quiet`, with the Quiet hover state and standard focus ring on hover/keyboard focus.
- **FR-010**: Loading skeleton rows MUST use the row height of the kind they replace (`ROW_HEIGHT` 56 for track lists, `WIDE_ROW_HEIGHT` 72 for Saved Albums / Followed Artists / Playlists) and show an artwork square plus text bars; count stays 3. The detail header skeleton MUST occupy exactly the loaded header's height, with the back control live during loading.
- **FR-011**: Existing empty states MUST keep their current explanatory sentence and exactly one primary action.
- **FR-012**: All new or changed controls MUST expose an accessible name, role and state (NFR-6.1, NFR-6.2), all new strings MUST be Fluent keys with en and pt-BR (NFR-7.1), and header contents MUST use theme roles only and remain legible in all supported appearances including high contrast (017).
- **FR-013**: Library sorting and offline-pin indicators are out of scope and MUST NOT be altered by this feature.

### Key Entities

- **Collection Header**: Presentation of an album, playlist or artist: back control, artwork (128 px), title (DISPLAY, one line), facts line (per-kind fields per FR-002), Primary Play button, "…" six-action menu. Fixed height.
- **Library View Position**: The existing `SectionMemory` entry for the active `LibraryTab` (offset) plus `LibraryViewState.tab`; no new state type.
- **Row Action Set**: The existing six row actions, unchanged in behaviour, now with quiet styling and a keyboard path.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Opening any album, playlist or artist shows artwork, title, exactly one facts line and a play action in the header, in 100% of cases checked, including collections with missing facts or artwork.
- **SC-002**: After opening a detail page and going back, the library is at its previous position (same first visible item) in 100% of tested cases.
- **SC-003**: At minimum window width, 100% of visible rows have their overflow control fully inside the row bounds.
- **SC-004**: Skeleton row height equals loaded row height (zero pixel difference) for every list, and no visible content shift occurs when data arrives.
- **SC-005**: All six row actions and the header's play, collection actions and back control can be completed without a pointer.
- **SC-006**: Every existing empty state still shows its sentence and one primary action; existing action behaviour shows no regressions.

## Assumptions

- The shared row, tab and panel components from 016 and the library/detail views from 004 already exist and are reused.
- "Display style" and "secondary line" refer to the type-scale tokens established in 014-design-tokens-and-type-scale.
- The minimum window size is 960 × 640 px (018; `main.rs` `with_min_inner_size`).
- Scroll restoration covers in-session navigation only; restoring across app restarts is out of scope.
- Library sorting (007 playback polish) and offline pinning indicators (offline cache) are out of scope.
- The six existing row actions are Play now, Play next, Add to queue, Add to playlist, Save to library, Pin for offline.
