# Contract: UI surface — Search, Library, detail views, rows (`modplayer-ui`)

**Modules**: `search_view.rs`, `library_view.rs`, `detail_view.rs`, `rows.rs`, `artwork.rs`, `widgets/skeleton.rs`, `widgets/initials.rs` (new); `shell.rs`, `app.rs`, `settings/developer.rs`, `settings/mod.rs` (modified)
**Locale**: `locales/en-US/library.ftl` (new), `app.ftl`/`playback.ftl`/`settings.ftl` (modified)
**Research**: R8, R9, R12
**Extends**: 003 `contracts/ui-surface.md` — the Library section stops being a placeholder; Now Playing/Queue unchanged.

## 1. Shell

`Section::Library` renders `library_view` (was `library_placeholder`);
`Section::Search` is **added** to the rail between Library and Now Playing
(`Ctrl/Cmd+1..5`; `Ctrl/Cmd+F`/`/` from anywhere focuses the search box).
`placeholder-library` key removed.

## 2. Search view (`search_view.rs`)

| Element | Role / a11y name | Behaviour |
|---|---|---|
| Search box | `TextInput`, "Search" | edits `SearchSession.raw_query`; Esc clears; never blocks |
| Offline state | `Label` | "Search needs a connection — you're offline" when `connectivity == Offline` (FR-018) |
| Group header ×4 | `Header`, "Tracks"/"Albums"/"Artists"/"Playlists" | fixed order; omitted when `Empty`/`Unsupported`/`Idle` |
| Group list | virtualised `show_rows` | 20 per page; skeleton rows while `Pending` |
| Show more | `Button`, "Show more <group>" | present when `next_offset.is_some()`; in-place spinner on the button only |
| No-results state | `Label` | "No results for “<query>” — check your spelling, or you might be offline." only when all four are `Empty` |
| Refreshing indicator | `Label`, "Refreshing…" (`aria-live` polite via `Role::Status`) | while any group `RateLimited`; stale items stay |

## 3. Library view (`library_view.rs`)

Tabs (`Role::Tab`, in order): **Saved Tracks**, **Saved Albums**,
**Followed Artists**, **Playlists**, **Recently Played**. Opens on Saved
Tracks. No "Pinned for offline" tab.

| State | Rendering |
|---|---|
| `loading` (index not yet read from disk) | skeleton rows in the active tab |
| `first_sync_failed && index empty` | "Couldn't load your library — check your connection" + **Retry** (`library_retry_sync`) — FR-021 |
| all sections empty (known) | "Your library is empty — search to add music" + **Search** (focuses search box) |
| section empty | per-section copy (spec Clarifications) + **Search**; Playlists: "No playlists yet — create one" + **Create** (placeholder ⇒ `coming-soon`) |
| `refreshing` | "Refreshing…" status label in the tab header; content unchanged |
| offline with snapshot | snapshot renders normally, no error |

Rows are virtualised; visible-range ids are passed to
`library_hydrate_visible`. Un-hydrated ids render skeleton rows.

Playlist rows show "Owner: <name>" only when `!editable`; no
rename/reorder/edit control exists anywhere (FR-010).

## 4. Detail views (`detail_view.rs`)

Opened by activating an album/playlist/artist row's **Open** (or Enter on
non-track rows — Enter is **Play now** only on track rows; on album /
artist / playlist rows Enter opens the detail, Play now is in the menu).

| Detail | Header | List |
|---|---|---|
| Album | artwork, name, artists, year | tracks in album order |
| Playlist | artwork, name, owner label (when not owned), count | tracks in playlist order; "This playlist has no tracks" when empty |
| Artist | artwork, name | up to 10 top tracks (all greyed rows still listed if all unavailable) |

Back: `Button` "Back" + `Alt+Left`/`Backspace` when the list has focus.

## 5. Row widget (`rows.rs`) — identical everywhere (FR-004)

