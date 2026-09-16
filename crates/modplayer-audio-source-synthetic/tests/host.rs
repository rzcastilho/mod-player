// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SyntheticHost` pins the `SourceHost` seam's additive guarantee
//! (contracts/audio-source-host.md §5): `attach` returns a
//! `SyntheticSource` at the requested position, and every 001 synthetic
//! test (continuity/determinism/segments) keeps passing unchanged
//! (verified by those still-present test files alongside this one).

use modplayer_audio_source::{AudioSource, SourceCommand, SourceEvent, SourceHost};
use modplayer_audio_source_synthetic::SyntheticHost;

#[test]
fn synthetic_host_attach_restores_position() {
    let mut host = SyntheticHost::new(44_100);
    let source = host.attach(12_345);
    assert_eq!(source.position(), 12_345);
    assert_eq!(source.sample_rate(), 44_100);
}

#[test]
fn synthetic_host_poll_after_initialize_yields_exactly_registered() {
    let mut host = SyntheticHost::new(44_100);
    let events = host.poll();
    assert!(events.is_empty(), "no events before any command");

    host.command(SourceCommand::Initialize {
        device_name: "ModPlayer on Test".to_string(),
        device_id: "0123456789abcdef".to_string(),
    });
    let events = host.poll();
    assert_eq!(
        events,
        vec![SourceEvent::Registered {
            device_name: "ModPlayer on Test".to_string()
        }]
    );
}
