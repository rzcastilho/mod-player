// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §4, §12: every chain edit and discrete
//! parameter switch is click-free — its max first-difference never exceeds
//! `1.5x` the steady-state processed signal's own (FR-005, SC-002). US1's
//! rows exercise a `PitchShift`/`TimeStretch` stage's own discrete switches
//! (mode, formant) and ordinary chain edits (add/remove/reorder) performed
//! while a stretch stage is engaged; Phase 5 adds the EQ band-type switch
//! and the filter mode switch (both carry their own shadow-biquad
//! crossfade, research R6).

use std::sync::Arc;

use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_effects::catalog::{NodeKind, NodeOwner, ParamId};
use modplayer_engine::{
    CeilingDb, Command, Event, Processor, ProcessorConfig, RtShared, Transport, VolumePercent,
};
use rtrb::{Producer, RingBuffer};

const RATE: u32 = 44_100;
const BUFFER: usize = 128;

fn build() -> (Processor<SyntheticSource>, Producer<Command>) {
    let (command_tx, command_rx) = RingBuffer::<Command>::new(4_096);
    let (event_tx, _event_rx) = RingBuffer::<Event>::new(4_096);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: RATE,
        device_rate: RATE,
        device_channels: 2,
        max_frames: BUFFER,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared,
    };
    let processor = Processor::new(config, SyntheticSource::new(RATE), command_rx, event_tx);
    (processor, command_tx)
}

/// Render `n` buffers, appending the left channel of every rendered frame
/// (passthrough device/source rate, so one render always yields exactly
/// `BUFFER` frames — contracts/engine-effect-chain.md §1 rule 1 — and the
/// captured stream is a faithful, unbroken listener-facing signal).
fn render_n(processor: &mut Processor<SyntheticSource>, n: usize, captured: &mut Vec<f32>) {
    let mut out = vec![0.0f32; BUFFER * 2];
    for _ in 0..n {
        processor.render(&mut out);
        captured.extend(out.chunks_exact(2).map(|frame| frame[0]));
    }
}

fn max_first_diff(samples: &[f32]) -> f32 {
    samples
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0f32, f32::max)
}

/// Drives `setup` to completion, lets every insert/engage fade settle,
/// applies `edit`, then compares the max first-difference right at the
/// transition (a window spanning the edit's own up-to-5-ms crossfade plus
/// a WSOLA/LPC stage's own up-to-512-sample envelope-refresh cycle)
/// against the *destination* configuration's own steady-state first-
/// difference (measured just after, once everything has settled) — **not**
/// against the pre-edit signal's, which can have a genuinely different
/// (but not "clicky") texture of its own (e.g. LPC formant correction on a
/// near-pure tone is inherently grainier than a plain sine; that is the
/// feature working, not a click). Only an excess *beyond* the edit's own
/// new steady state counts as a click (contracts/engine-effect-chain.md
/// §4). Panics with `name` in the message on failure so a table run
/// pinpoints the row.
fn assert_click_free(
    name: &str,
    setup: fn(&mut Producer<Command>),
    edit: fn(&mut Producer<Command>),
) {
    let (mut processor, mut commands) = build();
    let _ = commands.push(Command::Play);
    setup(&mut commands);

    // Let every setup fade/engage settle: insert crossfade (5 ms) plus a
    // WSOLA stage's own startup (its ring must fill before it produces
    // steady-state output) — generously bounded at ~1.16 s of audio.
    let mut warm = Vec::new();
    render_n(&mut processor, 400, &mut warm);

    edit(&mut commands);

    let mut transition = Vec::new();
    render_n(&mut processor, 40, &mut transition);
    let transition_delta = max_first_diff(&transition);

    let mut steady = Vec::new();
    render_n(&mut processor, 80, &mut steady);
    let steady_delta = max_first_diff(&steady);
    assert!(
        steady_delta > 0.0,
        "{name}: the destination steady state must be a real, moving signal"
    );

    assert!(
        transition_delta <= 1.5 * steady_delta + 1e-6,
        "{name}: transition_delta={transition_delta} steady_delta={steady_delta}"
    );
}

fn setup_engaged_pitch_shift(commands: &mut Producer<Command>) {
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::PitchShift,
        owner: NodeOwner::Host,
    });
    // Away from unity (semitones default 0) so the WSOLA path is engaged.
    let _ = commands.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(0),
        value: 5.0,
    });
}

fn setup_engaged_pitch_shift_plus_gain(commands: &mut Producer<Command>) {
    setup_engaged_pitch_shift(commands);
    let _ = commands.push(Command::ChainInsert {
        slot: 1,
        position: 1,
        kind: NodeKind::Gain,
        owner: NodeOwner::Host,
    });
}

/// A discrete mode switch (Performance -> Quality) on an engaged
/// `PitchShift` stage (research R6, T035's two-voice crossfade).
#[test]
fn mode_switch_is_click_free() {
    assert_click_free("mode_switch", setup_engaged_pitch_shift, |commands| {
        let _ = commands.push(Command::ChainSetParam {
            slot: 0,
            param: ParamId(2), // mode
            value: 1.0,
        });
    });
}

/// A discrete formant toggle on an engaged `PitchShift` stage.
#[test]
fn formant_switch_is_click_free() {
    assert_click_free("formant_switch", setup_engaged_pitch_shift, |commands| {
        let _ = commands.push(Command::ChainSetParam {
            slot: 0,
            param: ParamId(1), // formant
            value: 1.0,
        });
    });
}

