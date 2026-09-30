// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `search_view.rs` behaviour (contracts/ui-surface.md §2, FR-001-003/
//! 015-018): skeleton while pending, group omission, no-results vs offline
//! state, refreshing keeps stale rows, show-more paging, and the SC-001
//! "first group painted <= 300 ms" proxy (scripted 100 ms reply, injected
//! clock: 150 ms debounce + 100 ms reply comfortably inside the 300 ms
//! budget).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui::{Context, Event, PointerButton, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{
    AlbumId, AlbumRef, ArtistId, ArtistRef, Availability, CatalogError, PlaylistId, PlaylistRef,
    SearchGroupPage, SearchHit, SearchKind, SearchPage, TrackId, TrackRef,
};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{PlaybackController, tr, tr_args};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::section_memory::{SectionMemory, ViewKey};

/// 014-design-tokens-and-type-scale (US2, T026): a bare `Context::default()`
/// has none of the token `Style`'s `Name("section")` text style installed,
/// which each result group's own header now reaches — panicking on layout
/// otherwise. Install it once, exactly as `App::new`/`App::update` do
/// (mirrors `controls.rs` test's identically-named helper).
fn fresh_ctx() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-search-view-{label}-{}-{unique}",
            std::process::id(),
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

fn fresh_store(label: &str) -> (SettingsStore, TempDir) {
    let dir = TempDir::new(label);
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    (store, dir)
}

fn fake_device() -> FakeDevice {
    FakeDevice {
        id: DeviceId::new("dev-1").unwrap(),
        name: "Speakers".to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        is_default: true,
    }
}

fn track_hit(id: &str, title: &str) -> SearchHit {
    SearchHit::Track(TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap(),
        title,
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    ))
}

fn full_page(track_titles: &[&str]) -> SearchPage {
    SearchPage {
        groups: vec![
            SearchGroupPage {
                kind: SearchKind::Track,
                items: track_titles
                    .iter()
                    .enumerate()
                    .map(|(i, title)| track_hit(&i.to_string(), title))
                    .collect(),
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Album,
                items: vec![],
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Artist,
                items: vec![],
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Playlist,
                items: vec![],
                next_offset: None,
            },
        ],
        unsupported: vec![],
    }
}

/// A controller over a confirmed, registered device (mirrors
/// `accessibility.rs`'s `active_controller`) plus the `ScriptedHostHandle`
/// needed to script search replies.
fn active_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    modplayer_audio_source_synthetic::ScriptedHostHandle,
    TempDir,
) {
    let (store, dir) = fresh_store(label);
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![fake_device()];
    let mut controller = PlaybackController::new(FakeBackend::new(devices), host, store);
    controller.launch();
    controller.confirm_device(DeviceId::new("dev-1").unwrap(), BufferPreset::Balanced);
    controller.set_playback_permitted(true, None);
    controller.tick();
    (controller, handle, dir)
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 1200.0))),
        ..Default::default()
    }
}

#[derive(Debug, Clone)]
struct Node {
    role: Role,
    label: Option<String>,
    value: Option<String>,
}

