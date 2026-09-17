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
use egui::{Context, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{
    Availability, CatalogError, SearchGroupPage, SearchHit, SearchKind, SearchPage, TrackId,
    TrackRef,
};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{PlaybackController, tr, tr_args};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};

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
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut artwork = modplayer_ui::artwork::ArtworkCache::new();
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::search_view::show(ui, controller, &mut artwork, focus);
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
        "the Tracks header must render even while its rows are skeletons"
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
        has(nodes.as_slice(), Role::Header, &tr("search-group-tracks")),
        "Tracks header must render"
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
        has(nodes.as_slice(), Role::Status, &tr("refreshing")),
        "expected a \"Refreshing…\" status node: {nodes:?}"
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
