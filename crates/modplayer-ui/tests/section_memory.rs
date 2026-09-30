// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `section_memory::SectionMemory` integration tests (020-shell-navigation-
//! and-gates, US3, contracts/section-memory.md M1-M5): each scrollable
//! view keeps its own vertical offset across a round trip (M1), a Library
//! tab and its detail view — and different Settings categories — don't
//! bleed into each other's offsets (M2), a shrunk view's restored offset
//! clamps rather than jumping past the new content (M3), `reset_session_ui`
//! clears every offset and every listed sub-view (M4), and switching
//! Library *tabs* is treated exactly like switching sections — each tab
//! keeps its own key/offset (M5).
//!
//! Every test drives `SectionMemory::scroll_area`/`record`/`end_frame`
//! directly against a bare `egui::Context`, mirroring `shell_navigation.rs`'s
//! own pattern (no `App`, which needs a real `eframe::CreationContext`,
//! research.md R1). "Scroll to offset *y*" is simulated the way research
//! R10/the contract's own note allows: `record(key, y)` followed by
//! `end_frame(None)` (exactly what `App::ui` does whenever no memory-backed
//! view was on screen a given frame — a gate, Now Playing, another
//! section) — so the *next* draw of `key` sees `shown_last_frame != Some
//! (key)` and genuinely exercises `scroll_area`'s own
//! `.vertical_scroll_offset` restore path, rather than reaching into
//! `SectionMemory`'s private fields.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui::{Context, Pos2, RawInput, Rect, vec2};
use modplayer_audio_io::FakeBackend;
use modplayer_audio_source::AlbumId;
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::PlaybackController;
use modplayer_core::settings::SettingsStore;
use modplayer_core::settings_registry::SettingsCategory;
use modplayer_ui::app::reset_session_ui;
use modplayer_ui::detail_view::DetailTarget;
use modplayer_ui::library_view::{LibraryTab, LibraryViewState};
use modplayer_ui::search_view::SearchViewState;
use modplayer_ui::section_memory::{LibraryViewKey, SectionMemory, ViewKey};
use modplayer_ui::settings::SettingsScreen;
use modplayer_ui::shell::Shell;
use modplayer_ui::sign_in::{SignInScreen, TierResult};

const ROW_HEIGHT: f32 = 20.0;
const SCREEN_WIDTH: f32 = 800.0;
const SCREEN_HEIGHT: f32 = 300.0;

fn screen_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(
            Pos2::ZERO,
            vec2(SCREEN_WIDTH, SCREEN_HEIGHT),
        )),
        ..Default::default()
    }
}

fn fresh_ctx() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx
}

/// Draw `key`'s scroll area for one frame over `rows` rows, recording the
/// resulting offset into `memory` exactly as a real call site
/// (`library_view.rs`/`settings/mod.rs`/`App::show_main`) would, and
/// returning that offset.
fn draw_view(ctx: &Context, memory: &mut SectionMemory, key: &ViewKey, rows: usize) -> f32 {
    let mut scroll = Some(memory.scroll_area(key));
    let mut offset_y = 0.0_f32;
    {
        let offset_ref = &mut offset_y;
        let output = ctx.run_ui(screen_input(), move |ui| {
            let out = scroll
                .take()
                .unwrap_or_else(|| unreachable!("run_ui only calls its closure once"))
                .show_rows(ui, ROW_HEIGHT, rows, |ui, range| {
                    for i in range {
                        ui.label(format!("row {i}"));
                    }
                });
            *offset_ref = out.state.offset.y;
        });
        output.drop_without_applying_deltas();
    }
    memory.record(key.clone(), offset_y);
    offset_y
}

/// Seed `key` as if it had last been left scrolled to `y`, then simulate a
/// frame where nothing memory-backed was on screen (`end_frame(None)`) —
/// the round trip's "leave" half, so the very next `draw_view(key)` call
/// exercises the real restore path.
fn seed_and_leave(memory: &mut SectionMemory, key: &ViewKey, y: f32) {
    memory.record(key.clone(), y);
    memory.end_frame(None);
}

fn album_id(unique: &str) -> AlbumId {
    AlbumId::new(format!("spotify:album:{unique}")).unwrap_or_else(|_| unreachable!())
}