impl Node {
    /// The text a screen reader would announce: `label` for every role but
    /// `Role::Label` (whose own text sits in `value` —
    /// `Response::fill_accesskit_node_from_widget_info`), falling back to
    /// `value` so a plain content-only node still counts (mirrors
    /// `accessibility.rs`'s `AccessNode::accessible_name`).
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

fn render_nodes(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    focus: &mut bool,
) -> Vec<Node> {
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut state = modplayer_ui::search_view::SearchViewState::default();
    let mut memory = SectionMemory::default();
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::search_view::show(
            ui,
            controller,
            &mut artwork,
            focus,
            &mut state,
            &mut memory,
        );
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();
    update
        .nodes
        .iter()
        .map(|(_, node)| Node {
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
        })
        .collect()
}

fn has(nodes: &[Node], role: Role, label: &str) -> bool {
    nodes
        .iter()
        .any(|n| n.role == role && n.accessible_name() == Some(label))
}

fn count(nodes: &[Node], role: Role) -> usize {
    nodes.iter().filter(|n| n.role == role).count()
}

#[test]
fn skeleton_rows_render_while_a_group_is_pending() {
    let (mut controller, handle, _dir) = active_controller("skeleton");
    handle.script_search("abba", Ok(full_page(&["Dancing Queen"])));

    let now = controller.now();
    controller.search_mut().set_query("abba", now);
    controller.tick(); // debounce not yet elapsed on this same instant... advance below.

    // Advance past the 150 ms debounce and fire the request; the scripted
    // reply (delay = Duration::ZERO) is only drained on the *next* tick,
    // so this render sees the group still `Pending`.
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick();

    let mut focus = false;
    let nodes = render_nodes(&mut controller, &mut focus);
    assert!(
        has(nodes.as_slice(), Role::Status, &tr("loading")),
        "expected skeleton (Role::Status \"Loading\") rows while Pending: {nodes:?}"
    );
    assert!(
        has(nodes.as_slice(), Role::Header, &tr("search-group-tracks")),
        "the Tracks header (name only, no count) must render while its rows are skeletons"
    );
}

#[test]
fn loaded_group_renders_real_rows_and_omits_empty_groups() {
    let (mut controller, handle, _dir) = active_controller("loaded");
    handle.script_search("abba", Ok(full_page(&["Dancing Queen", "Waterloo"])));

    let now = controller.now();
    controller.search_mut().set_query("abba", now);
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick(); // issues the request
    controller.tick(); // drains the (already-scheduled) reply

    let mut focus = false;
    let nodes = render_nodes(&mut controller, &mut focus);
    assert!(
        has(
            nodes.as_slice(),
            Role::Header,
            &tr_args(
                "search-group-header",
                &[
                    ("group", tr("search-group-tracks")),
                    ("count", "2".to_string())
                ],
            )
        ),
        "Tracks header must render with its count"
    );
    assert!(
        !has(nodes.as_slice(), Role::Header, &tr("search-group-albums")),
        "an Empty group's header must be omitted: {nodes:?}"
    );
    assert!(
        !has(nodes.as_slice(), Role::Header, &tr("search-group-artists")),
        "an Empty group's header must be omitted: {nodes:?}"
    );
    assert_eq!(
        count(nodes.as_slice(), Role::ListItem),
        2,
        "both loaded tracks must render as ListItem rows: {nodes:?}"
    );
}

#[test]
fn offline_shows_the_offline_state_not_the_search_box_results() {
    let (mut controller, _handle, _dir) = active_controller("offline");
    // Force the derived "online" fact false (research R5: health `Ok` AND
    // registered) — deregistering is the simplest available lever.
    controller.set_playback_permitted(false, None);
    controller.tick();

    let now = controller.now();
    controller.search_mut().set_query("abba", now);
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick();

    let mut focus = false;
    let nodes = render_nodes(&mut controller, &mut focus);
    assert!(
        nodes
            .iter()
            .any(|n| n.accessible_name() == Some(tr("search-offline").as_str())),
        "expected the offline label: {nodes:?}"
    );
    assert!(
        !has(nodes.as_slice(), Role::Header, &tr("search-group-tracks")),
        "no groups render while offline: {nodes:?}"
    );
}

#[test]
fn no_results_state_shows_when_every_group_is_empty() {
    let (mut controller, handle, _dir) = active_controller("no-results");
    handle.script_search("zzzzz", Ok(full_page(&[])));

    let now = controller.now();
    controller.search_mut().set_query("zzzzz", now);
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick();
    controller.tick();

    let mut focus = false;
    let nodes = render_nodes(&mut controller, &mut focus);
    let expected = tr_args("search-no-results", &[("query", "zzzzz".to_string())]);
    assert!(
        nodes
            .iter()
            .any(|n| n.accessible_name() == Some(expected.as_str())),
        "expected the no-results label \"{expected}\": {nodes:?}"
    );
}

#[test]
fn refreshing_keeps_stale_rows_visible() {
    let (mut controller, handle, _dir) = active_controller("refreshing");
    handle.script_search("abba", Ok(full_page(&["Dancing Queen"])));

    let now = controller.now();
    controller.search_mut().set_query("abba", now);
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick();
    controller.tick();

    // Force the group into `Loaded{next_offset: Some}` so `show_more`
    // issues a page request, then reply to it with a rate limit — the
    // stale (single-item) snapshot must stay rendered with a "Refreshing…"
    // status.
    let mut page = full_page(&["Dancing Queen"]);
    page.groups[0].next_offset = Some(20);
    handle.script_search("abba", Ok(page));
    // Re-issue so the group actually has `next_offset: Some` to page from:
    // the simplest way, without reaching into `SearchSession` internals
    // from a UI-crate test, is a fresh query cycle.
    controller
        .search_mut()
        .set_query("", now + Duration::from_millis(1));
    controller
        .search_mut()
        .set_query("abba", now + Duration::from_millis(2));
    controller.set_clock(move || now + Duration::from_millis(400));
    controller.tick();
    controller.tick();

    handle.script_search(
        "abba",
        Err(CatalogError::RateLimited {
            retry_after_ms: None,
        }),
    );
    controller.search_show_more(SearchKind::Track);
    controller.tick();

    let mut focus = false;
    let nodes = render_nodes(&mut controller, &mut focus);
    assert!(
        has(nodes.as_slice(), Role::Status, &tr("search-stale")),
        "expected the stale status strip node: {nodes:?}"
    );
    assert!(
        !has(nodes.as_slice(), Role::Status, &tr("refreshing")),
        "the old Search-view \"Refreshing…\" status is gone (V7): {nodes:?}"
    );
    assert_eq!(
        count(nodes.as_slice(), Role::ListItem),
        1,
        "the stale row must stay visible: {nodes:?}"
    );
}

#[test]
fn first_group_paints_within_budget() {
    // SC-001 proxy: 150 ms debounce + a scripted 100 ms reply delay is
    // 250 ms, comfortably inside the 300 ms budget — proven by composing
    // the two injectable clocks rather than a wall-clock sleep.
    let (mut controller, handle, _dir) = active_controller("budget");
    let base = Instant::now();
    let clock_cell = Arc::new(Mutex::new(base));

    {
        let clock_cell = Arc::clone(&clock_cell);
        controller.set_clock(move || *clock_cell.lock().unwrap());
    }
    {
        let clock_cell = Arc::clone(&clock_cell);
        handle.set_clock(move || *clock_cell.lock().unwrap());
    }
    handle.script_catalog_delay(Duration::from_millis(100));
    handle.script_search("abba", Ok(full_page(&["Dancing Queen"])));

    controller.search_mut().set_query("abba", base);

    // 150 ms debounce elapses -> the request is issued.
    *clock_cell.lock().unwrap() = base + Duration::from_millis(150);
    controller.tick();
    assert!(matches!(
        controller.search().group(SearchKind::Track),
        modplayer_core::GroupState::Pending
    ));

    // 100 ms scripted reply delay elapses -> well inside the 300 ms
    // SC-001 budget measured from the last keystroke (150 + 100 = 250 ms).
    *clock_cell.lock().unwrap() = base + Duration::from_millis(250);
    controller.tick();

    assert!(
        matches!(
            controller.search().group(SearchKind::Track),
            modplayer_core::GroupState::Loaded { .. }
        ),
        "expected the Tracks group painted within the 300 ms budget"
    );

    let mut focus = false;
    let nodes = render_nodes(&mut controller, &mut focus);
    assert_eq!(count(nodes.as_slice(), Role::ListItem), 1);
}

// -- Selection (contract S2/S8) --------------------------------------------

fn album_hit(id: &str, name: &str) -> SearchHit {
    SearchHit::Album(AlbumRef {
        id: AlbumId::new(format!("spotify:album:{id}")).unwrap(),
        name: name.to_string(),
        artists: vec!["Artist".to_string()],
        artwork_url: None,
        release_date: None,
        track_count: 1,
    })
}

/// Unlike `full_page`, both the Tracks and Albums groups carry a row, so a
/// selection can move from one group to the other (contract S2).
fn two_group_page(track_titles: &[&str], album_names: &[&str]) -> SearchPage {
    SearchPage {
        groups: vec![
            SearchGroupPage {
                kind: SearchKind::Track,
                items: track_titles
                    .iter()
                    .enumerate()
                    .map(|(i, title)| track_hit(&i.to_string(), title))
                    .collect(),
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Album,
                items: album_names
                    .iter()
                    .enumerate()
                    .map(|(i, name)| album_hit(&i.to_string(), name))
                    .collect(),
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Artist,
                items: vec![],
                next_offset: None,
            },
            SearchGroupPage {
                kind: SearchKind::Playlist,
                items: vec![],
                next_offset: None,
            },
        ],
        unsupported: vec![],
    }
}

/// One frame's `FullOutput`, using a persistent `ctx`/`state` (unlike
/// `render_nodes`, which creates a fresh `SearchViewState` every call and
/// so cannot observe a selection surviving across frames).
fn render_output(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    focus: &mut bool,
    state: &mut modplayer_ui::search_view::SearchViewState,
    memory: &mut SectionMemory,
    input: RawInput,
) -> egui::FullOutput {
    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    ctx.run_ui(input, |ui| {
        modplayer_ui::search_view::show(ui, controller, &mut artwork, focus, state, memory);
    })
}

/// The centre of the first `role`/`name` node's bounds in `output`
/// (mirrors `library_view.rs`'s own `find_node_center`).
fn find_node_center(output: &egui::FullOutput, role: Role, name: &str) -> Pos2 {
    let update = output
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accesskit_update should be populated once enabled");
    let bounds = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == role && n.label() == Some(name))
        .and_then(|(_, n)| n.bounds())
        .unwrap_or_else(|| panic!("expected a {role:?} node named {name:?}"));
    Pos2::new(
        ((bounds.x0 + bounds.x1) / 2.0) as f32,
        ((bounds.y0 + bounds.y1) / 2.0) as f32,
    )
}

fn click_at(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    focus: &mut bool,
    state: &mut modplayer_ui::search_view::SearchViewState,
    memory: &mut SectionMemory,
    pos: Pos2,
) {
    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    render_output(ctx, controller, focus, state, memory, press).drop_without_applying_deltas();

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    render_output(ctx, controller, focus, state, memory, release).drop_without_applying_deltas();
}

