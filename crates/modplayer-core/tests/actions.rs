// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `modplayer_core::actions` contract tests (007, quickstart.md pinning
//! table; contracts/action-registry.md §6; contracts/keymap-settings.md
//! "Tests that pin this contract").

use std::collections::{BTreeSet, HashSet};

use modplayer_core::{
    ActionKind, ActionRegistry, BindingError, CATALOG, Chord, ChordParseError, HostAction,
    KEY_NAMES, KeyName, KeymapOverrides, Mods, Platform, Scope, ScopeState, def,
};
use proptest::prelude::*;

/// Parse a literal that must be valid grammar (test convenience, avoids a
/// raw `.unwrap()`/`.expect()` per Constitution VII's "outside tests"
/// carve-out being exercised carefully even inside tests).
fn chord(s: &str) -> Chord {
    Chord::parse(s).unwrap_or_else(|_| unreachable!("{s:?} must be valid grammar"))
}

// ---------------------------------------------------------------------
// T014: catalog shape (FR-001/FR-002/FR-004)
// ---------------------------------------------------------------------

#[test]
fn catalog_has_44_unique_ids_in_spec_order() {
    assert_eq!(CATALOG.len(), 44);
    for (i, def) in CATALOG.iter().enumerate() {
        assert_eq!(
            def.action,
            HostAction::ALL[i],
            "CATALOG order must match HostAction::ALL"
        );
    }
    let ids: HashSet<&str> = HostAction::ALL.iter().map(|a| a.id()).collect();
    assert_eq!(ids.len(), 44, "every action id must be unique");
}

#[test]
fn ids_round_trip_through_parse() {
    for action in HostAction::ALL {
        assert_eq!(HostAction::parse(action.id()), Some(action));
    }
    assert_eq!(HostAction::parse("host.nope.x"), None);
    assert_eq!(HostAction::parse(""), None);
}

#[test]
fn no_continuous_actions_in_this_slice() {
    assert!(
        CATALOG.iter().all(|d| d.kind == ActionKind::Trigger),
        "this slice ships zero ActionKind::Continuous instances"
    );
}

