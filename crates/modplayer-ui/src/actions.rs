// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]

//! The egui-side dispatcher/invoker: focus claims, the two-pass chord
//! matcher and `invoke()` over every `HostAction` (007, data-model.md §4,
//! contracts/ui-actions.md).
//!
//! Claims (data-model.md §4.1) are per-frame, "last frame's" state: a
//! widget registers its `Id` and key claim while it draws
//! ([`register_claim`]); [`dispatch`] (called before any widget draws) reads
//! whatever the *previous* frame's draw pass registered, then
//! [`clear_claims`] resets the accumulator for this frame's fresh
//! registrations. This mirrors 006's `WaveformState::text_field_ids`
//! lifecycle exactly, but keyed by `egui::Context` memory (a single fixed
//! `Id`) rather than a field threaded through every draw call, so widgets
//! anywhere in the tree can register with nothing more than `ui.ctx()`.

use egui::{Context, Event, Id, Key as EguiKey, Modifiers};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::actions::{
    ActionId, ActionRegistry, ActionSource, Chord, HostAction, KeyName, Mods, ScopeState,
};
use modplayer_core::{Intent, PlaybackController};

use crate::markers;
use crate::now_playing;
use crate::shell::{Section, Shell};
use crate::waveform::WaveformState;

/// One `(modifiers, key)` member of a claim set (data-model.md §4.1).
pub type ChordPattern = (Mods, KeyName);

/// A widget's declared key claim (data-model.md §4.1, contracts/
/// ui-actions.md §2): either "acts like a text field" (the dispatcher does
/// nothing at all this frame) or an explicit set of chords the widget
/// itself owns — anything else in that set is left untouched by
/// [`dispatch`], leaving the widget free to read it non-consumingly exactly
/// as 004-006 already do.
#[derive(Debug, Clone)]
pub enum Claim {
    TextLike,
    Keys(Vec<ChordPattern>),
}

/// The per-frame focus-claims registry (data-model.md §4.1). Constructed
/// fresh per lookup via [`register_claim`]/[`claims_snapshot`]/
/// [`clear_claims`], which store it in `egui::Context` memory under a
/// single fixed `Id` so any widget can register without a `FocusClaims`
/// reference threaded through its call chain.
#[derive(Debug, Clone, Default)]
pub struct FocusClaims {
    entries: Vec<(Id, Claim)>,
}

impl FocusClaims {
    /// Drop every registration (called once per frame, right after
    /// [`dispatch`] has read the previous frame's).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Register (or replace) `id`'s claim for this frame's draw pass.
    pub fn register(&mut self, id: Id, claim: Claim) {
        self.entries.retain(|(existing, _)| *existing != id);
        self.entries.push((id, claim));
    }

    /// `id`'s registered claim, if any.
    pub fn claim_for(&self, id: Id) -> Option<&Claim> {
        self.entries
            .iter()
            .rev()
            .find(|(existing, _)| *existing == id)
            .map(|(_, claim)| claim)
    }
}

/// The fixed `Id` [`FocusClaims`] lives under in `egui::Context` memory.
fn claims_memory_id() -> Id {
    Id::new("modplayer-ui::action-focus-claims")
}

/// Register `id`'s claim for this frame (called by a widget while it
/// draws, contracts/ui-actions.md §2's claim table).
pub fn register_claim(ctx: &Context, id: Id, claim: Claim) {
    ctx.memory_mut(|memory| {
        memory
            .data
            .get_temp_mut_or_default::<FocusClaims>(claims_memory_id())
            .register(id, claim);
    });
}

/// A snapshot of the claims registered during the *previous* frame's draw
/// pass — what [`dispatch`] reads.
pub fn claims_snapshot(ctx: &Context) -> FocusClaims {
    ctx.memory(|memory| memory.data.get_temp::<FocusClaims>(claims_memory_id()))
        .unwrap_or_default()
}

/// Reset the accumulator for this frame's fresh registrations (called once
/// per frame, right after [`dispatch`]).
pub fn clear_claims(ctx: &Context) {
    ctx.memory_mut(|memory| {
        memory
            .data
            .get_temp_mut_or_default::<FocusClaims>(claims_memory_id())
            .clear();
    });
}