/// **S2** (contract S2, FR-006/007): selecting a row in one of Search's
/// four groups, then clicking a row in a different group, leaves only the
/// second selected — the one `RowSelection` field is exclusive across all
/// four groups, not just within one.
#[test]
fn selecting_a_row_in_a_different_group_replaces_the_selection() {
    let (mut controller, handle, _dir) = active_controller("select-across-groups");
    handle.script_search("abba", Ok(two_group_page(&["Dancing Queen"], &["Arrival"])));

    let now = controller.now();
    controller.search_mut().set_query("abba", now);
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick();
    controller.tick();

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut focus = false;
    let mut state = modplayer_ui::search_view::SearchViewState::default();
    let mut memory = SectionMemory::default();
    assert_eq!(state.selection, modplayer_ui::rows::RowSelection::default());

    let discover = render_output(
        &ctx,
        &mut controller,
        &mut focus,
        &mut state,
        &mut memory,
        default_input(),
    );
    let track_center = find_node_center(&discover, Role::ListItem, "Dancing Queen — Artist");
    let album_center = find_node_center(&discover, Role::ListItem, "Arrival — Artist");
    discover.drop_without_applying_deltas();

    click_at(
        &ctx,
        &mut controller,
        &mut focus,
        &mut state,
        &mut memory,
        track_center,
    );
    assert_ne!(
        state.selection,
        modplayer_ui::rows::RowSelection::default(),
        "the Tracks row must be selected"
    );
    let after_track = state.selection.clone();

    click_at(
        &ctx,
        &mut controller,
        &mut focus,
        &mut state,
        &mut memory,
        album_center,
    );
    assert_ne!(
        state.selection,
        modplayer_ui::rows::RowSelection::default(),
        "the Albums row must be selected"
    );
    assert_ne!(
        state.selection, after_track,
        "selecting the Albums row must replace the Tracks row's selection, not add to it"
    );
}

/// **S8** (FR-028): a new search query clears the selection.
#[test]
fn a_new_query_clears_the_selection() {
    let (mut controller, handle, _dir) = active_controller("select-then-new-query");
    handle.script_search("abba", Ok(full_page(&["Dancing Queen"])));
    handle.script_search("queen", Ok(full_page(&["Bohemian Rhapsody"])));

    let now = controller.now();
    controller.search_mut().set_query("abba", now);
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick();
    controller.tick();

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut focus = false;
    let mut state = modplayer_ui::search_view::SearchViewState::default();
    let mut memory = SectionMemory::default();

    let discover = render_output(
        &ctx,
        &mut controller,
        &mut focus,
        &mut state,
        &mut memory,
        default_input(),
    );
    let track_center = find_node_center(&discover, Role::ListItem, "Dancing Queen — Artist");
    discover.drop_without_applying_deltas();

    click_at(
        &ctx,
        &mut controller,
        &mut focus,
        &mut state,
        &mut memory,
        track_center,
    );
    assert_ne!(
        state.selection,
        modplayer_ui::rows::RowSelection::default(),
        "the row must be selected before the query changes"
    );

    controller
        .search_mut()
        .set_query("queen", now + Duration::from_millis(201));
    controller.set_clock(move || now + Duration::from_millis(400));
    controller.tick();
    controller.tick();

    let output = render_output(
        &ctx,
        &mut controller,
        &mut focus,
        &mut state,
        &mut memory,
        default_input(),
    );
    output.drop_without_applying_deltas();
    assert_eq!(
        state.selection,
        modplayer_ui::rows::RowSelection::default(),
        "a new query must clear the prior query's selection"
    );
}

// -- 026 US1: one results page (contracts/results-layout.md RL1-RL12) -------

fn artist_hit(i: usize) -> SearchHit {
    SearchHit::Artist(ArtistRef {
        id: ArtistId::new(format!("spotify:artist:{i}")).unwrap(),
        name: format!("Artist {i}"),
        artwork_url: None,
    })
}

fn playlist_hit(i: usize) -> SearchHit {
    SearchHit::Playlist(PlaylistRef {
        id: PlaylistId::new(format!("spotify:playlist:{i}")).unwrap(),
        name: format!("Playlist {i}"),
        owner_name: "Owner".to_string(),
        editable: false,
        artwork_url: None,
        track_count: 1,
        revision: None,
    })
}

/// A page with `[tracks, albums, artists, playlists]` rows; `next` sets the
/// Tracks group's `next_offset`. Row indices start at `base` so a second
/// page's ids never collide with the first's.
fn sized_page(counts: [usize; 4], base: usize, next: Option<u32>) -> SearchPage {
    let kinds = [
        SearchKind::Track,
        SearchKind::Album,
        SearchKind::Artist,
        SearchKind::Playlist,
    ];
    SearchPage {
        groups: kinds
            .iter()
            .zip(counts)
            .map(|(kind, n)| SearchGroupPage {
                kind: *kind,
                items: (base..base + n)
                    .map(|i| match kind {
                        SearchKind::Track => track_hit(&i.to_string(), &format!("Song {i}")),
                        SearchKind::Album => album_hit(&i.to_string(), &format!("Album {i}")),
                        SearchKind::Artist => artist_hit(i),
                        SearchKind::Playlist => playlist_hit(i),
                    })
                    .collect(),
                next_offset: if *kind == SearchKind::Track {
                    next
                } else {
                    None
                },
            })
            .collect(),
        unsupported: vec![],
    }
}

fn settle(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    handle: &modplayer_audio_source_synthetic::ScriptedHostHandle,
    query: &str,
    page: SearchPage,
) {
    handle.script_search(query, Ok(page));
    let now = controller.now();
    controller.search_mut().set_query(query, now);
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick();
    controller.tick();
}

fn input_sized(w: f32, h: f32) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(w, h))),
        ..Default::default()
    }
}

/// Everything one frame of a persistent view needs.
struct Page {
    ctx: Context,
    focus: bool,
    state: modplayer_ui::search_view::SearchViewState,
    memory: SectionMemory,
}

impl Page {
    fn new() -> Self {
        let ctx = fresh_ctx();
        ctx.enable_accesskit();
        Self {
            ctx,
            focus: false,
            state: modplayer_ui::search_view::SearchViewState::default(),
            memory: SectionMemory::default(),
        }
    }

    /// Start the next frame at scroll offset `y` (the memory's "returning to
    /// the view" seeding, so no wheel animation is involved).
    fn seed_offset(&mut self, y: f32) {
        self.memory.record(ViewKey::Search, y);
        self.memory.end_frame(None);
    }

    fn frame(
        &mut self,
        controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
        input: RawInput,
    ) -> egui::FullOutput {
        render_output(
            &self.ctx,
            controller,
            &mut self.focus,
            &mut self.state,
            &mut self.memory,
            input,
        )
    }

    fn offset(&self) -> f32 {
        self.memory.offset(&ViewKey::Search).unwrap_or(0.0)
    }
}

/// Bounds (x0, y0, x1, y1) of every `role` node in `output`, in tree order
/// paired with its label.
fn bounds_of(output: &egui::FullOutput, role: Role) -> Vec<(String, [f64; 4])> {
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    update
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == role)
        .filter_map(|(_, n)| {
            let b = n.bounds()?;
            Some((
                n.label().or(n.value()).unwrap_or_default().to_string(),
                [b.x0, b.y0, b.x1, b.y1],
            ))
        })
        .collect()
}