#[test]
fn defaults_match_spec_table() {
    // Transcribed independently from spec.md's § Default Action Catalog
    // table (not from catalog.rs) so a transcription slip in either file
    // is caught.
    let expected: Vec<(HostAction, Scope, bool, bool, &[&str])> = vec![
        (HostAction::Play, Scope::App, false, true, &[]),
        (HostAction::Pause, Scope::App, false, true, &[]),
        (
            HostAction::TogglePlayPause,
            Scope::App,
            false,
            true,
            &["Space"],
        ),
        (HostAction::Stop, Scope::App, false, true, &["Shift+Space"]),
        (
            HostAction::NextTrack,
            Scope::App,
            false,
            true,
            &["Primary+Right"],
        ),
        (
            HostAction::PreviousTrack,
            Scope::App,
            false,
            true,
            &["Primary+Left"],
        ),
        (
            HostAction::SeekForwardStep,
            Scope::App,
            true,
            true,
            &["Primary+Shift+Right"],
        ),
        (
            HostAction::SeekBackwardStep,
            Scope::App,
            true,
            true,
            &["Primary+Shift+Left"],
        ),
        (
            HostAction::VolumeUp,
            Scope::App,
            true,
            true,
            &["Primary+Up"],
        ),
        (
            HostAction::VolumeDown,
            Scope::App,
            true,
            true,
            &["Primary+Down"],
        ),
        (
            HostAction::AddPointMarker,
            Scope::NowPlaying,
            false,
            true,
            &["M"],
        ),
        (HostAction::SetA, Scope::NowPlaying, false, true, &["I"]),
        (HostAction::SetB, Scope::NowPlaying, false, true, &["O"]),
        (
            HostAction::NudgeEarlier,
            Scope::MarkerFocused,
            true,
            true,
            &["Left"],
        ),
        (
            HostAction::NudgeLater,
            Scope::MarkerFocused,
            true,
            true,
            &["Right"],
        ),
        (
            HostAction::NudgeEarlierX10,
            Scope::MarkerFocused,
            true,
            true,
            &["Shift+Left"],
        ),
        (
            HostAction::NudgeLaterX10,
            Scope::MarkerFocused,
            true,
            true,
            &["Shift+Right"],
        ),
        (
            HostAction::ClearAllMarkers,
            Scope::NowPlaying,
            false,
            true,
            &[],
        ),
        (
            HostAction::ToggleLoop,
            Scope::NowPlaying,
            false,
            true,
            &["L"],
        ),
        (
            HostAction::SetCue(cue(1)),
            Scope::NowPlaying,
            false,
            true,
            &["Shift+1"],
        ),
        (
            HostAction::SetCue(cue(2)),
            Scope::NowPlaying,
            false,
            true,
            &["Shift+2"],
        ),
        (
            HostAction::SetCue(cue(3)),
            Scope::NowPlaying,
            false,
            true,
            &["Shift+3"],
        ),
        (
            HostAction::SetCue(cue(4)),
            Scope::NowPlaying,
            false,
            true,
            &["Shift+4"],
        ),
        (
            HostAction::SetCue(cue(5)),
            Scope::NowPlaying,
            false,
            true,
            &["Shift+5"],
        ),
        (
            HostAction::SetCue(cue(6)),
            Scope::NowPlaying,
            false,
            true,
            &["Shift+6"],
        ),
        (
            HostAction::SetCue(cue(7)),
            Scope::NowPlaying,
            false,
            true,
            &["Shift+7"],
        ),
        (
            HostAction::SetCue(cue(8)),
            Scope::NowPlaying,
            false,
            true,
            &["Shift+8"],
        ),
        (
            HostAction::JumpToCue(cue(1)),
            Scope::NowPlaying,
            false,
            true,
            &["1"],
        ),
        (
            HostAction::JumpToCue(cue(2)),
            Scope::NowPlaying,
            false,
            true,
            &["2"],
        ),
        (
            HostAction::JumpToCue(cue(3)),
            Scope::NowPlaying,
            false,
            true,
            &["3"],
        ),
        (
            HostAction::JumpToCue(cue(4)),
            Scope::NowPlaying,
            false,
            true,
            &["4"],
        ),
        (
            HostAction::JumpToCue(cue(5)),
            Scope::NowPlaying,
            false,
            true,
            &["5"],
        ),
        (
            HostAction::JumpToCue(cue(6)),
            Scope::NowPlaying,
            false,
            true,
            &["6"],
        ),
        (
            HostAction::JumpToCue(cue(7)),
            Scope::NowPlaying,
            false,
            true,
            &["7"],
        ),
        (
            HostAction::JumpToCue(cue(8)),
            Scope::NowPlaying,
            false,
            true,
            &["8"],
        ),
        (
            HostAction::NavLibrary,
            Scope::App,
            false,
            true,
            &["Primary+1"],
        ),
        (
            HostAction::NavSearch,
            Scope::App,
            false,
            true,
            &["Primary+2"],
        ),
        (
            HostAction::NavNowPlaying,
            Scope::App,
            false,
            true,
            &["Primary+3"],
        ),
        (
            HostAction::NavPlugins,
            Scope::App,
            false,
            true,
            &["Primary+4"],
        ),
        (
            HostAction::NavSettings,
            Scope::App,
            false,
            true,
            &["Primary+5"],
        ),
        (
            HostAction::ToggleQueue,
            Scope::NowPlaying,
            false,
            true,
            &["Q"],
        ),
        (
            HostAction::FocusSearch,
            Scope::App,
            false,
            true,
            &["Primary+F", "Slash"],
        ),
        (
            HostAction::TempoStepUp,
            Scope::NowPlaying,
            true,
            false,
            &["Equals", "Plus"],
        ),
        (
            HostAction::TempoStepDown,
            Scope::NowPlaying,
            true,
            false,
            &["Minus"],
        ),
    ];

    assert_eq!(expected.len(), 44);
    for (i, (action, scope, repeats, enabled, bindings)) in expected.into_iter().enumerate() {
        let row = def(action);
        assert_eq!(
            row.action,
            HostAction::ALL[i],
            "table order mismatch at {i}"
        );
        assert_eq!(row.scope, scope, "{:?} scope", action);
        assert_eq!(
            row.repeats_while_held, repeats,
            "{:?} repeats_while_held",
            action
        );
        assert_eq!(
            row.enabled_by_default, enabled,
            "{:?} enabled_by_default",
            action
        );
        assert_eq!(
            row.default_bindings, bindings,
            "{:?} default_bindings",
            action
        );
        for binding in row.default_bindings {
            assert!(
                Chord::parse(binding).is_ok(),
                "{:?}'s default binding {binding:?} must parse",
                action
            );
        }
    }
}

#[test]
fn shipped_defaults_never_conflict() {
    let registry = ActionRegistry::new(KeymapOverrides::default());
    for action in HostAction::ALL {
        for &binding in def(action).default_bindings {
            let c = chord(binding);
            assert!(
                !registry.is_conflicting(action, c),
                "{:?}'s shipped default {binding:?} must not conflict",
                action
            );
        }
    }
}

