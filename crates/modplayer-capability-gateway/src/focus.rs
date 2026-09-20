// SPDX-License-Identifier: MIT OR Apache-2.0

//! Transport focus: a single global holder shared by every plugin and
//! the gateway that admits their requests (010-transport-focus research
//! R1/R12, data-model.md §3.3). The read side (`holder()`) is used by
//! every plugin thread's `Gateway::admit`; the write side (`set_holder`)
//! is core-only — `modplayer-core`'s `FocusArbiter` decides, `PluginHost::
//! apply_focus_changes` writes.

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

    /// Set (or clear) the holder. Core-only write side (research R1,
    /// data-model.md §3.3): `PluginHost::apply_focus_changes` is the sole
    /// caller, driven by `FocusArbiter`'s decisions.
    ///
    /// ```
    /// use modplayer_capability_gateway::focus::{FocusToken, PluginId};
    ///
    /// let token = FocusToken::new();
    /// assert_eq!(token.holder(), None);
    /// token.set_holder(Some(PluginId(0)));
    /// assert_eq!(token.holder(), Some(PluginId(0)));
    /// token.set_holder(None);
    /// assert_eq!(token.holder(), None);
    /// ```
    pub fn set_holder(&self, holder: Option<PluginId>) {
        let raw = match holder {
            None => 0,
            Some(id) => id.0 + 1,
        };
        self.0.store(raw, Ordering::SeqCst);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn set_holder_then_clear() {
        let focus = FocusToken::new();
        assert_eq!(focus.holder(), None);
        focus.set_holder(Some(PluginId(0)));
        assert_eq!(focus.holder(), Some(PluginId(0)));
        focus.set_holder(Some(PluginId(1)));
        assert_eq!(focus.holder(), Some(PluginId(1)));
        focus.set_holder(None);
        assert_eq!(focus.holder(), None);
    }
}