// -- M1 -------------------------------------------------------------------

/// M1: for every key in the attachment table, scrolling to offset *y*,
/// drawing elsewhere, then drawing the key again restores *y* within
/// ±1 px — across 2 consecutive round trips.
#[test]
fn m1_restores_each_views_offset_across_two_round_trips() {
    let ctx = fresh_ctx();
    let keys = [
        ViewKey::Library(LibraryViewKey::Tab(LibraryTab::SavedTracks)),
        ViewKey::Library(LibraryViewKey::Detail(DetailTarget::Album(album_id("m1")))),
        ViewKey::Search,
        ViewKey::Settings(SettingsCategory::Audio),
        ViewKey::Plugins,
    ];
    for key in keys {
        let mut memory = SectionMemory::default();
        for y in [500.0_f32, 1500.0_f32] {
            seed_and_leave(&mut memory, &key, y);
            let restored = draw_view(&ctx, &mut memory, &key, 200);
            assert!(
                (restored - y).abs() <= 1.0,
                "key={key:?}: expected offset within 1px of {y}, got {restored}"
            );
        }
    }
}

// -- M2 -------------------------------------------------------------------

/// M2: a Library tab, that tab's own detail view, and two different
/// Settings categories each keep their own offset — round-tripping through
/// one does not disturb another's already-recorded offset (sub-view
/// retention, in addition to M1).
#[test]
fn m2_library_tab_detail_and_settings_categories_keep_independent_offsets() {
    let ctx = fresh_ctx();
    let mut memory = SectionMemory::default();

    let cases = [
        (
            ViewKey::Library(LibraryViewKey::Tab(LibraryTab::SavedAlbums)),
            400.0_f32,
        ),
        (
            ViewKey::Library(LibraryViewKey::Detail(DetailTarget::Album(album_id("m2")))),
            1200.0,
        ),
        (ViewKey::Settings(SettingsCategory::Audio), 200.0),
        (ViewKey::Settings(SettingsCategory::About), 900.0),
    ];

    for (key, y) in &cases {
        seed_and_leave(&mut memory, key, *y);
        let restored = draw_view(&ctx, &mut memory, key, 200);
        assert!(
            (restored - y).abs() <= 1.0,
            "key={key:?}: expected offset within 1px of {y}, got {restored}"
        );
    }

    // None of the later round trips disturbed an earlier key's own
    // recorded offset.
    for (key, y) in &cases {
        let stored = memory
            .offset(key)
            .unwrap_or_else(|| panic!("expected a stored offset for {key:?}"));
        assert!(
            (stored - y).abs() <= 1.0,
            "key={key:?}: a later round trip on a different key must not disturb this one \
             (stored={stored}, expected ~{y})"
        );
    }
}

// -- M3 -------------------------------------------------------------------

/// M3: if the content shrinks while a view is away (200 rows -> 20 rows),
/// the restored offset clamps to `max(0, content - viewport)` (egui's own
/// clamp), never staying at the old, now-unreachable offset.
#[test]
fn m3_restored_offset_clamps_when_content_shrank() {
    let ctx = fresh_ctx();
    let mut memory = SectionMemory::default();
    let key = ViewKey::Search;

    // Sanity: against the full 200-row list, the seeded offset applies
    // untouched.
    seed_and_leave(&mut memory, &key, 3000.0);
    let full = draw_view(&ctx, &mut memory, &key, 200);
    assert!(
        (full - 3000.0).abs() <= 1.0,
        "sanity: offset must apply as-is against a list tall enough to hold it, got {full}"
    );

    // The list shrinks to 20 rows while we're away. Rather than hand-
    // computing egui's own row-height-plus-spacing arithmetic, derive the
    // true reachable maximum for a fresh 20-row list by seeding an offset
    // far beyond any possible content height and reading back what egui
    // itself clamps it to (`max(0, content - viewport)` by construction,
    // since nothing this tall could ever be un-clamped).
    let mut baseline = SectionMemory::default();
    seed_and_leave(&mut baseline, &key, 1_000_000.0);
    let true_max = draw_view(&ctx, &mut baseline, &key, 20);

    seed_and_leave(&mut memory, &key, full);
    let shrunk = draw_view(&ctx, &mut memory, &key, 20);
    assert!(
        (shrunk - true_max).abs() <= 1.0,
        "restored offset must clamp to max(0, content - viewport) (= {true_max} for a fresh \
         20-row list here), got {shrunk}"
    );
    assert!(
        shrunk < 3000.0,
        "must not stay at the old, now-unreachable offset, got {shrunk}"
    );
}