fn headers(output: &egui::FullOutput) -> Vec<(String, [f64; 4])> {
    let mut h = bounds_of(output, Role::Header);
    h.sort_by(|a, b| a.1[1].total_cmp(&b.1[1]));
    h
}

fn settled_controller(
    label: &str,
    counts: [usize; 4],
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    modplayer_audio_source_synthetic::ScriptedHostHandle,
    TempDir,
) {
    let (mut controller, handle, dir) = active_controller(label);
    settle(
        &mut controller,
        &handle,
        "abba",
        sized_page(counts, 0, None),
    );
    (controller, handle, dir)
}

fn group_label(key: &str, n: usize) -> String {
    tr_args(
        "search-group-header",
        &[("group", tr(key)), ("count", n.to_string())],
    )
}

/// **RL1**: the view owns the only scroll area — the per-group capped lists
/// are gone from the source and the section memory records the one offset.
#[test]
fn rl1_there_is_exactly_one_results_scroll_area() {
    let src = include_str!("../src/search_view.rs");
    assert!(
        !src.contains(&["GROUP_VISIBLE", "ROWS"].join("_")),
        "capped group lists must be gone"
    );
    assert!(
        !src.contains("virtualized_list("),
        "no per-group virtualized_list"
    );
    assert_eq!(
        src.matches(".scroll_area(").count(),
        1,
        "exactly one scroll area"
    );

    let (mut controller, _handle, _dir) = settled_controller("rl1", [30, 30, 30, 30]);
    let mut page = Page::new();
    page.frame(&mut controller, input_sized(960.0, 640.0))
        .drop_without_applying_deltas();
    assert!(
        page.memory.offset(&ViewKey::Search).is_some(),
        "the view records its offset"
    );
}

/// **RL2**: scrolling the results leaves the field row where it was.
#[test]
fn rl2_field_row_does_not_move_when_results_scroll() {
    let (mut controller, _handle, _dir) = settled_controller("rl2", [200, 5, 5, 5]);
    let mut page = Page::new();
    let top = page.frame(&mut controller, input_sized(960.0, 640.0));
    let field_before = bounds_of(&top, Role::TextInput);
    top.drop_without_applying_deltas();
    page.seed_offset(2000.0);
    let scrolled = page.frame(&mut controller, input_sized(960.0, 640.0));
    let field_after = bounds_of(&scrolled, Role::TextInput);
    scrolled.drop_without_applying_deltas();
    assert!(
        page.offset() > 1000.0,
        "sanity: scrolled, got {}",
        page.offset()
    );
    assert_eq!(field_before.len(), 1);
    assert_eq!(field_before, field_after);
}

/// **RL3**: empty/unsupported groups contribute no header.
#[test]
fn rl3_omitted_groups_have_no_header() {
    let (mut controller, handle, _dir) = active_controller("rl3");
    let mut p = sized_page([2, 0, 0, 2], 0, None);
    p.unsupported = vec![SearchKind::Artist];
    p.groups.retain(|g| g.kind != SearchKind::Artist);
    settle(&mut controller, &handle, "abba", p);
    let mut page = Page::new();
    let out = page.frame(&mut controller, input_sized(960.0, 1200.0));
    let labels: Vec<String> = headers(&out).into_iter().map(|h| h.0).collect();
    out.drop_without_applying_deltas();
    assert_eq!(
        labels,
        vec![
            group_label("search-group-tracks", 2),
            group_label("search-group-playlists", 2)
        ]
    );
}

/// **RL4**: only the viewport's rows are laid out.
#[test]
fn rl4_long_group_is_virtualised() {
    let (mut controller, _handle, _dir) = settled_controller("rl4", [400, 0, 0, 0]);
    let mut page = Page::new();
    page.seed_offset(5000.0);
    let out = page.frame(&mut controller, input_sized(960.0, 640.0));
    let rows = bounds_of(&out, Role::ListItem).len();
    out.drop_without_applying_deltas();
    assert!(
        rows > 0 && rows < 20,
        "expected < 20 rows laid out, got {rows}"
    );
}

/// **RL5**: one scroll gesture (to the end) reaches Playlists at 960x640.
#[test]
fn rl5_scrolling_to_the_end_reaches_playlists() {
    let (mut controller, _handle, _dir) = settled_controller("rl5", [20, 20, 20, 20]);
    let mut page = Page::new();
    page.frame(&mut controller, input_sized(960.0, 640.0))
        .drop_without_applying_deltas();
    page.seed_offset(1_000_000.0);
    let out = page.frame(&mut controller, input_sized(960.0, 640.0));
    let labels: Vec<String> = headers(&out).into_iter().map(|h| h.0).collect();
    out.drop_without_applying_deltas();
    assert!(
        labels.contains(&group_label("search-group-playlists", 20)),
        "Playlists header must be reachable: {labels:?}"
    );
}

/// **RL6/RL7**: mid-group, the group's header is pinned at the viewport top,
/// drawn once, and swallows clicks.
#[test]
fn rl6_rl7_header_pins_once_and_blocks_clicks() {
    let (mut controller, _handle, _dir) = settled_controller("rl6", [400, 0, 0, 0]);
    let mut page = Page::new();
    let first = page.frame(&mut controller, input_sized(960.0, 640.0));
    let top_y = headers(&first)[0].1[1];
    first.drop_without_applying_deltas();

    page.seed_offset(2000.0);
    let out = page.frame(&mut controller, input_sized(960.0, 640.0));
    let hs = headers(&out);
    let pinned = hs[0].clone();
    out.drop_without_applying_deltas();
    assert_eq!(hs.len(), 1, "exactly one Header while pinned: {hs:?}");
    assert!(
        (pinned.1[1] - top_y).abs() < 1.0,
        "pinned at the viewport top: {pinned:?} vs {top_y}"
    );

    let centre = Pos2::new(
        ((pinned.1[0] + pinned.1[2]) / 2.0) as f32,
        ((pinned.1[1] + pinned.1[3]) / 2.0) as f32,
    );
    click_at(
        &page.ctx,
        &mut controller,
        &mut page.focus,
        &mut page.state,
        &mut page.memory,
        centre,
    );
    assert_eq!(
        page.state.selection,
        modplayer_ui::rows::RowSelection::default(),
        "a click on the pinned header must not select the row beneath"
    );
}

