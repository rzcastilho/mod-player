// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Now Playing screen (contracts/ui-surface.md §1, extended by
//! 005-now-playing-waveform's contracts/ui-waveform.md §1, and by
//! 021-transport-bar-and-panel-layout's contracts/ui-now-playing-
//! layout.md): a pinned transport bar (identity, transport, time, level,
//! panel toggles) drawn with `egui::Panel::top`, followed by one scroll
//! region holding the status line/transfer banner, the waveform, and the
//! four collapsible panel cards (Markers, Effect Chain, Transport focus,
//! Queue) in that order. The bar never scrolls away (FR-001, FR-010); the
//! waveform's height still follows 018's unchanged formula (FR-003,
//! FR-011).

use std::time::Duration;

use egui::scroll_area::{DragScroll, ScrollSource};
use egui::{Button, Label, RichText, Sense, TextWrapMode, Ui, Vec2};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{SourceHealth, SourceHost};
use modplayer_core::{
    ActiveState, AnalysisStatus, Intent, NowPlayingPanel, PlaybackController, tr, tr_args,
};

use crate::artwork::{ArtworkCache, ArtworkState};
use crate::effects_view;
use crate::layout::{self, DockPresentation};
use crate::markers;
use crate::plugin_overlays::{self, ViewKind};
use crate::plugin_panels;
use crate::queue_view;
use crate::section_memory::{SectionMemory, ViewKey};
use crate::theme;
use crate::transport_view;
use crate::waveform::{
    self, DetailWindow, DragOrigin, DragPreview, WaveformEvent, WaveformPaint, WaveformState,
};
use crate::widgets::controls::{SwitchKind, collapsible_panel_card, switch};
use crate::widgets::initials::initials_placeholder;
use crate::widgets::{peak_meter, volume};

/// The bar identity group's artwork square side length
/// (021-transport-bar-and-panel-layout, contract B2.1, research R4) —
/// smaller than the old 96px heading artwork it replaces (FR-018).
const ARTWORK_SIZE: f32 = 40.0;

/// `Q` (007, `HostAction::ToggleQueue`, contracts/ui-actions.md §3):
/// flips the persisted `[now_playing_panels] queue_open` flag through the
/// controller, so a keyboard toggle and the header switch stay in sync and
/// the state survives a restart (016-list-row-and-panel-components,
/// FR-019). Returns the panel's new open state
/// (021-transport-bar-and-panel-layout, contract R5, research R6): `true`
/// tells the caller to also [`request_reveal`] it.
pub fn toggle_queue_panel<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
) -> bool {
    let open = controller.now_playing_panel_open(NowPlayingPanel::Queue);
    let new_open = !open;
    controller.set_now_playing_panel_open(NowPlayingPanel::Queue, new_open);
    new_open
}

/// The egui temp-memory id [`RevealRequest`] lives under (data-model.md
/// §4, research R6) — a fixed `Id`, like [`plugin_panels::overlay_open_
/// key`]'s sibling pattern.
fn reveal_request_id() -> egui::Id {
    egui::Id::new("now-playing-reveal")
}

/// A one-shot request to scroll `panel`'s card into view, recorded at
/// `pass` (data-model.md §4, research R6). Never persisted, and never
/// mirrors a panel's own open/closed state (016 contract P1).
#[derive(Debug, Clone, Copy, PartialEq)]
struct RevealRequest {
    panel: NowPlayingPanel,
    pass: u64,
}

/// Record a reveal request for `panel` (data-model.md §4, research R6):
/// called when the bar's toggle or a `Toggle*` keyboard action turns the
/// panel **on** this pass. A newer request replaces an older one — the
/// most recently opened panel wins (spec Clarification 9, contract R6).
pub fn request_reveal(ctx: &egui::Context, panel: NowPlayingPanel) {
    let pass = ctx.cumulative_pass_nr();
    ctx.memory_mut(|memory| {
        memory
            .data
            .insert_temp(reveal_request_id(), RevealRequest { panel, pass });
    });
}

/// Consume the reveal request for `panel`, if any (data-model.md §4,
/// research R6). Returns `true` only when a request for `panel` exists and
/// is still within its two-pass window
/// (`ctx.cumulative_pass_nr() - pass <= 1`); a request for a *different*
/// panel is left untouched so a later call for that panel can still claim
/// it. Either way, a matching request is always removed, so it is never
/// honoured twice.
fn take_reveal(ctx: &egui::Context, panel: NowPlayingPanel) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.memory_mut(|memory| {
        let request = memory.data.get_temp::<RevealRequest>(reveal_request_id());
        match request {
            Some(request) if request.panel == panel => {
                memory.data.remove::<RevealRequest>(reveal_request_id());
                pass.saturating_sub(request.pass) <= 1
            }
            _ => false,
        }
    })
}

/// Scroll `card_rect` into the scroll region's own viewport when a reveal
/// was requested for `panel` (contracts/ui-now-playing-layout.md R1-R3,
/// R6): `layout::reveal_align` decides whether, and how, to scroll — `None`
/// leaves the offset untouched when the card is already fully visible
/// (R2). This holds for Queue, Effects and Transport; Markers has no bar
/// toggle and is never revealed (contract R1).
fn reveal_if_requested(ui: &Ui, panel: NowPlayingPanel, card_rect: egui::Rect) {
    if take_reveal(ui.ctx(), panel)
        && let Some(align) = layout::reveal_align(card_rect.y_range(), ui.clip_rect().y_range())
    {
        ui.scroll_to_rect(card_rect, align);
    }
}

