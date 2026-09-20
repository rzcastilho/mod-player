// SPDX-License-Identifier: MIT OR Apache-2.0

//! `FocusArbiter`: the pure transport-focus state machine
//! (010-transport-focus, data-model.md §1, contracts/focus-arbitration.md
//! §1). At any moment the host or exactly one plugin holds transport
//! focus (A1); the arbiter decides who, under the user's [`FocusPolicy`],
//! and returns the [`FocusChange`]s the caller (`PluginHost::
//! apply_focus_changes`) must apply — it never touches a channel, a
//! `FocusToken` or a plugin record itself (research R1, design note 3).

use super::PluginId;

/// The user's global transport-focus assignment policy (spec Key
/// Entities "Focus Policy", data-model.md §1.1). Persisted as
/// `AudioSettings.focus_policy`; never the holder or the pending queue
/// (FR-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusPolicy {
    /// Only the user's "Give focus" grants; a plugin's `request_focus()`
    /// is always recorded, never auto-granted.
    Manual,
    /// Default: "Give focus" grants immediately; a local host transport
    /// action returns focus to the host (FR-002, FR-005).
    #[default]
    AutoOnInteraction,
    /// Per track: the first requester (once the host is free and not
    /// locked by a "Take back") is granted; vacancies refill from the
    /// FIFO queue.
    FirstRequestWins,
}

impl FocusPolicy {
    /// Every policy, in the order the Transport panel's selector lists
    /// them (contracts/ui-transport-panel.md §2).
    pub const ALL: [FocusPolicy; 3] = [
        FocusPolicy::Manual,
        FocusPolicy::AutoOnInteraction,
        FocusPolicy::FirstRequestWins,
    ];

    /// The `[transport] focus_policy` wire value (research R5,
    /// data-model.md §2.2).
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            FocusPolicy::Manual => "manual",
            FocusPolicy::AutoOnInteraction => "auto_on_interaction",
            FocusPolicy::FirstRequestWins => "first_request_wins",
        }
    }

    /// The inverse of [`FocusPolicy::wire_name`]; `None` on an unknown
    /// string — the caller (`settings::store`) falls back to
    /// [`FocusPolicy::default`] with an `InvalidField` warning (research
    /// R5), exactly like 007's unknown-theme handling.
    ///
    /// ```
    /// use modplayer_core::FocusPolicy;
    ///
    /// assert_eq!(FocusPolicy::parse("manual"), Some(FocusPolicy::Manual));
    /// assert_eq!(FocusPolicy::parse("not_a_policy"), None);
    /// ```
    #[must_use]
    pub fn parse(s: &str) -> Option<FocusPolicy> {
        match s {
            "manual" => Some(FocusPolicy::Manual),
            "auto_on_interaction" => Some(FocusPolicy::AutoOnInteraction),
            "first_request_wins" => Some(FocusPolicy::FirstRequestWins),
            _ => None,
        }
    }

    /// The Fluent key for this policy's label in the selector
    /// (contracts/ui-transport-panel.md §3).
    #[must_use]
    pub const fn label_key(self) -> &'static str {
        match self {
            FocusPolicy::Manual => "transport-policy-manual",
            FocusPolicy::AutoOnInteraction => "transport-policy-auto",
            FocusPolicy::FirstRequestWins => "transport-policy-first",
        }
    }
}

/// Who currently holds transport focus (A1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusHolder {
    #[default]
    Host,
    Plugin(PluginId),
}

/// Why a plugin lost focus outside of a user or arbiter decision
/// (contracts/focus-arbitration.md A8, research R11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vacancy {
    /// The holder itself called `release_focus()`.
    Voluntary,
    /// The holder was suspended, disabled, or crashed — its thread is
    /// exiting or gone, so no `focus_revoked` is sent (A8).
    Fault,
}

/// One delivery the caller must make after an arbiter call (data-model.md
/// §1.4). A returned `Vec<FocusChange>` always lists every `Revoked`
/// before any `Granted` (A11, FR-014).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusChange {
    /// Deliver `focus_revoked { holder: new_holder }` to `plugin`.
    Revoked {
        plugin: PluginId,
        new_holder: FocusHolder,
    },
    /// Deliver `focus_granted { holder: plugin }` to `plugin`.
    Granted { plugin: PluginId },
}