/// `name` looked up in the closed `KEY_NAMES` vocabulary — every name this
/// module passes is a literal drawn from that table, so a lookup failure
/// here is a programmer error, not a runtime condition (Constitution VII:
/// no `unwrap`/`expect` outside tests).
fn key(name: &str) -> KeyName {
    KeyName::parse(name).unwrap_or_else(|| unreachable!("{name:?} is not in KEY_NAMES"))
}

fn shift(key: KeyName) -> ChordPattern {
    (
        Mods {
            shift: true,
            ..Mods::default()
        },
        key,
    )
}

fn plain(key: KeyName) -> ChordPattern {
    (Mods::default(), key)
}

/// `Space`, `Enter`, `Tab`, `Shift+Tab`, `Escape`, plain `←↑→↓` — what
/// egui already does for any focused widget that registered no claim of
/// its own (contracts/ui-actions.md §2).
fn toolkit_default_claims() -> Vec<ChordPattern> {
    vec![
        plain(key("Space")),
        plain(key("Enter")),
        plain(key("Tab")),
        shift(key("Tab")),
        plain(key("Escape")),
        plain(key("Left")),
        plain(key("Right")),
        plain(key("Up")),
        plain(key("Down")),
    ]
}

/// `widgets/volume.rs`'s master-volume slider (contracts/ui-actions.md
/// §2).
pub fn volume_slider_claims() -> Vec<ChordPattern> {
    vec![
        plain(key("Left")),
        plain(key("Right")),
        plain(key("PageUp")),
        plain(key("PageDown")),
        plain(key("Tab")),
        shift(key("Tab")),
    ]
}

/// `waveform/input.rs`'s overview and detail widgets (contracts/
/// ui-actions.md §2) — deliberately excludes `Space` so a focused waveform
/// still lets it toggle play/pause (US1 AS9).
pub fn waveform_claims() -> Vec<ChordPattern> {
    let alt = Mods {
        alt: true,
        ..Mods::default()
    };
    let alt_shift = Mods {
        alt: true,
        shift: true,
        ..Mods::default()
    };
    vec![
        plain(key("Left")),
        plain(key("Right")),
        shift(key("Left")),
        shift(key("Right")),
        (alt, key("Left")),
        (alt, key("Right")),
        (alt_shift, key("Left")),
        (alt_shift, key("Right")),
        plain(key("Home")),
        plain(key("End")),
        plain(key("Plus")),
        plain(key("Equals")),
        plain(key("Minus")),
        plain(key("0")),
        plain(key("Escape")),
        plain(key("Tab")),
        shift(key("Tab")),
    ]
}

/// `markers.rs`'s glyph/row focus (contracts/ui-actions.md §2) —
/// deliberately excludes arrows: the four nudge actions own them while a
/// marker is focused (`Scope::MarkerFocused`).
pub fn marker_claims() -> Vec<ChordPattern> {
    vec![
        plain(key("Delete")),
        plain(key("Backspace")),
        plain(key("F2")),
        plain(key("Enter")),
        plain(key("C")),
        plain(key("Escape")),
        plain(key("Tab")),
        shift(key("Tab")),
    ]
}

/// `effects_view.rs`'s drag handle (008, contracts/ui-effect-chain.md
/// §4) — only `↑`/`↓`, so `effects_view::handle_focused_handle_keys` (not
/// this dispatcher) moves the focused node while the handle has focus.
pub fn effect_handle_claims() -> Vec<ChordPattern> {
    vec![
        plain(key("Up")),
        plain(key("Down")),
        plain(key("Tab")),
        shift(key("Tab")),
    ]
}

/// `rows.rs`'s library/search/queue rows (contracts/ui-actions.md §2).
pub fn row_claims() -> Vec<ChordPattern> {
    vec![
        plain(key("Enter")),
        plain(key("Space")),
        shift(key("F10")),
        plain(key("Escape")),
        plain(key("Tab")),
        shift(key("Tab")),
        plain(key("Up")),
        plain(key("Down")),
        plain(key("Left")),
        plain(key("Right")),
    ]
}

