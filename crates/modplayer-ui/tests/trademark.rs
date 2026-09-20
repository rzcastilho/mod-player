// SPDX-License-Identifier: MIT OR Apache-2.0

//! Trademark rule (FR-006, contracts/ui-surface.md "Trademark rule"): the
//! product name, window title, application identifier and any icon must
//! never contain the streaming service's name or mark. Body text keys may
//! say "Spotify" descriptively (`disclosure-*`, `welcome-description`), but
//! a fixed set of branding-surface keys must not resolve to text
//! containing it. `about-product` (Settings › About) joins this list in
//! US2 once it exists.

use modplayer_core::tr;

/// Keys whose resolved text must never contain the service's name
/// (contracts/ui-surface.md: "any key whose name starts with `nav-`,
/// `app-`, `welcome-title`, `about-product`"). None of the current
/// `nav-`/`app-` keys exist as prefixed branding surfaces beyond the nav
/// rail's own labels, which are generic section names, not product
/// branding — `welcome-title` (US1) and `about-product` (US2) are the
/// branding-surface keys introduced so far.
const TRADEMARK_GUARDED_KEYS: &[&str] = &["welcome-title", "about-product"];

#[test]
fn no_service_trademark_in_branding_keys() {
    for key in TRADEMARK_GUARDED_KEYS {
        let resolved = tr(key);
        assert!(
            !resolved.to_lowercase().contains("spotify"),
            "Fluent key `{key}` resolves to `{resolved}`, which names the streaming \
             service — branding-surface keys must stay service-neutral (FR-006)"
        );
    }
}