| Row | Content |
|---|---|
| Track | artwork 40 px, title, artists, album, `m:ss`, "E" badge (name "Explicit"), availability reason text when greyed |
| Album | artwork, name, artists, release year |
| Artist | artwork, name |
| Playlist | artwork, name, "Owner: <name>" when not owned |

Accessibility: the row is one focusable `ListItem` whose name is
"<title> — <artists>[ — Unavailable in your region | Removed from the service]";
**Enter**/double-click ⇒ Play now (track rows); **Menu** key / `Shift+F10`
/ right-click / the trailing "…" `Button` ("Actions for <name>") opens a
`Menu` with the six `MenuItem`s in fixed order: Play now, Play next, Add to
queue, Add to playlist, Save to library, Pin for offline — all enabled on
every row, including unavailable ones (FR-019). The three placeholders
raise `coming-soon` (Info) and do nothing else (FR-005).

## 6. Artwork (`artwork.rs`, `widgets/initials.rs`)

`ArtworkCache::get(url) -> ArtworkState { Ready(TextureId) | Loading | Failed }`;
`Loading` draws a neutral square, `Failed` (or no URL) draws the initials
placeholder: `initials(name)` → up to two uppercase letters (first letter of
first two words) or the music glyph `♪` when the name has no
letter/digit. Name source: album title (tracks/albums), artist name,
playlist name (FR-020). Only rows in the visible range request artwork.

## 7. Skeleton rows (`widgets/skeleton.rs`)

A row-shaped shimmer with `Role::Status` name "Loading" — the **only**
loading affordance in this slice; no full-view spinner exists (FR-016).

## 8. Settings › Developer (modified)

"Play from account" button, its `PlayFromAccountState`, the
`developer.play_from_account` descriptor and its `.ftl` keys are removed
(FR-022). Buffer readout and the three notification buttons stay.

## 9. Fluent keys (`library.ftl`, all new; en-US only)

`nav-search`, `search-placeholder`, `search-offline`, `search-no-results`
(`{ $query }`), `search-group-tracks|albums|artists|playlists`,
`search-show-more` (`{ $group }`), `refreshing`, `library-tab-*` ×5,
`library-empty`, `library-empty-albums`, `library-empty-artists`,
`library-empty-playlists`, `library-empty-recent`, `library-first-sync-failed`,
`library-retry`, `action-search`, `action-create`, `playlist-owner`
(`{ $name }`), `playlist-no-tracks`, `row-explicit`, `row-actions`
(`{ $name }`), `row-unavailable-region`, `row-removed`, `action-play-now`,
`action-play-next`, `action-add-to-queue`, `action-add-to-playlist`,
`action-save-to-library`, `action-pin-offline`, `coming-soon`,
`detail-back`, `loading`.

## 10. Tests pinning this contract (`modplayer-ui`)

- `tests/accessibility.rs` (extended): every element in §2–§5 has role +
  name; row menu order; Enter/menu-key behaviour; placeholders inert
  (`ScriptedHost` records no command; `coming-soon` raised).
- `tests/search_view.rs`: skeleton while pending, group omission, no-results
  vs offline state, refreshing keeps stale rows, show-more paging, first
  group painted ≤ 300 ms with a scripted 100 ms reply (SC-001 proxy).
- `tests/library_view.rs`: tab order/initial tab, each empty state + its
  action, FR-021 state, owner label, no edit controls, offline snapshot,
  virtualisation (50 000 × 1 000 fixture: frame ≤ 8 ms in `cargo test
  --release` proxy for SC-002).
- `tests/rows.rs`: acting-list rule per origin, cursor placement,
  unavailable row greyed + reason in name + actions enabled, initials
  rule, artwork failure ⇒ initials.
- `tests/fluent_keys.rs` (extended): every key above exists; scaffold keys
  gone.
- `tests/developer_removed.rs`: no "Play from account" control renders.
