// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T044: live manual test against the real Spotify Connect protocol.
//! `#[ignore = "manual"]` — needs a live, valid `MODPLAYER_TEST_ACCESS_TOKEN`
//! (scope `streaming`) and a real Premium account; never run in CI
//! (contracts/connect-source.md §7).
//!
//! Run explicitly with:
//! `MODPLAYER_TEST_ACCESS_TOKEN=... cargo test -p modplayer-audio-source-connect --test live -- --ignored --nocapture`

use std::sync::Arc;
use std::time::Duration;

use modplayer_audio_source::{
    AudioSource, Program, SourceCommand, SourceEvent, SourceHost, TrackId,
};
use modplayer_audio_source_connect::{
    ConnectConfig, ConnectSource, CredentialError, ReceiverCredentials,
};

struct EnvCredentials;

impl ReceiverCredentials for EnvCredentials {
    fn access_token(&self) -> Result<String, CredentialError> {
        std::env::var("MODPLAYER_TEST_ACCESS_TOKEN").map_err(|_| CredentialError::Unavailable)
    }
}

/// A well-known, long-lived, freely-playable Spotify track (used by
/// librespot's own examples): "Sanctuary" is not assumed available
/// forever, so this is deliberately overridable via
/// `MODPLAYER_TEST_TRACK_URI` for whoever runs the manual suite next.
fn test_track_uri() -> String {
    std::env::var("MODPLAYER_TEST_TRACK_URI")
        .unwrap_or_else(|_| "spotify:track:6rqhFgbbKwnb9MLmUQDhG6".to_string())
}

#[test]
#[ignore = "manual"]
fn plays_five_seconds_of_a_real_track() {
    let Ok(_) = std::env::var("MODPLAYER_TEST_ACCESS_TOKEN") else {
        panic!("set MODPLAYER_TEST_ACCESS_TOKEN to run this manual test");
    };

    let mut source = ConnectSource::new(ConnectConfig {
        device_name: "ModPlayer Live Test".to_string(),
        device_id: "0123456789abcdef0123456789abcdef".to_string(),
        credentials: Arc::new(EnvCredentials),
    });

    let rt = source.attach(0);
    let shared = source.rt_shared();

    source.command(SourceCommand::Initialize {
        device_name: "ModPlayer Live Test".to_string(),
        device_id: "0123456789abcdef0123456789abcdef".to_string(),
    });

    let track = TrackId::new(test_track_uri()).expect("valid test track uri");
    let mut registered = false;
    let mut track_started = false;
    let mut playing = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);

    while std::time::Instant::now() < deadline && !(registered && track_started && playing) {
        for event in source.poll() {
            match event {
                SourceEvent::Registered { .. } => {
                    registered = true;
                    source.command(SourceCommand::LoadProgram(Program {
                        order: vec![track.clone()],
                        cursor_index: 0,
                        position_ms: 0,
                        start_playing: true,
                        repeat_all: false,
                        repeat_one: false,
                        generation: 1,
                    }));
                }
                SourceEvent::TrackStarted { .. } => track_started = true,
                SourceEvent::Playing { .. } => playing = true,
                SourceEvent::Health(health) => println!("health: {health:?}"),
                other => println!("event: {other:?}"),
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(registered, "must register with the service");
    assert!(track_started, "must start the requested track");
    assert!(playing, "must reach the Playing state");

    // Pull ~5 s of audio through the RT half and confirm it is not
    // silence throughout (contract §7's "non-silent frames consumed").
    let mut rt = rt;
    let mut buffer = vec![0.0f32; 4096];
    let mut non_silent_frames: u64 = 0;
    let target_frames: u64 = 44_100 * 5;
    let read_deadline = std::time::Instant::now() + Duration::from_secs(15);
    while shared.consumed_frames() < target_frames && std::time::Instant::now() < read_deadline {
        rt.fill(&mut buffer);
        non_silent_frames += buffer
            .chunks(2)
            .filter(|frame| frame[0].abs() > 1e-6 || frame[1].abs() > 1e-6)
            .count() as u64;
        std::thread::sleep(Duration::from_millis(20));
    }

    // Require ≥ 4 s of the ~5 s window to be non-silent. The original
    // "all 220_500 frames non-silent" demanded zero silence across the
    // whole window, which no real network stream meets — there is always a
    // brief initial buffering gap (measured ≈ 90 ms / ~4 k frames on a
    // verified live run, i.e. ~4.9 s of the 5 s was audible). 4 s still
    // proves sustained real music, not a decode blip.
    assert!(
        non_silent_frames >= 176_400,
        "expected at least 176400 non-silent frames (4s @ 44.1kHz), got {non_silent_frames}"
    );

    source.command(SourceCommand::Shutdown);
}
