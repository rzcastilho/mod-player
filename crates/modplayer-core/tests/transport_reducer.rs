// SPDX-License-Identifier: MIT OR Apache-2.0

//! Table-driven tests for `transport::reduce`, rules T1-T12
//! (contracts/transport-and-queue.md §2, §7). T13-T24 (transfer/health/
//! session) land with US3/US4.

use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_core::queue::{AdvanceReason, PlaybackChange, Queue, QueueChange};
use modplayer_core::transport::{
    Effect, Input, Intent, QueueChangeOrigin, QueueOp, TransportState, reduce,
};
use modplayer_engine::Command;

fn state() -> TransportState {
    TransportState::default()
}

/// A real `QueueItemId` (opaque outside `modplayer-core`'s own tests) for
/// building a `QueueChange::MoveTo` fixture.
fn any_uid() -> modplayer_core::queue::QueueItemId {
    let mut q = Queue::new();
    q.replace_context(vec![TrackRef::new(
        TrackId::new("spotify:track:fixture").unwrap_or_else(|_| unreachable!()),
        "Fixture",
        vec![],
        None,
        None,
        1000,
        Availability::Available,
    )]);
    q.current().unwrap_or_else(|| unreachable!()).uid
}

#[test]
fn t1_play_in_stopped_loads_program_when_not_loaded() {
    let (state, effects) = reduce(
        state(),
        Input::Play {
            has_current: true,
            track_loaded_in_source: false,
            buffer_ready: false,
        },
    );
    assert_eq!(state.intent, Intent::Playing);
    assert!(state.buffering);
    assert!(effects.contains(&Effect::LoadCurrentProgram {
        position_ms: 0,
        start_playing: true
    }));
    assert!(effects.contains(&Effect::Engine(Command::Play)));
}

#[test]
fn t1_play_in_stopped_with_loaded_track_just_plays() {
    let (state, effects) = reduce(
        state(),
        Input::Play {
            has_current: true,
            track_loaded_in_source: true,
            buffer_ready: true,
        },
    );
    assert_eq!(state.intent, Intent::Playing);
    assert!(!state.buffering);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Engine(Command::Play)))
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::LoadCurrentProgram { .. }))
    );
}

#[test]
fn t1_play_in_stopped_with_no_current_is_a_no_op() {
    let (state, effects) = reduce(
        state(),
        Input::Play {
            has_current: false,
            track_loaded_in_source: true,
            buffer_ready: true,
        },
    );
    assert_eq!(state.intent, Intent::Stopped);
    assert!(effects.is_empty());
}

#[test]
fn t2_play_in_paused_resumes_without_reload() {
    let mut s = state();
    s.intent = Intent::Paused;
    let (state, effects) = reduce(
        s,
        Input::Play {
            has_current: true,
            track_loaded_in_source: true,
            buffer_ready: true,
        },
    );
    assert_eq!(state.intent, Intent::Playing);
    assert!(!state.buffering);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Engine(Command::Play)))
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::LoadCurrentProgram { .. }))
    );
}

#[test]
fn t3_play_while_buffering_is_a_no_op() {
    let mut s = state();
    s.intent = Intent::Playing;
    s.buffering = true;
    let (state, effects) = reduce(
        s,
        Input::Play {
            has_current: true,
            track_loaded_in_source: true,
            buffer_ready: false,
        },
    );
    assert!(state.buffering);
    assert!(effects.is_empty());
}

#[test]
fn t4_pause_in_playing() {
    let mut s = state();
    s.intent = Intent::Playing;
    let (state, effects) = reduce(s, Input::Pause);
    assert_eq!(state.intent, Intent::Paused);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Engine(Command::Pause)))
    );
}

#[test]
fn t4_pause_while_already_paused_is_a_no_op() {
    let mut s = state();
    s.intent = Intent::Paused;
    let (state, effects) = reduce(s, Input::Pause);
    assert_eq!(state.intent, Intent::Paused);
    assert!(effects.is_empty());
}

#[test]
fn t5_stop_from_any_state_resets_intent() {
    for start in [Intent::Playing, Intent::Paused, Intent::Stopped] {
        let mut s = state();
        s.intent = start;
        let (state, effects) = reduce(s, Input::Stop);
        assert_eq!(state.intent, Intent::Stopped);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Engine(Command::Stop)))
        );
    }
}

#[test]
fn t6_seek_in_stopped_becomes_paused_at_position() {
    let (state, effects) = reduce(
        state(),
        Input::Seek {
            position_ms: 5_000,
            buffer_ready: true,
        },
    );
    assert_eq!(state.intent, Intent::Paused);
    assert!(effects.contains(&Effect::SeekTo { position_ms: 5_000 }));
}

#[test]
fn t7_seek_past_end_requests_seek_past_end_advance() {
    let mut s = state();
    s.track_len_ms = Some(1_000);
    let (_, effects) = reduce(
        s,
        Input::Seek {
            position_ms: 1_500,
            buffer_ready: true,
        },
    );
    assert_eq!(
        effects,
        vec![Effect::Queue(QueueOp::Advance(AdvanceReason::SeekPastEnd))]
    );
}

