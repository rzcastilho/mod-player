// SPDX-License-Identifier: MIT OR Apache-2.0

//! Transport focus: a single global holder shared by every plugin and
//! the gateway that admits their requests (research R12, data-model.md
//! §1.6, FR-017).

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

/// `PluginId` as seen by the gateway/runtime crates: a 1-based session
/// index (`0` reserved for "the host"). `modplayer-core` maps its own
/// `PluginId(u16)` (re-exported from `modplayer-effects`) to this by
/// adding 1; the two never need to agree on layout beyond that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PluginId(pub u16);

/// The single transport-focus holder (research R12): `0` = the host
/// (nobody, or the host's own transport actions, which ignore this
/// atomic entirely per FR-017), `n` = `PluginId(n - 1)`. Shared (`Arc`)
/// between every plugin's `Gateway` and the host's `PluginHost`.
#[derive(Debug, Clone)]
pub struct FocusToken(Arc<AtomicU16>);

impl Default for FocusToken {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusToken {
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(AtomicU16::new(0)))
    }

    /// The plugin currently holding focus, if any.
    #[must_use]
    pub fn holder(&self) -> Option<PluginId> {
        match self.0.load(Ordering::SeqCst) {
            0 => None,
            n => Some(PluginId(n - 1)),
        }
    }

    /// CAS `0 -> id+1`: `true` on success (focus was free), `false` when
    /// another plugin already holds it (`invalid_state`/`focus_held`,
    /// contract §3). Calling this while `id` already holds focus succeeds
    /// (idempotent; the CAS's expected value `0` simply won't match, so a
    /// caller must check `holder() == Some(id)` first if it needs to tell
    /// "already mine" from "someone else's").
    pub fn try_acquire(&self, id: PluginId) -> bool {
        self.0
            .compare_exchange(0, id.0 + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Release focus, but only if `id` currently holds it (a no-op, never
    /// an error, otherwise — contract §3 "release_focus … ok always").
    pub fn release_if(&self, id: PluginId) {
        let _ = self
            .0
            .compare_exchange(id.0 + 1, 0, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// Unconditionally clear focus (host actions ignore the token per
    /// FR-017; used by suspend/disable teardown, R17).
    pub fn clear(&self) {
        self.0.store(0, Ordering::SeqCst);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn acquire_then_contend() {
        let focus = FocusToken::new();
        assert!(focus.try_acquire(PluginId(0)));
        assert!(!focus.try_acquire(PluginId(1)));
        assert_eq!(focus.holder(), Some(PluginId(0)));
    }

    #[test]
    fn release_if_only_releases_the_holder() {
        let focus = FocusToken::new();
        focus.try_acquire(PluginId(0));
        focus.release_if(PluginId(1));
        assert_eq!(focus.holder(), Some(PluginId(0)));
        focus.release_if(PluginId(0));
        assert_eq!(focus.holder(), None);
    }
}
