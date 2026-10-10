// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! 028-settings-fields-and-account, US3 (contracts/settings-fields.md
//! F16–F20, A8): choosing a descriptor search result opens its category,
//! scrolls to, focuses and outlines the whole field.

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use egui::{Context, Event, Key, Modifiers, Pos2, RawInput, Rect, Shape};
use modplayer_account::AccountService;
use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::settings::SettingsStore;
use modplayer_core::settings_registry::SettingsCategory;
use modplayer_core::{PlaybackController, tr};
use modplayer_ui::section_memory::SectionMemory;
use modplayer_ui::settings::{SettingsScreen, field};
use modplayer_ui::theme::tokens;

type Controller = PlaybackController<FakeBackend, ScriptedHost>;

struct Fixture {
    ctx: Context,
    controller: Controller,
    account: AccountService,
    screen: SettingsScreen,
    memory: SectionMemory,
    dir: PathBuf,
    time: f64,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Frame {
    focus: Option<u64>,
    labels: Vec<(u64, String, Option<Rect>)>,
    shapes: Vec<Shape>,
}

impl Fixture {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-search-highlight-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let store = SettingsStore::with_path(dir.join("settings.toml"));
        let controller =
            PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
        let ctx = Context::default();
        modplayer_ui::theme::apply_tokens(&ctx);
        ctx.enable_accesskit();
        let screen = SettingsScreen::new(&controller);
        Self {
            ctx,
            controller,
            account: common::active_account(label),
            screen,
            memory: SectionMemory::default(),
            dir,
            time: 1.0,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> Frame {
        self.time += 0.05;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(960.0, 640.0))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let Self {
            ctx,
            controller,
            account,
            screen,
            memory,
            ..
        } = self;
        let mut output = ctx.run_ui(input, |ui| {
            let _ = modplayer_ui::settings::show(ui, controller, account, screen, memory);
        });
        let update = output
            .platform_output
            .accesskit_update
            .take()
            .expect("accesskit enabled");
        let shapes = output.shapes.iter().map(|c| c.shape.clone()).collect();
        output.drop_without_applying_deltas();
        Frame {
            focus: Some(update.focus.0),
            labels: update
                .nodes
                .iter()
                .map(|(id, n)| {
                    (
                        id.0,
                        n.label()
                            .or_else(|| n.value())
                            .unwrap_or_default()
                            .to_string(),
                        n.bounds().map(|b| {
                            Rect::from_min_max(
                                Pos2::new(b.x0 as f32, b.y0 as f32),
                                Pos2::new(b.x1 as f32, b.y1 as f32),
                            )
                        }),
                    )
                })
                .collect(),
            shapes,
        }
    }

    /// Type `query` in the search box, then choose the result whose text
    /// is `result` with the keyboard. Returns the frame of the choice.
    fn search_and_choose(&mut self, query: &str, result: &str) -> Frame {
        let _ = self.frame(vec![]);
        let _ = self.frame(vec![key(Key::Tab)]); // search box
        let _ = self.frame(vec![Event::Text(query.to_string())]);
        let mut found = false;
        for _ in 0..6 {
            let frame = self.frame(vec![key(Key::Tab)]);
            let focused = frame
                .labels
                .iter()
                .find(|(id, ..)| Some(*id) == frame.focus)
                .map(|(_, t, _)| t.clone());
            if focused.as_deref() == Some(result) {
                found = true;
                break;
            }
        }
        assert!(found, "result `{result}` never took focus");
        self.frame(vec![key(Key::Enter)])
    }
}

fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn result_text(category: SettingsCategory, title_key: &str) -> String {
    format!("{} › {}", tr(category.label_key()), tr(title_key))
}

fn has_accent_outline(frame: &Frame, ctx: &Context) -> bool {
    let roles = tokens::roles(&ctx.global_style().visuals);
    frame.shapes.iter().any(|shape| match shape {
        Shape::Rect(r) => r.stroke.width >= 2.0 && r.stroke.color == roles.accent,
        _ => false,
    })
}

fn assert_highlighted(label: &str, category: SettingsCategory, title_key: &str, id: &str) {
    let mut fx = Fixture::new(label);
    let frame = fx.search_and_choose(&tr(title_key), &result_text(category, title_key));
    assert_eq!(fx.screen.category(), category, "F17: category selected");
    let rect = field::painted_highlight(&fx.ctx)
        .unwrap_or_else(|| panic!("F17/F18: highlight drawn for {id}"));
    let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(960.0, 640.0));
    assert!(
        viewport.contains_rect(rect),
        "F17: field rect {rect:?} fully inside {viewport:?}"
    );
    let focused = frame
        .labels
        .iter()
        .find(|(fid, ..)| Some(*fid) == frame.focus)
        .and_then(|(_, _, b)| *b)
        .expect("F17: a control is focused");
    assert!(
        rect.expand(1.0).contains_rect(focused),
        "F17: the focused control {focused:?} is inside the highlight {rect:?}"
    );
    assert!(
        has_accent_outline(&frame, &fx.ctx),
        "F18: accent stroke >= 2px"
    );
}