/// **RL6** at the end of the content (026 T043 M1 walk): scrolled to the
/// maximum offset with a Show more footer below the last row, the pinned
/// header still sits at the viewport top, not a few points above it.
#[test]
fn rl6_header_pins_at_viewport_top_at_max_scroll() {
    let (mut controller, handle, _dir) = active_controller("rl6-max");
    settle(
        &mut controller,
        &handle,
        "abba",
        sized_page([20, 0, 0, 0], 0, Some(20)),
    );
    let mut page = Page::new();
    let first = page.frame(&mut controller, input_sized(960.0, 640.0));
    let top_y = headers(&first)[0].1[1];
    first.drop_without_applying_deltas();

    page.seed_offset(1_000_000.0);
    for _ in 0..3 {
        page.frame(&mut controller, input_sized(960.0, 640.0))
            .drop_without_applying_deltas();
    }
    let out = page.frame(&mut controller, input_sized(960.0, 640.0));
    let hs = headers(&out);
    out.drop_without_applying_deltas();
    assert!(
        page.offset() > 100.0,
        "sanity: scrolled, got {}",
        page.offset()
    );
    assert_eq!(hs.len(), 1, "exactly one Header while pinned: {hs:?}");
    assert!(
        (hs[0].1[1] - top_y).abs() < 0.5,
        "pinned at the viewport top at max scroll: {:?} vs {top_y}",
        hs[0]
    );
}

/// **RL8**: header counts follow the rendered rows; Pending shows the name.
#[test]
fn rl8_header_counts_follow_rows() {
    let (mut controller, handle, _dir) = active_controller("rl8");
    handle.script_search("abba", Ok(sized_page([20, 0, 0, 0], 0, Some(20))));
    handle.script_search("abba", Ok(sized_page([20, 0, 0, 0], 20, None)));
    let now = controller.now();
    controller.search_mut().set_query("abba", now);
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick();
    let mut page = Page::new();
    let out = page.frame(&mut controller, input_sized(960.0, 1200.0));
    let pending = headers(&out);
    out.drop_without_applying_deltas();
    assert_eq!(pending[0].0, tr("search-group-tracks"));

    controller.tick();
    let out = page.frame(&mut controller, input_sized(960.0, 1200.0));
    assert_eq!(headers(&out)[0].0, group_label("search-group-tracks", 20));
    out.drop_without_applying_deltas();

    controller.search_show_more(SearchKind::Track);
    controller.tick();
    controller.tick();
    let out = page.frame(&mut controller, input_sized(960.0, 1200.0));
    assert_eq!(headers(&out)[0].0, group_label("search-group-tracks", 40));
    out.drop_without_applying_deltas();
}

/// **RL9**: Show more follows the last row and precedes the next header.
#[test]
fn rl9_show_more_is_the_groups_last_element() {
    let (mut controller, handle, _dir) = active_controller("rl9");
    settle(
        &mut controller,
        &handle,
        "abba",
        sized_page([3, 2, 0, 0], 0, Some(3)),
    );
    let mut page = Page::new();
    let out = page.frame(&mut controller, input_sized(960.0, 1200.0));
    let rows = bounds_of(&out, Role::ListItem);
    let hs = headers(&out);
    let label = tr_args("search-show-more", &[("group", tr("search-group-tracks"))]);
    let button = bounds_of(&out, Role::Button)
        .into_iter()
        .find(|(l, _)| *l == label)
        .expect("Show more button");
    out.drop_without_applying_deltas();
    let last_track_bottom = rows
        .iter()
        .filter(|(_, b)| b[1] > hs[0].1[3] && b[1] < hs[1].1[1])
        .map(|(_, b)| b[3])
        .fold(0.0_f64, f64::max);
    assert!(
        button.1[1] >= last_track_bottom - 0.5,
        "{button:?} vs {last_track_bottom}"
    );
    assert!(
        button.1[3] <= hs[1].1[1] + 0.5,
        "button above the next header"
    );
}

/// **RL10**: one column at every width.
#[test]
fn rl10_headers_share_a_left_edge_at_every_width() {
    for w in [960.0_f32, 1920.0] {
        let (mut controller, _handle, _dir) = settled_controller("rl10", [1, 1, 1, 1]);
        let mut page = Page::new();
        let out = page.frame(&mut controller, input_sized(w, 1200.0));
        let hs = headers(&out);
        out.drop_without_applying_deltas();
        assert_eq!(hs.len(), 4, "width {w}: {hs:?}");
        assert!(
            hs.iter().all(|h| (h.1[0] - hs[0].1[0]).abs() < 0.5),
            "width {w}: {hs:?}"
        );
    }
}

/// **RL11**: a new query resets the offset; Show more does not.
#[test]
fn rl11_offset_resets_on_new_query_not_on_show_more() {
    let (mut controller, handle, _dir) = active_controller("rl11");
    settle(
        &mut controller,
        &handle,
        "abba",
        sized_page([400, 0, 0, 0], 0, Some(400)),
    );
    let mut page = Page::new();
    page.frame(&mut controller, input_sized(960.0, 640.0))
        .drop_without_applying_deltas();
    page.seed_offset(800.0);
    page.frame(&mut controller, input_sized(960.0, 640.0))
        .drop_without_applying_deltas();
    assert!(
        (page.offset() - 800.0).abs() < 1.0,
        "sanity: {}",
        page.offset()
    );

    handle.script_search("abba", Ok(sized_page([400, 0, 0, 0], 400, None)));
    controller.search_show_more(SearchKind::Track);
    controller.tick();
    controller.tick();
    page.frame(&mut controller, input_sized(960.0, 640.0))
        .drop_without_applying_deltas();
    assert!(
        (page.offset() - 800.0).abs() < 1.0,
        "Show more keeps the offset: {}",
        page.offset()
    );

    let now = controller.now() + Duration::from_secs(1);
    handle.script_search("queen", Ok(sized_page([400, 0, 0, 0], 0, None)));
    controller.search_mut().set_query("queen", now);
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick();
    controller.tick();
    page.frame(&mut controller, input_sized(960.0, 640.0))
        .drop_without_applying_deltas();
    assert!(
        page.offset() < 1.0,
        "a new query resets to the top: {}",
        page.offset()
    );
}

// ---------------------------------------------------------------------------
// 026 US2 — query-field feedback (contracts/search-feedback.md F/C/S/N)
// ---------------------------------------------------------------------------

use egui::accesskit::Live;

/// One accesskit node with the fields the feedback contracts assert on.
#[derive(Debug, Clone)]
struct Snap {
    id: egui::accesskit::NodeId,
    role: Role,
    label: Option<String>,
    value: Option<String>,
    placeholder: Option<String>,
    live: Option<Live>,
    bounds: Option<[f64; 4]>,
}

struct Frame {
    nodes: Vec<Snap>,
    focus: egui::accesskit::NodeId,
}

impl Frame {
    fn find(&self, role: Role, label: &str) -> Option<&Snap> {
        self.nodes
            .iter()
            .find(|n| n.role == role && n.label.as_deref() == Some(label))
    }

    fn field(&self) -> &Snap {
        self.nodes
            .iter()
            .find(|n| n.role == Role::TextInput)
            .expect("the search field")
    }

    /// The count-line node: a `Role::Status` whose label is `text`.
    fn status(&self, text: &str) -> Option<&Snap> {
        self.find(Role::Status, text)
    }
}

fn snap(mut output: egui::FullOutput) -> Frame {
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();
    Frame {
        focus: update.focus,
        nodes: update
            .nodes
            .iter()
            .map(|(id, n)| Snap {
                id: *id,
                role: n.role(),
                label: n.label().map(str::to_string),
                value: n.value().map(str::to_string),
                placeholder: n.placeholder().map(str::to_string),
                live: n.live(),
                bounds: n.bounds().map(|b| [b.x0, b.y0, b.x1, b.y1]),
            })
            .collect(),
    }
}

