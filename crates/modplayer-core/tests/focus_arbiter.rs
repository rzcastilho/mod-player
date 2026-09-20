// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `FocusArbiter` rules A1-A12 (contracts/focus-arbitration.md §1,
//! data-model.md §1.3-§1.6): a pure state machine over the three
//! `FocusPolicy` values, exercised with no channel, no plugin thread and
//! no controller — the full transition table plus a proptest single-
//! holder invariant.

use std::collections::HashSet;

use modplayer_core::PluginId;
use modplayer_core::plugins::{FocusArbiter, FocusChange, FocusHolder, FocusPolicy, Vacancy};
use proptest::prelude::*;

fn pid(n: u16) -> PluginId {
    PluginId(n)
}

#[test]
fn manual_never_auto_grants() {
    let mut a = FocusArbiter::new();
    a.set_policy(FocusPolicy::Manual);
    assert_eq!(a.request(pid(0), true), vec![]);
    assert_eq!(a.holder(), FocusHolder::Host);
    assert_eq!(a.pending(), &[pid(0)]);
}

#[test]
fn auto_never_grants_on_request() {
    let mut a = FocusArbiter::new(); // default AutoOnInteraction
    assert_eq!(a.request(pid(0), true), vec![]);
    assert_eq!(a.holder(), FocusHolder::Host);
    assert_eq!(a.pending(), &[pid(0)]);
}

#[test]
fn first_wins_grants_first_only() {
    let mut a = FocusArbiter::new();
    a.set_policy(FocusPolicy::FirstRequestWins);
    assert_eq!(
        a.request(pid(0), true),
        vec![FocusChange::Granted { plugin: pid(0) }]
    );
    assert_eq!(a.request(pid(1), true), vec![]);
    assert_eq!(a.holder(), FocusHolder::Plugin(pid(0)));
    assert_eq!(a.pending(), &[pid(1)]);
}

#[test]
fn first_wins_later_request_pending() {
    let mut a = FocusArbiter::new();
    a.set_policy(FocusPolicy::FirstRequestWins);
    let _ = a.request(pid(0), true);
    let changes = a.request(pid(1), true);
    assert!(changes.is_empty());
    assert_eq!(a.request_order(pid(1)), Some(1));
}

#[test]
fn first_wins_track_change_resets_holder_and_queue() {
    let mut a = FocusArbiter::new();
    a.set_policy(FocusPolicy::FirstRequestWins);
    let _ = a.request(pid(0), true);
    let _ = a.request(pid(1), true);
    let changes = a.on_track_changed();
    assert_eq!(
        changes,
        vec![FocusChange::Revoked {
            plugin: pid(0),
            new_holder: FocusHolder::Host
        }]
    );
    assert_eq!(a.holder(), FocusHolder::Host);
    assert!(a.pending().is_empty());
}

#[test]
fn first_wins_release_refills_earliest() {
    let mut a = FocusArbiter::new();
    a.set_policy(FocusPolicy::FirstRequestWins);
    let _ = a.request(pid(0), true);
    let _ = a.request(pid(1), true);
    let _ = a.request(pid(2), true);
    let changes = a.release(pid(0));
    assert_eq!(
        changes,
        vec![
            FocusChange::Revoked {
                plugin: pid(0),
                new_holder: FocusHolder::Host
            },
            FocusChange::Granted { plugin: pid(1) }
        ]
    );
    assert_eq!(a.holder(), FocusHolder::Plugin(pid(1)));
    assert_eq!(a.pending(), &[pid(2)]);
}

#[test]
fn first_wins_fault_refills_earliest() {
    let mut a = FocusArbiter::new();
    a.set_policy(FocusPolicy::FirstRequestWins);
    let _ = a.request(pid(0), true);
    let _ = a.request(pid(1), true);
    let changes = a.vacate(pid(0), Vacancy::Fault);
    // Fault never emits Revoked, but the refill still Grants.
    assert_eq!(changes, vec![FocusChange::Granted { plugin: pid(1) }]);
    assert_eq!(a.holder(), FocusHolder::Plugin(pid(1)));
}

#[test]
fn first_wins_take_back_locks_until_track_change() {
    let mut a = FocusArbiter::new();
    a.set_policy(FocusPolicy::FirstRequestWins);
    let _ = a.request(pid(0), true);
    let _ = a.request(pid(1), true);
    let changes = a.take_back();
    assert_eq!(
        changes,
        vec![FocusChange::Revoked {
            plugin: pid(0),
            new_holder: FocusHolder::Host
        }]
    );
    assert_eq!(a.holder(), FocusHolder::Host);
    // No refill despite a pending requester.
    assert_eq!(a.pending(), &[pid(1)]);
    // A fresh request still does not grant while locked.
    assert_eq!(a.request(pid(2), true), vec![]);
    let _ = a.on_track_changed();
    assert_eq!(
        a.request(pid(3), true),
        vec![FocusChange::Granted { plugin: pid(3) }]
    );
}