/// One resolved key press to apply this frame (data-model.md §4.2;
/// 011-plugin-ui-contributions D1: `action` widened from `HostAction` to
/// `ActionId` so a plugin shortcut dispatches through this exact same
/// path). Not `Copy`: `ActionId::Plugin` owns a `PluginActionId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub action: ActionId,
    pub repeat: bool,
}

/// `Mods::from_egui` (research R14): `primary` from `modifiers.command`
/// (egui already resolves that to Ctrl on Windows/Linux, ⌘ on macOS);
/// `None` on macOS when the *physical* Control key is held, so neither
/// capture nor dispatch ever sees a chord the spec declares unbindable.
/// `pub(crate)`: `settings/controls.rs`'s capture mode (007, US2) reuses
/// this exact conversion rather than a second copy (contracts/
/// ui-actions.md §5's canonical-chord rule is the same one dispatch
/// uses).
pub(crate) fn mods_from_egui(modifiers: Modifiers, is_mac: bool) -> Option<Mods> {
    if is_mac && modifiers.ctrl {
        return None;
    }
    Some(Mods {
        primary: modifiers.command,
        shift: modifiers.shift,
        alt: modifiers.alt,
    })
}

/// `egui::Key::name()` through the closed `KeyName` vocabulary — always
/// `Some` for a real key event (T054 pins the bijection), `None` is only
/// reachable if egui's own key table ever outgrows core's mirrored one.
/// `pub(crate)`: shared with `settings/controls.rs`'s capture mode (see
/// [`mods_from_egui`]).
pub(crate) fn key_name_from_egui(key: EguiKey) -> Option<KeyName> {
    KeyName::parse(key.name())
}

/// The two chords one `Event::Key` could mean (research R3): the logical
/// chord (with `Shift` dropped when the layout consumed it to produce a
/// different character than the physical key), then the physical
/// fallback. `None` for the first slot only when the event itself is
/// unbindable (macOS physical Control, or an unmapped key name).
fn candidate_chords(
    logical: EguiKey,
    physical: Option<EguiKey>,
    modifiers: Modifiers,
    is_mac: bool,
) -> (Option<Chord>, Option<Chord>) {
    let Some(base_mods) = mods_from_egui(modifiers, is_mac) else {
        return (None, None);
    };
    let Some(logical_name) = key_name_from_egui(logical) else {
        return (None, None);
    };

    let shift_consumed_by_layout = physical.is_some_and(|phys| phys != logical);
    let pass1_mods = if shift_consumed_by_layout {
        Mods {
            shift: false,
            ..base_mods
        }
    } else {
        base_mods
    };
    let pass1 = Chord::new(pass1_mods, logical_name);

    let pass2 =
        physical.and_then(|phys| key_name_from_egui(phys).map(|name| Chord::new(base_mods, name)));

    (Some(pass1), pass2)
}

/// Whether the focused widget's own claim already accounts for this event
/// — either its declared `Keys` set, egui's toolkit default for an
/// unclaimed focused widget, or `Tab`/`Shift+Tab` (always left to the
/// toolkit once a widget is focused, contracts/ui-actions.md §2 rule 2).
///
/// Matched against the same layout-normalised chord the registry lookup
/// below uses (`candidate_chords`' first slot: `Shift` dropped when the
/// layout consumed it to produce a different character), not the raw
/// modifiers — a real `+` is `Shift`+`=` on a US layout, so a claim for
/// `plain(Plus)` otherwise never matched and `+` on a focused waveform
/// stepped tempo instead of zooming (2026-09-19 manual walk, M6).
fn focused_widget_owns_event(
    claims: &FocusClaims,
    focused: Id,
    key: EguiKey,
    physical_key: Option<EguiKey>,
    modifiers: Modifiers,
    is_mac: bool,
) -> bool {
    let (normalised, _) = candidate_chords(key, physical_key, modifiers, is_mac);
    let pattern = normalised.map(|chord| (chord.mods, chord.key));
    match claims.claim_for(focused) {
        Some(Claim::Keys(set)) => {
            key == EguiKey::Tab || pattern.is_some_and(|pattern| set.contains(&pattern))
        }
        // Unreachable: rule 1 (text-edit-focused) already short-circuits
        // the whole frame before any per-event check runs.
        Some(Claim::TextLike) => true,
        None => pattern.is_some_and(|pattern| toolkit_default_claims().contains(&pattern)),
    }
}

