// SPDX-License-Identifier: MIT OR Apache-2.0

//! Catalog command dispatch skeleton + error classification
//! (contracts/catalog-source.md §2 rule 4, §3 "Threads").
//!
//! Foundational phase (004-search-and-library-browse T016): this module
//! wires the concurrency cap and the shared, redacting error classifier
//! that every per-endpoint mapper needs. The endpoint mappers themselves
//! (`search`, `collection`, `playlists`, `track_lists`, `hydrate`) are
//! added as sibling modules, file-by-file, by the user-story tasks that
//! need them (T036/T037 US1; T057-T059 US2) — until then, `worker.rs`
//! dispatches every new `SourceCommand` straight to
//! `CatalogError::Unsupported` through this module's helpers, so the
//! concurrency-gated, per-`request_id` reply contract (§1-2) is already
//! correct and later tasks only add mapping, never plumbing.

use librespot_core::error::ErrorKind;
use modplayer_audio_source::CatalogError;

pub mod collection;
pub mod hydrate;
pub mod playlists;
pub mod search;
pub mod track_lists;

/// Max concurrent in-flight catalog tasks on the worker's tokio runtime
/// (contracts/catalog-source.md §3 "Threads"); further commands queue
/// FIFO behind a `tokio::sync::Semaphore` of this size.
pub const MAX_CONCURRENT: usize = 4;

/// Classify a librespot session error into the redacted `CatalogError`
/// contract (contracts/catalog-source.md §2 rule 4): never the raw error
/// text, never a URL with a token.
pub fn classify_session_error(kind: ErrorKind) -> CatalogError {
    match kind {
        ErrorKind::Unavailable
        | ErrorKind::DeadlineExceeded
        | ErrorKind::Aborted
        | ErrorKind::Cancelled
        | ErrorKind::Unknown => CatalogError::Offline,
        ErrorKind::ResourceExhausted => CatalogError::RateLimited {
            retry_after_ms: None,
        },
        ErrorKind::NotFound => CatalogError::NotFound,
        ErrorKind::PermissionDenied | ErrorKind::Unimplemented => CatalogError::Unsupported,
        _ => CatalogError::Unavailable("catalog request failed".to_string()),
    }
}

/// Classify an HTTP-style status code (hand-encoded `collection2v2`
/// requests, research R3) per the same rule, using `Retry-After` when the
/// caller has it.
pub fn classify_http_status(status: u16, retry_after_ms: Option<u32>) -> CatalogError {
    match status {
        429 => CatalogError::RateLimited { retry_after_ms },
        404 => CatalogError::NotFound,
        403 | 410 => CatalogError::Unsupported,
        500..=599 => CatalogError::Offline,
        _ => CatalogError::Unavailable("catalog request failed".to_string()),
    }
}

/// `MODPLAYER_CATALOG_FORCE_429` (quickstart M12, US3 T072, debug builds
/// only): when set, `worker.rs` answers every catalog command with
/// `CatalogError::RateLimited` instead of dispatching it, so the "stale +
/// Refreshing…" degrade path (FR-015/SC-005) can be rehearsed without a
/// real 429. Read fresh on every command — like `health.rs`'s
/// `MODPLAYER_CONNECT_FORCE_UNAVAILABLE` — so unsetting it and retrying
/// recovers without restarting the app; compiled out of release builds
/// (`debug_assertions`) so it can never fire for a real user.
///
/// The value `paged-once` is handled by [`force_rate_limited_search`]
/// instead and does not rate-limit library or track-list commands.
#[cfg(debug_assertions)]
pub fn force_rate_limited() -> bool {
    std::env::var_os("MODPLAYER_CATALOG_FORCE_429").is_some_and(|v| v != PAGED_ONCE)
}

#[cfg(not(debug_assertions))]
pub fn force_rate_limited() -> bool {
    false
}

