// SPDX-License-Identifier: MIT OR Apache-2.0

//! The plugin host: discovery, lifecycle bookkeeping, and the read model
//! the controller drives (009-plugin-runtime-and-permissions,
//! data-model.md §3, contracts/plugin-host-service.md).
//!
//! This module owns nothing real-time: it talks to a plugin only through
//! `modplayer-capability-gateway`'s types and a `modplayer-plugin-
//! runtime::PluginHandle`'s channels (Constitution I, II).

pub mod apply;
pub mod bundled;
pub mod fanout;
pub mod focus;
pub mod host;
pub mod log;
pub mod view;

use std::collections::BTreeMap;
use std::time::Instant;

use modplayer_capability_gateway::budgets::Budgets;
use modplayer_capability_gateway::grants::Grants;
use modplayer_capability_gateway::manifest::{ApiRange, Manifest, ManifestError, PluginIdentifier};

use modplayer_plugin_runtime::events::SuspendCause;
use modplayer_plugin_runtime::handle::PluginHandle;

pub use bundled::BundledPackage;
pub(crate) use focus::TransportActor;
pub use focus::{FocusArbiter, FocusChange, FocusHolder, FocusPolicy, Vacancy};
pub use host::PluginHost;
pub use log::{LogEntry, PluginLog};
pub use view::{FocusRow, PluginRow, PluginsView, TransportFocusView};

/// A plugin's session-stable identity within `modplayer-core` — the same
/// numeric space `modplayer-effects`' `NodeOwner::Plugin`/`markers::model::
/// Owner::Plugin` already use (008's `PluginId`, reserved ahead of this
/// slice). `modplayer-capability-gateway`/`modplayer-plugin-runtime` see a
/// different, 1-based `PluginId` (`focus::PluginId`) — [`to_gateway_id`]/
/// [`from_gateway_id`] convert between the two at the boundary (research
/// R10).
pub use modplayer_effects::catalog::PluginId;

/// `modplayer-core`'s 0-based [`PluginId`] as the gateway/runtime crates'
/// 1-based `focus::PluginId` (`0` reserved there for "the host").
#[must_use]
pub const fn to_gateway_id(id: PluginId) -> modplayer_capability_gateway::focus::PluginId {
    modplayer_capability_gateway::focus::PluginId(id.0 + 1)
}

/// The inverse of [`to_gateway_id`].
#[must_use]
pub const fn from_gateway_id(id: modplayer_capability_gateway::focus::PluginId) -> PluginId {
    PluginId(id.0.saturating_sub(1))
}

/// Interns plugin identifiers into stable, session-scoped [`PluginId`]s
/// (L2, research R10): discovery seeds it in sorted identifier order and
/// ids never change afterward; an owner string a track-state file names
/// that discovery didn't already know is interned too (data-model.md §4).
#[derive(Debug, Clone, Default)]
pub struct PluginIdTable {
    by_identifier: BTreeMap<PluginIdentifier, PluginId>,
    by_id: Vec<PluginIdentifier>,
}

impl PluginIdTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// L2: seed the table from every discovered identifier, in sorted
    /// order, each getting the next stable id in that order.
    #[must_use]
    pub fn seed(identifiers: impl IntoIterator<Item = PluginIdentifier>) -> Self {
        let mut sorted: Vec<PluginIdentifier> = identifiers.into_iter().collect();
        sorted.sort();
        sorted.dedup();
        let mut table = Self::default();
        for identifier in sorted {
            table.intern(identifier);
        }
        table
    }

    /// The existing id for `identifier`, or a freshly allocated one
    /// (data-model.md §4: "owner strings found in marker files are
    /// interned on load").
    pub fn intern(&mut self, identifier: PluginIdentifier) -> PluginId {
        if let Some(id) = self.by_identifier.get(&identifier) {
            return *id;
        }
        let id = PluginId(u16::try_from(self.by_id.len()).unwrap_or(u16::MAX));
        self.by_identifier.insert(identifier.clone(), id);
        self.by_id.push(identifier);
        id
    }

    #[must_use]
    pub fn identifier_of(&self, id: PluginId) -> Option<&PluginIdentifier> {
        self.by_id.get(id.0 as usize)
    }

    #[must_use]
    pub fn id_of(&self, identifier: &PluginIdentifier) -> Option<PluginId> {
        self.by_identifier.get(identifier).copied()
    }
}

/// Where a plugin package came from (data-model.md §3.1). Only `Bundled`
/// is reachable this slice (research: no external install path exists
/// yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Bundled,
}

/// A plugin's coarse health, as shown in the list (FR-023, data-model.md
/// §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Ok,
    Warning,
    Suspended,
}

/// A plugin's lifecycle state (FR-005, FR-011, FR-012, data-model.md
/// §3.1). Transitions:
///
/// ```text
/// Disabled --enable()--> Loading --Ready--> Active
/// Loading/Active --Suspended{cause}--> Suspended  (suspensions += 1; 3rd -> disable())
/// Suspended --Restart--> Loading         --Disable--> Draining --Exited--> Disabled
/// Active --disable()--> Draining --Exited--> Disabled
/// any running --shutdown()--> Draining
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum Lifecycle {
    /// The manifest never parsed/validated; this plugin never runs.
    Invalid(ManifestError),
    Disabled,
    Loading,
    Active,
    Suspended {
        cause: SuspendCause,
    },
    Draining,
}

/// One installed plugin's full bookkeeping (data-model.md §3.1).
pub struct PluginRecord {
    pub id: PluginId,
    pub identifier: PluginIdentifier,
    pub package: BundledPackage,
    pub manifest: Result<Manifest, ManifestError>,
    pub source: Source,
    pub enabled: bool,
    pub lifecycle: Lifecycle,
    /// `None` while `Invalid` (or not yet loaded this session).
    pub health: Option<Health>,
    pub grants: Grants,
    pub suspensions_this_session: u8,
    /// Every `HandlerAborted` timestamp within the trailing 60 s (L5,
    /// FR-010); evicted lazily.
    pub abort_window: std::collections::VecDeque<Instant>,
    /// Set to `now + 5 min` on the 3rd abort within `abort_window`;
    /// `Health::Warning` while `Some(t) if t > now` (L5).
    pub warning_until: Option<Instant>,
    pub handle: Option<PluginHandle>,
    pub gauges: Option<std::sync::Arc<modplayer_plugin_runtime::budget::PluginGauges>>,
    pub api_range: ApiRange,
    pub budgets: Budgets,
    /// Reserved (research: no API-minor compatibility shim exists this
    /// slice — always `false`).
    pub compatibility_mode: bool,
}

impl PluginRecord {
    /// Health derivation (data-model.md §3.1): `Suspended` while the
    /// lifecycle is `Suspended`; `Warning` while a 3-abort warning window
    /// is still open; else `Ok`; `None` for `Invalid`.
    #[must_use]
    pub fn derive_health(&self, now: Instant) -> Option<Health> {
        match &self.lifecycle {
            Lifecycle::Invalid(_) => None,
            Lifecycle::Suspended { .. } => Some(Health::Suspended),
            _ => {
                if self.warning_until.is_some_and(|t| t > now) {
                    Some(Health::Warning)
                } else {
                    Some(Health::Ok)
                }
            }
        }
    }

    #[must_use]
    pub fn is_invalid(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Invalid(_))
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(
            self.lifecycle,
            Lifecycle::Loading | Lifecycle::Active | Lifecycle::Draining
        )
    }
}