#[test]
fn give_revokes_then_grants_in_order() {
    let mut a = FocusArbiter::new();
    let _ = a.give(pid(0));
    let changes = a.give(pid(1));
    assert_eq!(
        changes,
        vec![
            FocusChange::Revoked {
                plugin: pid(0),
                new_holder: FocusHolder::Plugin(pid(1))
            },
            FocusChange::Granted { plugin: pid(1) }
        ]
    );
}

#[test]
fn give_to_holder_is_noop() {
    let mut a = FocusArbiter::new();
    let _ = a.give(pid(0));
    assert_eq!(a.give(pid(0)), vec![]);
}

#[test]
fn request_twice_keeps_position() {
    let mut a = FocusArbiter::new();
    let _ = a.request(pid(0), true);
    let _ = a.request(pid(1), true);
    assert_eq!(a.request(pid(0), true), vec![]);
    assert_eq!(a.pending(), &[pid(0), pid(1)]);
}

#[test]
fn release_while_pending_withdraws() {
    let mut a = FocusArbiter::new();
    let _ = a.request(pid(0), true);
    let _ = a.request(pid(1), true);
    assert_eq!(a.release(pid(0)), vec![]);
    assert_eq!(a.pending(), &[pid(1)]);
}

#[test]
fn release_by_non_holder_is_noop() {
    let mut a = FocusArbiter::new();
    let _ = a.give(pid(0));
    assert_eq!(a.release(pid(1)), vec![]);
    assert_eq!(a.holder(), FocusHolder::Plugin(pid(0)));
}

#[test]
fn local_action_revokes_only_under_auto() {
    let mut a = FocusArbiter::new();
    a.set_policy(FocusPolicy::Manual);
    let _ = a.give(pid(0));
    assert_eq!(a.local_host_action(), vec![]);
    assert_eq!(a.holder(), FocusHolder::Plugin(pid(0)));

    a.set_policy(FocusPolicy::AutoOnInteraction);
    let changes = a.local_host_action();
    assert_eq!(
        changes,
        vec![FocusChange::Revoked {
            plugin: pid(0),
            new_holder: FocusHolder::Host
        }]
    );
    assert_eq!(a.holder(), FocusHolder::Host);
}

#[test]
fn policy_switch_keeps_holder_and_queue() {
    let mut a = FocusArbiter::new();
    a.set_policy(FocusPolicy::Manual);
    let _ = a.give(pid(0));
    let _ = a.request(pid(1), true);
    a.set_policy(FocusPolicy::FirstRequestWins);
    assert_eq!(a.holder(), FocusHolder::Plugin(pid(0)));
    assert_eq!(a.pending(), &[pid(1)]);
}

#[test]
fn revoked_precede_granted() {
    let mut a = FocusArbiter::new();
    let _ = a.give(pid(0));
    let changes = a.give(pid(1));
    let revoked_idx = changes
        .iter()
        .position(|c| matches!(c, FocusChange::Revoked { .. }));
    let granted_idx = changes
        .iter()
        .position(|c| matches!(c, FocusChange::Granted { .. }));
    assert!(revoked_idx.unwrap() < granted_idx.unwrap());
}

/// A12: a caller-reported not-running plugin is purged, never granted.
#[test]
fn not_running_request_is_purged_never_granted() {
    let mut a = FocusArbiter::new();
    a.set_policy(FocusPolicy::FirstRequestWins);
    let _ = a.request(pid(1), true);
    assert_eq!(a.request(pid(1), false), vec![]);
    assert!(!a.pending().contains(&pid(1)));
}

proptest! {
    /// Constitution VIII: over any sequence of arbiter operations, the
    /// holder is never also listed as pending, and `pending` never
    /// duplicates an id (data-model.md §1.3 invariants) — together with
    /// `FocusHolder`'s own shape (it can only ever name at most one
    /// plugin), this is the single-holder invariant the contract names.
    #[test]
    fn single_holder_invariant(ops in proptest::collection::vec(0u8..7, 0..200)) {
        let mut a = FocusArbiter::new();
        let policies = FocusPolicy::ALL;
        for (i, op) in ops.into_iter().enumerate() {
            let p = pid(u16::from(op % 4));
            match op % 7 {
                0 => { let _ = a.request(p, true); }
                1 => { let _ = a.release(p); }
                2 => { let _ = a.give(p); }
                3 => { let _ = a.take_back(); }
                4 => { let _ = a.local_host_action(); }
                5 => { let _ = a.vacate(p, Vacancy::Fault); }
                _ => { let _ = a.on_track_changed(); }
            }
            if i % 11 == 0 {
                a.set_policy(policies[i % policies.len()]);
            }
            if let FocusHolder::Plugin(h) = a.holder() {
                assert!(
                    !a.pending().contains(&h),
                    "holder {h:?} must never also be pending: {:?}",
                    a.pending()
                );
            }
            let mut seen = HashSet::new();
            for &q in a.pending() {
                assert!(seen.insert(q), "pending must never duplicate {q:?}");
            }
        }
    }
}
