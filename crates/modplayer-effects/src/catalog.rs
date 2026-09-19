// SPDX-License-Identifier: MIT OR Apache-2.0

//! The shared parameter catalog (data-model.md §1): what a node *of a
//! given `NodeKind`* is, in one place, consumed identically by the
//! real-time `ChainRt` and the controller's `ChainModel` so the UI and
//! the RT agree bit-for-bit on clamping (SC-005).

/// The six built-in effect node types (FR-006). `ALL` is what the
/// "Add node…" control enumerates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NodeKind {
    PitchShift = 0,
    TimeStretch = 1,
    Gain = 2,
    Equalizer = 3,
    Filter = 4,
    StereoTools = 5,
}

impl NodeKind {
    /// Every built-in node kind, in a stable, arbitrary order used by the
    /// "Add node…" control and by tests that iterate every kind.
    pub const ALL: [NodeKind; 6] = [
        NodeKind::PitchShift,
        NodeKind::TimeStretch,
        NodeKind::Gain,
        NodeKind::Equalizer,
        NodeKind::Filter,
        NodeKind::StereoTools,
    ];

    /// The Fluent key for this kind's label (`locales/en-US/effects.ftl`).
    #[must_use]
    pub const fn label_key(self) -> &'static str {
        match self {
            NodeKind::PitchShift => "effects-kind-pitch-shift",
            NodeKind::TimeStretch => "effects-kind-time-stretch",
            NodeKind::Gain => "effects-kind-gain",
            NodeKind::Equalizer => "effects-kind-equalizer",
            NodeKind::Filter => "effects-kind-filter",
            NodeKind::StereoTools => "effects-kind-stereo-tools",
        }
    }
}

/// An id reserved for a future plugin host (009); never constructed by
/// this feature's controller paths — every node this slice creates is
/// [`NodeOwner::Host`] (DM-17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PluginId(pub u16);

/// Who owns a node: only `Host` is reachable from the controller in this
/// slice; `Plugin(_)` is constructible so engine tests can exercise the
/// non-host auto-bypass path (FR-012) ahead of 009's plugin runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeOwner {
    Host,
    Plugin(PluginId),
}

impl NodeOwner {
    /// FR-012's auto-bypass gate: only a non-host node is ever
    /// auto-bypassed.
    #[must_use]
    pub const fn is_host(self) -> bool {
        matches!(self, NodeOwner::Host)
    }
}

/// A parameter's index within its kind's parameter list (data-model.md
/// §1.3). The equalizer's band parameters use the `16 + 4*band + n`
/// scheme so a single `u8` never collides with a future per-node
/// parameter (`0..16` stays reserved for non-band kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParamId(pub u8);

impl ParamId {
    /// The `ParamId` of equalizer band `band`'s (`0..8`) sub-parameter
    /// `n` (`0` = freq, `1` = gain, `2` = q, `3` = type), per data-model.md
    /// §1.3.
    #[must_use]
    pub const fn eq_band(band: u8, n: u8) -> ParamId {
        ParamId(16 + 4 * band + n)
    }
}

/// A parameter's value shape: a continuous range (optionally further
/// clamped to the Nyquist frequency at the current source rate) or a
/// small discrete set of integer-valued states.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamShape {
    Continuous {
        min: f32,
        max: f32,
        /// FR-006, AS 3.4b: effective max is further clamped to
        /// `min(max, 0.45 * source_rate)`.
        nyquist_clamped: bool,
    },
    Discrete {
        count: u8,
    },
}

/// The display unit for a parameter's value (UI hint only; clamping
/// itself only ever looks at [`ParamShape`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Semitones,
    Ratio,
    Decibels,
    Hertz,
    Q,
    Toggle,
    Combo,
    Multiplier,
    Pan,
}

/// One parameter's static definition: id, label key, value shape,
/// transparent default (SC-013) and display unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamDef {
    pub id: ParamId,
    pub key: &'static str,
    pub shape: ParamShape,
    pub default: f32,
    pub unit: Unit,
}

