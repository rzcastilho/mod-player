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
#[cfg(debug_assertions)]
pub fn force_rate_limited() -> bool {
    std::env::var_os("MODPLAYER_CATALOG_FORCE_429").is_some()
}

#[cfg(not(debug_assertions))]
pub fn force_rate_limited() -> bool {
    false
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
}