/// Search-command variant of [`force_rate_limited`] (contracts/search-
/// status.md D1, research R12): with `MODPLAYER_CATALOG_FORCE_429=paged-once`
/// only the first command with `offset > 0` in the process is rate-limited,
/// so stale rows → automatic retry → recovery can be driven live.
#[cfg(debug_assertions)]
pub fn force_rate_limited_search(offset: u32) -> bool {
    static PAGED_SEEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let value = std::env::var_os("MODPLAYER_CATALOG_FORCE_429");
    force_429_decision(value.as_deref(), offset, &PAGED_SEEN)
}

#[cfg(not(debug_assertions))]
pub fn force_rate_limited_search(_offset: u32) -> bool {
    false
}

#[cfg(debug_assertions)]
const PAGED_ONCE: &str = "paged-once";

/// Decision for the debug-only toggle on a search command: `paged-once`
/// rate-limits only the first command with `offset > 0`, any other set
/// value rate-limits everything, unset rate-limits nothing.
#[cfg(debug_assertions)]
fn force_429_decision(
    value: Option<&std::ffi::OsStr>,
    offset: u32,
    paged_seen: &std::sync::atomic::AtomicBool,
) -> bool {
    use std::sync::atomic::Ordering;
    match value {
        None => false,
        Some(v) if v == PAGED_ONCE => offset > 0 && !paged_seen.swap(true, Ordering::Relaxed),
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_session_errors_per_contract() {
        assert_eq!(
            classify_session_error(ErrorKind::Unavailable),
            CatalogError::Offline
        );
        assert_eq!(
            classify_session_error(ErrorKind::DeadlineExceeded),
            CatalogError::Offline
        );
        assert_eq!(
            classify_session_error(ErrorKind::ResourceExhausted),
            CatalogError::RateLimited {
                retry_after_ms: None
            }
        );
        assert_eq!(
            classify_session_error(ErrorKind::NotFound),
            CatalogError::NotFound
        );
        assert_eq!(
            classify_session_error(ErrorKind::PermissionDenied),
            CatalogError::Unsupported
        );
        assert_eq!(
            classify_session_error(ErrorKind::Unimplemented),
            CatalogError::Unsupported
        );
        assert!(matches!(
            classify_session_error(ErrorKind::Internal),
            CatalogError::Unavailable(_)
        ));
    }

    #[test]
    fn classifies_http_status_per_contract() {
        assert_eq!(
            classify_http_status(429, Some(2_000)),
            CatalogError::RateLimited {
                retry_after_ms: Some(2_000)
            }
        );
        assert_eq!(classify_http_status(404, None), CatalogError::NotFound);
        assert_eq!(classify_http_status(403, None), CatalogError::Unsupported);
        assert_eq!(classify_http_status(410, None), CatalogError::Unsupported);
        assert_eq!(classify_http_status(503, None), CatalogError::Offline);
        assert!(matches!(
            classify_http_status(400, None),
            CatalogError::Unavailable(_)
        ));
    }

    #[cfg(debug_assertions)]
    mod force_429 {
        use std::ffi::OsStr;
        use std::sync::atomic::AtomicBool;

        use super::super::force_429_decision;

        /// D1: `paged-once` fires for the first `offset > 0` only.
        #[test]
        fn paged_once_limits_only_first_paged_command() {
            let seen = AtomicBool::new(false);
            let v = Some(OsStr::new("paged-once"));
            assert!(!force_429_decision(v, 0, &seen), "offset 0 dispatches");
            assert!(force_429_decision(v, 20, &seen), "first paged limited");
            assert!(!force_429_decision(v, 20, &seen), "later paged dispatch");
            assert!(!force_429_decision(v, 0, &seen));
        }

        /// D1: any other value limits everything; unset limits nothing.
        #[test]
        fn other_values_limit_everything() {
            let seen = AtomicBool::new(false);
            for offset in [0, 20, 20] {
                assert!(force_429_decision(Some(OsStr::new("1")), offset, &seen));
            }
            assert!(!force_429_decision(None, 20, &seen));
        }
    }
}