// -- M4 -------------------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-section-memory-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fresh_settings_screen(label: &str) -> (SettingsScreen, TempDir) {
    let dir = TempDir::new(label);
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    let controller = PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
    (SettingsScreen::new(&controller), dir)
}

/// M4: `reset_session_ui` clears every recorded offset, bumps the epoch,
/// and resets `shell`/`library_view`/`library_detail`/`search_view`/
/// `settings`' selected category to their defaults — directly testable
/// through the free, `App`-free helper (no `eframe::CreationContext`
/// needed).
#[test]
fn m4_reset_session_ui_clears_memory_and_every_listed_sub_view() {
    let ctx = fresh_ctx();
    let mut memory = SectionMemory::default();
    let epoch_before = memory.epoch();

    let tracks_key = ViewKey::Library(LibraryViewKey::Tab(LibraryTab::SavedTracks));
    let search_key = ViewKey::Search;
    seed_and_leave(&mut memory, &tracks_key, 400.0);
    let _ = draw_view(&ctx, &mut memory, &tracks_key, 200);
    seed_and_leave(&mut memory, &search_key, 900.0);
    let _ = draw_view(&ctx, &mut memory, &search_key, 200);
    assert!(memory.offset(&tracks_key).is_some());
    assert!(memory.offset(&search_key).is_some());

    let mut shell = Shell {
        section: modplayer_ui::shell::Section::Settings,
        focus_search_requested: true,
    };
    let mut library_view = LibraryViewState {
        tab: LibraryTab::RecentlyPlayed,
        ..LibraryViewState::default()
    };
    let mut library_detail = Some(DetailTarget::Album(album_id("m4")));
    let mut search_view = SearchViewState {
        last_query: "a query".to_string(),
        ..SearchViewState::default()
    };
    let (mut settings, _dir) = fresh_settings_screen("m4");
    // `SettingsScreen::reset_category`'s own transition away from
    // `ALL[0]` and back is unit-tested in `settings/mod.rs` (private-field
    // access); this integration test only needs to see `reset_session_ui`
    // reach it, so a fresh screen (already on `ALL[0]`) is enough here.
    // Issue #35: a tier result left over from the signed-in session.
    let mut sign_in = SignInScreen::default();
    sign_in.set_tier_result(TierResult::Unknown);

    reset_session_ui(
        &mut memory,
        &mut shell,
        &mut library_view,
        &mut library_detail,
        &mut search_view,
        &mut settings,
        &mut sign_in,
    );

    assert_eq!(memory.epoch(), epoch_before + 1, "epoch must bump by 1");
    assert_eq!(memory.offset(&tracks_key), None);
    assert_eq!(memory.offset(&search_key), None);
    assert_eq!(shell.section, modplayer_ui::shell::Section::Library);
    assert!(!shell.focus_search_requested);
    assert_eq!(library_view.tab, LibraryTab::SavedTracks);
    assert_eq!(library_detail, None);
    assert_eq!(search_view.last_query, "");
    assert_eq!(settings.category(), SettingsCategory::ALL[0]);
    assert_eq!(
        sign_in.tier_result, None,
        "issue #35: the old tier-result overlay must not survive sign-out"
    );

    // The next draw of any key starts at the top (no stale offset, no
    // stale `shown_last_frame` from before the reset).
    let restarted = draw_view(&ctx, &mut memory, &tracks_key, 200);
    assert!(
        restarted.abs() <= 1.0,
        "the next draw after reset must start at 0, got {restarted}"
    );
}

// -- M5 -------------------------------------------------------------------

