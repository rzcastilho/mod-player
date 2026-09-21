// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §8 (FR-012, SC-004): the overload
//! state machine — an event counted and naming the costliest active
//! slot, host included; the auto-bypass that may follow is restricted to
//! a non-host owner and fires at most once per excursion; the excursion
//! clears after a clean window. Uses `Processor::debug_set_burn_ns`
//! (`NodeSlot::burn_ns`, contracts/engine-effect-chain.md §12) to inflate
//! a test node's cost deterministically rather than needing genuinely
//! heavy DSP.

use std::sync::Arc;

use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_effects::catalog::{NodeKind, NodeOwner, PluginId};
use modplayer_effects::consts::COST_RING_CAPACITY;
use modplayer_engine::{
    CeilingDb, Command, Event, Processor, ProcessorConfig, RtShared, Transport, VolumePercent,
};
use rtrb::{Consumer, RingBuffer};

const FRAMES: usize = 256;
const SOURCE_RATE: u32 = 44_100;
/// This render's period at `SOURCE_RATE`/`FRAMES` is ~5.8 ms; a burn well
/// past that alone pushes a single render's `render_pct` past 100 %
/// (contracts/engine-effect-chain.md §8's immediate-trigger rule).
const OVERLOAD_BURN_NS: u64 = 10_000_000; // 10 ms

fn build() -> (
    Processor<SyntheticSource>,
    rtrb::Producer<Command>,
    Consumer<Event>,
    Arc<RtShared>,
) {
    let (command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, event_rx) = RingBuffer::<Event>::new(1_024);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: SOURCE_RATE,
        device_rate: SOURCE_RATE,
        device_channels: 2,
        max_frames: FRAMES,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared: Arc::clone(&shared),
    };
    let processor = Processor::new(
        config,
        SyntheticSource::new(SOURCE_RATE),
        command_rx,
        event_tx,
    );
    (processor, command_tx, event_rx, shared)
}

fn drain_events(events: &mut Consumer<Event>) -> Vec<Event> {
    let mut out = Vec::new();
    while let Ok(event) = events.pop() {
        out.push(event);
    }
    out
}

/// contracts/engine-effect-chain.md §8: a single render whose measured
/// cost alone exceeds 100 % of the callback period raises exactly one
/// `Event::Overload`, naming the costliest slot, and increments the
/// counter.
#[test]
fn overload_counts_and_names_costliest() {
    let (mut processor, mut commands, mut events, shared) = build();
    let _ = commands.push(Command::Play);
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::Gain,
        owner: NodeOwner::Host,
    });
    let mut out = vec![0.0f32; FRAMES * 2];
    // Settle the insert crossfade before burning, so the burned render's
    // own timing isn't muddied by dry/wet blending overhead.
    for _ in 0..10 {
        processor.render(&mut out);
    }
    let _ = drain_events(&mut events);
    // The counter is cumulative and a settle render can itself overload
    // on a slow machine (a debug-build render on the Windows CI runner
    // did, 256 frames at 44.1 kHz being a 5.8 ms period), so measure the
    // burned render as a delta.
    let overloads_before = shared.overload_count();

    processor.debug_set_burn_ns(0, OVERLOAD_BURN_NS);
    processor.render(&mut out);

    let overloads: Vec<_> = drain_events(&mut events)
        .into_iter()
        .filter(|e| matches!(e, Event::Overload { .. }))
        .collect();
    assert_eq!(
        overloads.len(),
        1,
        "exactly one Overload event for this render: {overloads:?}"
    );
    assert!(
        matches!(
            overloads[0],
            Event::Overload {
                costliest_slot: 0,
                ..
            }
        ),
        "must name the only (and so costliest) active slot: {overloads:?}"
    );
    assert_eq!(shared.overload_count(), overloads_before + 1);
    assert!(shared.over_budget());
}

/// A sustained overload with a non-host costliest slot auto-bypasses it
/// exactly once — not once per render — even though the overload itself
/// (and so `Event::Overload`) keeps firing every render the burn stays
/// on (the slot's own cost never drops just because it was bypassed; the
/// debug burn happens unconditionally).
#[test]
fn non_host_costliest_is_bypassed_exactly_once_per_window() {
    let (mut processor, mut commands, mut events, _shared) = build();
    let _ = commands.push(Command::Play);
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::Gain,
        owner: NodeOwner::Plugin(PluginId(1)),
    });
    let mut out = vec![0.0f32; FRAMES * 2];
    for _ in 0..10 {
        processor.render(&mut out);
    }
    let _ = drain_events(&mut events);

    processor.debug_set_burn_ns(0, OVERLOAD_BURN_NS);
    let mut auto_bypassed = 0;
    for _ in 0..10 {
        processor.render(&mut out);
        for event in drain_events(&mut events) {
            if matches!(event, Event::AutoBypassed { slot: 0 }) {
                auto_bypassed += 1;
            }
        }
    }
    assert_eq!(
        auto_bypassed, 1,
        "a sustained overload must auto-bypass the non-host costliest slot exactly once"
    );
}

/// The same sustained overload against a `Host`-owned costliest slot
/// keeps reporting `Event::Overload` but never bypasses it (FR-012:
/// auto-bypass is restricted to a non-host owner).
#[test]
fn host_costliest_is_never_bypassed() {
    let (mut processor, mut commands, mut events, _shared) = build();
    let _ = commands.push(Command::Play);
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::Gain,
        owner: NodeOwner::Host,
    });
    let mut out = vec![0.0f32; FRAMES * 2];
    for _ in 0..10 {
        processor.render(&mut out);
    }
    let _ = drain_events(&mut events);

    processor.debug_set_burn_ns(0, OVERLOAD_BURN_NS);
    let mut overloads = 0;
    let mut auto_bypassed = 0;
    for _ in 0..10 {
        processor.render(&mut out);
        for event in drain_events(&mut events) {
            match event {
                Event::Overload { .. } => overloads += 1,
                Event::AutoBypassed { .. } => auto_bypassed += 1,
                _ => {}
            }
        }
    }
    assert!(overloads > 0, "a sustained overload must still be reported");
    assert_eq!(auto_bypassed, 0, "a host node is never auto-bypassed");
}

/// Once the burn stops, `over_budget` clears after a clean window
/// (`COST_RING_CAPACITY` consecutive renders under 90 %,
/// contracts/engine-effect-chain.md §8).
#[test]
fn over_budget_clears_after_clean_window() {
    let (mut processor, mut commands, mut events, shared) = build();
    let _ = commands.push(Command::Play);
    let _ = commands.push(Command::ChainInsert {
        slot: 0,
        position: 0,
        kind: NodeKind::Gain,
        owner: NodeOwner::Host,
    });
    let mut out = vec![0.0f32; FRAMES * 2];
    for _ in 0..10 {
        processor.render(&mut out);
    }
    let _ = drain_events(&mut events);

    processor.debug_set_burn_ns(0, OVERLOAD_BURN_NS);
    processor.render(&mut out);
    assert!(
        shared.over_budget(),
        "must be over budget right after the burn"
    );

    processor.debug_set_burn_ns(0, 0);
    for _ in 0..=COST_RING_CAPACITY {
        processor.render(&mut out);
        if !shared.over_budget() {
            break;
        }
    }
    assert!(
        !shared.over_budget(),
        "a clean window of COST_RING_CAPACITY consecutive clean renders must clear over_budget"
    );
}