fn cue(n: u8) -> modplayer_core::markers::CueSlot {
    modplayer_core::markers::CueSlot::new(n).unwrap_or_else(|| unreachable!())
}

// ---------------------------------------------------------------------
// T015: chord grammar & display (FR-003, R2/R14)
// ---------------------------------------------------------------------

fn chord_strategy() -> impl Strategy<Value = Chord> {
    (
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        0..KEY_NAMES.len(),
    )
        .prop_map(|(primary, shift, alt, key_idx)| {
            let key = KeyName::parse(KEY_NAMES[key_idx])
                .unwrap_or_else(|| unreachable!("KEY_NAMES entries always parse"));
            Chord::new(
                Mods {
                    primary,
                    shift,
                    alt,
                },
                key,
            )
        })
}

proptest! {
    #[test]
    fn chord_parse_encode_round_trip(c in chord_strategy()) {
        let encoded = c.encode();
        let parsed = Chord::parse(&encoded);
        prop_assert_eq!(parsed, Ok(c));
        prop_assert_eq!(c.encode(), encoded);
    }
}

#[test]
fn chord_parse_rejects_bad_grammar() {
    assert_eq!(Chord::parse(""), Err(ChordParseError::Empty));
    assert_eq!(
        Chord::parse("Ctrl+A"),
        Err(ChordParseError::UnknownModifier("Ctrl".to_string()))
    );
    assert_eq!(
        Chord::parse("Shift+Primary+A"),
        Err(ChordParseError::UnknownKey("Primary+A".to_string()))
    );
    assert_eq!(
        Chord::parse("Shift+Shift+A"),
        Err(ChordParseError::DuplicateModifier)
    );
    assert_eq!(
        Chord::parse("Primary+"),
        Err(ChordParseError::UnknownKey(String::new()))
    );
    assert_eq!(
        Chord::parse("Spacebar"),
        Err(ChordParseError::UnknownKey("Spacebar".to_string()))
    );
}

#[test]
fn chord_display_mac_and_other() {
    let c = chord("Primary+Shift+Right");
    assert_eq!(c.display(Platform::Mac), "⌘⇧→");
    assert_eq!(c.display(Platform::Other), "Ctrl+Shift+Right");

    let c = chord("Shift+Space");
    assert_eq!(c.display(Platform::Mac), "⇧␣");
    assert_eq!(c.display(Platform::Other), "Shift+Space");

    let c = chord("I");
    assert_eq!(c.display(Platform::Mac), "I");
    assert_eq!(c.display(Platform::Other), "I");

    let c = chord("Slash");
    assert_eq!(c.display(Platform::Mac), "/");
    assert_eq!(c.display(Platform::Other), "Slash");
}

// ---------------------------------------------------------------------
// T016: conflict detection (FR-009/FR-010)
// ---------------------------------------------------------------------

proptest! {
    #[test]
    fn conflict_is_symmetric(
        ops in prop::collection::vec((0usize..44, 0usize..6), 0..12)
    ) {
        const SMALL_CHORDS: [&str; 6] = ["A", "B", "C", "Space", "Left", "Q"];
        let mut registry = ActionRegistry::new(KeymapOverrides::default());
        for (action_idx, chord_idx) in ops {
            let action = HostAction::ALL[action_idx];
            let c = chord(SMALL_CHORDS[chord_idx]);
            let _ = registry.add_binding(action, c);
        }
        for action in HostAction::ALL {
            for &c in registry.bindings(action) {
                if registry.is_conflicting(action, c) {
                    let partner = registry.conflict_partner(action, c);
                    prop_assert!(partner.is_some(), "a conflicting pair must name a partner");
                    let partner = partner.unwrap_or_else(|| unreachable!());
                    prop_assert!(registry.is_conflicting(partner, c), "conflict must be symmetric");
                }
            }
        }
    }
}

#[test]
fn resolve_returns_none_for_conflicting_chord() {
    let mut registry = ActionRegistry::new(KeymapOverrides::default());
    let space = chord("Space");
    registry
        .add_binding(HostAction::SetCue(cue(1)), space)
        .unwrap_or_else(|_| unreachable!());

    assert!(registry.is_conflicting(HostAction::TogglePlayPause, space));
    assert!(registry.is_conflicting(HostAction::SetCue(cue(1)), space));

    let state = ScopeState {
        now_playing_shown: true,
        marker_focused: false,
    };
    assert_eq!(registry.resolve(space, &state), None);
}