/// Resolve this frame's key events against `registry` (contracts/
/// ui-actions.md §2, FR-019's precedence): consumes every matched event
/// out of `ctx`'s input so no later widget can also act on it (SC-010),
/// and returns the actions to invoke, in event order.
pub fn dispatch(
    ctx: &Context,
    claims: &FocusClaims,
    registry: &ActionRegistry,
    scope: &ScopeState,
) -> Vec<Invocation> {
    if ctx.text_edit_focused() {
        return Vec::new();
    }
    let focused = ctx.memory(|memory| memory.focused());
    if let Some(id) = focused
        && matches!(claims.claim_for(id), Some(Claim::TextLike))
    {
        return Vec::new();
    }

    let is_mac = ctx.os().is_mac();
    let mut invocations = Vec::new();

    ctx.input_mut(|input| {
        input.events.retain(|event| {
            let &Event::Key {
                key,
                physical_key,
                pressed: true,
                repeat,
                modifiers,
            } = event
            else {
                return true;
            };

            if let Some(id) = focused
                && focused_widget_owns_event(claims, id, key, physical_key, modifiers, is_mac)
            {
                return true;
            }

            let (pass1, pass2) = candidate_chords(key, physical_key, modifiers, is_mac);
            let Some(pass1) = pass1 else {
                return true;
            };

            if let Some(action) = registry.resolve(pass1, scope) {
                push_invocation(&mut invocations, registry, action, repeat);
                return false;
            }
            if let Some(pass2) = pass2
                && let Some(action) = registry.resolve(pass2, scope)
            {
                push_invocation(&mut invocations, registry, action, repeat);
                return false;
            }
            true
        });
    });

    invocations
}

/// FR-019's repeat rule: a repeat event for an action that does not
/// `repeats_while_held` is consumed (by the caller) but produces no
/// invocation; every other case fires. D1: `repeats_while_held` is read
/// from `registry` (the catalog for a `HostAction`, the plugin def for a
/// plugin action) rather than the catalog alone.
fn push_invocation(
    invocations: &mut Vec<Invocation>,
    registry: &ActionRegistry,
    action: ActionId,
    repeat: bool,
) {
    if repeat && !registry.repeats_while_held(&action) {
        return;
    }
    invocations.push(Invocation { action, repeat });
}

/// `nudge_marker` for whichever marker is focused (contracts/ui-actions.md
/// §3) — the scope guarantees `Some`; a stale `None` (should not happen)
/// is a silent no-op rather than a panic.
fn nudge<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
    waveform: &WaveformState,
    direction: i8,
    multiplier: u8,
) {
    if let Some(id) = waveform.focused_marker {
        let _ = controller.nudge_marker(id, direction, multiplier);
    }
}