const PITCH_SHIFT_PARAMS: [ParamDef; 3] = [
    ParamDef {
        id: ParamId(0),
        key: "effects-param-semitones",
        shape: ParamShape::Continuous {
            min: -12.0,
            max: 12.0,
            nyquist_clamped: false,
        },
        default: 0.0,
        unit: Unit::Semitones,
    },
    ParamDef {
        id: ParamId(1),
        key: "effects-param-formant",
        shape: ParamShape::Discrete { count: 2 },
        default: 0.0,
        unit: Unit::Toggle,
    },
    ParamDef {
        id: ParamId(2),
        key: "effects-param-mode",
        shape: ParamShape::Discrete { count: 2 },
        default: 0.0,
        unit: Unit::Combo,
    },
];

const TIME_STRETCH_PARAMS: [ParamDef; 2] = [
    ParamDef {
        id: ParamId(0),
        key: "effects-param-ratio",
        shape: ParamShape::Continuous {
            min: 0.25,
            max: 2.0,
            nyquist_clamped: false,
        },
        default: 1.0,
        unit: Unit::Ratio,
    },
    ParamDef {
        id: ParamId(1),
        key: "effects-param-mode",
        shape: ParamShape::Discrete { count: 2 },
        default: 0.0,
        unit: Unit::Combo,
    },
];

const GAIN_PARAMS: [ParamDef; 2] = [
    ParamDef {
        id: ParamId(0),
        key: "effects-param-level",
        shape: ParamShape::Continuous {
            min: -60.0,
            max: 12.0,
            nyquist_clamped: false,
        },
        default: 0.0,
        unit: Unit::Decibels,
    },
    ParamDef {
        id: ParamId(1),
        key: "effects-param-mute",
        shape: ParamShape::Discrete { count: 2 },
        default: 0.0,
        unit: Unit::Toggle,
    },
];

const FILTER_PARAMS: [ParamDef; 3] = [
    ParamDef {
        id: ParamId(0),
        key: "effects-param-filter-mode",
        shape: ParamShape::Discrete { count: 2 },
        default: 0.0,
        unit: Unit::Combo,
    },
    ParamDef {
        id: ParamId(1),
        key: "effects-param-cutoff",
        shape: ParamShape::Continuous {
            min: 20.0,
            max: 20_000.0,
            nyquist_clamped: true,
        },
        default: 20.0,
        unit: Unit::Hertz,
    },
    ParamDef {
        id: ParamId(2),
        key: "effects-param-resonance",
        shape: ParamShape::Continuous {
            min: 0.0,
            max: 1.0,
            nyquist_clamped: false,
        },
        default: 0.0,
        unit: Unit::Q,
    },
];

const STEREO_TOOLS_PARAMS: [ParamDef; 5] = [
    ParamDef {
        id: ParamId(0),
        key: "effects-param-width",
        shape: ParamShape::Continuous {
            min: 0.0,
            max: 2.0,
            nyquist_clamped: false,
        },
        default: 1.0,
        unit: Unit::Multiplier,
    },
    ParamDef {
        id: ParamId(1),
        key: "effects-param-balance",
        shape: ParamShape::Continuous {
            min: -1.0,
            max: 1.0,
            nyquist_clamped: false,
        },
        default: 0.0,
        unit: Unit::Pan,
    },
    ParamDef {
        id: ParamId(2),
        key: "effects-param-mono-sum",
        shape: ParamShape::Discrete { count: 2 },
        default: 0.0,
        unit: Unit::Toggle,
    },
    ParamDef {
        id: ParamId(3),
        key: "effects-param-phase-invert",
        shape: ParamShape::Discrete { count: 2 },
        default: 0.0,
        unit: Unit::Toggle,
    },
    ParamDef {
        id: ParamId(4),
        key: "effects-param-channel-swap",
        shape: ParamShape::Discrete { count: 2 },
        default: 0.0,
        unit: Unit::Toggle,
    },
];

const EQ_BAND_COUNT: usize = 8;
const EQ_PARAM_COUNT: usize = EQ_BAND_COUNT * 4;

/// Band `b`'s default centre frequency, `63 * 2^b` Hz (data-model.md
/// §1.3): 63, 126, 252, 504, 1008, 2016, 4032, 8064 Hz.
const fn eq_band_default_freq(band: u8) -> f32 {
    63.0 * (1u32 << band) as f32
}

