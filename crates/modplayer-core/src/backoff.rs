// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared exponential backoff for rate-limited catalog requests (library
//! sync and search retry).

use std::time::Duration;

/// contracts/library-and-search-core.md §4: `15 s x 2^attempt, max 4 min`.
pub(crate) const SYNC_BACKOFF_BASE: Duration = Duration::from_secs(15);
/// Upper bound for any backoff delay.
pub(crate) const SYNC_BACKOFF_MAX: Duration = Duration::from_secs(4 * 60);

/// Delay before retry `attempt`: the server's `retry_after_ms` when given
/// (capped at [`SYNC_BACKOFF_MAX`]), else `15 s x 2^attempt` capped likewise.
pub(crate) fn backoff_delay(attempt: u8, retry_after_ms: Option<u32>) -> Duration {
    if let Some(ms) = retry_after_ms {
        return Duration::from_millis(u64::from(ms)).min(SYNC_BACKOFF_MAX);
    }
    let factor = 1u64
        .checked_shl(u32::from(attempt.min(10)))
        .unwrap_or(1 << 10);
    (SYNC_BACKOFF_BASE * factor as u32).min(SYNC_BACKOFF_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_exponentially_then_caps() {
        assert_eq!(backoff_delay(0, None), Duration::from_secs(15));
        assert_eq!(backoff_delay(1, None), Duration::from_secs(30));
        assert_eq!(backoff_delay(2, None), Duration::from_secs(60));
        assert_eq!(backoff_delay(4, None), SYNC_BACKOFF_MAX);
        assert_eq!(backoff_delay(u8::MAX, None), SYNC_BACKOFF_MAX);
    }

    #[test]
    fn retry_after_wins_but_is_capped() {
        assert_eq!(backoff_delay(5, Some(2_000)), Duration::from_secs(2));
        assert_eq!(backoff_delay(0, Some(u32::MAX)), SYNC_BACKOFF_MAX);
    }
}
