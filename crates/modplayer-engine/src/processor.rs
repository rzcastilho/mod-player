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

/// The real-time copy of an armed loop region (006, data-model.md §2):
/// written by `LoopSetA`/`LoopSetB`/`LoopSetSeam` into `Processor::
/// loop_staged`, swapped into `loop_active` only by `LoopCommit`
/// (contracts/engine-loop.md §2).
#[derive(Debug, Clone, Copy, Default)]
struct LoopRt {
    a: u64,
    b: u64,
    /// Configured crossfade, in source frames — *not* yet reduced by
    /// `loop_math::effective_crossfade` (that reduction happens at seam
    /// start, contracts/engine-loop.md §4 rule 4).
    crossfade_frames: u32,
    /// `0` = infinite (contracts/engine-loop.md §2).
    repeat: u32,
    wraps: u32,
}

/// An in-progress seam (006, data-model.md §2): captured at its first
/// frame so a `LoopCommit` arriving mid-seam does not disturb it — it
/// finishes with these bounds regardless of what `loop_active` becomes
/// (FR-011a, contracts/engine-loop.md §4 rule 6).
#[derive(Debug, Clone, Copy)]
struct SeamRt {
    a: u64,
    b: u64,
    x: u64,
    gapless: bool,
}

/// Shared wrap bookkeeping for both the zero-width hard-cut path (`x ==
/// 0`) and the seam-completion path (contracts/engine-loop.md §4 rule 7):
/// seeks `source` to `a`, increments and publishes `wraps`, pushes
/// `Event::LoopWrapped`, and evaluates the repeat count. A free function
/// (not a `Processor` method) so its explicit, disjoint field-reference
/// parameters can be called while `render`'s `fresh` slice — a live
/// borrow of the *different* `Processor::scratch` field — is still held.
fn wrap_loop<S: AudioSource>(
    source: &mut S,
    shared: &RtShared,
    events: &mut Producer<Event>,
    loop_active: &mut Option<LoopRt>,
    a: u64,
    gapless: bool,
) {
    source.seek(a);
    let wraps = match loop_active {
        Some(active) => {
            active.wraps = active.wraps.saturating_add(1);
            active.wraps
        }
        None => 0,
    };
    shared.set_loop_wraps(wraps);
    let _ = events.push(Event::LoopWrapped { wraps, gapless });
    if let Some(active) = loop_active
        && active.repeat != 0
        && wraps >= active.repeat
    {
        *loop_active = None;
        let _ = events.push(Event::LoopReleased { wraps });
    }
}

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
    /// Written by `LoopSetA`/`LoopSetB`/`LoopSetSeam`; copied into
    /// `loop_active` only by `LoopCommit` (006, contracts/engine-loop.md
    /// §2).
    loop_staged: LoopRt,
    /// The armed loop region, if any (006, contracts/engine-loop.md §2).
    loop_active: Option<LoopRt>,
    /// An in-progress seam, spanning renders until its jump (or a
    /// suppressed jump on disarm) clears it (006, contracts/engine-loop.md
    /// §4).
    seam: Option<SeamRt>,
    /// Set by `LoopDisarm` while a seam is in flight (006, contracts/
    /// engine-loop.md §2): the seam still finishes for audio continuity,
    /// but no jump happens at its `b`.
    seam_jump_suppressed: bool,
    /// Preallocated incoming-seam buffer, `2 * ceil(0.05 * source_rate)`
    /// samples (stereo), read once per seam from `decoded_store()` (006,
    /// contracts/engine-loop.md §4 rule 1).
    seam_in: Vec<f32>,
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
        // 006, contracts/engine-loop.md §4 rule 1: 2 x ceil(0.05 x
        // source_rate) samples (stereo), sized once here so the seam
        // never allocates on the real-time path.
        let seam_cap_frames = (u64::from(config.source_rate) * 50).div_ceil(1000).max(1) as usize;
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
            loop_staged: LoopRt::default(),
            loop_active: None,
            seam: None,
            seam_jump_suppressed: false,
            seam_in: vec![0.0; seam_cap_frames * 2],
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
                // 006, contracts/engine-loop.md §2: staged setters write
                // `loop_staged`; only `LoopCommit` swaps it into
                // `loop_active`, so a queue that fills mid-sequence can
                // never expose a half-updated region.
                Command::LoopSetA(frame) => self.loop_staged.a = frame,
                Command::LoopSetB(frame) => self.loop_staged.b = frame,
                Command::LoopSetSeam {
                    crossfade_frames,
                    repeat,
                } => {
                    self.loop_staged.crossfade_frames = crossfade_frames;
                    self.loop_staged.repeat = repeat;
                }
                Command::LoopCommit { reset_wraps } => {
                    let staged = self.loop_staged;
                    // Rejected (no-op): degenerate bounds (contracts/
                    // engine-loop.md §4 rule 11) — the controller never
                    // sends this, but the RT enforces its own guard.
                    if staged.b > staged.a && staged.b - staged.a >= 2 {
                        let wraps = if reset_wraps {
                            0
                        } else {
                            self.loop_active.map(|active| active.wraps).unwrap_or(0)
                        };
                        self.loop_active = Some(LoopRt { wraps, ..staged });
                        self.shared.set_loop_wraps(wraps);
                    }
                }
                Command::LoopDisarm => {
                    self.loop_active = None;
                    // A seam already in flight still finishes (audio
                    // continuity) but must not jump at its `b` (contracts/
                    // engine-loop.md §2).
                    if self.seam.is_some() {
                        self.seam_jump_suppressed = true;
                    }
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
        let fresh_frames = fresh.len() / 2;

        // The bounds of the *last* loop wrap performed while filling
        // `fresh` this render, if any — feeds the mid-render position
        // publication rule below (research R8).
        let mut last_wrap: Option<(u64, u64)> = None;

        if playing {
            // 006, contracts/engine-loop.md §4: the loop is evaluated
            // segment by segment, purely from `self.source.position()` —
            // never wall time or a UI value. No allocation, lock, I/O, or
            // log: the seam's incoming frames are read into the
            // preallocated `seam_in` via `DecodedStore::read_frames`
            // (atomics only).
            let mut filled = 0usize;
            while filled < fresh_frames {
                // A seam already in flight (it can span several renders)
                // always takes priority and finishes with its own
                // captured bounds, regardless of what `loop_active`
                // becomes in the meantime (FR-011a).
                if let Some(seam) = self.seam {
                    let pos = self.source.position();
                    let remaining = seam.b.saturating_sub(pos) as usize;
                    let take = remaining.min(fresh_frames - filled);
                    self.shared
                        .set_loop_state(if self.loop_active.is_some() { 2 } else { 0 });
                    if take > 0 {
                        self.source
                            .fill(&mut fresh[filled * 2..(filled + take) * 2]);
                        let seam_start = seam.b - seam.x;
                        for i in 0..take {
                            let idx = (pos + i as u64 - seam_start) as usize;
                            let (g_out, g_in) =
                                crate::loop_math::crossfade_gains(idx as u64, seam.x);
                            let l_in = self.seam_in[idx * 2];
                            let r_in = self.seam_in[idx * 2 + 1];
                            let oi = (filled + i) * 2;
                            fresh[oi] = fresh[oi] * g_out + l_in * g_in;
                            fresh[oi + 1] = fresh[oi + 1] * g_out + r_in * g_in;
                        }
                        filled += take;
                    }
                    if self.source.position() == seam.b {
                        // A `LoopDisarm` that arrived mid-seam suppresses
                        // only the jump — the mix above already played
                        // out for continuity (contracts/engine-loop.md §2).
                        if !self.seam_jump_suppressed {
                            wrap_loop(
                                &mut self.source,
                                &self.shared,
                                &mut self.events,
                                &mut self.loop_active,
                                seam.a,
                                seam.gapless,
                            );
                            last_wrap = Some((seam.a, seam.b));
                            // A repeat-release just cleared `loop_active`;
                            // re-publish `loop_state` now rather than
                            // leaving this iteration's earlier `2` as the
                            // last word if no further iteration re-
                            // classifies before the buffer ends.
                            if self.loop_active.is_none() {
                                self.shared.set_loop_state(0);
                            }
                        }
                        self.seam = None;
                        self.seam_jump_suppressed = false;
                    }
                    continue;
                }

                let Some(active) = self.loop_active else {
                    self.shared.set_loop_state(0);
                    self.source.fill(&mut fresh[filled * 2..]);
                    filled = fresh_frames;
                    continue;
                };
                let pos = self.source.position();
                if pos < active.a {
                    // Natural entry: fill up to `a`, then re-classify
                    // (contracts/engine-loop.md §4 rule 3).
                    self.shared.set_loop_state(1);
                    let take = ((active.a - pos) as usize).min(fresh_frames - filled);
                    self.source
                        .fill(&mut fresh[filled * 2..(filled + take) * 2]);
                    filled += take;
                    continue;
                }
                if pos >= active.b {
                    // Outside from the `b` side, arrived here from a
                    // seek/edit rather than by playing through the seam
                    // this render — plain fill, no jump (FR-012).
                    self.shared.set_loop_state(1);
                    self.source.fill(&mut fresh[filled * 2..]);
                    filled = fresh_frames;
                    continue;
                }

                // a <= pos < b: armed-active.
                self.shared.set_loop_state(2);
                let x = crate::loop_math::effective_crossfade(
                    u64::from(active.crossfade_frames),
                    active.a,
                    active.b,
                )
                .min((self.seam_in.len() / 2) as u64);
                if x == 0 {
                    // Hard cut: fill straight to `b`, then jump
                    // immediately in the same iteration — unlike the `x >
                    // 0` seam below, this never leaves an ambiguous
                    // "arrived at `b`" position for a later iteration or
                    // render to misclassify as "outside" (contracts/
                    // engine-loop.md §4 rules 4, 7; `a_at_zero_hard_cuts`).
                    let take = ((active.b - pos) as usize).min(fresh_frames - filled);
                    self.source
                        .fill(&mut fresh[filled * 2..(filled + take) * 2]);
                    filled += take;
                    if self.source.position() == active.b {
                        let gapless = self
                            .source
                            .decoded_store()
                            .is_some_and(|store| store.covers(active.a));
                        wrap_loop(
                            &mut self.source,
                            &self.shared,
                            &mut self.events,
                            &mut self.loop_active,
                            active.a,
                            gapless,
                        );
                        last_wrap = Some((active.a, active.b));
                        // See the seam-branch comment above: a repeat-
                        // release may have just cleared `loop_active`.
                        if self.loop_active.is_none() {
                            self.shared.set_loop_state(0);
                        }
                    }
                    continue;
                }

                let seam_start = active.b - x;
                if pos < seam_start {
                    let take = ((seam_start - pos) as usize).min(fresh_frames - filled);
                    self.source
                        .fill(&mut fresh[filled * 2..(filled + take) * 2]);
                    filled += take;
                    continue;
                }

                // pos in [seam_start, b), x > 0: start the seam now
                // (contracts/engine-loop.md §4 rule 4) — captured here so
                // a `LoopCommit` arriving before it finishes cannot change
                // these bounds (FR-011a).
                let x_usize = x as usize;
                let mut copied = 0usize;
                let mut covers_a = false;
                if let Some(store) = self.source.decoded_store() {
                    copied = store.read_frames(active.a - x, &mut self.seam_in[..x_usize * 2]);
                    covers_a = store.covers(active.a);
                }
                let gapless = copied == x_usize && covers_a;
                self.seam = Some(SeamRt {
                    a: active.a,
                    b: active.b,
                    x,
                    gapless,
                });
                self.seam_jump_suppressed = false;
            }
        } else {
            fresh.fill(0.0);
            // Rule 10: paused/stopped renders still classify state (so
            // `loop_state` stays right for the UI) but never seam or jump.
            let state = match self.loop_active {
                None => 0,
                Some(active) => {
                    let pos = self.source.position();
                    if pos >= active.a && pos < active.b {
                        2
                    } else {
                        1
                    }
                }
            };
            self.shared.set_loop_state(state);
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
            let raw_pos = self.source.position();
            // research R8: a wrap earlier in this same render can make the
            // naive `raw_pos - leftover` underflow back past `A` once the
            // carried guard frames straddle the wrap — republish through
            // `B` instead in that case; `carry_len` itself is *not*
            // cleared on a loop wrap (unlike `Command::Seek`).
            let published = match last_wrap {
                Some((a, b)) if raw_pos.saturating_sub(a) < leftover as u64 => {
                    b - (leftover as u64 - (raw_pos - a))
                }
                _ => raw_pos.saturating_sub(leftover as u64),
            };
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
