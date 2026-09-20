// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fluent message-key constants for account/session notifications
//! (contracts/account-session.md "Events", contracts/ui-surface.md).
//! Content lives in `locales/en-US/account.ftl` (filled in starting US2);
//! named here so `service.rs` and `modplayer-ui::app`'s event mapping
//! reference them by constant rather than scattering string literals
//! (FR-023), matching 001's `modplayer_core::notifications` convention.

/// `StoreUnavailable` (probe failed) — Warning (FR-017).
pub const KEY_STORE_UNAVAILABLE: &str = "signin-store-unavailable";
/// `StoreUnreadable` (launch found a session the secure store can't read)
/// — Warning (FR-017).
pub const KEY_STORE_UNREADABLE: &str = "store-unreadable";
/// `RefreshFailing` — Warning, raised once after the 3rd consecutive
/// refresh failure (FR-014).
pub const KEY_SIGNIN_AGAIN: &str = "signin-again";
/// `SessionExpired` — Critical, raised once per expiry (FR-021).
pub const KEY_SESSION_EXPIRED: &str = "session-expired";
/// `SessionRevoked` — Critical, raised on definitive rejection (FR-019).
pub const KEY_SESSION_REVOKED: &str = "session-revoked";
/// `SignedOut { categories }` — Info, joined category labels (FR-015).
pub const KEY_SIGNED_OUT: &str = "signed-out";
/// `SignOutIncomplete { category }` — Warning, one registry store failed
/// to clear (FR-015).
pub const KEY_SIGNOUT_INCOMPLETE: &str = "signout-incomplete";
