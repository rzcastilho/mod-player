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
/// fixtures. Empty this slice.
#[must_use]
pub fn packages() -> Vec<BundledPackage> {
    Vec::new()
}

/// Every fixture package (research R8/R18) — loaded only when
/// [`fixtures_enabled`]. US1's four fault-isolation fixtures land first;
/// US2 adds the three least-privilege fixtures; US3's own task adds the
/// last (well-behaved) one.
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
    ]
}

fn fixture_hang() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.hang",
        manifest_toml: include_str!("../../../../plugins/fixtures/hang/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/hang/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/hang/README.md"),
        fixture: true,
    }
}

fn fixture_throw() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.throw",
        manifest_toml: include_str!("../../../../plugins/fixtures/throw/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/throw/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/throw/README.md"),
        fixture: true,
    }
}

fn fixture_leak() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.leak",
        manifest_toml: include_str!("../../../../plugins/fixtures/leak/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/leak/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/leak/README.md"),
        fixture: true,
    }
}

fn fixture_noready() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.noready",
        manifest_toml: include_str!("../../../../plugins/fixtures/noready/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/noready/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/noready/README.md"),
        fixture: true,
    }
}

fn fixture_observer() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.observer",
        manifest_toml: include_str!("../../../../plugins/fixtures/observer/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/observer/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/observer/README.md"),
        fixture: true,
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
    }
}

fn fixture_flood() -> BundledPackage {
    BundledPackage {
        identifier: "org.modplayer.fixture.flood",
        manifest_toml: include_str!("../../../../plugins/fixtures/flood/plugin.toml"),
        entry: include_str!("../../../../plugins/fixtures/flood/main.luau"),
        readme: include_str!("../../../../plugins/fixtures/flood/README.md"),
        fixture: true,
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
