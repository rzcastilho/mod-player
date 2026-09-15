// SPDX-License-Identifier: MIT OR Apache-2.0

//! Disclosure bundle constants (data-model.md §1.2). `disclosure.ftl`'s
//! real text and the `DISCLOSURE_EN_US_SHA256` hash-pin (research R9).

/// Starts at `1`. Bumped only for substantive changes to the disclosure or
/// privacy notice; a version bump re-shows the welcome screen (FR-004).
pub const DISCLOSURE_BUNDLE_VERSION: u32 = 1;

/// `locales/en-US/disclosure.ftl`'s source, embedded at compile time so the
/// hash-pin test (`disclosure_text_is_pinned_to_bundle_version`) needs no
/// runtime path (research R9).
pub const DISCLOSURE_EN_US_SOURCE: &str = include_str!("../../../locales/en-US/disclosure.ftl");

/// SHA-256 hex of `DISCLOSURE_EN_US_SOURCE`. Pinned by
/// `disclosure_text_is_pinned_to_bundle_version` (SC-007): any change to
/// `disclosure.ftl` fails CI until a human updates this hash (and bumps
/// `DISCLOSURE_BUNDLE_VERSION` too, if the change is substantive).
pub const DISCLOSURE_EN_US_SHA256: &str =
    "8caab1db43385e13ad48a33c60d52b047fd9ba569f65e6b531d6cfd0a9461513";

/// The streaming service's terms page (opened in the system browser).
pub const TERMS_URL: &str = "https://www.spotify.com/legal/end-user-agreement/";

/// The streaming service's Premium upgrade page.
pub const UPGRADE_URL: &str = "https://www.spotify.com/premium/";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_version_is_at_least_one() {
        // Data-model.md §4: "acknowledgement with version 0 is never
        // written" — enforced here as a compile-time-checkable invariant
        // on the constant itself, not a runtime comparison of variables.
        const { assert!(DISCLOSURE_BUNDLE_VERSION >= 1) };
    }
}
