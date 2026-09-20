// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PluginStateStore`: a plugin's own key-value storage, both device
//! (`plugin`) and per-track scopes (G6, G7, FR-014, FR-021, data-model.md
//! §1.7).

use std::collections::BTreeMap;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::refusal::Refusal;

/// The longest a key may be, in bytes (G6).
pub const MAX_KEY_BYTES: usize = 256;
/// The largest a single value's serialized JSON may be, in bytes (G6).
pub const MAX_VALUE_BYTES: usize = 1024 * 1024;
/// The most a plugin's whole store (both scopes, keys + values) may use
/// (G6, FR-014).
pub const STORAGE_CAP: usize = 10 * 1024 * 1024;

/// Which of a plugin's two key-value scopes an operation targets
/// (contract §3 `state.plugin.*` / `state.track.*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Device-scoped; survives sign-out (FR-014).
    Plugin,
    /// Scoped to the current track; cleared on sign-out and absent with
    /// no current track (`no_track`).
    Track,
}

/// A failure loading a persisted state file — distinct from [`Refusal`]
/// since it never reaches Lua (RT11 just discards and logs).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("the state file could not be parsed")]
    Unreadable,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ScopeFile {
    version: u32,
    entries: BTreeMap<String, serde_json::Value>,
}

fn entry_size(key: &str, value: &serde_json::Value) -> usize {
    key.len() + serde_json::to_vec(value).map(|v| v.len()).unwrap_or(0)
}

/// One row of the state store, for a future inspection/debug view
/// (DM-12).
#[derive(Debug, Clone, PartialEq)]
pub struct PluginStateEntry {
    pub scope: Scope,
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: SystemTime,
}

/// A plugin's whole key-value state: the device-scoped `plugin` map and
/// an optional per-track `track` map, with running serialized-size
/// accounting so [`Self::set`] can reject *before* mutating anything that
/// would cross [`STORAGE_CAP`] (G6). Owned by the plugin's own thread —
/// never shared, never RPC'd (research R3).
#[derive(Debug, Clone, Default)]
pub struct PluginStateStore {
    plugin: BTreeMap<String, serde_json::Value>,
    track: Option<(String, BTreeMap<String, serde_json::Value>)>,
    used_bytes: usize,
    plugin_dirty: bool,
    track_dirty: bool,
}