const fn build_eq_params() -> [ParamDef; EQ_PARAM_COUNT] {
    let placeholder = ParamDef {
        id: ParamId(0),
        key: "",
        shape: ParamShape::Discrete { count: 0 },
        default: 0.0,
        unit: Unit::Toggle,
    };
    let mut arr = [placeholder; EQ_PARAM_COUNT];
    let mut b: u8 = 0;
    while (b as usize) < EQ_BAND_COUNT {
        let base = (b as usize) * 4;
        arr[base] = ParamDef {
            id: ParamId::eq_band(b, 0),
            key: "effects-param-band-freq",
            shape: ParamShape::Continuous {
                min: 20.0,
                max: 20_000.0,
                nyquist_clamped: true,
            },
            default: eq_band_default_freq(b),
            unit: Unit::Hertz,
        };
        arr[base + 1] = ParamDef {
            id: ParamId::eq_band(b, 1),
            key: "effects-param-band-gain",
            shape: ParamShape::Continuous {
                min: -24.0,
                max: 24.0,
                nyquist_clamped: false,
            },
            default: 0.0,
            unit: Unit::Decibels,
        };
        arr[base + 2] = ParamDef {
            id: ParamId::eq_band(b, 2),
            key: "effects-param-band-q",
            shape: ParamShape::Continuous {
                min: 0.1,
                max: 10.0,
                nyquist_clamped: false,
            },
            default: 1.0,
            unit: Unit::Q,
        };
        arr[base + 3] = ParamDef {
            id: ParamId::eq_band(b, 3),
            key: "effects-param-band-type",
            shape: ParamShape::Discrete { count: 3 },
            default: 0.0,
            unit: Unit::Combo,
        };
        b += 1;
    }
    arr
}

static EQUALIZER_PARAMS: [ParamDef; EQ_PARAM_COUNT] = build_eq_params();

/// Every parameter of `kind`, in `ParamId` order (data-model.md §1.3).
#[must_use]
pub fn params(kind: NodeKind) -> &'static [ParamDef] {
    match kind {
        NodeKind::PitchShift => &PITCH_SHIFT_PARAMS,
        NodeKind::TimeStretch => &TIME_STRETCH_PARAMS,
        NodeKind::Gain => &GAIN_PARAMS,
        NodeKind::Equalizer => &EQUALIZER_PARAMS,
        NodeKind::Filter => &FILTER_PARAMS,
        NodeKind::StereoTools => &STEREO_TOOLS_PARAMS,
    }
}

/// Clamp `value` to `kind`'s `id` parameter's range at `source_rate`
/// (FR-006, FR-010, SC-005): `NaN` becomes the parameter's default; a
/// continuous range further narrows to `min(max, 0.45 * source_rate)`
/// when `nyquist_clamped`; a discrete value rounds to the nearest valid
/// state index. An unknown `(kind, id)` pair (never sent by the model or
/// the RT in practice) clamps to `0.0` defensively rather than panicking.
///
/// ```
/// use modplayer_effects::catalog::{clamp, NodeKind, ParamId};
/// // Gain's level_db (-60..=12): a value above range clamps to the max.
/// assert_eq!(clamp(NodeKind::Gain, ParamId(0), 100.0, 44_100), 12.0);
/// ```
#[must_use]
pub fn clamp(kind: NodeKind, id: ParamId, value: f32, source_rate: u32) -> f32 {
    let Some(def) = params(kind).iter().find(|p| p.id == id) else {
        return 0.0;
    };
    if value.is_nan() {
        return def.default;
    }
    match def.shape {
        ParamShape::Continuous {
            min,
            max,
            nyquist_clamped,
        } => {
            let effective_max = if nyquist_clamped {
                max.min(0.45 * source_rate as f32)
            } else {
                max
            };
            value.clamp(min, effective_max.max(min))
        }
        ParamShape::Discrete { count } => {
            let max_index = f32::from(count.saturating_sub(1));
            value.round().clamp(0.0, max_index)
        }
    }
}

