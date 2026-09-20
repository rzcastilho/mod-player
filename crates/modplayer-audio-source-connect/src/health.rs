// SPDX-License-Identifier: MIT OR Apache-2.0

//! Health classification (`librespot_core::ErrorKind` -> `SourceHealth`)
//! and the reconnect backoff state machine (research R7,
//! contracts/connect-source.md §4).

use std::time::{Duration, Instant};

use librespot_core::error::ErrorKind;
use modplayer_audio_source::SourceHealth;

/// What a librespot error implies, beyond the plain `SourceHealth` values
/// (contracts/connect-source.md §4): a `PermissionDenied` on connect means
/// the account was refused (non-Premium) without the device becoming
/// unhealthy — the worker raises `SourceEvent::TierRejected` for that case
/// and leaves health as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    TierRejected,
    Health(HealthOutcome),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthOutcome {
    Transient,
    Unavailable { client_update_required: bool },
}

/// Classify a librespot `ErrorKind` per contracts/connect-source.md §4.
/// `MODPLAYER_CONNECT_FORCE_UNAVAILABLE` (quickstart M6, debug builds
/// only) overrides every classification to `Unavailable`, so the critical
/// "source unavailable" notification, the Status page link, and **Retry**
/// can be rehearsed without a real broken server or account. Read fresh on
/// every call rather than cached, so unsetting the variable and pressing
/// **Retry** recovers without restarting the app.
pub fn classify(kind: ErrorKind) -> Classification {
    if force_unavailable() {
        return Classification::Health(HealthOutcome::Unavailable {
            client_update_required: false,
        });
    }
    match kind {
        ErrorKind::PermissionDenied => Classification::TierRejected,
        ErrorKind::FailedPrecondition | ErrorKind::Unimplemented | ErrorKind::InvalidArgument => {
            Classification::Health(HealthOutcome::Unavailable {
                client_update_required: true,
            })
        }
        ErrorKind::Internal | ErrorKind::DataLoss => {
            Classification::Health(HealthOutcome::Unavailable {
                client_update_required: false,
            })
        }
        // Unauthenticated (token refresh pending), Aborted, Unavailable,
        // DeadlineExceeded, and any other transport/I-O style failure:
        // retryable (contract §4's catch-all row).
        _ => Classification::Health(HealthOutcome::Transient),
    }
}

/// `MODPLAYER_CONNECT_FORCE_UNAVAILABLE`'s debug-only escape hatch
/// (quickstart M6): compiled out of release builds (`debug_assertions`)
/// so it can never fire for a real user, regardless of their environment.
#[cfg(debug_assertions)]
fn force_unavailable() -> bool {
    std::env::var_os("MODPLAYER_CONNECT_FORCE_UNAVAILABLE").is_some()
}

#[cfg(not(debug_assertions))]
fn force_unavailable() -> bool {
    false
}

/// Turn a `HealthOutcome` into the wire `SourceHealth`, given `since` for a
/// fresh `Transient` classification.
pub fn to_source_health(
    outcome: HealthOutcome,
    since: Instant,
    next_retry_in: Duration,
) -> SourceHealth {
    match outcome {
        HealthOutcome::Transient => SourceHealth::Transient {
            since,
            next_retry_in,
        },
        HealthOutcome::Unavailable {
            client_update_required,
        } => SourceHealth::Unavailable {
            client_update_required,
        },
    }
}

/// Exponential backoff, 1 s doubling to a 60 s cap, reset on success
/// (contracts/connect-source.md §4: "identical constants to 002's
/// `RefreshScheduler`").
#[derive(Debug, Clone, Copy)]
pub struct Backoff {
    next: Duration,
}

impl Backoff {
    const START: Duration = Duration::from_secs(1);
    const CAP: Duration = Duration::from_secs(60);

    pub fn new() -> Self {
        Self { next: Self::START }
    }

    /// The delay to wait before the next attempt, then double it
    /// (capped) for the attempt after that.
    pub fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        self.next = (self.next * 2).min(Self::CAP);
        delay
    }

    /// A connection succeeded: restart from `START`.
    pub fn reset(&mut self) {
        self.next = Self::START;
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_matches_the_contract_table() {
        assert_eq!(
            classify(ErrorKind::Unauthenticated),
            Classification::Health(HealthOutcome::Transient)
        );
        assert_eq!(
            classify(ErrorKind::PermissionDenied),
            Classification::TierRejected
        );
        for kind in [
            ErrorKind::FailedPrecondition,
            ErrorKind::Unimplemented,
            ErrorKind::InvalidArgument,
        ] {
            assert_eq!(
                classify(kind),
                Classification::Health(HealthOutcome::Unavailable {
                    client_update_required: true
                })
            );
        }
        for kind in [ErrorKind::Internal, ErrorKind::DataLoss] {
            assert_eq!(
                classify(kind),
                Classification::Health(HealthOutcome::Unavailable {
                    client_update_required: false
                })
            );
        }
        for kind in [
            ErrorKind::Aborted,
            ErrorKind::Unavailable,
            ErrorKind::DeadlineExceeded,
        ] {
            assert_eq!(
                classify(kind),
                Classification::Health(HealthOutcome::Transient)
            );
        }
    }

    #[test]
    fn backoff_doubles_and_caps_at_sixty_seconds() {
        let mut backoff = Backoff::new();
        let delays: Vec<Duration> = (0..8).map(|_| backoff.next_delay()).collect();
        assert_eq!(
            delays,
            vec![1, 2, 4, 8, 16, 32, 60, 60]
                .into_iter()
                .map(Duration::from_secs)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn backoff_resets_to_start_on_success() {
        let mut backoff = Backoff::new();
        let _ = backoff.next_delay();
        let _ = backoff.next_delay();
        backoff.reset();
        assert_eq!(backoff.next_delay(), Duration::from_secs(1));
    }
}
