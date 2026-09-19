// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §12: `auto_switch_table` — pins
//! FR-008's pure auto-switch rule.

use modplayer_effects::catalog::{
    ModeState, NodeKind, QualityMode, mode_after_user_set, mode_after_value_change,
};

#[test]
fn auto_switch_table() {
    struct Case {
        name: &'static str,
        kind: NodeKind,
        value: f32,
        before: ModeState,
        after: ModeState,
    }

    let cases = [
        // Rule 1: Performance leaving stage-use range -> Quality, flagged.
        Case {
            name: "pitch shift beyond 3 semitones auto-switches to quality",
            kind: NodeKind::PitchShift,
            value: 7.0,
            before: ModeState {
                mode: QualityMode::Performance,
                auto_switched: false,
            },
            after: ModeState {
                mode: QualityMode::Quality,
                auto_switched: true,
            },
        },
        Case {
            name: "time stretch beyond 1.5 ratio auto-switches to quality",
            kind: NodeKind::TimeStretch,
            value: 1.8,
            before: ModeState {
                mode: QualityMode::Performance,
                auto_switched: false,
            },
            after: ModeState {
                mode: QualityMode::Quality,
                auto_switched: true,
            },
        },
        // Rule 2: an auto-switched Quality node reverts once back in range.
        Case {
            name: "auto-switched quality reverts once back in stage-use range",
            kind: NodeKind::PitchShift,
            value: 1.0,
            before: ModeState {
                mode: QualityMode::Quality,
                auto_switched: true,
            },
            after: ModeState {
                mode: QualityMode::Performance,
                auto_switched: false,
            },
        },
        // A user-forced Quality (not auto-switched) never auto-reverts.
        Case {
            name: "user-forced quality persists inside stage-use range",
            kind: NodeKind::PitchShift,
            value: 1.0,
            before: ModeState {
                mode: QualityMode::Quality,
                auto_switched: false,
            },
            after: ModeState {
                mode: QualityMode::Quality,
                auto_switched: false,
            },
        },
        // Staying in range/out of range with no mode change is a no-op.
        Case {
            name: "performance inside stage-use range is unchanged",
            kind: NodeKind::TimeStretch,
            value: 1.0,
            before: ModeState {
                mode: QualityMode::Performance,
                auto_switched: false,
            },
            after: ModeState {
                mode: QualityMode::Performance,
                auto_switched: false,
            },
        },
        Case {
            name: "quality already outside range from an excursion stays quality",
            kind: NodeKind::PitchShift,
            value: 7.0,
            before: ModeState {
                mode: QualityMode::Quality,
                auto_switched: true,
            },
            after: ModeState {
                mode: QualityMode::Quality,
                auto_switched: true,
            },
        },
        // Boundary values.
        Case {
            name: "exactly 3 semitones is still stage-use",
            kind: NodeKind::PitchShift,
            value: 3.0,
            before: ModeState {
                mode: QualityMode::Performance,
                auto_switched: false,
            },
            after: ModeState {
                mode: QualityMode::Performance,
                auto_switched: false,
            },
        },
        Case {
            name: "just past 3 semitones leaves stage-use",
            kind: NodeKind::PitchShift,
            value: 3.01,
            before: ModeState {
                mode: QualityMode::Performance,
                auto_switched: false,
            },
            after: ModeState {
                mode: QualityMode::Quality,
                auto_switched: true,
            },
        },
    ];

    for case in cases {
        let got = mode_after_value_change(case.kind, case.value, case.before);
        assert_eq!(
            got, case.after,
            "{}: mode_after_value_change({:?}, {}, {:?}) = {:?}, want {:?}",
            case.name, case.kind, case.value, case.before, got, case.after
        );
    }
}

/// FR-008 rule 3: an explicit user mode choice always clears the
/// auto-switch flag, whatever it was before.
#[test]
fn user_set_always_clears_auto_switched_flag() {
    let forced = mode_after_user_set(QualityMode::Quality);
    assert_eq!(forced.mode, QualityMode::Quality);
    assert!(!forced.auto_switched);

    let forced = mode_after_user_set(QualityMode::Performance);
    assert_eq!(forced.mode, QualityMode::Performance);
    assert!(!forced.auto_switched);
}