/// Draw the Now Playing screen, applying any transport/volume/seek change
/// directly to `controller`. `waveform` is the session's own waveform
/// widget state (drag preview, detail window — `App`-owned, like
/// `artwork`). `memory` is `App`'s own `SectionMemory`
/// (021-transport-bar-and-panel-layout, research R2, contract S4): Now
/// Playing's single scroll region is recorded/restored through
/// `ViewKey::NowPlaying`, exactly like Search and Plugins, so it keeps its
/// offset across section switches and resets on sign-out with every other
/// section's scroll state.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    memory: &mut SectionMemory,
) {
    // 018-window-sizing-and-responsive-dock (contract D8, research R11,
    // FR-012), unchanged by 021 (contract S2, research R3): captured as
    // the very first statement — the `CentralPanel` inner height,
    // unaffected by anything drawn into `ui` below (dock, bar, scroll
    // area) — and resolved once per frame into the overview/detail
    // heights `show_waveform` allocates. Independent of scroll offset and
    // of the bar's own (possibly wrapped) height.
    let waveform_h = ui.max_rect().height();
    let (overview_height, detail_height) = layout::waveform_heights(waveform_h);

    // A track change drops any in-flight drag preview from the previous
    // track (data-model.md §5.2) and re-centres the detail window on the
    // new track's playhead, keeping the previous width if any
    // (contracts/ui-waveform.md §5) — `show_waveform` applies the latter
    // once it has computed `playhead_frame`/`len_frames`.
    let current_id = controller.current_track().map(|item| item.track.id.clone());
    let track_changed = current_id != waveform.last_track;
    if track_changed {
        waveform.drag = None;
        waveform.last_track = current_id;
    }

    // 018-window-sizing-and-responsive-dock (contract D1, data-model.md
    // §3): captured before anything else draws into `ui` — both
    // `show_dock`'s docked column and `show_overlay`'s right-anchored
    // `Area` need the Now Playing content `Ui`'s own rect, and only the
    // former (if either runs) actually consumes `ui` below.
    let content_rect = ui.max_rect();
    let presentation = plugin_panels::dock_presentation(ui.ctx(), controller);
    let mut focus_panels_toggle = plugin_panels::continue_focus_past_dock(ui.ctx(), presentation);

    // 011-plugin-ui-contributions, contracts/ui-panels.md L1 (018-window-
    // sizing-and-responsive-dock, contract D1): the docked column, when
    // drawn, must draw first, exactly like `app.rs`'s own nav rail
    // (`Panel::left`), so the rest of this content correctly sees the
    // narrower remaining width. Below 1024 pt wide it auto-hides instead
    // (`Hidden`), reachable through the "Panels" toggle in the transport
    // bar below as a dismissible overlay (`Overlay`) drawn over this same
    // content rect rather than narrowing it.
    match presentation {
        DockPresentation::Docked => plugin_panels::show_dock(ui, controller),
        DockPresentation::Overlay => {
            if plugin_panels::show_overlay(ui.ctx(), controller, content_rect) {
                focus_panels_toggle = true;
            }
        }
        DockPresentation::Hidden | DockPresentation::None => {}
    }

    let available = controller.transport_enabled();

    // 018-window-sizing-and-responsive-dock (contract D1/D4, research R8/
    // R12), unchanged by 021: while `Overlay` is open, it covers the
    // right portion of this same content rect but, being an `Area`, never
    // reflows `ui` itself — so each bar group below pre-measures its own
    // width and forces itself onto a fresh row whenever it would cross
    // this boundary. Every other presentation still needs the same
    // pre-measure against the row's own right edge.
    let wrap_boundary = match presentation {
        DockPresentation::Overlay | DockPresentation::Docked => {
            content_rect.right()
                - plugin_panels::live_dock_width(ui.ctx(), controller, content_rect.width())
        }
        DockPresentation::Hidden | DockPresentation::None => ui.max_rect().right(),
    };

    // 018-window-sizing-and-responsive-dock (contract D1/D4, FR-007): the
    // "Panels" toggle is the bar's own last group, present only while the
    // dock is auto-hidden (`Hidden`/`Overlay`) — absent at ≥ 1024 pt wide
    // or with nothing docked (`Docked`/`None`).
    let show_panels_toggle = matches!(
        presentation,
        DockPresentation::Hidden | DockPresentation::Overlay
    );

    // 021-transport-bar-and-panel-layout (contract B1, research R1): the
    // pinned bar, drawn after the dock and before the scroll region — a
    // `Panel::top` never scrolls away, and its height comes only from its
    // own content (contract B3).
    show_transport_bar(
        ui,
        controller,
        artwork,
        waveform,
        available,
        wrap_boundary,
        show_panels_toggle,
        focus_panels_toggle,
    );

    // 021-transport-bar-and-panel-layout (contract S1-S5, research R2):
    // the single vertical scroll region, `SectionMemory`-backed exactly
    // like Search/Plugins — no drag source (waveform drags, marker
    // handles, effect-chain knobs and the volume slider are all drag
    // targets of their own), not animated (the reveal in a later user
    // story applies in the same pass).
    let key = ViewKey::NowPlaying;
    let scroll_area = memory
        .scroll_area(&key)
        .animated(false)
        .scroll_source(ScrollSource {
            drag: DragScroll::Never,
            ..ScrollSource::ALL
        });
    let has_track = controller.current_track().is_some();
    // A vertical-only egui scroll area widens its clip on the horizontal axis
    // to its parent's clip, so any card content wider than the centre
    // column painted over the docked plugin panels (2026-09-28 manual
    // walk, research R15). Pin the clip to the region left of the dock.
    let scroll_region = ui.available_rect_before_wrap();
    ui.set_clip_rect(ui.clip_rect().intersect(scroll_region));
    let output = scroll_area.show(ui, |ui| {
        show_status_line(ui, controller);
        show_transfer_banner(ui, controller);

        // 006/010, contracts/ui-markers.md §1, ui-waveform.md: drawn only
        // with a track loaded (US1 Scenario, FR-003/FR-011). With no
        // track, the bar's own identity group already shows `now-playing-
        // empty` (contract B7); this hint (unchanged from the old
        // `show_heading`'s empty branch) tells the user what to do next.
        if has_track {
            show_waveform(
                ui,
                controller,
                waveform,
                available,
                track_changed,
                overview_height,
                detail_height,
            );
        } else {
            ui.label(tr("now-playing-pick-a-track"));
        }

        // 021 contract S3: the waveform (or, with no track, the banner/
        // status line) and the first card are separated by `space::XL`.
        ui.add_space(theme::space::XL);

        // 021 contract C4, data-model.md §1: `NowPlayingPanel::RENDER_
        // ORDER` is Markers, Effect Chain, Transport, Queue. Markers
        // renders only with a track (contract C4); the other three
        // always render, either open or as a header-only collapsed card
        // (contract C2, superseding 016 C10).
        if has_track {
            let mut markers_open = controller.now_playing_panel_open(NowPlayingPanel::Markers);
            let response = markers::panel(ui, controller, waveform, &mut markers_open);
            if response.toggled {
                // 021 contract C3, research R7: a header click writes the
                // flag through the single controller setter and discards
                // this pass, so the (non-existent, for Markers) bar state
                // and the card agree in the same output for every panel
                // that does have one.
                controller.set_now_playing_panel_open(NowPlayingPanel::Markers, markers_open);
                ui.ctx().request_discard("panel header toggle");
            }
            markers::handle_focused_marker_keys(ui, controller, waveform);
            ui.add_space(theme::space::XL);
        }

        let mut effects_open = controller.now_playing_panel_open(NowPlayingPanel::EffectChain);
        let effects_response = effects_view::show(ui, controller, &mut effects_open);
        if effects_response.toggled {
            controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, effects_open);
            ui.ctx().request_discard("panel header toggle");
        }
        effects_view::handle_focused_handle_keys(ui, controller);
        // 021 contract R1-R3, R6: reveal after the card is drawn, so
        // `effects_response.rect` reflects this pass's actual layout.
        reveal_if_requested(ui, NowPlayingPanel::EffectChain, effects_response.rect);
        ui.add_space(theme::space::XL);

        let mut transport_open = controller.now_playing_panel_open(NowPlayingPanel::Transport);
        let transport_response = transport_view::show(ui, controller, &mut transport_open);
        if transport_response.toggled {
            controller.set_now_playing_panel_open(NowPlayingPanel::Transport, transport_open);
            ui.ctx().request_discard("panel header toggle");
        }
        reveal_if_requested(ui, NowPlayingPanel::Transport, transport_response.rect);
        ui.add_space(theme::space::XL);

        // 021 contract C1/C4: the Queue card's chrome is owned by this
        // caller — `queue_view::show` draws the body only (header
        // controls: shuffle, repeat).
        let mut queue_open = controller.now_playing_panel_open(NowPlayingPanel::Queue);
        let queue_response =
            collapsible_panel_card(ui, &tr("queue-panel-title"), &mut queue_open, |ui| {
                queue_view::show(ui, controller, artwork);
            });
        if queue_response.toggled {
            controller.set_now_playing_panel_open(NowPlayingPanel::Queue, queue_open);
            ui.ctx().request_discard("panel header toggle");
        }
        reveal_if_requested(ui, NowPlayingPanel::Queue, queue_response.rect);
    });
    memory.record(key, output.state.offset.y);

    // contracts/ui-panels.md L2: floated panels last (position-independent
    // overlay windows; drawn after all other Now Playing content).
    plugin_panels::show_floated_windows(ui.ctx(), controller);

    // Position advances at the audio-clock rate (>= 60 Hz, FR-006/SC-003)
    // independent of egui's own input-driven repaint cadence; keep the
    // screen repainting while playing so the readout/waveform playhead
    // stay live even with no mouse/keyboard activity.
    if controller.transport_state().intent == Intent::Playing {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

/// The pinned transport bar (021-transport-bar-and-panel-layout, contracts/
/// ui-now-playing-layout.md B1-B7, research R1/R4): one `horizontal_wrapped`
/// row of atomic groups — identity, transport, time, level, the Queue/
/// Effects/Transport toggles (`space::XL`-separated from the level group,
/// FR-019), then 018's Panels toggle under its own conditions. Each group is
/// pre-measured and, via [`wrap_group_before`]/[`wrap_switch_before`], moved
/// to a fresh row whole rather than letting any control's label truncate
/// (FR-017).
#[allow(clippy::too_many_arguments)]
fn show_transport_bar<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    waveform: &WaveformState,
    available: bool,
    wrap_boundary: f32,
    show_panels_toggle: bool,
    focus_panels_toggle: bool,
) {
    let mut queue_open = controller.now_playing_panel_open(NowPlayingPanel::Queue);
    let mut effects_open = controller.now_playing_panel_open(NowPlayingPanel::EffectChain);
    let mut transport_open = controller.now_playing_panel_open(NowPlayingPanel::Transport);
    let mut panels_open = plugin_panels::overlay_open(ui.ctx());

    egui::Panel::top("now-playing-transport-bar")
        .resizable(false)
        .show(ui, |ui| {
            // research R4: a fixed width budget, independent of the title/
            // artist text (P-I3) — measured once, before any group draws.
            let bar_width = ui.available_width();
            let identity_column_width = layout::identity_width(bar_width);

            let toggled = ui
                .horizontal_wrapped(|ui| {
                    // 1. identity: 40px artwork + fixed-width title/artist
                    // column (contract B2.1, B4, B7).
                    let identity_group_width =
                        ARTWORK_SIZE + ui.spacing().item_spacing.x + identity_column_width;
                    wrap_group_before(ui, wrap_boundary, identity_group_width);
                    show_identity(ui, controller, artwork, identity_column_width);

                    // 2. transport: skip back, play/pause, stop, skip
                    // forward (contract B2.2).
                    let transport_group_width = transport_group_width(ui, controller);
                    wrap_group_before(ui, wrap_boundary, transport_group_width);
                    show_transport_controls(ui, controller, available);

                    // 3. time: elapsed / remaining (contract B2.3).
                    let (elapsed_text, remaining_text) = time_texts(controller, waveform);
                    let spacing = ui.spacing().item_spacing.x;
                    let time_group_width =
                        measure_text_width(ui, &elapsed_text, egui::TextStyle::Monospace)
                            + spacing
                            + measure_text_width(ui, &remaining_text, egui::TextStyle::Monospace);
                    wrap_group_before(ui, wrap_boundary, time_group_width);
                    ui.horizontal(|ui| {
                        ui.label(theme::mono_text(elapsed_text));
                        ui.label(theme::mono_text(remaining_text));
                    });

                    // 4. level: master volume over the peak meter
                    // (contract B2.4).
                    let level_group_width = level_group_width(ui, controller);
                    wrap_group_before(ui, wrap_boundary, level_group_width);
                    ui.vertical(|ui| {
                        if let Some(new_volume) =
                            volume::master_volume(ui, controller.master_volume())
                        {
                            controller.set_master_volume(new_volume);
                        }
                        let peak = controller.shared().peak();
                        let ceiling_db = controller.ceiling().db();
                        peak_meter::peak_meter(ui, peak, ceiling_db);
                    });

                    // 5. panel toggles, at least `space::XL` from the
                    // level group (contract B2.5, B6, FR-019).
                    ui.add_space(theme::space::XL);

                    let queue_label = tr("queue-toggle");
                    wrap_switch_before(ui, wrap_boundary, &queue_label);
                    let queue_changed =
                        switch(ui, SwitchKind::Toggle, &mut queue_open, &queue_label).changed();

                    let effects_label = tr("effects-toggle");
                    wrap_switch_before(ui, wrap_boundary, &effects_label);
                    let effects_changed =
                        switch(ui, SwitchKind::Toggle, &mut effects_open, &effects_label).changed();

                    let transport_label = tr("transport-toggle");
                    wrap_switch_before(ui, wrap_boundary, &transport_label);
                    let transport_changed = switch(
                        ui,
                        SwitchKind::Toggle,
                        &mut transport_open,
                        &transport_label,
                    )
                    .changed();

                    // 6. Panels (contract B2.6): D5/D6, focus lands here
                    // exactly when the dock/overlay just stopped being
                    // drawn (`Hidden` transition, `continue_focus_past_
                    // dock`) or an in-overlay `Escape` just closed it
                    // (`show_overlay`) — never on a widget that is no
                    // longer drawn.
                    let panels_changed = if show_panels_toggle {
                        let panels_label = tr("plugin-dock-panels-toggle");
                        wrap_switch_before(ui, wrap_boundary, &panels_label);
                        let response =
                            switch(ui, SwitchKind::Toggle, &mut panels_open, &panels_label);
                        if focus_panels_toggle {
                            response.request_focus();
                        }
                        response.changed()
                    } else {
                        false
                    };

                    (
                        queue_changed,
                        effects_changed,
                        transport_changed,
                        panels_changed,
                    )
                })
                .inner;

            // Persist only the flag(s) actually toggled this frame
            // (016 contract P3) — never an unconditional write every
            // frame. Toggling from the bar needs no `request_discard`
            // (research R7): the scroll region's cards are drawn after
            // the bar, in the same pass.
            //
            // 021-transport-bar-and-panel-layout (contract R1, research
            // R6): a toggle that just turned a panel **on** also records a
            // reveal request, consumed once that panel's card is drawn
            // below. A toggle that turned a panel off never reveals
            // anything (contract R4).
            if toggled.0 {
                controller.set_now_playing_panel_open(NowPlayingPanel::Queue, queue_open);
                if queue_open {
                    request_reveal(ui.ctx(), NowPlayingPanel::Queue);
                }
            }
            if toggled.1 {
                controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, effects_open);
                if effects_open {
                    request_reveal(ui.ctx(), NowPlayingPanel::EffectChain);
                }
            }
            if toggled.2 {
                controller.set_now_playing_panel_open(NowPlayingPanel::Transport, transport_open);
                if transport_open {
                    request_reveal(ui.ctx(), NowPlayingPanel::Transport);
                }
            }
            // D5: the overlay's open flag is session-only (never
            // persisted) — written straight to egui memory, not through
            // the controller.
            if toggled.3 {
                plugin_panels::set_overlay_open(ui.ctx(), panels_open);
            }
        });
}