fn count_text(n: usize) -> String {
    tr_args("search-result-count", &[("count", n.to_string())])
}

fn key_input(key: egui::Key) -> RawInput {
    let mut input = input_sized(960.0, 1200.0);
    input.events.push(Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    });
    input
}

/// Frame with the field focused (via the focus shortcut), then one more so
/// focus has settled.
fn focus_field(
    page: &mut Page,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
) -> Frame {
    page.focus = true;
    snap(page.frame(controller, input_sized(960.0, 1200.0)));
    snap(page.frame(controller, input_sized(960.0, 1200.0)))
}

/// **F1/F2**: no visible "Search" label; the field's own accessible name and
/// hint come from `search-field-label` / `search-hint`.
#[test]
fn f1_f2_field_is_named_once_with_a_hint() {
    let (mut controller, _handle, _dir) = active_controller("f1");
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(
        !frame
            .nodes
            .iter()
            .any(|n| n.role == Role::Label && n.value.as_deref() == Some("Search")),
        "no visible \"Search\" label"
    );
    let field = frame.field();
    assert_eq!(
        field.label.as_deref(),
        Some(tr("search-field-label").as_str())
    );
    assert_eq!(
        field.placeholder.as_deref(),
        Some(tr("search-hint").as_str())
    );
}

/// **C1/C2**: the clear control exists iff the raw query is non-empty
/// (whitespace counts) and sits at the field's trailing edge.
#[test]
fn c1_c2_clear_control_presence_and_placement() {
    let (mut controller, _handle, _dir) = active_controller("c1");
    let mut page = Page::new();
    let clear = tr("search-clear");

    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(
        frame.find(Role::Button, &clear).is_none(),
        "absent for \"\""
    );

    for q in [" ", "abba"] {
        let now = controller.now();
        controller.search_mut().set_query(q, now);
        let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
        let button = frame
            .find(Role::Button, &clear)
            .unwrap_or_else(|| panic!("present for {q:?}"));
        let field = frame.field();
        assert!(
            button.bounds.unwrap()[2] >= field.bounds.unwrap()[2] - 0.5,
            "clear control sits at the field's trailing edge: {button:?} vs {field:?}"
        );
    }
}

/// **C3**: Tab from the field lands on the clear control; Enter empties the
/// query.
#[test]
fn c3_keyboard_reaches_and_activates_clear() {
    let (mut controller, _handle, _dir) = active_controller("c3");
    let now = controller.now();
    controller.search_mut().set_query("abba", now);
    let mut page = Page::new();
    focus_field(&mut page, &mut controller);

    snap(page.frame(&mut controller, key_input(egui::Key::Tab)));
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    let button = frame.find(Role::Button, &tr("search-clear")).unwrap();
    assert_eq!(frame.focus, button.id, "Tab lands on the clear control");

    snap(page.frame(&mut controller, key_input(egui::Key::Enter)));
    assert_eq!(controller.search().raw_query(), "");
}

/// **F4**: Escape in the focused field empties the query (026 T043 M9 walk:
/// egui's `TextEdit` gives up focus on the Escape frame itself, so a
/// `has_focus()`-only check never fired).
#[test]
fn f4_escape_in_the_field_empties_the_query() {
    let (mut controller, _handle, _dir) = active_controller("f4");
    let now = controller.now();
    controller.search_mut().set_query("abba", now);
    let mut page = Page::new();
    focus_field(&mut page, &mut controller);

    snap(page.frame(&mut controller, key_input(egui::Key::Escape)));
    assert_eq!(controller.search().raw_query(), "");
}

/// **C4**: clicking clear empties the query (groups `Idle`) and the field
/// has focus on the next frame.
#[test]
fn c4_clear_resets_to_idle_and_refocuses_the_field() {
    let (mut controller, handle, _dir) = active_controller("c4");
    settle(
        &mut controller,
        &handle,
        "abba",
        sized_page([3, 0, 0, 0], 0, None),
    );
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    let b = frame
        .find(Role::Button, &tr("search-clear"))
        .unwrap()
        .bounds
        .unwrap();
    let pos = Pos2::new(((b[0] + b[2]) / 2.0) as f32, ((b[1] + b[3]) / 2.0) as f32);
    click_at(
        &page.ctx,
        &mut controller,
        &mut page.focus,
        &mut page.state,
        &mut page.memory,
        pos,
    );
    assert_eq!(controller.search().raw_query(), "");
    assert!(
        controller
            .search()
            .groups()
            .all(|(_, g)| matches!(g, modplayer_core::GroupState::Idle))
    );
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    let frame2 = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    let field = frame2.field();
    assert!(
        frame.focus == field.id || frame2.focus == field.id,
        "field refocused after clear"
    );
}

/// **S1–S3**: spinner appears on the frame the query changes and goes the
/// frame the reply lands; never for Show more; accessible.
#[test]
fn s1_s2_s3_spinner_follows_in_flight() {
    let (mut controller, handle, _dir) = active_controller("s2");
    let mut page = Page::new();
    let label = tr("search-in-flight");

    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(frame.find(Role::ProgressIndicator, &label).is_none());

    handle.script_search("abba", Ok(sized_page([20, 0, 0, 0], 0, Some(20))));
    let now = controller.now();
    controller.search_mut().set_query("abba", now);
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(
        frame.find(Role::ProgressIndicator, &label).is_some(),
        "spinner on the first frame after typing"
    );

    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick(); // request issued
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(frame.find(Role::ProgressIndicator, &label).is_some());
    controller.tick(); // reply applied
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(
        frame.find(Role::ProgressIndicator, &label).is_none(),
        "spinner gone on the reply frame"
    );

    // Show more never shows it.
    let mut more = sized_page([20, 0, 0, 0], 20, None);
    more.groups.truncate(1);
    handle.script_search("abba", Ok(more));
    controller.search_show_more(SearchKind::Track);
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(frame.find(Role::ProgressIndicator, &label).is_none());
}

/// **N1/N2**: status matrix and count arithmetic across Show more.
#[test]
fn n1_n2_count_line_matrix_and_arithmetic() {
    let (mut controller, handle, _dir) = active_controller("n1");
    let mut page = Page::new();

    // Idle: no count line.
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(frame.nodes.iter().all(|n| {
        n.label
            .as_deref()
            .is_none_or(|l| !l.ends_with("results") && l != "1 result")
    }));

    // Settled: 20 tracks (more available) + 5 albums → "25 results".
    let mut first = sized_page([20, 5, 0, 0], 0, Some(20));
    first.groups.truncate(2);
    settle(&mut controller, &handle, "abba", first);
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(frame.status(&count_text(25)).is_some(), "25 results");

    // Show more → "45 results".
    let mut more = sized_page([20, 0, 0, 0], 20, None);
    more.groups.truncate(1);
    handle.script_search("abba", Ok(more));
    controller.search_show_more(SearchKind::Track);
    controller.tick();
    controller.tick();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(frame.status(&count_text(45)).is_some(), "45 results");
}