/// M5: switching Library *tabs* is not a round trip in any special sense —
/// each tab keeps its own key/offset, exactly like switching sections.
#[test]
fn m5_switching_library_tabs_keeps_independent_offsets_per_tab() {
    let ctx = fresh_ctx();
    let mut memory = SectionMemory::default();
    let tracks = ViewKey::Library(LibraryViewKey::Tab(LibraryTab::SavedTracks));
    let albums = ViewKey::Library(LibraryViewKey::Tab(LibraryTab::SavedAlbums));

    seed_and_leave(&mut memory, &tracks, 800.0);
    let tracks_first = draw_view(&ctx, &mut memory, &tracks, 200);
    assert!((tracks_first - 800.0).abs() <= 1.0);

    // Switch to Saved Albums — a tab never visited before starts at 0.
    let albums_first = draw_view(&ctx, &mut memory, &albums, 200);
    assert!(
        albums_first.abs() <= 1.0,
        "a tab never visited before must start at 0, got {albums_first}"
    );
    seed_and_leave(&mut memory, &albums, 300.0);
    let albums_restored = draw_view(&ctx, &mut memory, &albums, 200);
    assert!((albums_restored - 300.0).abs() <= 1.0);

    // Returning to Saved Tracks restores its own 800.0, unaffected by
    // Saved Albums' 300.0.
    seed_and_leave(&mut memory, &tracks, tracks_first);
    let tracks_restored = draw_view(&ctx, &mut memory, &tracks, 200);
    assert!(
        (tracks_restored - 800.0).abs() <= 1.0,
        "Saved Tracks' own offset must survive a visit to Saved Albums, got {tracks_restored}"
    );
    assert!((memory.offset(&albums).unwrap_or(-1.0) - 300.0).abs() <= 1.0);
}

// -- S1-S6 (025 US2): back to the library where I left off -----------------

/// Like [`draw_view`] at an explicit screen width, also returning the first
/// row `show_rows` laid out (the "first visible row").
fn draw_view_at(
    ctx: &Context,
    memory: &mut SectionMemory,
    key: &ViewKey,
    rows: usize,
    width: f32,
) -> (f32, usize) {
    let mut scroll = Some(memory.scroll_area(key));
    let mut offset_y = 0.0_f32;
    let mut first_row = usize::MAX;
    {
        let (offset_ref, first_ref) = (&mut offset_y, &mut first_row);
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, SCREEN_HEIGHT))),
            ..Default::default()
        };
        let output = ctx.run_ui(input, move |ui| {
            let out = scroll
                .take()
                .unwrap_or_else(|| unreachable!("run_ui only calls its closure once"))
                .show_rows(ui, ROW_HEIGHT, rows, |ui, range| {
                    *first_ref = (*first_ref).min(range.start);
                    for i in range {
                        ui.label(format!("row {i}"));
                    }
                });
            *offset_ref = out.state.offset.y;
        });
        output.drop_without_applying_deltas();
    }
    memory.record(key.clone(), offset_y);
    (offset_y, first_row)
}

fn detail_key() -> ViewKey {
    ViewKey::Library(LibraryViewKey::Detail(DetailTarget::Album(album_id("s"))))
}

/// **S1**: library list -> detail -> Back restores the same offset and the
/// same first visible row.
#[test]
fn s1_round_trip_restores_offset_and_first_visible_row() {
    let ctx = fresh_ctx();
    let mut memory = SectionMemory::default();
    let tab = ViewKey::Library(LibraryViewKey::Tab(LibraryTab::Playlists));
    seed_and_leave(&mut memory, &tab, 1500.0);
    let (before, first_before) = draw_view_at(&ctx, &mut memory, &tab, 500, SCREEN_WIDTH);
    assert!((before - 1500.0).abs() <= 1.0, "{before}");

    // Open an item (detail drawn), then Back to the tab.
    draw_view_at(&ctx, &mut memory, &detail_key(), 50, SCREEN_WIDTH);
    let (after, first_after) = draw_view_at(&ctx, &mut memory, &tab, 500, SCREEN_WIDTH);
    assert!((after - before).abs() <= 1.0, "offset {before} -> {after}");
    assert_eq!(first_before, first_after, "first visible row changed");
}

