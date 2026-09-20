// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PluginLog`: the 1000-entry ring of every plugin's console output —
//! `log.*` calls and aborted-handler error text (research R19,
//! contracts/plugin-host-service.md §1 `plugin_log()`).

use std::collections::VecDeque;
use std::time::Instant;

use log::Level;

use modplayer_capability_gateway::manifest::PluginIdentifier;

/// The most entries [`PluginLog`] keeps before dropping the oldest
/// (across every plugin — a noisy one can push out an older, quieter
/// one's entries, same trade-off as any single bounded ring).
pub const MAX_LOG_ENTRIES: usize = 1000;

/// One line of a plugin's console (research R19).
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub at: Instant,
    pub plugin: PluginIdentifier,
    pub level: Level,
    pub message: String,
}

/// Every plugin's console output, oldest first, capped at
/// [`MAX_LOG_ENTRIES`].
#[derive(Debug, Default)]
pub struct PluginLog {
    entries: VecDeque<LogEntry>,
}

impl PluginLog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one entry (from a drained `RuntimeEvent::Log`, or the
    /// host's own record of an aborted handler's error text), evicting
    /// the oldest beyond the cap. Also forwards to the `log` facade at
    /// `target: "plugin:<identifier>"` (research R19) so a host-observed
    /// entry (as opposed to a `log.*` call the plugin thread already
    /// logs itself) still reaches the process's own log sink.
    pub fn push(&mut self, plugin: PluginIdentifier, level: Level, message: String) {
        let target = format!("plugin:{plugin}");
        log::log!(target: &target, level, "{message}");
        if self.entries.len() >= MAX_LOG_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(LogEntry {
            at: Instant::now(),
            plugin,
            level,
            message,
        });
    }

    /// Every entry, oldest first.
    pub fn entries(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn caps_at_max_entries() {
        let mut log = PluginLog::new();
        let id = PluginIdentifier::parse("org.modplayer.fixture.log").unwrap();
        for i in 0..MAX_LOG_ENTRIES + 10 {
            log.push(id.clone(), Level::Info, format!("line {i}"));
        }
        assert_eq!(log.len(), MAX_LOG_ENTRIES);
        assert_eq!(log.entries().next().unwrap().message, "line 10");
    }
}
