// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SyntheticHost`: the trivial `SourceHost` implementor over
//! `SyntheticSource` (contracts/audio-source-host.md §5). Proves the
//! `SourceHost` seam is additive — every command besides `Initialize` is a
//! no-op, and `attach` always hands back the same deterministic built-in
//! track, seeked to the requested position.

use std::sync::Arc;

use modplayer_audio_source::{
    AudioSource, BufferStatus, SourceCommand, SourceEvent, SourceHealth, SourceHost, SourceRtShared,
};

use crate::SyntheticSource;

/// The default `SourceHost` used before any real streaming source is
/// wired up (001's `SyntheticSource`, now behind the `SourceHost` seam).
pub struct SyntheticHost {
    sample_rate: u32,
    events: Vec<SourceEvent>,
    rt_shared: Arc<SourceRtShared>,
}

impl SyntheticHost {
    /// Construct a host producing `sample_rate` Hz synthetic audio.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            events: Vec::new(),
            rt_shared: Arc::new(SourceRtShared::new()),
        }
    }
}

impl Default for SyntheticHost {
    fn default() -> Self {
        Self::new(44_100)
    }
}

impl SourceHost for SyntheticHost {
    type Rt = SyntheticSource;

    fn attach(&mut self, position_frames: u64) -> Self::Rt {
        let mut source = SyntheticSource::new(self.sample_rate);
        source.seek(position_frames);
        source
    }

    fn command(&mut self, cmd: SourceCommand) {
        // Every command besides `Initialize` is a no-op (contracts/
        // audio-source-host.md §2's "Synthetic/Scripted behaviour" column).
        if let SourceCommand::Initialize { device_name, .. } = cmd {
            self.events.push(SourceEvent::Registered { device_name });
        }
    }

    fn poll(&mut self) -> Vec<SourceEvent> {
        std::mem::take(&mut self.events)
    }

    fn buffer_status(&self) -> BufferStatus {
        // "Always ready" (contracts/audio-source-host.md §1).
        BufferStatus {
            ring_fill_frames: u32::MAX,
            ready: true,
            current_prefetched: true,
            next: None,
        }
    }

    fn health(&self) -> SourceHealth {
        SourceHealth::Ok
    }

    fn rt_shared(&self) -> Arc<SourceRtShared> {
        Arc::clone(&self.rt_shared)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn attach_restores_position() {
        let mut host = SyntheticHost::new(44_100);
        let source = host.attach(1_000);
        assert_eq!(source.position(), 1_000);
    }

    #[test]
    fn poll_after_initialize_yields_exactly_registered() {
        let mut host = SyntheticHost::new(44_100);
        host.command(SourceCommand::Initialize {
            device_name: "ModPlayer on Test".to_string(),
            device_id: "deadbeef".to_string(),
        });
        let events = host.poll();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            SourceEvent::Registered { device_name } if device_name == "ModPlayer on Test"
        ));
        // Draining again yields nothing further.
        assert!(host.poll().is_empty());
    }

    #[test]
    fn other_commands_are_no_ops() {
        let mut host = SyntheticHost::new(44_100);
        host.command(SourceCommand::Play);
        host.command(SourceCommand::Pause);
        host.command(SourceCommand::Stop);
        assert!(host.poll().is_empty());
    }
}
