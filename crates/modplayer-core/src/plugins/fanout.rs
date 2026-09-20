// SPDX-License-Identifier: MIT OR Apache-2.0

//! `fan_out(event)` (C4): deliver a `HostEvent` to every `Active` plugin
//! holding the permission it requires, dropping (never blocking) on a
//! full inbox and logging at most once per plugin per second.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use modplayer_capability_gateway::event::HostEvent;
use modplayer_capability_gateway::manifest::PluginIdentifier;

use super::{Lifecycle, PluginId, PluginRecord};

/// How often a full-inbox drop is logged for the same plugin (C4:
/// "drop + log-once-per-second-per-plugin").
const DROP_LOG_INTERVAL: Duration = Duration::from_secs(1);

/// The per-plugin drop-log rate limiting [`FanOut::send`] needs across
/// calls — host-side bookkeeping, kept separate from `PluginRecord`
/// since it is not plugin state.
#[derive(Debug, Default)]
pub struct FanOut {
    last_drop_logged: HashMap<PluginId, Instant>,
}

impl FanOut {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// C4: deliver `event` to every `Active` plugin whose grants hold
    /// `event.kind().requires()` (`None` ⇒ every `Active` plugin, e.g.
    /// `ready_ack`/`unloading`, which this generic broadcast is not
    /// actually used for — those go straight to one plugin's inbox — but
    /// the check stays permission-symmetric with every other event).
    pub fn send(&mut self, records: &[PluginRecord], event: &HostEvent, now: Instant) {
        let requires = event.kind().requires();
        for record in records {
            if !matches!(record.lifecycle, Lifecycle::Active) {
                continue;
            }
            if let Some(permission) = requires
                && !record.grants.holds(permission)
            {
                continue;
            }
            let Some(handle) = &record.handle else {
                continue;
            };
            if !handle.send_event(event.clone()) {
                self.log_drop(record.id, &record.identifier, now);
            }
        }
    }

    fn log_drop(&mut self, id: PluginId, identifier: &PluginIdentifier, now: Instant) {
        let should_log = self
            .last_drop_logged
            .get(&id)
            .is_none_or(|last| now.saturating_duration_since(*last) >= DROP_LOG_INTERVAL);
        if should_log {
            self.last_drop_logged.insert(id, now);
            log::warn!(target: "plugin", "[{identifier}] inbox full — event dropped");
        }
    }
}
