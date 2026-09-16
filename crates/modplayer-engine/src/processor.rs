// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Processor`: the only code in ModPlayer permitted to run on the audio
//! callback thread. `render()` implements the fixed pipeline (drain
//! commands -> source -> master gain -> + test tone -> limiter -> peak ->
//! output stage -> clock) and must never allocate, lock, block, do I/O, or
//! log (contracts/engine-commands.md, data-model.md §3.2-3.3).
//!
//! `Command::PlayTestTone` (re)starts a `TestTone` from its fade-in,
//! independent of transport state; it is summed into the mix after master
//! gain and before the limiter (US1 T045).

use std::sync::Arc;
use std::time::Instant;

use modplayer_audio_source::AudioSource;
use modplayer_audio_source_synthetic::TestTone;
use rtrb::{Consumer, Producer};

use crate::command::Command;
use crate::event::Event;
use crate::limiter::Limiter;
use crate::output_stage::{MAX_FRAMES, OutputStage};
use crate::shared::RtShared;
use crate::types::{CeilingDb, Transport, VolumePercent};

/// Upper bound on guard frames a single render can hand forward to the
/// next as unconsumed, not-yet-fully-output raw material (engine-delta.md
/// §2). Comfortably above the actual worst case (`required_source_frames`'s
/// "+1" interpolation guard rounds to at most a couple of frames of
/// leftover), so the carry never has to truncate a real leftover.
const MAX_GUARD: usize = 8;

/// Constructor input for `Processor`, built by the controller from its
/// shadow state (data-model.md §3.3).
pub struct ProcessorConfig {
    pub source_rate: u32,
    pub device_rate: u32,
    pub device_channels: u16,
    pub max_frames: usize,
    pub transport: Transport,
    pub position_frames: u64,
    pub master_volume: VolumePercent,
    pub ceiling: CeilingDb,
    pub shared: Arc<RtShared>,
}

/// The real-time audio processor: owned by the stream callback, one per
/// stream. Generic over `AudioSource` so the real-time path carries no
/// trait object (Constitution Principle IV).
pub struct Processor<S: AudioSource> {
    shared: Arc<RtShared>,
    commands: Consumer<Command>,
    // `TrackLooped` is wired up by US2 (T059-T060); kept here now so the
    // queue pair is allocated once at construction, per
    // contracts/engine-commands.md.
    events: Producer<Event>,
    source: S,
    source_rate: u32,
    transport: Transport,
    master_gain: f32,
    /// The test tone, running independently of transport while `Some`
    /// (US1 T045). `PlayTestTone` (re)starts it from its fade-in.
    tone: Option<TestTone>,
    limiter: Limiter,
    output_stage: OutputStage,
    /// Preallocated stereo mix buffer at source rate, sized for
    /// `OutputStage::MAX_FRAMES` frames so no render call ever allocates.
    scratch: Vec<f32>,
    /// Fully-processed (gain/tone/limiter already applied) guard frames the
    /// output stage did not consume last render, prepended to this
    /// render's buffer instead of re-fetching them from the source
    /// (engine-delta.md §2: a ring-buffer source cannot un-consume
    /// frames). `carry_len` of `carry`'s `2 * MAX_GUARD` samples are live.
    carry: [f32; MAX_GUARD * 2],
    carry_len: usize,
}

impl<S: AudioSource> Processor<S> {
    /// Construct a processor from `config`, driving `source` and reading
    /// commands from / pushing events to the given queue halves. Restores
    /// `source`'s position from `config.position_frames` (used on
    /// snapshot-rebuild, research R3).
    pub fn new(
        config: ProcessorConfig,
        mut source: S,
        commands: Consumer<Command>,
        events: Producer<Event>,
    ) -> Self {
        source.seek(config.position_frames);
        let output_stage = OutputStage::new(
            config.source_rate,
            config.device_rate,
            config.device_channels,
        );
        // Size for the largest callback `OutputStage` will ever process,
        // not just `config.max_frames`: the backend may deliver more frames
        // than the preset requested (e.g. a stream rebuilt for a larger
        // preset, or a device that ignores the requested buffer size), and
        // `render` must never index past `scratch` on the real-time thread.
        let scratch_len =
            output_stage.required_source_frames(config.max_frames.max(1).max(MAX_FRAMES)) * 2;
        Self {
            shared: config.shared,
            commands,
            events,
            source,
            source_rate: config.source_rate,
            transport: config.transport,
            master_gain: config.master_volume.to_linear(),
            tone: None,
            limiter: Limiter::new(config.ceiling),
            output_stage,
            scratch: vec![0.0; scratch_len.max(2)],
            carry: [0.0; MAX_GUARD * 2],
            carry_len: 0,
        }
    }

    /// The shared real-time atomics this processor publishes to. Lets an
    /// `OutputBackend` adapter (e.g. `CpalBackend`) record the first
    /// callback's actual frame count into `RtShared::negotiated_frames`
    /// (contracts/output-backend.md) without reaching into processor
    /// internals.
    pub fn shared(&self) -> Arc<RtShared> {
        Arc::clone(&self.shared)
    }

