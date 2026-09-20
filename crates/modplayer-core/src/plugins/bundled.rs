// SPDX-License-Identifier: MIT OR Apache-2.0

//! Embedded plugin packages (research R8, FR-001, FR-003): `plugins/
//! bundled/` for the bundled source (still empty — 012/013 add their own
//! packages later), `plugins/fixtures/` for the story fixtures each user
//! story's own tasks add (research R18). Each package is one
//! `BundledPackage` literal here, backed by an `include_str!` trio
//! (`plugin.toml`, its entry script, `README.md`), per plan.md § Project
//! Structure.

/// One package's embedded bytes (contracts/manifest.md §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundledPackage {
    pub identifier: &'static str,
    pub manifest_toml: &'static str,
    pub entry: &'static str,
    pub readme: &'static str,
    /// `true` for a `plugins/fixtures/` package (only ever loaded when
    /// `MODPLAYER_PLUGIN_FIXTURES=1`); `false` for a real `plugins/
    /// bundled/` package.
    pub fixture: bool,
    /// 011-plugin-ui-contributions (US3 T091, research R11): this
    /// package's non-Lua/non-TOML files — a manifest-declared `icon`/
    /// `[glyphs]` path resolves against this table (package-relative,
    /// `("icon.png", include_bytes!("../../../../plugins/fixtures/
    /// <name>/icon.png"))`, `plugins/ui/assets.rs::resource_bytes`'s own
    /// linear lookup). Empty for every package that declares no
    /// `icon`/`glyphs` at all.
    pub resources: &'static [(&'static str, &'static [u8])],
}

/// The environment variable that gates [`fixtures`] (research R8,
/// contracts/plugin-host-service.md L1) — read once at discovery.
pub const FIXTURES_ENV: &str = "MODPLAYER_PLUGIN_FIXTURES";

/// Whether `MODPLAYER_PLUGIN_FIXTURES=1` is set in the current process
/// environment.
#[must_use]
pub fn fixtures_enabled() -> bool {
    std::env::var(FIXTURES_ENV).as_deref() == Ok("1")
}

/// Every bundled package (FR-001) — always loaded, regardless of
/// fixtures. 012-section-loop-plugin (data-model.md §2.6, research R5)
/// added the first; 013-key-and-tempo-plugin (data-model.md §2.7,
/// research R8) adds the second, in declaration order.
#[must_use]
pub fn packages() -> Vec<BundledPackage> {
    vec![section_loop(), key_tempo()]
}

/// 012-section-loop-plugin: Section Loop, the reference bundled plugin —
/// mark A/B and drill a passage hands-free (data-model.md §2.6/§3).
fn section_loop() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.section-loop",
        manifest_toml: include_str!(
            "../../../../plugins/bundled/org.modplayer.section-loop/plugin.toml"
        ),
        entry: include_str!("../../../../plugins/bundled/org.modplayer.section-loop/main.luau"),
        readme: include_str!("../../../../plugins/bundled/org.modplayer.section-loop/README.md"),
        fixture: false,
        resources: &[],
    }
}

/// 013-key-and-tempo-plugin: Key & Tempo — independent semitone
/// transpose and tempo/time-stretch control, with optional per-track
/// memory of the chosen key and tempo (data-model.md §2.7/§3).
fn key_tempo() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.key-tempo",
        manifest_toml: include_str!(
            "../../../../plugins/bundled/org.modplayer.key-tempo/plugin.toml"
        ),
        entry: include_str!("../../../../plugins/bundled/org.modplayer.key-tempo/main.luau"),
        readme: include_str!("../../../../plugins/bundled/org.modplayer.key-tempo/README.md"),
        fixture: false,
        resources: &[],
    }
}

/// Every fixture package (research R8/R18) — loaded only when
/// [`fixtures_enabled`]. US1's four fault-isolation fixtures land first;
/// US2 adds the three least-privilege fixtures; US3's own task adds the
/// well-behaved one; 010-transport-focus adds the last two (contention).
#[must_use]
pub fn fixtures() -> Vec<BundledPackage> {
    vec![
        fixture_hang(),
        fixture_throw(),
        fixture_leak(),
        fixture_noready(),
        fixture_observer(),
        fixture_invalid(),
        fixture_flood(),
        fixture_wellbehaved(),
        fixture_focus_a(),
        fixture_focus_b(),
        fixture_ui_panel(),
        fixture_ui_shortcuts(),
        fixture_ui_overlay(),
        fixture_ui_icons(),
        fixture_ui_settings(),
        fixture_ui_notify(),
        fixture_effects_observer(),
    ]
}

fn fixture_hang() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.hang",
        manifest_toml: include_str!("../../../../plugins/fixtures/hang/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/hang/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/hang/README.md"),
        fixture: true,
        resources: &[],
    }
}

