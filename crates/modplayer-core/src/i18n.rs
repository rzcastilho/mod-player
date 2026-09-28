// SPDX-License-Identifier: MIT OR Apache-2.0

//! `tr(key)`: the one Fluent lookup helper UI code is allowed to call.
//! Locale resources are embedded at compile time from `locales/en-US/*.ftl`
//! (research R10); en-US is the only shipped locale this slice (FR-021).
//!
//! No UI code may contain a literal user-facing string; the `fluent_keys`
//! test (US4/US5) enforces the key table in contracts/ui-surface.md.

use std::borrow::Cow;
use std::cell::Cell;
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
    let resolved = LOCALES
        .try_lookup(&lang, key)
        .unwrap_or_else(|| key.to_string());
    apply_pseudo_expansion(resolved)
}

/// Resolve a Fluent message key with placeholder `args` (e.g. `Notification`'s
/// `{ $device }`-style variables). Same fallback behaviour as `tr`.
pub fn tr_args(key: &str, args: &[(&'static str, String)]) -> String {
    let lang = langid!("en-US");
    let mut map: HashMap<Cow<'static, str>, FluentValue<'_>> = HashMap::with_capacity(args.len());
    for (name, value) in args {
        map.insert(Cow::Borrowed(*name), FluentValue::from(value.clone()));
    }
    let resolved = LOCALES
        .try_lookup_with_args(&lang, key, &map)
        .unwrap_or_else(|| key.to_string());
    apply_pseudo_expansion(resolved)
}

thread_local! {
    /// 0 = inactive (the common case: a plain read + branch, no
    /// allocation — "zero cost when inactive" per research R12).
    static PSEUDO_EXPANSION_PERCENT: Cell<u32> = const { Cell::new(0) };
}

/// Test-only hook (018-window-sizing-and-responsive-dock, research R12,
/// NFR-7.4): while `f` runs *on this thread*, every string `tr`/`tr_args`
/// resolve is padded by `max(1, ceil(len * percent / 100))` extra
/// characters, simulating a locale whose translations run longer than
/// en-US — the mechanism the responsive-dock/header layout tests use to
/// prove buttons/labels never overlap or lose their content under a
/// longer string. The previous percentage is restored when `f` returns
/// (even on panic), so nested calls compose correctly.
///
/// # Examples
///
/// ```
/// use modplayer_core::i18n::{tr, with_pseudo_expansion};
///
/// let plain = tr("definitely-not-a-real-key");
/// let padded = with_pseudo_expansion(40, || tr("definitely-not-a-real-key"));
/// assert!(padded.len() > plain.len());
/// assert_eq!(tr("definitely-not-a-real-key"), plain, "percentage restored after the call");
/// ```
#[doc(hidden)]
pub fn with_pseudo_expansion<R>(percent: u32, f: impl FnOnce() -> R) -> R {
    let previous = PSEUDO_EXPANSION_PERCENT.with(|cell| cell.replace(percent));
    struct Restore(u32);
    impl Drop for Restore {
        fn drop(&mut self) {
            PSEUDO_EXPANSION_PERCENT.with(|cell| cell.set(self.0));
        }
    }
    let _restore = Restore(previous);
    f()
}

/// Pads `resolved` per `with_pseudo_expansion`'s active percentage on this
/// thread, or returns it unchanged when inactive (percent `0`).
fn apply_pseudo_expansion(resolved: String) -> String {
    let percent = PSEUDO_EXPANSION_PERCENT.with(Cell::get);
    if percent == 0 {
        return resolved;
    }
    let len = resolved.chars().count();
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let extra = (((len * percent as usize) as f64) / 100.0).ceil() as usize;
    let extra = extra.max(1);
    let mut padded = resolved;
    padded.extend(std::iter::repeat_n('~', extra));
    padded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_key_falls_back_to_the_key_itself() {
        assert_eq!(tr("definitely-not-a-real-key"), "definitely-not-a-real-key");
    }
}