impl PluginStateStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Recompute `used_bytes` from scratch (used after [`Self::
    /// load_plugin`]/[`Self::load_track`]).
    fn recompute_used_bytes(&mut self) {
        let mut total = 0usize;
        for (k, v) in &self.plugin {
            total += entry_size(k, v);
        }
        if let Some((_, entries)) = &self.track {
            for (k, v) in entries {
                total += entry_size(k, v);
            }
        }
        self.used_bytes = total;
    }

    fn scope_map(&self, scope: Scope) -> Option<&BTreeMap<String, serde_json::Value>> {
        match scope {
            Scope::Plugin => Some(&self.plugin),
            Scope::Track => self.track.as_ref().map(|(_, m)| m),
        }
    }

    fn scope_map_mut(&mut self, scope: Scope) -> Option<&mut BTreeMap<String, serde_json::Value>> {
        match scope {
            Scope::Plugin => Some(&mut self.plugin),
            Scope::Track => self.track.as_mut().map(|(_, m)| m),
        }
    }

    /// `None` when `scope` is `Track` and there is no current track
    /// (`no_track`, contract §3), else the value at `key`.
    #[must_use]
    pub fn get(&self, scope: Scope, key: &str) -> Option<&serde_json::Value> {
        self.scope_map(scope).and_then(|m| m.get(key))
    }

    /// Whether `scope` is currently addressable (`Track` needs a current
    /// track loaded).
    #[must_use]
    pub fn scope_available(&self, scope: Scope) -> bool {
        match scope {
            Scope::Plugin => true,
            Scope::Track => self.track.is_some(),
        }
    }

    /// G6: rejects (leaving every field unchanged) a key over
    /// [`MAX_KEY_BYTES`], a value whose JSON exceeds [`MAX_VALUE_BYTES`],
    /// a NaN/infinite float (`invalid_argument`), or a total store size
    /// that would exceed [`STORAGE_CAP`]. Otherwise sets `key = value`
    /// and marks the scope dirty.
    ///
    /// ```
    /// use modplayer_capability_gateway::state::PluginStateStore;
    /// use modplayer_capability_gateway::state::store::Scope;
    /// use serde_json::json;
    ///
    /// let mut store = PluginStateStore::new();
    /// store.set(Scope::Plugin, "volume", json!(0.8)).expect("within budget");
    /// assert_eq!(store.get(Scope::Plugin, "volume"), Some(&json!(0.8)));
    /// ```
    pub fn set(
        &mut self,
        scope: Scope,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), Refusal> {
        if key.len() > MAX_KEY_BYTES {
            return Err(Refusal::budget_exceeded(
                "key_too_long",
                "The key exceeds the 256-byte limit.",
            ));
        }
        if contains_non_finite_number(&value) {
            return Err(Refusal::invalid_state(
                "invalid_argument",
                "The value contains a NaN or infinite number.",
            ));
        }
        let value_bytes = serde_json::to_vec(&value).map(|v| v.len()).unwrap_or(0);
        if value_bytes > MAX_VALUE_BYTES {
            return Err(Refusal::budget_exceeded(
                "value_too_large",
                "The value exceeds the 1 MB limit.",
            ));
        }
        let new_size = key.len() + value_bytes;
        let old_size = self
            .scope_map(scope)
            .and_then(|m| m.get(key))
            .map(|old| entry_size(key, old))
            .unwrap_or(0);
        let prospective = self.used_bytes - old_size + new_size;
        if prospective > STORAGE_CAP {
            return Err(Refusal::budget_exceeded(
                "storage_cap",
                "This plugin's 10 MB storage cap would be exceeded.",
            ));
        }
        let Some(map) = self.scope_map_mut(scope) else {
            return Err(Refusal::no_track());
        };
        map.insert(key.to_string(), value);
        self.used_bytes = prospective;
        match scope {
            Scope::Plugin => self.plugin_dirty = true,
            Scope::Track => self.track_dirty = true,
        }
        Ok(())
    }

    /// A no-op if `key` was never set.
    pub fn remove(&mut self, scope: Scope, key: &str) {
        let removed_size = self
            .scope_map(scope)
            .and_then(|m| m.get(key))
            .map(|v| entry_size(key, v));
        if let Some(size) = removed_size {
            if let Some(map) = self.scope_map_mut(scope) {
                map.remove(key);
            }
            self.used_bytes = self.used_bytes.saturating_sub(size);
            match scope {
                Scope::Plugin => self.plugin_dirty = true,
                Scope::Track => self.track_dirty = true,
            }
        }
    }

    #[must_use]
    pub fn is_dirty(&self, scope: Scope) -> bool {
        match scope {
            Scope::Plugin => self.plugin_dirty,
            Scope::Track => self.track_dirty,
        }
    }

    pub fn mark_clean(&mut self, scope: Scope) {
        match scope {
            Scope::Plugin => self.plugin_dirty = false,
            Scope::Track => self.track_dirty = false,
        }
    }

    /// Load the device-scoped `plugin` map from a persisted file's bytes
    /// (bad bytes are treated as an empty store, mirroring `markers::
    /// store::load`'s unreadable-file rule).
    pub fn load_plugin(&mut self, bytes: &[u8]) -> Result<(), StoreError> {
        let file: ScopeFile = serde_json::from_slice(bytes).map_err(|_| StoreError::Unreadable)?;
        self.plugin = file.entries;
        self.plugin_dirty = false;
        self.recompute_used_bytes();
        Ok(())
    }

    /// RT11: load `track_id`'s per-track scope from `bytes`, replacing
    /// whatever track scope was active. An empty/absent file (no bytes)
    /// is a fresh, empty scope for that track — not an error.
    pub fn load_track(&mut self, track_id: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let entries = if bytes.is_empty() {
            BTreeMap::new()
        } else {
            let file: ScopeFile =
                serde_json::from_slice(bytes).map_err(|_| StoreError::Unreadable)?;
            file.entries
        };
        self.track = Some((track_id.to_string(), entries));
        self.track_dirty = false;
        self.recompute_used_bytes();
        Ok(())
    }

    /// Track change / sign-out: drop the in-memory track scope entirely
    /// (no current track until the next `load_track`).
    pub fn clear_track_scope(&mut self) {
        self.track = None;
        self.track_dirty = false;
        self.recompute_used_bytes();
    }

    /// G7: deterministic (sorted keys — `BTreeMap` already is — pretty
    /// JSON) encoding of one scope, for the writer.
    #[must_use]
    pub fn encode(&self, scope: Scope) -> Vec<u8> {
        let entries = self.scope_map(scope).cloned().unwrap_or_default();
        let file = ScopeFile {
            version: 1,
            entries,
        };
        serde_json::to_vec_pretty(&file).unwrap_or_else(|_| b"{}".to_vec())
    }

    #[must_use]
    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    #[must_use]
    pub fn current_track(&self) -> Option<&str> {
        self.track.as_ref().map(|(id, _)| id.as_str())
    }
}

fn contains_non_finite_number(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|f| !f.is_finite()),
        serde_json::Value::Array(items) => items.iter().any(contains_non_finite_number),
        serde_json::Value::Object(map) => map.values().any(contains_non_finite_number),
        _ => false,
    }
}

// Named tests (`set_rejects_long_key`, `set_rejects_large_value`,
// `cap_is_atomic`, `encode_is_deterministic`, proptest `state_roundtrip`,
// plus `writer_is_atomic_and_acks` for `state::writer`) live in
// `tests/state_store.rs` (Constitution VIII, contracts/gateway-and-
// runtime.md §4) so they exercise the crate's public API only.