/// The bar's identity group (contract B2.1, B4, B7, research R4): 40px
/// artwork, then a `column_width`-wide title/artist column, each truncated
/// to one line with `Label::truncate()`. The full title/artist appear in
/// the hover text and in the group's own AccessKit label
/// (`now-playing-bar-identity`, "{ $title }, { $artist }"). With no track,
/// shows the placeholder square and the `now-playing-empty` text on one
/// truncated line (edge case "no track").
fn show_identity<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
    column_width: f32,
) {
    let (title, artist, url, name) = match controller.current_track() {
        Some(item) => (
            item.track.title.clone(),
            item.track.artists.join(", "),
            item.track.artwork_url.clone(),
            artwork_name(item).to_string(),
        ),
        None => (tr("now-playing-empty"), String::new(), None, String::new()),
    };

    let group = ui.horizontal(|ui| {
        draw_artwork(ui, artwork, url.as_deref(), &name);
        ui.vertical(|ui| {
            ui.set_width(column_width);
            // `show_tooltip_when_elided(false)`: egui's own elision tooltip
            // stacked a second, identical tooltip on top of the explicit
            // `on_hover_text` (2026-09-28 manual walk, research R15).
            let title_response = ui.add(
                Label::new(&title)
                    .truncate()
                    .show_tooltip_when_elided(false),
            );
            let _ = title_response.on_hover_text(title.clone());
            if !artist.is_empty() {
                let artist_response = ui.add(
                    Label::new(RichText::new(&artist).weak())
                        .truncate()
                        .show_tooltip_when_elided(false),
                );
                let _ = artist_response.on_hover_text(artist.clone());
            }
        });
    });

    let a11y_name = if artist.is_empty() {
        title.clone()
    } else {
        tr_args(
            "now-playing-bar-identity",
            &[("title", title.clone()), ("artist", artist.clone())],
        )
    };
    let id = ui.id().with("now-playing-bar-identity");
    let response = ui.interact(group.response.rect, id, Sense::hover());
    ui.ctx().accesskit_node_builder(response.id, |b| {
        b.set_label(a11y_name);
    });
}