#[test]
fn n1_count_line_hidden_when_offline_no_results_or_in_flight() {
    let (mut controller, handle, _dir) = active_controller("n1-hidden");
    let mut page = Page::new();
    let is_count = |f: &Frame| {
        f.nodes.iter().any(|n| {
            n.role == Role::Status
                && n.label
                    .as_deref()
                    .is_some_and(|l| l.contains("result") && !l.contains("No results"))
        })
    };

    // In flight.
    let now = controller.now();
    controller.search_mut().set_query("abba", now);
    assert!(!is_count(&snap(
        page.frame(&mut controller, input_sized(960.0, 1200.0))
    )));

    // No results.
    settle(
        &mut controller,
        &handle,
        "zzzzz",
        sized_page([0, 0, 0, 0], 0, None),
    );
    assert!(!is_count(&snap(
        page.frame(&mut controller, input_sized(960.0, 1200.0))
    )));

    // Offline.
    settle(
        &mut controller,
        &handle,
        "abba",
        sized_page([3, 0, 0, 0], 0, None),
    );
    controller.set_playback_permitted(false, None);
    controller.tick();
    assert!(!is_count(&snap(
        page.frame(&mut controller, input_sized(960.0, 1200.0))
    )));
}

/// **N3**: `Live::Polite` only on the first frame a generation settles; a
/// new query announces again.
#[test]
fn n3_count_is_announced_once_per_generation() {
    let (mut controller, handle, _dir) = active_controller("n3");
    let mut page = Page::new();
    let mut first = sized_page([20, 0, 0, 0], 0, Some(20));
    first.groups.truncate(1);
    settle(&mut controller, &handle, "abba", first);

    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    let node = frame.status(&count_text(20)).unwrap();
    assert_eq!(node.live, Some(Live::Polite), "announced on settle");

    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    let node = frame.status(&count_text(20)).unwrap();
    assert_ne!(node.live, Some(Live::Polite), "not re-announced");

    // Show more does not re-announce.
    let mut more = sized_page([20, 0, 0, 0], 20, None);
    more.groups.truncate(1);
    handle.script_search("abba", Ok(more));
    controller.search_show_more(SearchKind::Track);
    controller.tick();
    controller.tick();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert_ne!(
        frame.status(&count_text(40)).unwrap().live,
        Some(Live::Polite)
    );

    // A new query announces again.
    settle(
        &mut controller,
        &handle,
        "queen",
        sized_page([2, 0, 0, 0], 0, None),
    );
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert_eq!(
        frame.status(&count_text(2)).unwrap().live,
        Some(Live::Polite)
    );
}

/// **N4**: while a rate-limit strip is showing, the count line is hidden.
/// (The status strip, US3, takes the count line's place.)
#[test]
fn n4_count_hidden_while_rate_limited() {
    let (mut controller, handle, _dir) = active_controller("n4");
    let mut page = Page::new();
    let mut first = sized_page([20, 0, 0, 0], 0, Some(20));
    first.groups.truncate(1);
    settle(&mut controller, &handle, "abba", first);
    handle.script_search(
        "abba",
        Err(CatalogError::RateLimited {
            retry_after_ms: Some(30_000),
        }),
    );
    controller.search_show_more(SearchKind::Track);
    controller.tick();
    controller.tick();
    assert!(controller.search().refreshing());
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(frame.status(&count_text(20)).is_none());
}

// ---------------------------------------------------------------------------
// 026 US3 — stale / rate-limited strip and empty state
// (contracts/search-status.md V1–V7, E1–E3)
// ---------------------------------------------------------------------------

/// Settled 20-row Tracks page whose Show more was rate-limited: stale rows
/// under a scheduled retry (30 s away, so it stays scheduled in-test).
fn stale_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    modplayer_audio_source_synthetic::ScriptedHostHandle,
    TempDir,
) {
    let (mut controller, handle, dir) = active_controller(label);
    let mut first = sized_page([20, 0, 0, 0], 0, Some(20));
    first.groups.truncate(1);
    settle(&mut controller, &handle, "abba", first);
    handle.script_search(
        "abba",
        Err(CatalogError::RateLimited {
            retry_after_ms: Some(30_000),
        }),
    );
    controller.search_show_more(SearchKind::Track);
    controller.tick();
    controller.tick();
    assert!(controller.search().refreshing());
    assert!(controller.search().retry_scheduled());
    (controller, handle, dir)
}

/// A query whose combined request was rate-limited: nothing to show yet.
fn combined_limited_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    modplayer_audio_source_synthetic::ScriptedHostHandle,
    TempDir,
) {
    let (mut controller, handle, dir) = active_controller(label);
    handle.script_search(
        "abba",
        Err(CatalogError::RateLimited {
            retry_after_ms: Some(30_000),
        }),
    );
    let now = controller.now();
    controller.search_mut().set_query("abba", now);
    controller.set_clock(move || now + Duration::from_millis(200));
    controller.tick();
    controller.tick();
    assert!(controller.search().retry_scheduled());
    (controller, handle, dir)
}

fn show_more_label() -> String {
    tr_args("search-show-more", &[("group", tr("search-group-tracks"))])
}

