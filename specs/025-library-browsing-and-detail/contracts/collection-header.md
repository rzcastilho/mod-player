# Contract: Collection Header (025)

Surface: `detail_view::show` → `collection_header` / `header_skeleton` (`crates/modplayer-ui/src/detail_view.rs`). Types: [data-model.md §1–§3](../data-model.md). Test home: `tests/library_view.rs`, `tests/accessibility.rs`, `tests/high_contrast.rs`, unit tests in `detail_view.rs`.

| ID | Requirement | Verified by |
|----|-------------|-------------|
| H1 | Header rect height == `HEADER_HEIGHT` for loaded, skeleton, and 300-char-title states; width-independent (test at 560 and 1400 px list width). | FR-001, FR-010 — geometry of header group node |
| H2 | Artwork square is `DETAIL_ARTWORK_SIZE` (128) × 128; on `MODPLAYER_ARTWORK_FORCE_FAIL`/no URL shows the initials placeholder; decorative (no separate accesskit node). | FR-001, SC-001 |
| H3 | Title drawn in `text::DISPLAY`, single line, truncated with "…"; full title = header group accessible name and hover text. | FR-001 |
| H4 | Exactly one facts label, `text::SECONDARY`, content == `facts_line(header_facts(..))` (see data-model §2). Playlist owner shown for editable and non-editable playlists. | FR-002, US1-AS1/AS2 |
| H5 | Missing fact (no year, no artists, list `Loading`/`Failed`) omitted; no leading/trailing/double " · ". Proptest over all field subsets. | FR-002, US1-AS5 |
| H6 | Runtime: `format_runtime` table (data-model §3) incl. 59 999 ms → "1 min", 3 600 000 → "1 hr 0 min", u32::MAX-sum lists do not overflow. | FR-002 |
| H7 | Play button: `Variant::Primary`, accessible name `detail-play-name{name}` (visible text `detail-play`); click / Enter / Space dispatches `RowAction::PlayNow` for the collection entity through `apply_row_action` → queue == same tracks & cursor 0 as a row's Play now on that entity. | FR-003, US1-AS3 |
| H8 | `track_count == 0`: Play rendered, disabled (accesskit `disabled`), description + tooltip `detail-no-tracks-hint`; "…" enabled; `playlist-no-tracks` still shown for playlists. | FR-003, edge case |
| H9 | "…" opens the same 6 `Role::MenuItem`s, same order and labels as a row; placeholders raise `coming-soon`; nothing hidden for non-owned playlists. Accessible name `row-actions{name=title}`. | FR-004, US1-AS4 |
| H10 | Back: `Variant::Quiet`, visible label `detail-back`, top-left of header (its rect min.x/min.y ≤ every other header child), first focusable in the header (Tab from page start reaches Back before Play). Click, Backspace, Alt+Left → `DetailOutcome::Back`. Back usable while header is a skeleton. | FR-005, US2-AS2 |
| H11 | Header is an accesskit `Group` named by the title; facts exposed as a text label; Play/"…"/Back expose name, role, disabled state. | FR-012, NFR-6.1/6.2 |
| H12 | All header paints use `theme::roles`; in each of the 4 appearances (light, dark, light-HC, dark-HC) title and facts text meet the 017 contrast checks. | FR-012 |

Out of scope: detail opened from Search (never happens), library sorting, offline-pin badges (FR-013).
