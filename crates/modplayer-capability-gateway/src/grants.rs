// SPDX-License-Identifier: MIT OR Apache-2.0

//! Permission grants: what a plugin was actually allowed at load time
//! (data-model.md §1.3, contracts/manifest.md §4, FR-003).

use std::time::SystemTime;

use crate::api::Permission;
use crate::manifest::Manifest;

/// Whether a permission is currently in effect for a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantState {
    Granted,
    Denied,
    NotRequested,
}

/// Who granted a permission (bundled: always at install/load time; no
/// other source exists this slice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantedBy {
    Install,
}

/// One permission row for the list/detail view (DM-11).
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionGrant {
    pub permission: Permission,
    pub state: GrantState,
    pub granted_at: Option<SystemTime>,
    pub granted_by: GrantedBy,
    /// Network hosts recorded by the manifest; unused this slice (no
    /// `network` permission is operable).
    pub hosts: Vec<String>,
}

/// A plugin's grants across the whole catalog (`Copy`: 25 bytes). Built
/// once at load and immutable for the life of a [`crate::gateway::
/// Gateway`] (G4) — the bundled source pre-approves everything requested
/// (FR-003), so there is no setter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grants([GrantState; 25]);

impl Grants {
    /// Every permission `NotRequested` (used by tests and by an `Invalid`
    /// record, which never runs).
    #[must_use]
    pub const fn none() -> Self {
        Self([GrantState::NotRequested; 25])
    }

    /// FR-003: the bundled source pre-approves every permission the
    /// manifest requested, `required` or `optional` alike.
    #[must_use]
    pub fn from_bundled(manifest: &Manifest) -> Self {
        let mut grants = Self::none();
        for req in manifest.required.iter().chain(manifest.optional.iter()) {
            grants.0[permission_index(req.permission)] = GrantState::Granted;
        }
        grants
    }

    /// Whether `permission` is currently granted (the only check
    /// [`crate::gateway::Gateway::admit`] performs).
    #[must_use]
    pub fn holds(&self, permission: Permission) -> bool {
        self.0[permission_index(permission)] == GrantState::Granted
    }

    /// Every granted permission, in catalog order.
    pub fn granted(&self) -> impl Iterator<Item = Permission> + '_ {
        Permission::ALL.into_iter().filter(move |p| self.holds(*p))
    }

    /// Every permission's grant state, in catalog order, for the list/
    /// detail view (DM-11).
    #[must_use]
    pub fn rows(&self) -> Vec<PermissionGrant> {
        Permission::ALL
            .into_iter()
            .map(|permission| PermissionGrant {
                permission,
                state: self.0[permission_index(permission)],
                granted_at: None,
                granted_by: GrantedBy::Install,
                hosts: Vec::new(),
            })
            .collect()
    }
}

fn permission_index(permission: Permission) -> usize {
    Permission::ALL
        .iter()
        .position(|p| *p == permission)
        .unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::api::Permission;

    #[test]
    fn none_holds_nothing() {
        let grants = Grants::none();
        assert!(!grants.holds(Permission::PlaybackObserve));
    }
}