#[test]
fn t8_seek_while_playing_sets_buffering_when_not_ready() {
    let mut s = state();
    s.intent = Intent::Playing;
    s.track_len_ms = Some(10_000);
    let (state, effects) = reduce(
        s,
        Input::Seek {
            position_ms: 2_000,
            buffer_ready: false,
        },
    );
    assert!(state.buffering);
    assert!(effects.contains(&Effect::SeekTo { position_ms: 2_000 }));
}

#[test]
fn t8_seek_while_playing_and_ready_clears_buffering() {
    let mut s = state();
    s.intent = Intent::Playing;
    s.track_len_ms = Some(10_000);
    let (state, _) = reduce(
        s,
        Input::Seek {
            position_ms: 2_000,
            buffer_ready: true,
        },
    );
    assert!(!state.buffering);
}

#[test]
fn t9_skip_forward_requests_a_skip_advance() {
    let (_, effects) = reduce(state(), Input::SkipForward);
    assert_eq!(
        effects,
        vec![Effect::Queue(QueueOp::Advance(AdvanceReason::Skip))]
    );
}

#[test]
fn t9_user_skip_forward_move_to_reloads_the_program() {
    let mut s = state();
    s.intent = Intent::Playing;
    let change = QueueChange {
        order_changed: true,
        cursor_changed: true,
        playback: PlaybackChange::MoveTo(any_uid()),
    };
    let (_, effects) = reduce(
        s,
        Input::QueueChanged {
            change,
            origin: QueueChangeOrigin::UserSkipForward,
        },
    );
    assert!(effects.contains(&Effect::LoadCurrentProgram {
        position_ms: 0,
        start_playing: true
    }));
}

#[test]
fn t9_skip_forward_end_of_queue_stops_at_position_zero() {
    let change = QueueChange {
        order_changed: false,
        cursor_changed: true,
        playback: PlaybackChange::EndOfQueue,
    };
    let (state, effects) = reduce(
        state(),
        Input::QueueChanged {
            change,
            origin: QueueChangeOrigin::UserSkipForward,
        },
    );
    assert_eq!(state.intent, Intent::Stopped);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Engine(Command::Stop)))
    );
}

#[test]
fn t10_skip_back_requests_a_skip_back() {
    let (_, effects) = reduce(state(), Input::SkipBack { position_ms: 1_000 });
    assert_eq!(
        effects,
        vec![Effect::Queue(QueueOp::SkipBack { position_ms: 1_000 })]
    );
}

#[test]
fn t10_skip_back_restart_seeks_to_zero() {
    let change = QueueChange {
        order_changed: false,
        cursor_changed: false,
        playback: PlaybackChange::Restart,
    };
    let (_, effects) = reduce(
        state(),
        Input::QueueChanged {
            change,
            origin: QueueChangeOrigin::UserSkipBack,
        },
    );
    assert_eq!(effects, vec![Effect::SeekTo { position_ms: 0 }]);
}

#[test]
fn t10_skip_back_move_to_reloads_the_program() {
    let change = QueueChange {
        order_changed: false,
        cursor_changed: true,
        playback: PlaybackChange::MoveTo(any_uid()),
    };
    let (_, effects) = reduce(
        state(),
        Input::QueueChanged {
            change,
            origin: QueueChangeOrigin::UserSkipBack,
        },
    );
    assert!(effects.contains(&Effect::LoadCurrentProgram {
        position_ms: 0,
        start_playing: false
    }));
}

#[test]
fn t11_end_of_track_only_mirrors_the_advance() {
    let (_, effects) = reduce(state(), Input::EndOfTrack);
    assert_eq!(
        effects,
        vec![Effect::Queue(QueueOp::Advance(AdvanceReason::TrackEnd))]
    );
}

#[test]
fn t11_end_of_track_mirror_move_to_does_not_reload_the_program() {
    let change = QueueChange {
        order_changed: true,
        cursor_changed: true,
        playback: PlaybackChange::MoveTo(any_uid()),
    };
    let (_, effects) = reduce(
        state(),
        Input::QueueChanged {
            change,
            origin: QueueChangeOrigin::EndOfTrackMirror,
        },
    );
    assert!(
        effects.is_empty(),
        "the source already advanced itself; the host must not resend the program"
    );
}

#[test]
fn t12_track_started_updates_track_len_when_generation_matches() {
    let (state, _) = reduce(
        state(),
        Input::TrackStarted {
            generation: 0,
            position_ms: 0,
            playing: true,
            track_len_ms: 210_000,
        },
    );
    assert_eq!(state.track_len_ms, Some(210_000));
}

#[test]
fn t12_track_started_ignores_a_stale_generation() {
    let mut s = state();
    s.current_generation = 5;
    let (state, _) = reduce(
        s,
        Input::TrackStarted {
            generation: 4,
            position_ms: 0,
            playing: true,
            track_len_ms: 210_000,
        },
    );
    assert_eq!(state.track_len_ms, None);
}

#[test]
fn t12_track_revealed_requests_a_queue_reveal() {
    let track = TrackRef::new(
        TrackId::new("spotify:track:x").unwrap_or_else(|_| unreachable!()),
        "Title",
        vec![],
        None,
        None,
        1000,
        Availability::Available,
    );
    let (_, effects) = reduce(
        state(),
        Input::TrackRevealed {
            track: track.clone(),
        },
    );
    assert_eq!(effects, vec![Effect::Queue(QueueOp::Reveal(track))]);
}