fn all_shapes(output: &egui::FullOutput) -> Vec<egui::epaint::Shape> {
    fn flatten(shape: &egui::epaint::Shape, out: &mut Vec<egui::epaint::Shape>) {
        if let egui::epaint::Shape::Vec(v) = shape {
            for s in v {
                flatten(s, out);
            }
        } else {
            out.push(shape.clone());
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        flatten(&clipped.shape, &mut out);
    }
    out
}

/// **V1/V2**: strip visible iff rate-limited with a scheduled retry; the
/// text depends on whether stale rows are showing.
#[test]
fn v1_v2_strip_visibility_and_text() {
    let (mut controller, _handle, _dir) = stale_controller("v1-stale");
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(frame.status(&tr("search-stale")).is_some());
    assert!(frame.status(&tr("search-rate-limited")).is_none());

    let (mut controller, _handle, _dir) = combined_limited_controller("v2-none");
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(frame.status(&tr("search-rate-limited")).is_some());
    assert!(frame.status(&tr("search-stale")).is_none());

    let (mut controller, handle, _dir) = active_controller("v1-settled");
    settle(
        &mut controller,
        &handle,
        "abba",
        sized_page([3, 0, 0, 0], 0, None),
    );
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(frame.status(&tr("search-stale")).is_none());
    assert!(frame.status(&tr("search-rate-limited")).is_none());
}

/// **V3**: raised surface, rounded, warning glyph in the warning colour,
/// text in `text_primary`.
#[test]
fn v3_strip_uses_raised_surface_and_warning_glyph() {
    let (mut controller, _handle, _dir) = stale_controller("v3");
    let mut page = Page::new();
    let output = page.frame(&mut controller, input_sized(960.0, 1200.0));
    let roles = modplayer_ui::theme::roles(&page.ctx.global_style().visuals);
    let shapes = all_shapes(&output);
    output.drop_without_applying_deltas();
    let raised = shapes.iter().any(|s| {
        matches!(s, egui::epaint::Shape::Rect(r)
            if r.fill == roles.surface_raised
                && r.corner_radius == modplayer_ui::theme::tokens::radius::SM)
    });
    assert!(raised, "strip background is surface_raised with radius::SM");
    let colours_of = |needle: &str| -> Vec<egui::Color32> {
        shapes
            .iter()
            .filter_map(|s| match s {
                egui::epaint::Shape::Text(t) if t.galley.text().contains(needle) => Some(
                    t.galley
                        .job
                        .sections
                        .iter()
                        .map(|sec| sec.format.color)
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten()
            .collect()
    };
    assert!(
        colours_of("⚠").contains(&roles.warning),
        "glyph drawn in roles.warning"
    );
    assert!(
        colours_of(&tr("search-stale")).contains(&roles.text_primary),
        "text drawn in text_primary"
    );
}

/// **V4**: one `Status` node whose label is the text alone; no
/// notification raised.
#[test]
fn v4_strip_is_one_status_node_and_raises_no_notification() {
    let (mut controller, _handle, _dir) = stale_controller("v4");
    let before = controller.notifications().all().count();
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    let strip: Vec<_> = frame
        .nodes
        .iter()
        .filter(|n| n.role == Role::Status && n.label.as_deref() == Some(&tr("search-stale")))
        .collect();
    assert_eq!(strip.len(), 1);
    assert!(
        frame
            .nodes
            .iter()
            .all(|n| !n.label.as_deref().unwrap_or("").contains('⚠')),
        "glyph excluded from accessible names"
    );
    assert_eq!(controller.notifications().all().count(), before);
}

/// **V5**: stale rows and header counts stay; no Show more.
#[test]
fn v5_stale_rows_keep_counts_and_lose_show_more() {
    let (mut controller, _handle, _dir) = stale_controller("v5");
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert_eq!(
        frame
            .nodes
            .iter()
            .filter(|n| n.role == Role::ListItem)
            .count(),
        20
    );
    assert!(
        frame
            .find(Role::Header, &group_label("search-group-tracks", 20))
            .is_some()
    );
    assert!(frame.find(Role::Button, &show_more_label()).is_none());
}

/// **V6**: nothing stale → no groups, spinner or count line.
#[test]
fn v6_nothing_stale_draws_no_groups_spinner_or_count() {
    let (mut controller, _handle, _dir) = combined_limited_controller("v6");
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(!frame.nodes.iter().any(|n| n.role == Role::Header));
    assert!(
        !frame
            .nodes
            .iter()
            .any(|n| n.role == Role::ProgressIndicator)
    );
    assert!(!frame.nodes.iter().any(|n| {
        n.label
            .as_deref()
            .is_some_and(|l| l.contains("results") && n.role == Role::Status)
    }));
}

/// **V7**: the old `refreshing` Status node no longer exists in Search.
#[test]
fn v7_old_refreshing_status_is_gone() {
    let (mut controller, _handle, _dir) = stale_controller("v7");
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(frame.status(&tr("refreshing")).is_none());
}

fn no_results_controller(
    label: &str,
    query: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    modplayer_audio_source_synthetic::ScriptedHostHandle,
    TempDir,
) {
    let (mut controller, handle, dir) = active_controller(label);
    settle(
        &mut controller,
        &handle,
        query,
        sized_page([0, 0, 0, 0], 0, None),
    );
    assert!(controller.search().is_no_results());
    (controller, handle, dir)
}

/// **E1**: a 200-char query is truncated to 60 scalars + "…" and the
/// message wraps within the body measure.
#[test]
fn e1_long_query_is_truncated_and_wrapped() {
    let query = "q".repeat(200);
    let (mut controller, _handle, _dir) = no_results_controller("e1", &query);
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(1400.0, 1200.0)));
    let shown = format!("{}…", "q".repeat(60));
    let expected = tr_args("search-no-results", &[("query", shown)]);
    let node = frame
        .nodes
        .iter()
        .find(|n| {
            n.label.as_deref() == Some(expected.as_str())
                || n.value.as_deref() == Some(expected.as_str())
        })
        .unwrap_or_else(|| panic!("no-results message {expected:?}"));
    let b = node.bounds.unwrap();
    let measure = f64::from(modplayer_ui::theme::body_measure(&page.ctx));
    assert!(
        b[2] - b[0] <= measure + 1.0,
        "width {} > {measure}",
        b[2] - b[0]
    );
}

/// **E1** with the live source's shape (026 T043 M9 walk): the source
/// answers Tracks only and reports the other three kinds `Unsupported`. An
/// empty Tracks group must still reach the empty state — unsupported groups
/// are omitted (V3), not "has results".
#[test]
fn e1_empty_state_shows_when_other_groups_are_unsupported() {
    let (mut controller, handle, _dir) = active_controller("e1-unsupported");
    let mut p = sized_page([0, 0, 0, 0], 0, None);
    p.groups.retain(|g| g.kind == SearchKind::Track);
    p.unsupported = vec![SearchKind::Album, SearchKind::Artist, SearchKind::Playlist];
    settle(&mut controller, &handle, "qqqq", p);
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 640.0)));
    let expected = tr_args("search-no-results", &[("query", "qqqq".to_string())]);
    assert!(
        frame.nodes.iter().any(|n| {
            n.label.as_deref() == Some(expected.as_str())
                || n.value.as_deref() == Some(expected.as_str())
        }),
        "no-results message {expected:?}"
    );
    empty_state_clear(&frame);
}

/// The empty-state Clear button: the `search-clear` button lowest on screen
/// (the field's trailing × sits above it).
fn empty_state_clear(frame: &Frame) -> [f64; 4] {
    frame
        .nodes
        .iter()
        .filter(|n| n.role == Role::Button && n.label.as_deref() == Some(&tr("search-clear")))
        .filter_map(|n| n.bounds)
        .max_by(|a, b| a[1].total_cmp(&b[1]))
        .expect("empty-state clear button")
}

/// **E2**: one click clears the query and refocuses the field.
#[test]
fn e2_clear_search_button_empties_and_refocuses() {
    let (mut controller, _handle, _dir) = no_results_controller("e2", "zzzz");
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    let b = empty_state_clear(&frame);
    let trailing = frame
        .find(Role::Button, &tr("search-clear"))
        .map(|n| n.bounds.unwrap());
    assert!(trailing.is_some());
    let pos = Pos2::new(((b[0] + b[2]) / 2.0) as f32, ((b[1] + b[3]) / 2.0) as f32);
    click_at(
        &page.ctx,
        &mut controller,
        &mut page.focus,
        &mut page.state,
        &mut page.memory,
        pos,
    );
    assert_eq!(controller.search().raw_query(), "");
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    let frame2 = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(
        frame.focus == frame2.field().id || frame2.focus == frame2.field().id,
        "field refocused"
    );
}

/// **E3**: no count line, spinner or strip in the empty state.
#[test]
fn e3_empty_state_has_no_count_spinner_or_strip() {
    let (mut controller, _handle, _dir) = no_results_controller("e3", "zzzz");
    let mut page = Page::new();
    let frame = snap(page.frame(&mut controller, input_sized(960.0, 1200.0)));
    assert!(
        !frame
            .nodes
            .iter()
            .any(|n| n.role == Role::ProgressIndicator)
    );
    assert!(!frame.nodes.iter().any(|n| n.role == Role::Status
        && n.label.as_deref().is_some_and(|l| l.contains("results")
            || l == tr("search-stale")
            || l == tr("search-rate-limited"))));
}
