// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Gateway::admit`: the one path every plugin request takes before it
//! reaches a local capability or an RPC to the host (G1, G2, data-model.md
//! §1.6).

use std::time::Instant;

use crate::api::RequestKind;
use crate::focus::{FocusToken, PluginId};
use crate::grants::Grants;
use crate::limiter::RateLimiter;
use crate::refusal::Refusal;

/// A plugin's admission checkpoint: its grants, the shared focus token,
/// and its own rate-limit buckets. One per running plugin, held by that
/// plugin's own thread (RT1).
#[derive(Debug, Clone)]
pub struct Gateway {
    plugin: PluginId,
    grants: Grants,
    focus: FocusToken,
    limiter: RateLimiter,
}

impl Gateway {
    #[must_use]
    pub fn new(plugin: PluginId, grants: Grants, focus: FocusToken) -> Self {
        Self {
            plugin,
            grants,
            focus,
            limiter: RateLimiter::new(),
        }
    }

    #[must_use]
    pub fn plugin(&self) -> PluginId {
        self.plugin
    }

    #[must_use]
    pub fn grants(&self) -> &Grants {
        &self.grants
    }

    #[must_use]
    pub fn focus(&self) -> &FocusToken {
        &self.focus
    }

    /// G2: permission, then focus, then the rate limiter — in that fixed
    /// order. A refusal at any step means the call never reaches the rate
    /// limiter (a refused call consumes no quota) or the host.
    ///
    /// ```
    /// use std::time::Instant;
    /// use modplayer_capability_gateway::api::RequestKind;
    /// use modplayer_capability_gateway::focus::{FocusToken, PluginId};
    /// use modplayer_capability_gateway::gateway::Gateway;
    /// use modplayer_capability_gateway::grants::Grants;
    ///
    /// let mut gateway = Gateway::new(PluginId(0), Grants::none(), FocusToken::new());
    /// let refusal = gateway
    ///     .admit(RequestKind::TransportSeek, Instant::now())
    ///     .unwrap_err();
    /// assert_eq!(refusal.reason, "not_granted");
    /// ```
    pub fn admit(&mut self, kind: RequestKind, now: Instant) -> Result<(), Refusal> {
        if let Some(permission) = kind.requires()
            && !self.grants.holds(permission)
        {
            return Err(Refusal::permission_denied());
        }
        if kind.needs_focus() && self.focus.holder() != Some(self.plugin) {
            return Err(Refusal::no_focus());
        }
        self.limiter.admit(kind.category(), now)
    }
}

// Named tests (`admit_checks_permission_before_focus`,
// `admit_checks_focus_before_rate`, `refused_calls_consume_no_quota`,
// `rate_limit_101st_in_window`, `rate_limit_window_slides`) live in
// `tests/gateway.rs` (Constitution VIII, contracts/gateway-and-runtime.md
// §4) so they exercise the crate's public API only.