/// Who is currently driving a transport method call, from the
/// controller's point of view (controller-private, research R3,
/// data-model.md §1.5). Distinguishes the user's own local actions (the
/// only ones that can trigger the auto-policy revoke hook, FR-002a) from
/// a plugin's own RPC and from a remote controller/transfer command.
/// Set by `PlaybackController::with_transport_actor`, a scoped helper
/// that restores the previous value on exit (nested calls are safe).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TransportActor {
    #[default]
    LocalUser,
    Plugin(PluginId),
    Remote,
}

/// The transport-focus state machine (spec Key Entities "Transport Focus
/// State", data-model.md §1.3). Session-scoped; never serialized —
/// `policy` is the only part of this that `settings.toml` remembers
/// (FR-012, C7).
#[derive(Debug, Clone, Default)]
pub struct FocusArbiter {
    policy: FocusPolicy,
    holder: FocusHolder,
    /// FIFO of outstanding, ungranted requests; no duplicates (A2).
    pending: Vec<PluginId>,
    /// `FirstRequestWins` only: set by a user "Take back", cleared on
    /// track change or a "Give focus" (A6, A9).
    host_locked_for_track: bool,
}

impl FocusArbiter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn policy(&self) -> FocusPolicy {
        self.policy
    }

    #[must_use]
    pub fn holder(&self) -> FocusHolder {
        self.holder
    }

    /// Outstanding requests, FIFO order (earliest first).
    #[must_use]
    pub fn pending(&self) -> &[PluginId] {
        &self.pending
    }

    /// `p`'s 1-based position in [`FocusArbiter::pending`], or `None` if
    /// it is not currently requesting.
    #[must_use]
    pub fn request_order(&self, p: PluginId) -> Option<usize> {
        self.pending.iter().position(|&x| x == p).map(|i| i + 1)
    }

    /// FR-005, A10: changes nothing but `policy` — the holder and pending
    /// queue survive and are judged by the new policy from the next event
    /// on.
    pub fn set_policy(&mut self, policy: FocusPolicy) {
        self.policy = policy;
    }

    /// FR-003/FR-011, A2-A4, A12: `p`'s `request_focus()`. Never fails.
    /// A no-op (keeping `p`'s queue position, or granting nothing) when
    /// `p` already holds focus or is already pending; a caller passing
    /// `running: false` (a defensive path — no production caller does
    /// this for a live RPC) instead purges `p` from `pending` and grants
    /// nothing.
    ///
    /// ```
    /// use modplayer_core::plugins::{FocusArbiter, FocusChange, FocusHolder, FocusPolicy};
    /// use modplayer_effects::catalog::PluginId;
    ///
    /// let mut arbiter = FocusArbiter::new();
    /// arbiter.set_policy(FocusPolicy::FirstRequestWins);
    /// let changes = arbiter.request(PluginId(0), true);
    /// assert_eq!(changes, vec![FocusChange::Granted { plugin: PluginId(0) }]);
    /// assert_eq!(arbiter.holder(), FocusHolder::Plugin(PluginId(0)));
    /// ```
    #[must_use]
    pub fn request(&mut self, p: PluginId, running: bool) -> Vec<FocusChange> {
        if !running {
            self.pending.retain(|&x| x != p);
            return Vec::new();
        }
        if self.holder == FocusHolder::Plugin(p) {
            return Vec::new();
        }
        if self.pending.contains(&p) {
            return Vec::new();
        }
        if matches!(self.policy, FocusPolicy::FirstRequestWins)
            && matches!(self.holder, FocusHolder::Host)
            && !self.host_locked_for_track
            && self.pending.is_empty()
        {
            self.holder = FocusHolder::Plugin(p);
            return vec![FocusChange::Granted { plugin: p }];
        }
        self.pending.push(p);
        Vec::new()
    }

    /// FR-004/FR-011, A7: `p`'s `release_focus()`. The holder vacates
    /// (`Vacancy::Voluntary`); a pending `p` withdraws its request;
    /// neither is a no-op. Always succeeds.
    #[must_use]
    pub fn release(&mut self, p: PluginId) -> Vec<FocusChange> {
        if self.holder == FocusHolder::Plugin(p) {
            return self.vacate(p, Vacancy::Voluntary);
        }
        self.pending.retain(|&x| x != p);
        Vec::new()
    }

    /// FR-008 user "Give focus", A5: revoke the current plugin holder (if
    /// any), grant `p`, drop `p` from `pending`. `give(holder)` is a
    /// no-op. Under `FirstRequestWins`, clears `host_locked_for_track`
    /// (a user grant defines the track's holder).
    #[must_use]
    pub fn give(&mut self, p: PluginId) -> Vec<FocusChange> {
        if self.holder == FocusHolder::Plugin(p) {
            return Vec::new();
        }
        let mut changes = Vec::new();
        if let FocusHolder::Plugin(old) = self.holder {
            changes.push(FocusChange::Revoked {
                plugin: old,
                new_holder: FocusHolder::Plugin(p),
            });
        }
        self.holder = FocusHolder::Plugin(p);
        self.pending.retain(|&x| x != p);
        if matches!(self.policy, FocusPolicy::FirstRequestWins) {
            self.host_locked_for_track = false;
        }
        changes.push(FocusChange::Granted { plugin: p });
        changes
    }

    /// FR-008 user "Take back", A6: revoke to Host under every policy; a
    /// no-op while the host already holds focus (C8). Under
    /// `FirstRequestWins`, locks the host until the next track change (no
    /// refill).
    #[must_use]
    pub fn take_back(&mut self) -> Vec<FocusChange> {
        let FocusHolder::Plugin(old) = self.holder else {
            return Vec::new();
        };
        self.holder = FocusHolder::Host;
        if matches!(self.policy, FocusPolicy::FirstRequestWins) {
            self.host_locked_for_track = true;
        }
        vec![FocusChange::Revoked {
            plugin: old,
            new_holder: FocusHolder::Host,
        }]
    }

    /// FR-002/FR-005 research R3/R4: a *local* host transport action.
    /// Revokes to Host only under `AutoOnInteraction` and only while a
    /// plugin holds focus; a no-op under every other policy or while the
    /// host already holds focus.
    #[must_use]
    pub fn local_host_action(&mut self) -> Vec<FocusChange> {
        if !matches!(self.policy, FocusPolicy::AutoOnInteraction) {
            return Vec::new();
        }
        let FocusHolder::Plugin(old) = self.holder else {
            return Vec::new();
        };
        self.holder = FocusHolder::Host;
        vec![FocusChange::Revoked {
            plugin: old,
            new_holder: FocusHolder::Host,
        }]
    }

    /// FR-006/FR-007, A8: `p` vacates focus (whether it was the holder or
    /// merely pending) for `vacancy`'s reason. A `Voluntary` vacancy from
    /// the holder emits `Revoked`; a `Fault` vacancy never does (the
    /// plugin's thread is exiting or gone). Under `FirstRequestWins`,
    /// once the host is free and not locked, refills from the earliest
    /// still-pending request.
    #[must_use]
    pub fn vacate(&mut self, p: PluginId, vacancy: Vacancy) -> Vec<FocusChange> {
        self.pending.retain(|&x| x != p);
        let mut changes = Vec::new();
        if self.holder == FocusHolder::Plugin(p) {
            self.holder = FocusHolder::Host;
            if matches!(vacancy, Vacancy::Voluntary) {
                changes.push(FocusChange::Revoked {
                    plugin: p,
                    new_holder: FocusHolder::Host,
                });
            }
        }
        if matches!(self.policy, FocusPolicy::FirstRequestWins)
            && !self.host_locked_for_track
            && matches!(self.holder, FocusHolder::Host)
            && let Some(&next) = self.pending.first()
        {
            self.holder = FocusHolder::Plugin(next);
            self.pending.remove(0);
            changes.push(FocusChange::Granted { plugin: next });
        }
        changes
    }

    /// FR-006, A9, research R7: `FirstRequestWins` only — revoke the
    /// holder to Host, clear `pending`, clear the "Take back" lock. Every
    /// other policy is a no-op for focus (the `TrackChanged` fan-out still
    /// happens; it just carries no focus consequence).
    #[must_use]
    pub fn on_track_changed(&mut self) -> Vec<FocusChange> {
        if !matches!(self.policy, FocusPolicy::FirstRequestWins) {
            return Vec::new();
        }
        let mut changes = Vec::new();
        if let FocusHolder::Plugin(old) = self.holder {
            changes.push(FocusChange::Revoked {
                plugin: old,
                new_holder: FocusHolder::Host,
            });
        }
        self.holder = FocusHolder::Host;
        self.pending.clear();
        self.host_locked_for_track = false;
        changes
    }

    /// Defensive: as [`FocusArbiter::vacate`] with [`Vacancy::Fault`] —
    /// no uninstall path exists this slice, but a caller purging a
    /// plugin's other records can purge its focus state the same way.
    #[must_use]
    pub fn remove_plugin(&mut self, p: PluginId) -> Vec<FocusChange> {
        self.vacate(p, Vacancy::Fault)
    }
}
