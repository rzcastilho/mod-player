# Contract: Skeletons and Scroll Restore (025)

Surfaces: `widgets/skeleton.rs`, `library_view::show` loading branch, `detail_view::show` loading branches; `SectionMemory` (unchanged) as used by `App::show_library`. Types: [data-model.md §4, §6](../data-model.md). Test home: `tests/library_view.rs`, `tests/section_memory.rs`, unit tests in `skeleton.rs`.

## Skeletons

| ID | Requirement | Verified by |
|----|-------------|-------------|
| K1 | Library loading: 3 skeletons of `LibraryTab::skeleton_shape().height` — 56 for Saved Tracks / Recently Played, 72 for Saved Albums / Followed Artists / Playlists. | FR-010, SC-004, US4-AS1 |
| K2 | For each tab and the detail track list, skeleton rect height == loaded `list_row` rect height for that kind (0 px diff), and the first loaded row's y == first skeleton's y (no reflow). | FR-010, SC-004 |
| K3 | Skeleton paints an artwork square of `ARTWORK_SIZE` at the row's artwork x plus `text_lines` bars; all fills from `faint_bg_color`. | FR-010 |
| K4 | Each skeleton exposes one `Role::Status` named `loading`; no `ListItem`; never selectable (existing test stays green). | FR-012 |
| K5 | Detail header skeleton: rect height == `HEADER_HEIGHT` (== loaded header, H1); 128 px square, title bar, facts bar; Back button live and functional. | FR-010, US4-AS3 |
| K6 | Empty states unchanged: each `library-empty*` sentence + exactly one primary action (Search/Create/Retry as today); `playlist-no-tracks` unchanged. Existing empty-state tests pass unmodified. | FR-011, SC-006, US4-AS2 |

## Scroll restore

| ID | Requirement | Verified by |
|----|-------------|-------------|
| S1 | Library tab (large fixture) scrolled to offset y → open a detail → Back → next frame the tab's scroll offset == y and first visible row index unchanged. | FR-006, SC-002, US2-AS1 |
| S2 | Active tab preserved: open detail from Playlists tab → Back → Playlists tab shown (not Saved Tracks). | FR-006, edge case |
| S3 | Content shrank while away (fewer items than needed for y): restored offset == `max_scroll` for the new content, never beyond. | FR-006, US2-AS3 |
| S4 | Width changed while away (e.g. 1400 → 960): first visible row index unchanged (fixed row heights). | FR-006, edge case |
| S5 | Restore applied once: user scrolling on the frame after restore is not overridden. (Existing 020 behaviour, regression-guarded for the Tab key.) | 020 contract |
| S6 | Nothing written to disk; sign-out resets offsets (existing 020 M4/M6 tests stay green). | FR-006, CON VI |