/// The bar's transport group (contract B2.2): skip back, play/pause, stop,
/// skip forward — every `Button` built with `TextWrapMode::Extend` (018 D4)
/// so its own label is never elided; the whole group moves to the next row
/// instead ([`wrap_group_before`]).
fn show_transport_controls<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    available: bool,
) {
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                available,
                Button::new(tr("transport-skip-back")).wrap_mode(TextWrapMode::Extend),
            )
            .clicked()
        {
            controller.skip_back();
        }

        let playing = controller.transport_state().intent == Intent::Playing;
        let play_pause_key = if playing {
            "transport-pause"
        } else {
            "transport-play"
        };
        if ui
            .add_enabled(
                available,
                Button::new(tr(play_pause_key)).wrap_mode(TextWrapMode::Extend),
            )
            .clicked()
        {
            if playing {
                controller.pause();
            } else {
                controller.play();
            }
        }

        if ui
            .add_enabled(
                available,
                Button::new(tr("transport-stop")).wrap_mode(TextWrapMode::Extend),
            )
            .clicked()
        {
            controller.stop();
        }

        if ui
            .add_enabled(
                available,
                Button::new(tr("transport-skip-forward")).wrap_mode(TextWrapMode::Extend),
            )
            .clicked()
        {
            controller.skip_forward();
        }
    });
}