/// Inserting an ordinary node while a stretch stage is already engaged
/// elsewhere in the chain.
#[test]
fn add_with_stretch_stage_present_is_click_free() {
    assert_click_free(
        "add_with_stretch_present",
        setup_engaged_pitch_shift,
        |commands| {
            let _ = commands.push(Command::ChainInsert {
                slot: 1,
                position: 1,
                kind: NodeKind::Gain,
                owner: NodeOwner::Host,
            });
        },
    );
}

/// Removing an ordinary node while a stretch stage is already engaged
/// elsewhere in the chain.
#[test]
fn remove_with_stretch_stage_present_is_click_free() {
    assert_click_free(
        "remove_with_stretch_present",
        setup_engaged_pitch_shift_plus_gain,
        |commands| {
            let _ = commands.push(Command::ChainRemove { slot: 1 });
        },
    );
}

/// Reordering two nodes while one is an engaged stretch stage.
#[test]
fn reorder_with_stretch_stage_present_is_click_free() {
    assert_click_free(
        "reorder_with_stretch_present",
        setup_engaged_pitch_shift_plus_gain,
        |commands| {
            let _ = commands.push(Command::ChainMove {
                slot: 1,
                position: 0,
            });
        },
    );
}

/// An `Equalizer` node with band 0 tuned away from identity (freq
/// 1 kHz, +6 dB) so the switch below is audible.
fn setup_engaged_eq(commands: &mut Producer<Command>) {
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::Equalizer,
        owner: NodeOwner::Host,
    });
    let _ = commands.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(16), // band 0 freq
        value: 1_000.0,
    });
    let _ = commands.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(17), // band 0 gain
        value: 6.0,
    });
}

/// A `Filter` node away from its passthrough default (cutoff 1 kHz,
/// some resonance) so the mode switch below is audible.
fn setup_engaged_filter(commands: &mut Producer<Command>) {
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::Filter,
        owner: NodeOwner::Host,
    });
    let _ = commands.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(1), // cutoff
        value: 1_000.0,
    });
    let _ = commands.push(Command::ChainSetParam {
        slot: 0,
        param: ParamId(2), // resonance
        value: 0.5,
    });
}

/// research R6, T065: an EQ band's discrete `type` switch (peak ->
/// low-shelf) carries its own shadow-biquad crossfade.
#[test]
fn eq_band_type_switch_is_click_free() {
    assert_click_free("eq_band_type_switch", setup_engaged_eq, |commands| {
        let _ = commands.push(Command::ChainSetParam {
            slot: 0,
            param: ParamId(19), // band 0 type
            value: 1.0,         // low-shelf
        });
    });
}

/// research R6, T066: the filter's discrete `mode` switch (high-pass ->
/// low-pass) carries its own shadow-biquad crossfade.
#[test]
fn filter_mode_switch_is_click_free() {
    assert_click_free("filter_mode_switch", setup_engaged_filter, |commands| {
        let _ = commands.push(Command::ChainSetParam {
            slot: 0,
            param: ParamId(0), // mode
            value: 1.0,        // low-pass
        });
    });
}

/// The table itself (contracts/engine-effect-chain.md §12): every row
/// above, run together so a single named test matches the contract's own
/// test name. Each `#[test]` above also runs independently for a precise
/// failure location.
/// `(name, setup, edit)` row shape for the table below — named so
/// clippy's `type_complexity` lint doesn't flag the array's element type
/// spelled out inline.
type ClickFreeRow = (
    &'static str,
    fn(&mut Producer<Command>),
    fn(&mut Producer<Command>),
);

#[test]
fn every_edit_and_switch_is_click_free() {
    let rows: [ClickFreeRow; 7] = [
        ("mode_switch", setup_engaged_pitch_shift, |commands| {
            let _ = commands.push(Command::ChainSetParam {
                slot: 0,
                param: ParamId(2),
                value: 1.0,
            });
        }),
        ("formant_switch", setup_engaged_pitch_shift, |commands| {
            let _ = commands.push(Command::ChainSetParam {
                slot: 0,
                param: ParamId(1),
                value: 1.0,
            });
        }),
        (
            "add_with_stretch_present",
            setup_engaged_pitch_shift,
            |commands| {
                let _ = commands.push(Command::ChainInsert {
                    slot: 1,
                    position: 1,
                    kind: NodeKind::Gain,
                    owner: NodeOwner::Host,
                });
            },
        ),
        (
            "remove_with_stretch_present",
            setup_engaged_pitch_shift_plus_gain,
            |commands| {
                let _ = commands.push(Command::ChainRemove { slot: 1 });
            },
        ),
        (
            "reorder_with_stretch_present",
            setup_engaged_pitch_shift_plus_gain,
            |commands| {
                let _ = commands.push(Command::ChainMove {
                    slot: 1,
                    position: 0,
                });
            },
        ),
        ("eq_band_type_switch", setup_engaged_eq, |commands| {
            let _ = commands.push(Command::ChainSetParam {
                slot: 0,
                param: ParamId(19),
                value: 1.0,
            });
        }),
        ("filter_mode_switch", setup_engaged_filter, |commands| {
            let _ = commands.push(Command::ChainSetParam {
                slot: 0,
                param: ParamId(0),
                value: 1.0,
            });
        }),
    ];
    for (name, setup, edit) in rows {
        assert_click_free(name, setup, edit);
    }
}