/// **S2**: the active tab (Playlists) keeps its own offset; other tabs do
/// not disturb it.
#[test]
fn s2_active_tab_offset_is_preserved_across_detail() {
    let ctx = fresh_ctx();
    let mut memory = SectionMemory::default();
    let playlists = ViewKey::Library(LibraryViewKey::Tab(LibraryTab::Playlists));
    let albums = ViewKey::Library(LibraryViewKey::Tab(LibraryTab::SavedAlbums));
    seed_and_leave(&mut memory, &playlists, 900.0);
    seed_and_leave(&mut memory, &albums, 200.0);
    draw_view_at(&ctx, &mut memory, &playlists, 300, SCREEN_WIDTH);
    draw_view_at(&ctx, &mut memory, &detail_key(), 40, SCREEN_WIDTH);
    let (back, _) = draw_view_at(&ctx, &mut memory, &playlists, 300, SCREEN_WIDTH);
    assert!((back - 900.0).abs() <= 1.0, "Playlists offset {back}");
    assert_eq!(memory.offset(&albums), Some(200.0));
}

/// **S3**: content shrank while away -> offset clamps to `max_scroll`.
#[test]
fn s3_offset_clamps_to_max_scroll_when_list_shrank() {
    let ctx = fresh_ctx();
    let tab = ViewKey::Library(LibraryViewKey::Tab(LibraryTab::Playlists));
    let mut baseline = SectionMemory::default();
    seed_and_leave(&mut baseline, &tab, 1_000_000.0);
    let (max_scroll, _) = draw_view_at(&ctx, &mut baseline, &tab, 20, SCREEN_WIDTH);

    let mut memory = SectionMemory::default();
    seed_and_leave(&mut memory, &tab, 3000.0);
    draw_view_at(&ctx, &mut memory, &tab, 200, SCREEN_WIDTH);
    draw_view_at(&ctx, &mut memory, &detail_key(), 10, SCREEN_WIDTH);
    let (back, _) = draw_view_at(&ctx, &mut memory, &tab, 20, SCREEN_WIDTH);
    assert!(
        (back - max_scroll).abs() <= 1.0,
        "{back} != max_scroll {max_scroll}"
    );
}

/// **S4**: window narrowed 1400 -> 960 while in detail; the first visible
/// row is unchanged on return (fixed row height, width-independent).
#[test]
fn s4_width_change_keeps_first_visible_row() {
    let ctx = fresh_ctx();
    let mut memory = SectionMemory::default();
    let tab = ViewKey::Library(LibraryViewKey::Tab(LibraryTab::Playlists));
    seed_and_leave(&mut memory, &tab, 1200.0);
    let (_, first_wide) = draw_view_at(&ctx, &mut memory, &tab, 400, 1400.0);
    draw_view_at(&ctx, &mut memory, &detail_key(), 30, 960.0);
    let (_, first_narrow) = draw_view_at(&ctx, &mut memory, &tab, 400, 960.0);
    assert_eq!(first_wide, first_narrow);
}

/// **S5**: the restore is applied once; a later stored value never
/// overrides the live scroll state on subsequent frames.
#[test]
fn s5_restore_is_applied_once() {
    let ctx = fresh_ctx();
    let mut memory = SectionMemory::default();
    let tab = ViewKey::Library(LibraryViewKey::Tab(LibraryTab::Playlists));
    seed_and_leave(&mut memory, &tab, 1000.0);
    let (restored, _) = draw_view_at(&ctx, &mut memory, &tab, 300, SCREEN_WIDTH);
    assert!((restored - 1000.0).abs() <= 1.0);

    // Next frame: stored value differs (as if the user scrolled elsewhere);
    // the view must keep its live offset rather than re-apply the store.
    memory.record(tab.clone(), 100.0);
    let (next, _) = draw_view_at(&ctx, &mut memory, &tab, 300, SCREEN_WIDTH);
    assert!((next - restored).abs() <= 1.0, "restore re-applied: {next}");
}

/// **S6**: nothing is persisted to disk, and a session reset (sign-out)
/// forgets every offset.
#[test]
fn s6_nothing_on_disk_and_reset_forgets_offsets() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/section_memory.rs"
    ))
    .unwrap();
    for banned in ["std::fs", "serde", "File::", "persist_egui_memory"] {
        // The module doc mentions persistence in prose only; code must not.
        let code: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!code.contains(banned), "section_memory.rs uses {banned}");
    }
    let mut memory = SectionMemory::default();
    memory.record(detail_key(), 500.0);
    memory.reset();
    assert_eq!(memory.offset(&detail_key()), None);
    assert_eq!(memory.epoch(), 1);
}