/// A plain text run's rendered width at `style`, via the same
/// `WidgetText::into_galley` measurement `wrap_switch_before` already uses
/// for a switch's own label.
fn measure_text_width(ui: &Ui, text: &str, style: egui::TextStyle) -> f32 {
    egui::WidgetText::from(text)
        .into_galley(ui, None, f32::INFINITY, style)
        .size()
        .x
}

/// The transport group's total width (research R4): four buttons' own
/// measured label + padding, plus the row's `item_spacing` between them —
/// the same quantities `egui::Button` itself lays out with, so this tracks
/// what actually gets drawn.
fn transport_group_width<B: OutputBackend, H: SourceHost>(
    ui: &Ui,
    controller: &PlaybackController<B, H>,
) -> f32 {
    let playing = controller.transport_state().intent == Intent::Playing;
    let play_pause_key = if playing {
        "transport-pause"
    } else {
        "transport-play"
    };
    let labels = [
        tr("transport-skip-back"),
        tr(play_pause_key),
        tr("transport-stop"),
        tr("transport-skip-forward"),
    ];
    let padding = ui.spacing().button_padding.x * 2.0;
    let spacing = ui.spacing().item_spacing.x;
    let mut width = 0.0;
    for (index, label) in labels.iter().enumerate() {
        if index > 0 {
            width += spacing;
        }
        width += measure_text_width(ui, label, egui::TextStyle::Button) + padding;
    }
    width
}

/// The bar's own elapsed/remaining text (contract B2.3) — the same numbers
/// the waveform region's own time labels show ([`playhead_context`]), so
/// the two never drift apart.
fn time_texts<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
    waveform: &WaveformState,
) -> (String, String) {
    let (playhead_frame, len_frames, sample_rate) = playhead_context(controller, waveform);
    let remaining_frames = len_frames.saturating_sub(playhead_frame);
    (
        tr_args(
            "time-elapsed",
            &[("time", format_mmss_frames(playhead_frame, sample_rate))],
        ),
        tr_args(
            "time-remaining",
            &[("time", format_mmss_frames(remaining_frames, sample_rate))],
        ),
    )
}

/// The level group's (master volume over the peak meter) approximate
/// width: the volume row's own label + `slider_width` + readout, versus
/// the peak meter's own self-clamped `[80, 240]` bar (`widgets/
/// peak_meter.rs`) — 240 is always a safe upper bound for that row.
fn level_group_width<B: OutputBackend, H: SourceHost>(
    ui: &Ui,
    controller: &PlaybackController<B, H>,
) -> f32 {
    let label_w = measure_text_width(ui, &tr("master-volume"), egui::TextStyle::Body);
    let volume = controller.master_volume();
    let readout = format!("{} %, {:.1} dB", volume.value(), volume.to_db());
    let readout_w = measure_text_width(ui, &readout, egui::TextStyle::Body);
    let spacing = ui.spacing().item_spacing.x;
    let volume_row = label_w + spacing + ui.spacing().slider_width + spacing + readout_w;
    volume_row.max(240.0)
}