/// Apply one resolved [`Invocation`] (contracts/ui-actions.md §3,
/// contracts/action-registry-plugins.md D2): `ActionId::Host` dispatches
/// through the 007 match unchanged (total over every `HostAction`, so the
/// dispatcher's exhaustiveness invariant, plan.md Design Note 1, is a
/// compiler-checked match); `ActionId::Plugin` calls `PlaybackController::
/// invoke_plugin_action` with `ActionSource::Keyboard` (D2).
pub fn invoke<B: OutputBackend, H: SourceHost>(
    inv: Invocation,
    controller: &mut PlaybackController<B, H>,
    shell: &mut Shell,
    waveform: &mut WaveformState,
    ctx: &Context,
) {
    let action = match inv.action {
        ActionId::Host(action) => action,
        ActionId::Plugin(id) => {
            controller.invoke_plugin_action(&id, ActionSource::Keyboard);
            return;
        }
    };
    match action {
        HostAction::Play => controller.play(),
        HostAction::Pause => controller.pause(),
        HostAction::TogglePlayPause => {
            if controller.transport_state().intent == Intent::Playing {
                controller.pause();
            } else {
                controller.play();
            }
        }
        HostAction::Stop => controller.stop(),
        HostAction::NextTrack => controller.skip_forward(),
        HostAction::PreviousTrack => controller.skip_back(),
        HostAction::SeekForwardStep => controller.seek_step(1),
        HostAction::SeekBackwardStep => controller.seek_step(-1),
        HostAction::VolumeUp => controller.step_master_volume(1),
        HostAction::VolumeDown => controller.step_master_volume(-1),
        HostAction::AddPointMarker => {
            waveform.marker_status = controller
                .add_point_marker()
                .err()
                .map(markers::refusal_key);
        }
        HostAction::SetA => {
            waveform.marker_status = controller.set_loop_a().err().map(markers::refusal_key);
        }
        HostAction::SetB => {
            waveform.marker_status = controller.set_loop_b().err().map(markers::refusal_key);
        }
        HostAction::NudgeEarlier => nudge(controller, waveform, -1, 1),
        HostAction::NudgeLater => nudge(controller, waveform, 1, 1),
        HostAction::NudgeEarlierX10 => nudge(controller, waveform, -1, 10),
        HostAction::NudgeLaterX10 => nudge(controller, waveform, 1, 10),
        HostAction::ClearAllMarkers => controller.clear_all_markers(),
        HostAction::ToggleLoop => {
            waveform.marker_status = controller
                .toggle_current_loop()
                .err()
                .map(markers::refusal_key);
        }
        HostAction::SetCue(slot) => {
            waveform.marker_status = controller.set_cue(slot).err().map(markers::refusal_key);
        }
        HostAction::JumpToCue(slot) => {
            // Silent no-op on an empty slot (FR-014); a cue jump, whether
            // it moved playback or not, is never a refusal and always
            // clears any stale status.
            let _ = controller.jump_to_cue(slot);
            waveform.marker_status = None;
        }
        HostAction::NavLibrary => shell.section = Section::Library,
        HostAction::NavSearch => shell.section = Section::Search,
        HostAction::NavNowPlaying => shell.section = Section::NowPlaying,
        HostAction::NavPlugins => shell.section = Section::Plugins,
        HostAction::NavSettings => shell.section = Section::Settings,
        HostAction::ToggleQueue => now_playing::toggle_queue_panel(ctx),
        HostAction::FocusSearch => {
            shell.section = Section::Search;
            shell.focus_search_requested = true;
        }
        // FR-017/SC-012, US1 AS7/AS8: the waveform's own `WAVEFORM_CLAIMS`
        // already own `Plus`/`Equals`/`Minus` while it is focused, so
        // `dispatch` only ever produces one of these when the waveform is
        // not the focused widget.
        HostAction::TempoStepUp => controller.tempo_step(1),
        HostAction::TempoStepDown => controller.tempo_step(-1),
        // 008, contracts/ui-effect-chain.md §5.
        HostAction::ToggleEffectChain => crate::effects_view::toggle_effect_chain_panel(ctx),
        // 010-transport-focus, contracts/ui-transport-panel.md §1.
        HostAction::ToggleTransportPanel => crate::transport_view::toggle_transport_panel(ctx),
    }
}

/// `dispatch` then `invoke` over every resulting action, in one call
/// (research R12) — what `App::ui` does, and what tests drive directly
/// without an `eframe::CreationContext`.
pub fn dispatch_and_invoke<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    claims: &FocusClaims,
    scope: &ScopeState,
    controller: &mut PlaybackController<B, H>,
    shell: &mut Shell,
    waveform: &mut WaveformState,
) {
    let invocations = dispatch(ctx, claims, controller.actions(), scope);
    for inv in invocations {
        invoke(inv, controller, shell, waveform, ctx);
    }
}