fn fixture_throw() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.throw",
        manifest_toml: include_str!("../../../../plugins/fixtures/throw/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/throw/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/throw/README.md"),
        fixture: true,
        resources: &[],
    }
}

fn fixture_leak() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.leak",
        manifest_toml: include_str!("../../../../plugins/fixtures/leak/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/leak/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/leak/README.md"),
        fixture: true,
        resources: &[],
    }
}

fn fixture_noready() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.noready",
        manifest_toml: include_str!("../../../../plugins/fixtures/noready/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/noready/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/noready/README.md"),
        fixture: true,
        resources: &[],
    }
}

fn fixture_observer() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.observer",
        manifest_toml: include_str!("../../../../plugins/fixtures/observer/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/observer/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/observer/README.md"),
        fixture: true,
        resources: &[],
    }
}

/// US2 T082: the one fixture whose manifest is deliberately invalid
/// (`teleport.everywhere` is outside the permission catalog) — excluded
/// from [`tests::us1_fixtures_have_valid_manifests`] on purpose.
fn fixture_invalid() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.invalid",
        manifest_toml: include_str!("../../../../plugins/fixtures/invalid/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/invalid/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/invalid/README.md"),
        fixture: true,
        resources: &[],
    }
}

fn fixture_flood() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.flood",
        manifest_toml: include_str!("../../../../plugins/fixtures/flood/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/flood/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/flood/README.md"),
        fixture: true,
        resources: &[],
    }
}

/// US3 T091: the full-behavior-cycle fixture (all 9 operable
/// permissions).
fn fixture_wellbehaved() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.wellbehaved",
        manifest_toml: include_str!("../../../../plugins/fixtures/wellbehaved/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/wellbehaved/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/wellbehaved/README.md"),
        fixture: true,
        resources: &[],
    }
}

/// 010-transport-focus (data-model.md §5): requests focus, seeks and
/// arms a transient loop region on every grant, hangs on its third
/// `play_state_changed` (the manual/test-driven suspension trigger).
fn fixture_focus_a() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.focus-a",
        manifest_toml: include_str!("../../../../plugins/fixtures/focus-a/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/focus-a/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/focus-a/README.md"),
        fixture: true,
        resources: &[],
    }
}

/// 010-transport-focus (data-model.md §5): requests focus and seeks on
/// every grant; while not the holder, attempts a seek on every
/// `play_state_changed` and logs the resulting `no_focus` — the
/// single-holder contention proof paired with [`fixture_focus_a`].
fn fixture_focus_b() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.focus-b",
        manifest_toml: include_str!("../../../../plugins/fixtures/focus-b/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/focus-b/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/focus-b/README.md"),
        fixture: true,
        resources: &[],
    }
}

/// 011-plugin-ui-contributions (US1 T059/T060): registers a panel with
/// every widget kind; drives the request-focus-in-handler/timer,
/// hang and throw probes.
fn fixture_ui_panel() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.ui-panel",
        manifest_toml: include_str!("../../../../plugins/fixtures/ui-panel/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/ui-panel/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/ui-panel/README.md"),
        fixture: true,
        resources: &[],
    }
}

/// 011-plugin-ui-contributions (US2 T077/T079, contracts/
/// action-registry-plugins.md): registers `take_over` (`L`, collides with
/// the host's loop toggle, M5), `nudge` (continuous, `Shift+K`, collides
/// with `ui-panel`'s own `focus_me`, M6) and `tab_bound` (`Tab`, a
/// rejected default, G10).
fn fixture_ui_shortcuts() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.ui-shortcuts",
        manifest_toml: include_str!("../../../../plugins/fixtures/ui-shortcuts/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/ui-shortcuts/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/ui-shortcuts/README.md"),
        fixture: true,
        resources: &[],
    }
}

/// 011-plugin-ui-contributions (US3 T090, contracts/overlays-settings-
/// notify.md §1.1): one primitive of every kind per `track_changed`;
/// probes `add_501st`/`bad_region`/`bad_icon`.
fn fixture_ui_overlay() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.ui-overlay",
        manifest_toml: include_str!("../../../../plugins/fixtures/ui-overlay/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/ui-overlay/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/ui-overlay/README.md"),
        fixture: true,
        resources: &[],
    }
}

/// 011-plugin-ui-contributions (US3 T091, FR-014a): the first package to
/// actually embed `resources` — a manifest `icon` plus two `[glyphs]`
/// keys, one deliberately over the per-glyph size cap (`glyphs/big.png`,
/// 64x64 against `GLYPH_MAX_PX = 32`), proving the "warn and omit, never
/// a manifest error" asset path end to end.
fn fixture_ui_icons() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.ui-icons",
        manifest_toml: include_str!("../../../../plugins/fixtures/ui-icons/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/ui-icons/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/ui-icons/README.md"),
        fixture: true,
        resources: &[
            (
                "icon.png",
                include_bytes!("../../../../plugins/fixtures/ui-icons/icon.png"),
            ),
            (
                "glyphs/ok.png",
                include_bytes!("../../../../plugins/fixtures/ui-icons/glyphs/ok.png"),
            ),
            (
                "glyphs/big.png",
                include_bytes!("../../../../plugins/fixtures/ui-icons/glyphs/big.png"),
            ),
        ],
    }
}

