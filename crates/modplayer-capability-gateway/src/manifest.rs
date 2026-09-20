// SPDX-License-Identifier: MIT OR Apache-2.0

//! Plugin package manifest (`plugin.toml`) parsing and validation
//! (FR-001, FR-002; contracts/manifest.md; data-model.md §1.2).

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::api::Permission;
use crate::ui::limits::{MAX_GLYPHS, matches_glyph_key_grammar};

/// A plugin's reverse-domain identity (contract §2): at least two
/// `[a-z0-9-]` labels joined by `.`, at most 128 bytes, immutable once
/// chosen.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PluginIdentifier(String);

impl PluginIdentifier {
    /// `None` if `s` fails the grammar (contract §3 rule 3).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        if s.len() > 128 || s.is_empty() {
            return None;
        }
        let labels: Vec<&str> = s.split('.').collect();
        if labels.len() < 2 {
            return None;
        }
        let label_ok = |label: &str| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        };
        if labels.iter().all(|l| label_ok(l)) {
            Some(Self(s.to_string()))
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PluginIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// `major.minor.patch`, each a `u32` (contract §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Parse a strict `major.minor.patch` triple (contract §3 rule 3).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The API range a plugin supports: `"1.0"` means `>= 1.0, < 2.0`
/// (contract §2, research R7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiRange {
    pub major: u16,
    pub min_minor: u16,
}

impl ApiRange {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split('.');
        let major = parts.next()?.parse().ok()?;
        let min_minor = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self { major, min_minor })
    }

    /// Whether `api_version` (the host's `API_VERSION`) satisfies this
    /// range: same major, minor at least `min_minor`.
    #[must_use]
    pub fn supports(&self, major: u16, minor: u16) -> bool {
        self.major == major && minor >= self.min_minor
    }
}

/// Where a suggested effect-node insertion point resolves against the
/// current chain (contract §2, data-model.md §1.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestedPosition {
    Index(usize),
    Before(String),
    After(String),
}

/// One `[[permissions.required]]`/`[[permissions.optional]]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRequest {
    pub permission: Permission,
    pub justification: String,
}

/// One `[[ui]]` entry — recorded only, never rendered this slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiContribution {
    pub kind: String,
    pub id: String,
    pub title: String,
}

/// One `[[effect_nodes]]` entry — recorded intent, applied when the
/// plugin actually calls `create_node`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntendedNode {
    pub kind: String,
    pub suggested_position: Option<SuggestedPosition>,
}

/// A fully validated plugin manifest (contract §2, data-model.md §1.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub identifier: PluginIdentifier,
    pub name: String,
    pub description: String,
    pub version: Version,
    pub api: ApiRange,
    pub author: String,
    pub license: String,
    pub homepage: Option<String>,
    pub source: String,
    pub entry: String,
    pub required: Vec<PermissionRequest>,
    pub optional: Vec<PermissionRequest>,
    pub network_hosts: Vec<String>,
    pub ui: Vec<UiContribution>,
    pub effect_nodes: Vec<IntendedNode>,
    pub min_host_version: Option<Version>,
    /// 011-plugin-ui-contributions (FR-014a): package-relative PNG path,
    /// optional.
    pub icon: Option<String>,
    /// FR-014a: `[glyphs]` table, key ([`GLYPH_KEY_GRAMMAR`]-matching) ->
    /// package-relative PNG path; at most [`MAX_GLYPHS`] entries.
    ///
    /// [`GLYPH_KEY_GRAMMAR`]: crate::ui::limits::GLYPH_KEY_GRAMMAR
    pub glyphs: BTreeMap<String, String>,
    /// FR-021: the locale this plugin's own `[strings.*]` fall back to;
    /// defaults to `"en-US"`.
    pub default_locale: String,
    /// FR-021: `[strings.<locale>]` tables, resolved by `plugins::ui::
    /// strings::resolve` for any `"@key"`-prefixed user-facing string.
    pub strings: BTreeMap<String, BTreeMap<String, String>>,
}

/// Every way [`validate`] can reject a manifest (contract §3). Each
/// variant renders to exactly one Fluent sentence (`manifest-error-*`,
/// `locales/en-US/plugins.ftl`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("The manifest could not be read: {0}.")]
    UnreadableManifest(String),
    #[error("The manifest is missing the required field '{0}'.")]
    MissingField(&'static str),
    #[error("The field '{field}' is not valid: {detail}.")]
    MalformedField { field: &'static str, detail: String },
    #[error("The {list} permission '{permission}' is not in the permission catalog.")]
    UnknownPermission {
        permission: String,
        list: PermissionList,
    },
    #[error("The 'network' permission may only be requested as optional.")]
    NetworkMustBeOptional,
    #[error("The 'network' permission requires at least one declared network host.")]
    NetworkWithoutHosts,
    #[error("The permission '{}' is listed more than once.", .0.name())]
    DuplicatePermission(Permission),
    #[error("The entry script '{0}' is not in the package.")]
    EntryMissing(String),
}

/// Which permission list an [`ManifestError::UnknownPermission`] came
/// from (contract §3's `{ $list }` placeholder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionList {
    Required,
    Optional,
}