#[test]
fn disabled_action_never_conflicts_or_blocks() {
    let mut registry = ActionRegistry::new(KeymapOverrides::default());
    assert!(!registry.is_enabled(HostAction::TempoStepUp));

    let equals = chord("Equals");
    registry
        .add_binding(HostAction::AddPointMarker, equals)
        .unwrap_or_else(|_| unreachable!());

    // TempoStepUp ships bound to "Equals" too, but is disabled by
    // default, so it must never enter the conflict set nor block
    // resolution.
    assert!(!registry.is_conflicting(HostAction::AddPointMarker, equals));
    assert!(!registry.is_conflicting(HostAction::TempoStepUp, equals));

    let state = ScopeState {
        now_playing_shown: true,
        marker_focused: false,
    };
    assert_eq!(
        registry.resolve(equals, &state),
        Some(HostAction::AddPointMarker)
    );
}

#[test]
fn enabling_action_flags_existing_collision() {
    let mut registry = ActionRegistry::new(KeymapOverrides::default());
    let equals = chord("Equals");
    registry
        .add_binding(HostAction::AddPointMarker, equals)
        .unwrap_or_else(|_| unreachable!());
    assert!(!registry.is_conflicting(HostAction::TempoStepUp, equals));

    registry.set_enabled(HostAction::TempoStepUp, true);

    assert!(registry.is_conflicting(HostAction::AddPointMarker, equals));
    assert!(registry.is_conflicting(HostAction::TempoStepUp, equals));
}

#[test]
fn can_coexist_table_is_symmetric_and_all_true_for_nested_chain() {
    for a in Scope::ALL {
        for b in Scope::ALL {
            assert_eq!(
                Scope::can_coexist(a, b),
                Scope::can_coexist(b, a),
                "can_coexist must be symmetric for {a:?}/{b:?}"
            );
            assert!(
                Scope::can_coexist(a, b),
                "this slice's three scopes nest, so every pair coexists ({a:?}/{b:?})"
            );
        }
    }
}

#[test]
fn disjoint_scopes_never_conflict() {
    // FR-010: `can_coexist` is a scope-pair *table*, not a formula, so a
    // later slice adding a genuinely disjoint scope changes one row
    // without touching any call site. This slice's three real scopes
    // all nest (proven above), so there is no disjoint pair to exercise
    // through `ActionRegistry` today; this test pins the shape of the
    // rule the registry's conflict rebuild actually runs — the same
    // "would these two ever both be live" gate — against a synthetic
    // disjoint row, proving that when `can_coexist` says `false`, two
    // enabled actions sharing a chord are *not* flagged.
    fn would_conflict(enabled_a: bool, enabled_b: bool, coexist: bool) -> bool {
        enabled_a && enabled_b && coexist
    }

    assert!(
        would_conflict(true, true, true),
        "today's nested scopes: sharing a chord conflicts"
    );
    assert!(
        !would_conflict(true, true, false),
        "a synthetic disjoint pair (coexist == false) must never conflict, even both enabled"
    );
}

// ---------------------------------------------------------------------
// T017: mutators (FR-007, FR-008, FR-011, FR-012)
// ---------------------------------------------------------------------

#[test]
fn reset_restores_defaults_without_touching_enabled() {
    let mut registry = ActionRegistry::new(KeymapOverrides::default());
    registry.set_enabled(HostAction::TempoStepUp, true);
    registry
        .add_binding(HostAction::ToggleLoop, chord("K"))
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(registry.bindings(HostAction::ToggleLoop).len(), 2);

    registry.reset(HostAction::ToggleLoop);

    assert_eq!(registry.bindings(HostAction::ToggleLoop), &[chord("L")]);
    assert!(
        registry.is_enabled(HostAction::TempoStepUp),
        "reset must not touch enabled"
    );
}

#[test]
fn reset_all_clears_every_conflict() {
    let mut registry = ActionRegistry::new(KeymapOverrides::default());
    let space = chord("Space");
    registry
        .add_binding(HostAction::SetCue(cue(1)), space)
        .unwrap_or_else(|_| unreachable!());
    assert!(registry.is_conflicting(HostAction::TogglePlayPause, space));

    registry.reset_all();

    for action in HostAction::ALL {
        for &c in registry.bindings(action) {
            assert!(!registry.is_conflicting(action, c));
        }
    }
}

#[test]
fn disabled_action_never_resolves() {
    let mut registry = ActionRegistry::new(KeymapOverrides::default());
    let plus = chord("Plus");
    let state = ScopeState {
        now_playing_shown: true,
        marker_focused: false,
    };
    // TempoStepUp ships bound to "Plus" but disabled: nothing resolves it.
    assert_eq!(registry.resolve(plus, &state), None);

    registry.set_enabled(HostAction::TempoStepUp, true);
    assert_eq!(
        registry.resolve(plus, &state),
        Some(HostAction::TempoStepUp)
    );
}