/// 011-plugin-ui-contributions (US4 T105/T106, contracts/overlays-
/// settings-notify.md §2): registers a 4-field settings schema (boolean,
/// number, string, choice); logs `settings_changed`; probe `get`.
fn fixture_ui_settings() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.ui-settings",
        manifest_toml: include_str!("../../../../plugins/fixtures/ui-settings/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/ui-settings/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/ui-settings/README.md"),
        fixture: true,
        resources: &[],
    }
}

/// 011-plugin-ui-contributions (US5 T113/T115, contracts/overlays-
/// settings-notify.md §3): posts nothing on its own; `debug_probe`'s
/// `post:<level>:<n>`/`post_invalid:<n>` drive `api.ui.notify` directly
/// so a test controls exactly how many calls land inside the rolling
/// 6-per-60s window.
fn fixture_ui_notify() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.ui-notify",
        manifest_toml: include_str!("../../../../plugins/fixtures/ui-notify/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/ui-notify/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/ui-notify/README.md"),
        fixture: true,
        resources: &[],
    }
}

/// 013-key-and-tempo-plugin (Phase 2 Foundational, research R1/R3):
/// `audio.effects` only; logs a summary of every `effect_chain_changed`
/// it receives, so `controller_effects.rs` can prove the API 1.4 fan-out
/// mechanism (and the R4 delivery fix) against a real subscribed plugin
/// thread ahead of the bundled Key & Tempo package existing.
fn fixture_effects_observer() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.effects-observer",
        manifest_toml: include_str!("../../../../plugins/fixtures/effects-observer/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/effects-observer/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/effects-observer/README.md"),
        fixture: true,
        resources: &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modplayer_capability_gateway::manifest;

    /// Every fixture except `org.modplayer.fixture.invalid` (US2 T082,
    /// deliberately outside the permission catalog — see
    /// [`invalid_fixture_manifest_is_rejected`]) carries a well-formed,
    /// catalog-valid manifest — a typo here would otherwise only surface
    /// as a silently `Invalid` row at runtime.
    #[test]
    fn us1_fixtures_have_valid_manifests() {
        for package in fixtures() {
            if package.identifier == "org.modplayer.fixture.invalid" {
                continue;
            }
            let manifest = manifest::parse_and_validate(package.manifest_toml, true);
            assert!(
                manifest.is_ok(),
                "{}: {:?}",
                package.identifier,
                manifest.err()
            );
        }
    }

    /// 012-section-loop-plugin (contract P2): the bundled Section Loop
    /// package's manifest is well-formed and catalog-valid, `api = "1.3"`,
    /// and carries exactly the 7 required + 1 optional permissions
    /// data-model.md §3.1 lists.
    #[test]
    fn section_loop_manifest_is_valid() {
        let package = section_loop();
        assert_eq!(package.identifier, "org.modplayer.section-loop");
        assert!(!package.fixture);
        let manifest = manifest::parse_and_validate(package.manifest_toml, true)
            .unwrap_or_else(|e| unreachable!("section-loop manifest must validate: {e}"));
        assert_eq!(manifest.identifier.as_str(), "org.modplayer.section-loop");
        assert_eq!(manifest.api.major, 1);
        assert_eq!(manifest.api.min_minor, 3);
        assert_eq!(manifest.required.len(), 7);
        assert_eq!(manifest.optional.len(), 1);
    }

    /// US2 T082 (contracts/manifest.md rule 4, §5 `fixture_packages_
    /// parse`): the one fixture named `invalid` fails validation with
    /// exactly `UnknownPermission { permission: "teleport.everywhere" }`
    /// — never a different rule, and never `Ok`.
    #[test]
    fn invalid_fixture_manifest_is_rejected() {
        let package = fixtures()
            .into_iter()
            .find(|p| p.identifier == "org.modplayer.fixture.invalid")
            .unwrap_or_else(|| unreachable!("the invalid fixture must be registered"));
        let err = match manifest::parse_and_validate(package.manifest_toml, true) {
            Ok(_) => unreachable!("the invalid fixture's manifest must fail validation"),
            Err(e) => e,
        };
        assert!(
            matches!(
                &err,
                manifest::ManifestError::UnknownPermission { permission, .. }
                    if permission == "teleport.everywhere"
            ),
            "expected UnknownPermission(\"teleport.everywhere\"), got {err:?}"
        );
    }
}