#[test]
fn f16_result_text_is_category_arrow_field() {
    let mut fx = Fixture::new("f16");
    let _ = fx.frame(vec![]);
    let _ = fx.frame(vec![key(Key::Tab)]);
    let _ = fx.frame(vec![Event::Text("nudge".to_string())]);
    let frame = fx.frame(vec![]);
    let expected = result_text(SettingsCategory::Playback, "setting-nudge-step");
    assert!(
        frame.labels.iter().any(|(_, t, _)| *t == expected),
        "missing `{expected}`"
    );
}

#[test]
fn f17_audio_result_highlights_the_field() {
    assert_highlighted(
        "audio",
        SettingsCategory::Audio,
        "setting-limiter-ceiling",
        "audio.limiter_ceiling",
    );
}

#[test]
fn f17_playback_result_highlights_the_field() {
    assert_highlighted(
        "playback",
        SettingsCategory::Playback,
        "setting-nudge-step",
        "markers.nudge_step_ms",
    );
}

#[test]
fn f17_appearance_result_highlights_the_field() {
    assert_highlighted(
        "appearance",
        SettingsCategory::Appearance,
        "setting-theme",
        "appearance.theme",
    );
}

#[test]
fn f17_language_result_highlights_the_field() {
    assert_highlighted(
        "language",
        SettingsCategory::Language,
        "setting-locale",
        "language.locale",
    );
}

#[test]
fn a8_account_results_highlight_the_button() {
    assert_highlighted(
        "recheck",
        SettingsCategory::Account,
        "account-recheck",
        "account.recheck_subscription",
    );
    assert_highlighted(
        "signout",
        SettingsCategory::Account,
        "account-sign-out",
        "account.sign_out",
    );
}

#[test]
fn f19_highlight_expires_without_input() {
    let mut fx = Fixture::new("expire");
    let _ = fx.search_and_choose(
        &tr("setting-nudge-step"),
        &result_text(SettingsCategory::Playback, "setting-nudge-step"),
    );
    assert!(field::painted_highlight(&fx.ctx).is_some());
    let _ = fx.frame(vec![]); // arms and starts the clock
    assert!(field::painted_highlight(&fx.ctx).is_some());
    fx.time += 3.5;
    let _ = fx.frame(vec![]);
    let _ = fx.frame(vec![]);
    assert!(
        field::painted_highlight(&fx.ctx).is_none(),
        "gone after 3 s"
    );
}

#[test]
fn f19_highlight_clears_on_first_key_press() {
    let mut fx = Fixture::new("keyclear");
    let _ = fx.search_and_choose(
        &tr("setting-nudge-step"),
        &result_text(SettingsCategory::Playback, "setting-nudge-step"),
    );
    let _ = fx.frame(vec![]); // arms
    let _ = fx.frame(vec![key(Key::ArrowDown)]);
    let _ = fx.frame(vec![]);
    assert!(field::painted_highlight(&fx.ctx).is_none());
}

#[test]
fn f20_empty_query_has_no_highlight() {
    let mut fx = Fixture::new("empty");
    let _ = fx.frame(vec![]);
    let _ = fx.frame(vec![]);
    assert!(field::painted_highlight(&fx.ctx).is_none());
}