/// Whether `kind`'s `id` parameter is continuous (ramped 20 ms per FR-010)
/// or discrete (crossfaded 5 ms per FR-005); an unknown pair is treated
/// as discrete (the safer, faster-settling default).
#[must_use]
pub fn is_continuous(kind: NodeKind, id: ParamId) -> bool {
    matches!(
        params(kind).iter().find(|p| p.id == id).map(|p| p.shape),
        Some(ParamShape::Continuous { .. })
    )
}

/// FR-008's quality mode: `Performance` (cheaper, used for large
/// excursions) or `Quality` (used near unity, where artifacts are most
/// audible).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityMode {
    Performance = 0,
    Quality = 1,
}

/// A pitch-shift or time-stretch node's current mode plus whether it got
/// there by the FR-008 auto-switch rule (as opposed to an explicit user
/// choice) — the flag alone decides whether the node auto-reverts once
/// back in stage-use range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeState {
    pub mode: QualityMode,
    pub auto_switched: bool,
}

/// Whether `value` is small enough that the WSOLA "stage" path (research
/// R4) is in play: `|semitones| <= 3` for pitch shift, `ratio` in
/// `[0.5, 1.5]` for time stretch (FR-008). Any other kind is never in
/// stage-use range.
#[must_use]
pub fn stage_use(kind: NodeKind, value: f32) -> bool {
    match kind {
        NodeKind::PitchShift => value.abs() <= 3.0,
        NodeKind::TimeStretch => (0.5..=1.5).contains(&value),
        _ => false,
    }
}
/// FR-008 rules (1)/(2), run on every `semitones`/`ratio` change:
/// (1) a `Performance`-mode node whose new value leaves stage-use range
/// auto-switches to `Quality` (flagged `auto_switched`); (2) a `Quality`
/// node that got there by that same auto-switch reverts to `Performance`
/// once the value is back in stage-use range. An explicit `Quality`
/// (`auto_switched == false`) never auto-reverts.
#[must_use]
pub fn mode_after_value_change(kind: NodeKind, value: f32, state: ModeState) -> ModeState {
    let in_range = stage_use(kind, value);
    match state.mode {
        QualityMode::Performance if !in_range => ModeState {
            mode: QualityMode::Quality,
            auto_switched: true,
        },
        QualityMode::Quality if in_range && state.auto_switched => ModeState {
            mode: QualityMode::Performance,
            auto_switched: false,
        },
        _ => state,
    }
}

/// FR-008 rule (3): an explicit user mode choice always takes effect and
/// clears the auto-switch flag, so it persists until the *next* stage-use
/// excursion re-evaluates it.
#[must_use]
pub const fn mode_after_user_set(mode: QualityMode) -> ModeState {
    ModeState {
        mode,
        auto_switched: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_kind_all_has_six_distinct_kinds() {
        let mut seen: Vec<NodeKind> = NodeKind::ALL.to_vec();
        seen.sort_by_key(|k| *k as u8);
        seen.dedup();
        assert_eq!(seen.len(), 6);
    }

    #[test]
    fn eq_band_ids_start_at_sixteen_and_never_collide() {
        for b in 0..8u8 {
            for n in 0..4u8 {
                let id = ParamId::eq_band(b, n);
                assert!(id.0 >= 16);
            }
        }
        // No non-EQ kind uses an id >= 16.
        for kind in NodeKind::ALL {
            if kind == NodeKind::Equalizer {
                continue;
            }
            for def in params(kind) {
                assert!(
                    def.id.0 < 16,
                    "{kind:?} param {def:?} collides with EQ band ids"
                );
            }
        }
    }

    #[test]
    fn clamp_rounds_discrete_to_nearest_state() {
        assert_eq!(clamp(NodeKind::Gain, ParamId(1), 0.4, 44_100), 0.0);
        assert_eq!(clamp(NodeKind::Gain, ParamId(1), 0.6, 44_100), 1.0);
        assert_eq!(clamp(NodeKind::Gain, ParamId(1), 5.0, 44_100), 1.0);
    }

    #[test]
    fn clamp_nan_becomes_default() {
        assert_eq!(
            clamp(NodeKind::PitchShift, ParamId(0), f32::NAN, 44_100),
            0.0
        );
    }
}
