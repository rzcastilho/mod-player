// SPDX-License-Identifier: MIT OR Apache-2.0

//! Spec-fixed budget constants (FR-006, FR-009, FR-010, FR-011, FR-012,
//! FR-014, FR-021, FR-022; data-model.md §1.8). Every number here is
//! fixed by the spec; design note 15 — do not retune without recording a
//! deviation in the spec's Assumptions section.

use std::time::Duration;

/// Every plugin's budget envelope. Currently uniform (`DEFAULT`); the
/// struct exists so a future per-plugin override needs no call-site
/// change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budgets {
    /// CPU allowed per single handler invocation before it is aborted
    /// (FR-009).
    pub handler: Duration,
    /// The aggregate CPU allowed within `window` before the plugin is
    /// suspended (FR-011).
    pub share: Duration,
    /// The rolling window `share` is measured over.
    pub window: Duration,
    /// The Luau heap cap, in bytes (FR-009).
    pub memory: usize,
    /// The per-plugin key-value storage cap, in bytes, across both scopes
    /// (FR-014).
    pub storage: usize,
    /// How long a freshly loaded plugin has to call `api.ready()` before
    /// it is suspended as `did_not_start` (FR-006).
    pub ready_timeout: Duration,
    /// How long `unloading` may run + flush dirty state before the host
    /// proceeds regardless (FR-012).
    pub unload_window: Duration,
    /// How long a gateway RPC may block before returning `host_busy`
    /// (research R3).
    pub rpc_timeout: Duration,
    /// The most pending timers (all kinds combined) a plugin may hold
    /// (FR-022).
    pub max_timers: usize,
    /// The plugin's inbound event queue depth.
    pub inbox: usize,
}

impl Budgets {
    pub const DEFAULT: Budgets = Budgets {
        handler: Duration::from_millis(4),
        share: Duration::from_millis(100),
        window: Duration::from_secs(1),
        memory: 64 * 1024 * 1024,
        storage: 10 * 1024 * 1024,
        ready_timeout: Duration::from_secs(5),
        unload_window: Duration::from_millis(200),
        rpc_timeout: Duration::from_secs(1),
        max_timers: 256,
        inbox: 1024,
    };
}

impl Default for Budgets {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_spec_numbers() {
        assert_eq!(Budgets::DEFAULT.handler, Duration::from_millis(4));
        assert_eq!(Budgets::DEFAULT.share, Duration::from_millis(100));
        assert_eq!(Budgets::DEFAULT.memory, 64 * 1024 * 1024);
        assert_eq!(Budgets::DEFAULT.storage, 10 * 1024 * 1024);
        assert_eq!(Budgets::DEFAULT.ready_timeout, Duration::from_secs(5));
        assert_eq!(Budgets::DEFAULT.unload_window, Duration::from_millis(200));
        assert_eq!(Budgets::DEFAULT.max_timers, 256);
    }
}
