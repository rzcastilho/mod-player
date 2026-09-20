// SPDX-License-Identifier: MIT OR Apache-2.0

//! Plugin string resolution (FR-021, research R17): a literal renders
//! as-is; an `"@key"`-prefixed string resolves through the manifest's
//! own `[strings.<locale>]` tables — the host's active locale first
//! (en-US only this slice, no locale switch exists yet), then the
//! plugin's own `default_locale` — never through the host's Fluent
//! bundle (a plugin string is never a Fluent pattern, path, URL or
//! markup, Constitution VI). An unresolved key renders literally, with
//! a warning for the caller to log (`plugins/ui/panel.rs`'s registration
//! path, a later story's task, is where that warning actually reaches
//! `PluginLog`).

use modplayer_capability_gateway::manifest::Manifest;

/// A resolution warning's text — always logged by the caller at `Warn`,
/// never surfaced to the plugin itself.
pub type Warning = String;

/// Resolve one user-facing string against `manifest`'s `[strings.*]`
/// tables. `s` unprefixed by `@` is returned as-is (the common case: a
/// widget label given as a plain literal needs no lookup at all).
/// `locale` is the host's current active locale (`"en-US"` — no locale
/// switch exists yet, R17).
#[must_use]
pub fn resolve(manifest: &Manifest, locale: &str, s: &str) -> (String, Option<Warning>) {
    let Some(key) = s.strip_prefix('@') else {
        return (s.to_string(), None);
    };
    if let Some(text) = lookup(manifest, locale, key) {
        return (text, None);
    }
    if let Some(text) = lookup(manifest, &manifest.default_locale, key) {
        return (text, None);
    }
    (
        s.to_string(),
        Some(format!(
            "string key '@{key}' is not defined in [strings.{locale}] or [strings.{}]",
            manifest.default_locale
        )),
    )
}

fn lookup(manifest: &Manifest, locale: &str, key: &str) -> Option<String> {
    manifest.strings.get(locale)?.get(key).cloned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn test_manifest(strings: BTreeMap<String, BTreeMap<String, String>>) -> Manifest {
        let toml = "identifier = \"org.modplayer.test.strings\"\n\
             name = \"Strings test\"\n\
             version = \"1.0.0\"\n\
             api = \"1.0\"\n\
             author = \"Test\"\n\
             license = \"MIT\"\n\
             source = \"test\"\n";
        let mut manifest = modplayer_capability_gateway::manifest::parse_and_validate(toml, true)
            .unwrap_or_else(|e| unreachable!("test manifest must be valid: {e}"));
        manifest.strings = strings;
        manifest
    }

    #[test]
    fn a_literal_passes_through_unresolved() {
        let manifest = test_manifest(BTreeMap::new());
        let (text, warning) = resolve(&manifest, "en-US", "Tempo");
        assert_eq!(text, "Tempo");
        assert!(warning.is_none());
    }

    #[test]
    fn an_at_key_resolves_through_the_active_locale() {
        let mut strings = BTreeMap::new();
        strings.insert(
            "en-US".to_string(),
            BTreeMap::from([("tempo_label".to_string(), "Tempo".to_string())]),
        );
        let manifest = test_manifest(strings);
        let (text, warning) = resolve(&manifest, "en-US", "@tempo_label");
        assert_eq!(text, "Tempo");
        assert!(warning.is_none());
    }

    #[test]
    fn falls_back_to_the_plugin_default_locale() {
        let mut strings = BTreeMap::new();
        strings.insert(
            "fr-FR".to_string(),
            BTreeMap::from([("tempo_label".to_string(), "Tempo (par défaut)".to_string())]),
        );
        let mut manifest = test_manifest(strings);
        manifest.default_locale = "fr-FR".to_string();
        // The host's active locale (en-US) has no translation; the
        // plugin's own default_locale (fr-FR) does.
        let (text, warning) = resolve(&manifest, "en-US", "@tempo_label");
        assert_eq!(text, "Tempo (par défaut)");
        assert!(warning.is_none());
    }

    #[test]
    fn an_unresolved_key_renders_literally_with_a_warning() {
        let manifest = test_manifest(BTreeMap::new());
        let (text, warning) = resolve(&manifest, "en-US", "@missing");
        assert_eq!(text, "@missing");
        assert!(warning.is_some());
    }
}