/// Generalises `wrap_switch_before` (018 R8/R12) to any pre-measured
/// atomic bar group (021 research R4): if drawing `width` from the
/// current cursor position would cross `boundary` (the overlay's own left
/// edge, the docked column's edge, or the row's own right edge), forces
/// the group onto a fresh row first (`Ui::end_row()`) — `horizontal_
/// wrapped`'s own automatic wrap alone is not enough (018 M8: once any
/// sibling's rect already extends past a row's original wrap bound, egui
/// treats that as the row's new bound for everything drawn after it).
fn wrap_group_before(ui: &mut Ui, boundary: f32, width: f32) {
    if ui.cursor().min.x + width > boundary {
        ui.end_row();
    }
}

/// 018-window-sizing-and-responsive-dock (contract D1, research R8/R12):
/// pre-measures one `switch`'s own rendered width (its track plus item
/// spacing plus its label's galley, mirroring `widgets::controls::
/// switch`'s own internal layout) and, if drawing it from the current
/// cursor position would cross `boundary`, forces it onto a fresh row
/// first — [`wrap_group_before`]'s switch-specific sibling (it needs
/// `switch`'s own track metrics, which a bare width doesn't carry).
fn wrap_switch_before(ui: &mut Ui, boundary: f32, label: &str) {
    let metrics = theme::controls::switch_metrics();
    let galley =
        egui::WidgetText::from(label).into_galley(ui, None, f32::INFINITY, egui::TextStyle::Body);
    let width = metrics.track.x + ui.spacing().item_spacing.x + galley.size().x;
    if ui.cursor().min.x + width > boundary {
        ui.end_row();
    }
}

/// The playhead's current frame, the track's length in frames, and the
/// sample rate — shared by the bar's own elapsed/remaining group
/// ([`time_texts`]) and the waveform region's own time labels
/// (`show_waveform`), so both read one number each frame
/// (021-transport-bar-and-panel-layout).
fn playhead_context<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
    waveform: &WaveformState,
) -> (u64, u64, u32) {
    // A Connect track always plays at 44.1 kHz; `source_sample_rate()`
    // only reports it once a `TrackStarted`/`BecameActive` has actually
    // reached the engine, so fall back to it here rather than divide by
    // zero for the interval between `queue_replace` and first playback.
    let sample_rate = match controller.source_sample_rate() {
        0 => 44_100,
        rate => rate,
    };
    let track_len_ms = controller
        .transport_state()
        .track_len_ms
        .or_else(|| {
            controller
                .current_track()
                .map(|item| item.track.duration_ms)
        })
        .unwrap_or(0);
    let len_frames = (u64::from(track_len_ms) * u64::from(sample_rate)) / 1000;

    let playhead_frame = match waveform.preview_frame() {
        Some(frame) => frame,
        None => duration_to_frames(controller.position(), sample_rate),
    }
    .min(len_frames);
    (playhead_frame, len_frames, sample_rate)
}

/// The name [`initials_placeholder`] derives from on a `Failed`/no-URL
/// artwork (contracts/ui-waveform.md §1: "initials placeholder(album
/// title, 96)") — the track's own title when it has no album (mirrors
/// `rows.rs`'s `artwork_name`).
fn artwork_name(item: &modplayer_core::QueueItem) -> &str {
    item.track.album.as_deref().unwrap_or(&item.track.title)
}

/// Draw the artwork square: the decoded texture when `Ready`, a neutral
/// square while `Loading`, and the initials placeholder on `Failed` or no
/// URL at all (contracts/ui-waveform.md §1, FR-020). An empty `name`
/// (the bar identity group's no-track state, contract B7) resolves to the
/// music-glyph placeholder (`widgets::initials::initials`).
fn draw_artwork(ui: &mut Ui, artwork: &mut ArtworkCache, url: Option<&str>, name: &str) {
    let state = url.map(|url| artwork.get(ui.ctx(), url));
    match state {
        Some(ArtworkState::Ready(texture_id)) => {
            ui.add(egui::Image::from_texture((
                texture_id,
                Vec2::splat(ARTWORK_SIZE),
            )));
        }
        Some(ArtworkState::Loading) => {
            let (rect, _response) =
                ui.allocate_exact_size(Vec2::splat(ARTWORK_SIZE), egui::Sense::hover());
            if ui.is_rect_visible(rect) {
                ui.painter()
                    .rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
            }
        }
        Some(ArtworkState::Failed) | None => {
            initials_placeholder(ui, name, ARTWORK_SIZE);
        }
    }
}

/// The one-line status under the heading (contracts/ui-surface.md §1):
/// empty when playing and healthy, otherwise the disabled reason,
/// `Reconnecting…` (US4 T20 — a soft status: the transport stays enabled
/// and audio keeps playing from the buffer while transient), or
/// `buffering`.
fn show_status_line<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &PlaybackController<B, H>,
) {
    if let Some(reason) = controller.disabled_reason() {
        ui.label(tr(reason));
        return;
    }
    if matches!(
        controller.transport_state().health,
        SourceHealth::Transient { .. }
    ) {
        ui.label(tr("status-reconnecting"));
        return;
    }
    if controller.transport_state().buffering {
        ui.label(tr("status-buffering"));
    }
}

/// "Playing on <device>" banner + **Play here** button (contracts/
/// ui-surface.md §1, FR-016/018/019): shown only while
/// `ActiveState::Inactive`; a no-op render otherwise.
fn show_transfer_banner<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) {
    let device = match controller.active_state() {
        ActiveState::Inactive { other_device } => other_device
            .clone()
            .unwrap_or_else(|| tr("banner-unknown-device")),
        _ => return,
    };
    ui.horizontal(|ui| {
        ui.label(tr_args("banner-playing-elsewhere", &[("device", device)]));
        if ui.button(tr("banner-play-here")).clicked() {
            controller.play_here();
        }
    });
}

