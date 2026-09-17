// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! FR-022/T062 (contracts/ui-surface.md §8): the 003 "Play from account"
//! developer scaffold is gone — no control that starts, reports on, or
//! names that feature renders in Settings › Developer any more. The real
//! replacement is the Library view (`library_view.rs`, US2).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui::accesskit::Role;
use egui::{Context, Pos2, RawInput, Rect};
use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::SyntheticHost;
use modplayer_core::{PlaybackController, SettingsStore};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-developer-removed-{label}-{}-{unique}",
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
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

/// Render `settings::developer::show` and collect every accesskit node's
/// role + accessible name, mirroring `search_view.rs`'s `render_nodes`.
fn render_developer_nodes() -> Vec<Node> {
    let (store, _dir) = fresh_store("nodes");
    let mut controller =
        PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);

    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::settings::developer::show(ui, &mut controller);
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

#[test]
fn no_play_from_account_control_renders() {
    let nodes = render_developer_nodes();
    assert!(
        !nodes.iter().any(|n| {
            n.accessible_name()
                .is_some_and(|name| name.to_ascii_lowercase().contains("play from account"))
        }),
        "expected no \"Play from account\" control anywhere in Settings › \
         Developer (FR-022): {nodes:?}"
    );
}

#[test]
fn only_the_three_sample_notification_buttons_render() {
    let nodes = render_developer_nodes();
    let buttons: Vec<&str> = nodes
        .iter()
        .filter(|n| n.role == Role::Button)
        .filter_map(|n| n.accessible_name())
        .collect();
    assert_eq!(
        buttons,
        vec!["Critical", "Warning", "Info"],
        "Settings › Developer must render exactly the three sample-notification \
         buttons and nothing else (the scaffold's own button is gone): {nodes:?}"
    );
}