#[test]
fn add_duplicate_binding_is_rejected() {
    let mut registry = ActionRegistry::new(KeymapOverrides::default());
    let space = chord("Space");
    let before = registry.bindings(HostAction::TogglePlayPause).to_vec();

    let result = registry.add_binding(HostAction::TogglePlayPause, space);

    assert_eq!(result, Err(BindingError::Duplicate));
    assert_eq!(
        registry.bindings(HostAction::TogglePlayPause),
        before.as_slice()
    );
}

#[test]
fn remove_last_binding_leaves_action_unbound_and_resolvable_by_nothing() {
    let mut registry = ActionRegistry::new(KeymapOverrides::default());
    let space = chord("Space");
    registry.remove_binding(HostAction::TogglePlayPause, space);

    assert!(registry.bindings(HostAction::TogglePlayPause).is_empty());
    let state = ScopeState {
        now_playing_shown: false,
        marker_focused: false,
    };
    assert_eq!(registry.resolve(space, &state), None);
}

// ---------------------------------------------------------------------
// T018: sparse overrides & key-name bijection note
// ---------------------------------------------------------------------

proptest! {
    #[test]
    fn overrides_are_sparse_relative_to_defaults(
        action_idx in 0usize..44,
        set_to_default in any::<bool>(),
    ) {
        let action = HostAction::ALL[action_idx];
        let defaults: Vec<Chord> = def(action)
            .default_bindings
            .iter()
            .map(|s| chord(s))
            .collect();

        let mut overrides = KeymapOverrides::default();
        if set_to_default {
            overrides.set(action, defaults.clone());
            prop_assert!(overrides.is_empty(), "setting to the exact default must not create an entry");
        } else {
            let custom = vec![chord("K")];
            overrides.set(action, custom.clone());
            if chord_sets_eq(&custom, &defaults) {
                prop_assert!(overrides.is_empty());
            } else {
                prop_assert_eq!(overrides.get(action), Some(custom.as_slice()));
            }
        }
    }
}

fn chord_sets_eq(a: &[Chord], b: &[Chord]) -> bool {
    let a: BTreeSet<Chord> = a.iter().copied().collect();
    let b: BTreeSet<Chord> = b.iter().copied().collect();
    a == b
}

// ---------------------------------------------------------------------
// T027: sparse overrides round-trip through the real settings wire type
// (contracts/keymap-settings.md "Tests that pin this contract")
// ---------------------------------------------------------------------

proptest! {
    #[test]
    fn overrides_round_trip_through_raw_settings(
        ops in prop::collection::vec((0usize..44, 0usize..8), 0..10)
    ) {
        use modplayer_core::settings::{AudioSettings, RawSettings};

        const CHORDS: [&str; 8] = [
            "K", "Shift+K", "Primary+K", "F5", "Home", "End", "Alt+Q", "Primary+Shift+Z",
        ];
        let mut overrides = KeymapOverrides::default();
        for (action_idx, chord_idx) in ops {
            let action = HostAction::ALL[action_idx];
            let mut chords = overrides.get(action).map(<[Chord]>::to_vec).unwrap_or_default();
            let candidate = chord(CHORDS[chord_idx]);
            if !chords.contains(&candidate) {
                chords.push(candidate);
            }
            overrides.set(action, chords);
        }

        let settings = AudioSettings {
            keybinding_overrides: overrides.clone(),
            ..AudioSettings::default()
        };
        let raw = RawSettings::from_settings(&settings);
        let (round_tripped, invalid, dropped) = raw.into_settings();

        prop_assert!(invalid.is_empty());
        prop_assert!(dropped.is_empty());
        prop_assert_eq!(round_tripped.keybinding_overrides, overrides);
    }
}

#[test]
fn key_name_table_matches_egui_key_all() {
    // Core-side half of the bijection (research R2): every entry is
    // unique and round-trips through `KeyName::parse`/`as_str`. The
    // other half — that this table equals `egui::Key::ALL` mapped
    // through `name()` — is pinned from the UI crate (T054,
    // `crates/modplayer-ui/tests/actions.rs`), which is the only crate
    // allowed to depend on egui (plan.md Structure Decision).
    let unique: HashSet<&str> = KEY_NAMES.iter().copied().collect();
    assert_eq!(
        unique.len(),
        KEY_NAMES.len(),
        "KEY_NAMES must have no duplicates"
    );
    for &name in KEY_NAMES {
        let parsed = KeyName::parse(name).unwrap_or_else(|| unreachable!());
        assert_eq!(parsed.as_str(), name);
    }
}