    /// Drain every pending command, applying it to processor state. Called
    /// first thing in `render`, so a command pushed after `render` started
    /// is observed only by the next call (FR-025b).
    fn drain_commands(&mut self) {
        while let Ok(command) = self.commands.pop() {
            match command {
                Command::SetMasterVolume(volume) => self.master_gain = volume.to_linear(),
                Command::SetCeiling(ceiling) => self.limiter.set_ceiling(ceiling),
                Command::Play => self.transport = Transport::Playing,
                Command::Pause => self.transport = Transport::Paused,
                Command::Stop => {
                    self.transport = Transport::Stopped;
                    self.source.seek(0);
                    self.carry_len = 0;
                }
                // (Re)starts the tone from its fade-in, independent of
                // transport (contracts/engine-commands.md).
                Command::PlayTestTone => self.tone = Some(TestTone::new(self.source_rate)),
                // Seeking invalidates any carried guard frames — they were
                // decoded from the pre-seek position (engine-delta.md §1).
                Command::Seek(frame) => {
                    self.source.seek(frame);
                    self.carry_len = 0;
                }
            }
        }
    }

    /// Render one device buffer. `out` is interleaved with
    /// `device_channels` channels and `out.len() / device_channels`
    /// frames. Real-time safe: no allocation, lock, I/O, or logging.
    ///
    /// Note on FR-025(b) under resampling (engine-delta.md §2): a command
    /// pushed between two renders still takes effect from this render's
    /// first *freshly processed* sample; the (at most `MAX_GUARD`,
    /// sub-millisecond) carried prefix was already fully processed last
    /// render and is reused verbatim rather than reprocessed.
    pub fn render(&mut self, out: &mut [f32]) {
        self.drain_commands();

        let channels = usize::from(self.output_stage.device_channels()).max(1);
        let out_frames = out.len() / channels;
        let needed = self.output_stage.required_source_frames(out_frames);
        let scratch = &mut self.scratch[..needed * 2];

        let playing = self.transport == Transport::Playing;

        // Prepend last render's leftover guard frames (already fully
        // processed) and ask the source for only the remainder — a
        // streaming (ring-buffer) source cannot un-consume frames, so this
        // replaces the old rewind-by-`seek` (engine-delta.md §2).
        let carried = self.carry_len.min(needed);
        if carried > 0 {
            scratch[..carried * 2].copy_from_slice(&self.carry[..carried * 2]);
        }
        let fresh = &mut scratch[carried * 2..needed * 2];
        if playing {
            self.source.fill(fresh);
        } else {
            fresh.fill(0.0);
        }

        for sample in fresh.iter_mut() {
            *sample *= self.master_gain;
        }

        if let Some(tone) = &mut self.tone {
            let still_playing = tone.render_add(fresh);
            if !still_playing {
                self.tone = None;
                // Direct field access (not `self.push_event`): `fresh`
                // above already holds a disjoint mutable borrow of
                // `self.scratch`, and only a same-struct field-level borrow
                // (not a `&mut self` method call) can coexist with it.
                let _ = self.events.push(Event::ToneFinished);
            }
        }

        self.limiter.process(fresh);

        let peak = scratch.iter().fold(0.0f32, |max, &s| max.max(s.abs()));
        self.shared.set_peak(peak);

        let consumed = self.output_stage.process(scratch, needed, out, out_frames);

        // Save whatever the output stage did not consume (interpolation
        // guard frame(s)) so next render can reuse it instead of
        // re-fetching from the source.
        let leftover = (needed - consumed).min(MAX_GUARD);
        if leftover > 0 {
            self.carry[..leftover * 2]
                .copy_from_slice(&scratch[consumed * 2..consumed * 2 + leftover * 2]);
        }
        self.carry_len = leftover;

        self.shared.advance_clock(consumed as u64);
        let now = Instant::now();
        if playing {
            let published = self.source.position().saturating_sub(leftover as u64);
            self.shared.set_position_frames(published);
            self.shared
                .write_anchor(published, now, true, consumed as u32);
        } else {
            self.shared
                .write_anchor(self.shared.position_frames(), now, false, consumed as u32);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use modplayer_audio_source_synthetic::SyntheticSource;
    use rtrb::RingBuffer;

    fn build(
        source_rate: u32,
        device_rate: u32,
    ) -> (
        Processor<SyntheticSource>,
        Producer<Command>,
        Consumer<Event>,
        Arc<RtShared>,
    ) {
        let (command_tx, command_rx) = RingBuffer::<Command>::new(256);
        let (event_tx, event_rx) = RingBuffer::<Event>::new(256);
        let shared = Arc::new(RtShared::new());
        let config = ProcessorConfig {
            source_rate,
            device_rate,
            device_channels: 2,
            max_frames: 256,
            transport: Transport::Stopped,
            position_frames: 0,
            master_volume: VolumePercent::new(80),
            ceiling: CeilingDb::default(),
            shared: Arc::clone(&shared),
        };
        let processor = Processor::new(
            config,
            SyntheticSource::new(source_rate),
            command_rx,
            event_tx,
        );
        (processor, command_tx, event_rx, shared)
    }

    #[test]
    fn render_advances_clock_by_consumed_frames_passthrough() {
        let (mut processor, mut commands, _events, shared) = build(44_100, 44_100);
        let _ = commands.push(Command::Play);
        let mut out = vec![0.0f32; 256 * 2];
        processor.render(&mut out);
        assert_eq!(shared.clock_frames(), 256);
    }

    #[test]
    fn stopped_transport_still_advances_clock_with_silence() {
        let (mut processor, _commands, _events, shared) = build(44_100, 44_100);
        let mut out = vec![1.0f32; 256 * 2];
        processor.render(&mut out);
        assert_eq!(shared.clock_frames(), 256);
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn master_volume_command_applies_at_next_boundary() {
        let (mut processor, mut commands, _events, _shared) = build(44_100, 44_100);
        let _ = commands.push(Command::Play);
        let _ = commands.push(Command::SetMasterVolume(VolumePercent::new(0)));
        let mut out = vec![1.0f32; 256 * 2];
        processor.render(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
    }
}
