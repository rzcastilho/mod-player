// SPDX-License-Identifier: MIT OR Apache-2.0

//! T043: `health::classify` table and `mixer::{to_u16, to_pct}` round-trip
//! within ±1 % (contracts/connect-source.md §4, §7).

use librespot_core::error::ErrorKind;

#[path = "../src/health.rs"]
#[allow(dead_code)]
mod health;
#[path = "../src/mixer.rs"]
#[allow(dead_code)]
mod mixer;

use health::{Classification, HealthOutcome};

#[test]
fn classify_covers_every_row_of_the_contract_table() {
    assert_eq!(
        health::classify(ErrorKind::Unauthenticated),
        Classification::Health(HealthOutcome::Transient)
    );
    assert_eq!(
        health::classify(ErrorKind::PermissionDenied),
        Classification::TierRejected
    );
    assert_eq!(
        health::classify(ErrorKind::FailedPrecondition),
        Classification::Health(HealthOutcome::Unavailable {
            client_update_required: true
        })
    );
    assert_eq!(
        health::classify(ErrorKind::Unimplemented),
        Classification::Health(HealthOutcome::Unavailable {
            client_update_required: true
        })
    );
    assert_eq!(
        health::classify(ErrorKind::InvalidArgument),
        Classification::Health(HealthOutcome::Unavailable {
            client_update_required: true
        })
    );
    assert_eq!(
        health::classify(ErrorKind::Internal),
        Classification::Health(HealthOutcome::Unavailable {
            client_update_required: false
        })
    );
    assert_eq!(
        health::classify(ErrorKind::DataLoss),
        Classification::Health(HealthOutcome::Unavailable {
            client_update_required: false
        })
    );
    assert_eq!(
        health::classify(ErrorKind::Aborted),
        Classification::Health(HealthOutcome::Transient)
    );
    assert_eq!(
        health::classify(ErrorKind::Unavailable),
        Classification::Health(HealthOutcome::Transient)
    );
    assert_eq!(
        health::classify(ErrorKind::DeadlineExceeded),
        Classification::Health(HealthOutcome::Transient)
    );
}

#[test]
fn volume_round_trips_within_one_percent_across_the_whole_range() {
    for pct in 0..=100u8 {
        let round_tripped = mixer::to_pct(mixer::to_u16(pct));
        let diff = i16::from(round_tripped) - i16::from(pct);
        assert!(
            diff.abs() <= 1,
            "{pct}% round-tripped to {round_tripped}% (diff {diff})"
        );
    }
}
