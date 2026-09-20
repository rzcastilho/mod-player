// SPDX-License-Identifier: MIT OR Apache-2.0

//! `tr(key)`: the one Fluent lookup helper UI code is allowed to call.
//! Locale resources are embedded at compile time from `locales/en-US/*.ftl`
//! (research R10); en-US is the only shipped locale this slice (FR-021).
//!
//! No UI code may contain a literal user-facing string; the `fluent_keys`
//! test (US4/US5) enforces the key table in contracts/ui-surface.md.

use std::borrow::Cow;
use std::collections::HashMap;

use fluent_templates::fluent_bundle::FluentValue;
use fluent_templates::{Loader, langid};

// The `static_loader!` macro's generated bundle-construction code uses
// `unwrap()` internally (parsing resources embedded at compile time, which
// cannot fail short of a corrupt build). An outer attribute on the macro
// invocation itself is ignored by clippy (the lint fires inside the
// expansion, not at the call site), so the allow is scoped to a wrapping
// module instead.
#[allow(clippy::disallowed_methods)]
mod loader {
    use fluent_templates::static_loader;

    static_loader! {
        pub static LOCALES = {
            locales: "../../locales",
            fallback_language: "en-US",
        };
    }
}

use loader::LOCALES;

/// Resolve a Fluent message key against the embedded en-US bundle. Falls
/// back to the raw key when it is missing (a missing key is a content bug
/// to be caught by the `fluent_keys` test, not a reason to crash the UI).
pub fn tr(key: &str) -> String {
    let lang = langid!("en-US");
    LOCALES
        .try_lookup(&lang, key)
        .unwrap_or_else(|| key.to_string())
}

/// Resolve a Fluent message key with placeholder `args` (e.g. `Notification`'s
/// `{ $device }`-style variables). Same fallback behaviour as `tr`.
pub fn tr_args(key: &str, args: &[(&'static str, String)]) -> String {
    let lang = langid!("en-US");
    let mut map: HashMap<Cow<'static, str>, FluentValue<'_>> = HashMap::with_capacity(args.len());
    for (name, value) in args {
        map.insert(Cow::Borrowed(*name), FluentValue::from(value.clone()));
    }
    LOCALES
        .try_lookup_with_args(&lang, key, &map)
        .unwrap_or_else(|| key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_key_falls_back_to_the_key_itself() {
        assert_eq!(tr("definitely-not-a-real-key"), "definitely-not-a-real-key");
    }
}