impl std::fmt::Display for PermissionList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PermissionList::Required => "required",
            PermissionList::Optional => "optional",
        })
    }
}

// -- Raw TOML shape (`ManifestDto`) -----------------------------------------

#[derive(Debug, Deserialize, Default)]
struct PermissionsDto {
    #[serde(default)]
    required: Vec<PermissionRequestDto>,
    #[serde(default)]
    optional: Vec<PermissionRequestDto>,
}

#[derive(Debug, Deserialize)]
struct PermissionRequestDto {
    permission: String,
    #[serde(default)]
    justification: String,
}

#[derive(Debug, Deserialize)]
struct UiDto {
    kind: String,
    id: String,
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SuggestedPositionDto {
    Index(usize),
    Before(String),
    After(String),
}

#[derive(Debug, Deserialize)]
struct EffectNodeDto {
    kind: String,
    #[serde(default)]
    suggested_position: Option<SuggestedPositionDto>,
}

fn default_entry() -> String {
    "main.luau".to_string()
}

fn default_locale_default() -> String {
    "en-US".to_string()
}

/// The as-parsed-from-TOML manifest, before validation (data-model.md
/// §1.2). Every field is optional/loosely typed here; [`validate`]
/// applies contract §3's ordered rules to turn it into a [`Manifest`] or
/// a [`ManifestError`].
#[derive(Debug, Deserialize)]
pub struct ManifestDto {
    identifier: Option<String>,
    name: Option<String>,
    #[serde(default)]
    description: String,
    version: Option<String>,
    api: Option<String>,
    author: Option<String>,
    license: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    source: Option<String>,
    #[serde(default = "default_entry")]
    entry: String,
    #[serde(default)]
    min_host_version: Option<String>,
    #[serde(default)]
    permissions: PermissionsDto,
    #[serde(default)]
    network_hosts: Vec<String>,
    #[serde(default)]
    ui: Vec<UiDto>,
    #[serde(default)]
    effect_nodes: Vec<EffectNodeDto>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    glyphs: BTreeMap<String, String>,
    #[serde(default = "default_locale_default")]
    default_locale: String,
    #[serde(default)]
    strings: BTreeMap<String, BTreeMap<String, String>>,
}

/// Rule 1 (contract §3): parse the raw TOML text into a [`ManifestDto`].
///
/// ```
/// use modplayer_capability_gateway::manifest;
///
/// let toml = r#"
/// identifier = "org.modplayer.example.demo"
/// name = "Demo"
/// version = "1.0.0"
/// api = "1.0"
/// author = "Example"
/// license = "MIT"
/// source = "bundled"
/// "#;
/// let dto = manifest::parse(toml).expect("valid TOML");
/// let plugin = manifest::validate(dto, true).expect("entry file present");
/// assert_eq!(plugin.identifier.as_str(), "org.modplayer.example.demo");
/// ```
pub fn parse(toml_text: &str) -> Result<ManifestDto, ManifestError> {
    toml::from_str(toml_text).map_err(|e| ManifestError::UnreadableManifest(e.message().into()))
}

fn require_field<'a>(
    value: &'a Option<String>,
    name: &'static str,
) -> Result<&'a str, ManifestError> {
    match value {
        Some(v) if !v.is_empty() => Ok(v.as_str()),
        _ => Err(ManifestError::MissingField(name)),
    }
}

fn convert_permission_request(
    dto: PermissionRequestDto,
    list: PermissionList,
) -> Result<PermissionRequest, ManifestError> {
    let permission =
        Permission::parse(&dto.permission).ok_or_else(|| ManifestError::UnknownPermission {
            permission: dto.permission.clone(),
            list,
        })?;
    Ok(PermissionRequest {
        permission,
        justification: dto.justification,
    })
}

fn convert_suggested(dto: SuggestedPositionDto) -> SuggestedPosition {
    match dto {
        SuggestedPositionDto::Index(i) => SuggestedPosition::Index(i),
        SuggestedPositionDto::Before(k) => SuggestedPosition::Before(k),
        SuggestedPositionDto::After(k) => SuggestedPosition::After(k),
    }
}

