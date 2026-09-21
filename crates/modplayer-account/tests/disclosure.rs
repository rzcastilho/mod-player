// SPDX-License-Identifier: MIT OR Apache-2.0

//! Disclosure snapshot guard (research R9, SC-007): `disclosure.ftl` must
//! not change without a human deliberately updating
//! `DISCLOSURE_EN_US_SHA256` (and, if the change is substantive,
//! `DISCLOSURE_BUNDLE_VERSION` too, so every user re-sees the welcome
//! screen — FR-004).

use modplayer_account::disclosure::{DISCLOSURE_EN_US_SHA256, DISCLOSURE_EN_US_SOURCE};
use sha2::{Digest, Sha256};

#[test]
fn disclosure_text_is_pinned_to_bundle_version() {
    let mut hasher = Sha256::new();
    // The pin is over LF line endings. `.gitattributes` keeps the checkout
    // LF everywhere, but a CRLF working copy (a Windows clone predating
    // it, `core.autocrlf=true` overriding) must not read as "the text
    // changed": line endings carry no disclosure substance.
    hasher.update(DISCLOSURE_EN_US_SOURCE.replace("\r\n", "\n").as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    assert_eq!(
        hash, DISCLOSURE_EN_US_SHA256,
        "disclosure/privacy text changed: bump DISCLOSURE_BUNDLE_VERSION if the \
         substance changed, then update DISCLOSURE_EN_US_SHA256"
    );
}