/// Elapsed label, the waveform overview (with the detail window
/// highlighted), the remaining label, and the waveform detail
/// (contracts/ui-waveform.md §1; 021 region structure: "waveform region
/// (overview lane+view, time labels, detail lane+view)") — only called
/// with a current track (`show`'s empty-state branch skips this whole
/// block, FR-015).
fn show_waveform<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
    available: bool,
    track_changed: bool,
    overview_height: f32,
    detail_height: f32,
) {
    let (playhead_frame, len_frames, sample_rate) = playhead_context(controller, waveform);
    let playing = controller.transport_state().intent == Intent::Playing;

    // Follow application order (contracts/ui-waveform.md §5), applied
    // before either widget paints so both show the same, already-updated
    // window this frame.
    let mut detail = match waveform.detail {
        Some(previous) if track_changed => previous.recenter(playhead_frame, len_frames),
        Some(previous) => previous,
        None => DetailWindow::initial(playhead_frame, len_frames, sample_rate),
    };
    if waveform.drag.is_none() {
        detail = detail.follow_playhead(playhead_frame, playing, len_frames);
    }

    // Cloned (Arc pointer copies, not the peak data itself) rather than
    // borrowed, so this doesn't tie a `&controller` borrow across the
    // `apply_waveform_event` calls below (each of which needs `&mut
    // controller` for a committed seek).
    let snapshot = controller.analysis().cloned();
    let status = snapshot
        .as_ref()
        .map(|snapshot| snapshot.status)
        .unwrap_or(AnalysisStatus::Pending);
    let peaks = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.peaks.clone());
    let peaks = peaks.as_deref();
    let unavailable_text = tr("waveform-unavailable");

    ui.label(theme::mono_text(tr_args(
        "time-elapsed",
        &[("time", format_mmss_frames(playhead_frame, sample_rate))],
    )));

    let previewing = waveform.drag.is_some();
    let markers_snapshot = controller.markers().cloned();
    let loop_state = controller.shared().loop_state();
    let focused_marker = waveform.focused_marker;
    // 011-plugin-ui-contributions (US3, FR-014/FR-015): every `Active`
    // plugin's overlay layers, read once for both waveforms this frame —
    // painting them costs zero plugin calls (O10, SC-003).
    let overlay_layers = controller.plugin_overlays();
    // 017-high-contrast-appearance (S3): read once here, moved into both
    // paint closures below — the single selection site, no paint site
    // branches on the appearance flag itself.
    let roles = theme::roles(ui.visuals());
    markers::lane(
        ui,
        "overview",
        0..len_frames,
        sample_rate,
        len_frames,
        markers_snapshot.as_ref(),
        controller,
        waveform,
        &mut detail,
    );
    // 022-waveform-legibility (WL5, FR-009): either kind of drag in
    // progress suppresses the hover scrub indicator on both views, every
    // frame — never driven by which view the drag itself started on.
    let hover_suppressed = waveform.drag.is_some() || waveform.marker_drag.is_some();
    let overview_paint = WaveformPaint {
        status,
        peaks,
        playhead: Some(playhead_frame),
        unavailable_text: &unavailable_text,
        highlight: Some(detail.start_frame..(detail.start_frame + detail.width_frames)),
        hover_suppressed,
    };
    // US3 T089 (O7): reserve the 16px glyph/label lane directly above the
    // overview's own rect — `plugin_overlays::paint` (below) paints into
    // exactly this strip, mirroring `markers::lane`'s own reserved-height
    // precedent rather than a second widget call.
    ui.add_space(plugin_overlays::LANE_HEIGHT);
    let (_overview_response, overview_event) = waveform::overview(
        ui,
        len_frames,
        sample_rate,
        playhead_frame,
        previewing,
        available,
        overview_height,
        &overview_paint,
        &mut |painter, space| {
            markers::paint_overlay(
                painter,
                space,
                markers_snapshot.as_ref(),
                loop_state,
                focused_marker,
                roles,
            );
            // O5: after markers/loop, before the playhead (paint::paint
            // already ran, `waveform::overview`'s own body calls
            // `paint::playhead` right after this closure returns).
            plugin_overlays::paint(
                painter,
                space,
                len_frames,
                ViewKind::Overview,
                &overlay_layers,
            );
        },
    );
    if let Some(event) = overview_event {
        apply_waveform_event(
            event,
            controller,
            waveform,
            &mut detail,
            len_frames,
            sample_rate,
            playhead_frame,
            DragOrigin::Overview,
        );
    }

    let remaining_frames = len_frames.saturating_sub(playhead_frame);
    ui.label(theme::mono_text(tr_args(
        "time-remaining",
        &[("time", format_mmss_frames(remaining_frames, sample_rate))],
    )));

    let detail_window = detail.start_frame..(detail.start_frame + detail.width_frames);
    markers::lane(
        ui,
        "detail",
        detail_window.clone(),
        sample_rate,
        len_frames,
        markers_snapshot.as_ref(),
        controller,
        waveform,
        &mut detail,
    );
    let detail_window = detail.start_frame..(detail.start_frame + detail.width_frames);
    let detail_paint = WaveformPaint {
        status,
        peaks,
        playhead: Some(playhead_frame),
        unavailable_text: &unavailable_text,
        highlight: None,
        hover_suppressed,
    };
    // US3 T089: the detail view's own lane (O7 applies to both views).
    ui.add_space(plugin_overlays::LANE_HEIGHT);
    let (_detail_response, detail_event) = waveform::detail(
        ui,
        detail_window,
        sample_rate,
        playhead_frame,
        previewing,
        available,
        detail_height,
        &detail_paint,
        &mut |painter, space| {
            markers::paint_overlay(
                painter,
                space,
                markers_snapshot.as_ref(),
                loop_state,
                focused_marker,
                roles,
            );
            plugin_overlays::paint(
                painter,
                space,
                len_frames,
                ViewKind::Detail,
                &overlay_layers,
            );
        },
    );
    if let Some(event) = detail_event {
        apply_waveform_event(
            event,
            controller,
            waveform,
            &mut detail,
            len_frames,
            sample_rate,
            playhead_frame,
            DragOrigin::Detail,
        );
    }

    waveform.detail = Some(detail);
}