/// Rules 2-8 (contract §3), applied in order — the first failure is the
/// reason returned. `entry_exists` is the caller's answer to rule 7 (the
/// gateway crate itself does no filesystem/embedded-resource lookups).
pub fn validate(dto: ManifestDto, entry_exists: bool) -> Result<Manifest, ManifestError> {
    // Rule 2: required fields present.
    let identifier_str = require_field(&dto.identifier, "identifier")?;
    let name = require_field(&dto.name, "name")?.to_string();
    let version_str = require_field(&dto.version, "version")?;
    let api_str = require_field(&dto.api, "api")?;
    let author = require_field(&dto.author, "author")?.to_string();
    let license = require_field(&dto.license, "license")?.to_string();
    let source = require_field(&dto.source, "source")?.to_string();
    if dto.entry.is_empty() {
        return Err(ManifestError::MissingField("entry"));
    }

    // Rule 3: each field well-formed.
    let identifier =
        PluginIdentifier::parse(identifier_str).ok_or_else(|| ManifestError::MalformedField {
            field: "identifier",
            detail: format!("'{identifier_str}' is not a valid reverse-domain identifier"),
        })?;
    let version = Version::parse(version_str).ok_or_else(|| ManifestError::MalformedField {
        field: "version",
        detail: format!("'{version_str}' is not a valid major.minor.patch triple"),
    })?;
    let api = ApiRange::parse(api_str).ok_or_else(|| ManifestError::MalformedField {
        field: "api",
        detail: format!("'{api_str}' is not a valid major.minor range"),
    })?;
    let min_host_version = match &dto.min_host_version {
        Some(s) => Some(
            Version::parse(s).ok_or_else(|| ManifestError::MalformedField {
                field: "min_host_version",
                detail: format!("'{s}' is not a valid major.minor.patch triple"),
            })?,
        ),
        None => None,
    };

    // Rule 4: every permission string is in the catalog.
    let mut required = Vec::with_capacity(dto.permissions.required.len());
    for p in dto.permissions.required {
        required.push(convert_permission_request(p, PermissionList::Required)?);
    }
    let mut optional = Vec::with_capacity(dto.permissions.optional.len());
    for p in dto.permissions.optional {
        optional.push(convert_permission_request(p, PermissionList::Optional)?);
    }

    // Rule 5: `network` never required; `network` optional needs hosts.
    if required.iter().any(|p| p.permission == Permission::Network) {
        return Err(ManifestError::NetworkMustBeOptional);
    }
    if optional.iter().any(|p| p.permission == Permission::Network) && dto.network_hosts.is_empty()
    {
        return Err(ManifestError::NetworkWithoutHosts);
    }

    // Rule 6: no permission listed twice across both lists.
    let mut seen = std::collections::HashSet::new();
    for p in required.iter().chain(optional.iter()) {
        if !seen.insert(p.permission) {
            return Err(ManifestError::DuplicatePermission(p.permission));
        }
    }

    // Rule 7: the entry file exists in the package.
    if !entry_exists {
        return Err(ManifestError::EntryMissing(dto.entry.clone()));
    }

    // Rule 3 (011-plugin-ui-contributions, FR-014a): every `[glyphs]` key
    // matches `GLYPH_KEY_GRAMMAR` and there are at most `MAX_GLYPHS`
    // entries — the *map* is schema (checked here); asset presence/
    // format/size is never a manifest error (checked at discovery,
    // `plugins::ui::assets::load`).
    if dto.glyphs.len() > MAX_GLYPHS || dto.glyphs.keys().any(|k| !matches_glyph_key_grammar(k)) {
        return Err(ManifestError::MalformedField {
            field: "glyphs",
            detail: format!(
                "glyph keys must match [a-z][a-z0-9_]{{0,31}} and number at most {MAX_GLYPHS}"
            ),
        });
    }

    Ok(Manifest {
        identifier,
        name,
        description: dto.description,
        version,
        api,
        author,
        license,
        homepage: dto.homepage,
        source,
        entry: dto.entry,
        required,
        optional,
        network_hosts: dto.network_hosts,
        ui: dto
            .ui
            .into_iter()
            .map(|u| UiContribution {
                kind: u.kind,
                id: u.id,
                title: u.title,
            })
            .collect(),
        effect_nodes: dto
            .effect_nodes
            .into_iter()
            .map(|n| IntendedNode {
                kind: n.kind,
                suggested_position: n.suggested_position.map(convert_suggested),
            })
            .collect(),
        min_host_version,
        icon: dto.icon,
        glyphs: dto.glyphs,
        default_locale: dto.default_locale,
        strings: dto.strings,
    })
}

/// Rules 1-8 together: parse `toml_text` then validate it.
pub fn parse_and_validate(toml_text: &str, entry_exists: bool) -> Result<Manifest, ManifestError> {
    validate(parse(toml_text)?, entry_exists)
}

// Named tests (`round_trip`, `rejects_unknown_permission`,
// `network_rules`, `duplicate_permission`, `missing_field_names_the_field`,
// `malformed_identifier_cases`) live in `tests/manifest.rs` (Constitution
// VIII, contracts/manifest.md §5) so they exercise the crate's public API
// only.