/// Apply one [`WaveformEvent`] from either waveform widget
/// (contracts/ui-waveform.md §2-3, §5): `Preview`/`Commit`/`CancelDrag`
/// touch `waveform.drag` and the controller exactly as US1 did; the
/// zoom/pan/reset rows (new in US2) always target the single shared
/// `detail` window regardless of which widget produced them, since the
/// overview has no zoom of its own (contracts/ui-waveform.md §2's "on the
/// overview the anchor is the playhead").
#[allow(clippy::too_many_arguments)]
fn apply_waveform_event<B: OutputBackend, H: SourceHost>(
    event: WaveformEvent,
    controller: &mut PlaybackController<B, H>,
    waveform: &mut WaveformState,
    detail: &mut DetailWindow,
    len_frames: u64,
    sample_rate: u32,
    playhead_frame: u64,
    origin: DragOrigin,
) {
    match event {
        WaveformEvent::Preview(frame) => {
            waveform.drag = Some(DragPreview {
                target_frame: frame,
                origin,
            });
        }
        WaveformEvent::Commit(frame) => {
            waveform.drag = None;
            controller.seek_frames(frame);
            if !detail.contains(frame) {
                *detail = detail.recenter(frame, len_frames);
            }
        }
        WaveformEvent::CancelDrag => {
            waveform.drag = None;
        }
        WaveformEvent::Zoom {
            anchor_frame,
            factor,
        } => {
            *detail = detail
                .zoom_about(anchor_frame, factor, len_frames, sample_rate)
                .suspend_follow_if_outside(playhead_frame);
        }
        WaveformEvent::Pan { delta_frames } => {
            *detail = detail
                .pan(delta_frames, len_frames)
                .suspend_follow_if_outside(playhead_frame);
        }
        WaveformEvent::ZoomStep { zoom_in } => {
            *detail = detail
                .zoom_step(playhead_frame, zoom_in, len_frames, sample_rate)
                .suspend_follow_if_outside(playhead_frame);
        }
        WaveformEvent::PanStep { fraction } => {
            let delta_frames = (fraction * detail.width_frames as f64).round() as i64;
            *detail = detail
                .pan(delta_frames, len_frames)
                .suspend_follow_if_outside(playhead_frame);
        }
        WaveformEvent::Reset => {
            *detail = detail.reset(len_frames, sample_rate);
        }
    }
}

/// `duration.as_secs_f64() * sample_rate`, rounded — the inverse of
/// `waveform`'s own frame-to-duration conversions, used only for the
/// playhead's frame position (display precision; the real seek path is
/// always `seek_frames` with an already-integral frame).
fn duration_to_frames(duration: Duration, sample_rate: u32) -> u64 {
    (duration.as_secs_f64() * f64::from(sample_rate.max(1))).round() as u64
}

/// `"m:ss"` for a frame count at `sample_rate` (no leading-zero minutes;
/// contracts/ui-waveform.md's `time-elapsed`/`time-remaining`).
fn format_mmss_frames(frame: u64, sample_rate: u32) -> String {
    format_mmss(Duration::from_secs_f64(
        frame as f64 / f64::from(sample_rate.max(1)),
    ))
}

/// `"m:ss"` (no leading-zero minutes; contracts/ui-surface.md's former
/// seek-slider text, now `time-elapsed`/`time-remaining`).
fn format_mmss(duration: Duration) -> String {
    let total = duration.as_secs();
    format!("{}:{:02}", total / 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_mmss_pads_seconds_to_two_digits() {
        assert_eq!(format_mmss(Duration::from_secs(5)), "0:05");
        assert_eq!(format_mmss(Duration::from_secs(65)), "1:05");
        assert_eq!(format_mmss(Duration::from_secs(3661)), "61:01");
    }

    #[test]
    fn format_mmss_frames_matches_duration_based_formatting() {
        assert_eq!(format_mmss_frames(5 * 44_100, 44_100), "0:05");
        assert_eq!(format_mmss_frames(0, 44_100), "0:00");
    }

    #[test]
    fn duration_to_frames_round_trips_whole_seconds() {
        assert_eq!(duration_to_frames(Duration::from_secs(1), 44_100), 44_100);
        assert_eq!(duration_to_frames(Duration::ZERO, 44_100), 0);
    }

    #[test]
    fn wrap_group_before_wraps_only_when_the_group_would_cross_the_boundary() {
        let ctx = egui::Context::default();
        crate::theme::apply_tokens(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.horizontal_wrapped(|ui| {
                let before = ui.cursor().min.x;
                wrap_group_before(ui, before + 10.0, 5.0);
                assert!(
                    (ui.cursor().min.x - before).abs() < f32::EPSILON,
                    "a group that fits must not wrap"
                );
                ui.add_space(3.0);
                let cursor_before_wrap = ui.cursor().min.x;
                wrap_group_before(ui, cursor_before_wrap + 1.0, 50.0);
                assert!(
                    ui.cursor().min.x < cursor_before_wrap,
                    "a group that would cross the boundary must start a fresh row"
                );
            });
        })
        .drop_without_applying_deltas();
    }
}
