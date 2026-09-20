// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Regenerates/checks `docs/plugin-api/v1.md` from `api/v1.toml`
//! (Constitution IX, G8, research R6). Run with
//! `MODPLAYER_UPDATE_API_REFERENCE=1 cargo test -p modplayer-capability-gateway
//! --test api_reference` to rewrite the committed file after a schema
//! change.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Schema {
    api_version: ApiVersionDto,
    refusal_codes: Vec<String>,
    permission: Vec<PermissionDto>,
    request: Vec<RequestDto>,
    event: Vec<EventDto>,
    #[serde(default)]
    node_kind: Vec<NodeKindDto>,
}

#[derive(Debug, Deserialize)]
struct ApiVersionDto {
    major: u16,
    minor: u16,
}

#[derive(Debug, Deserialize)]
struct PermissionDto {
    name: String,
    category: String,
    operable: bool,
}

#[derive(Debug, Deserialize)]
struct RequestDto {
    name: String,
    namespace: String,
    method: String,
    #[serde(default)]
    requires: Option<String>,
    needs_focus: bool,
    category: String,
}

#[derive(Debug, Deserialize)]
struct EventDto {
    name: String,
    #[serde(default)]
    requires: Option<String>,
    payload: Vec<String>,
}

/// 013-key-and-tempo-plugin (data-model.md §1.1, research R1): a
/// documentary `[[node_kind]]` table, rendered below into a "Node
/// parameters" section. Not consumed by `build.rs`.
#[derive(Debug, Deserialize)]
struct NodeKindDto {
    name: String,
    params: Vec<NodeKindParamDto>,
}

#[derive(Debug, Deserialize)]
struct NodeKindParamDto {
    name: String,
    shape: String,
    #[serde(default)]
    range: Option<String>,
    #[serde(default)]
    values: Vec<String>,
}

fn generate(schema: &Schema) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "<!-- GENERATED from crates/modplayer-capability-gateway/api/v1.toml by \
tests/api_reference.rs. Do not edit by hand (Constitution IX). Regenerate with \
`MODPLAYER_UPDATE_API_REFERENCE=1 cargo test -p modplayer-capability-gateway --test api_reference`. -->\n"
    )
    .unwrap();
    writeln!(
        out,
        "# ModPlayer Plugin API v{}.{}\n",
        schema.api_version.major, schema.api_version.minor
    )
    .unwrap();

    writeln!(out, "## Refusal codes\n").unwrap();
    for code in &schema.refusal_codes {
        writeln!(out, "- `{code}`").unwrap();
    }
    writeln!(out).unwrap();

    writeln!(out, "## Permission catalog\n").unwrap();
    writeln!(out, "| Permission | Category | Operable |").unwrap();
    writeln!(out, "|---|---|---|").unwrap();
    for p in &schema.permission {
        writeln!(
            out,
            "| `{}` | {} | {} |",
            p.name,
            p.category,
            if p.operable { "yes" } else { "no" }
        )
        .unwrap();
    }
    writeln!(out).unwrap();

    writeln!(out, "## Requests\n").unwrap();
    writeln!(
        out,
        "| RequestKind | Lua call | Requires | Focus | Category |"
    )
    .unwrap();
    writeln!(out, "|---|---|---|---|---|").unwrap();
    for r in &schema.request {
        let call = if r.namespace.is_empty() {
            format!("api.{}(...)", r.method)
        } else {
            format!("api.{}.{}(...)", r.namespace, r.method)
        };
        writeln!(
            out,
            "| `{}` | `{}` | {} | {} | {} |",
            r.name,
            call,
            r.requires.as_deref().unwrap_or("–"),
            if r.needs_focus { "yes" } else { "–" },
            r.category
        )
        .unwrap();
    }
    writeln!(out).unwrap();

    writeln!(out, "## Events\n").unwrap();
    writeln!(out, "| Event | Requires | Payload |").unwrap();
    writeln!(out, "|---|---|---|").unwrap();
    for e in &schema.event {
        writeln!(
            out,
            "| `{}` | {} | {} |",
            to_snake_case(&e.name),
            e.requires.as_deref().unwrap_or("–"),
            e.payload.join(", ")
        )
        .unwrap();
    }
    writeln!(out).unwrap();

    if !schema.node_kind.is_empty() {
        writeln!(out, "## Node parameters\n").unwrap();
        writeln!(out, "| Kind | Parameter | Shape | Range / values |").unwrap();
        writeln!(out, "|---|---|---|---|").unwrap();
        for k in &schema.node_kind {
            for p in &k.params {
                let range_or_values = if !p.values.is_empty() {
                    p.values.join("\\|")
                } else {
                    p.range.clone().unwrap_or_default()
                };
                writeln!(
                    out,
                    "| `{}` | `{}` | {} | {} |",
                    k.name, p.name, p.shape, range_or_values
                )
                .unwrap();
            }
        }
    }
    out
}

fn to_snake_case(pascal: &str) -> String {
    let mut out = String::new();
    for (i, ch) in pascal.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

#[test]
fn reference_is_current() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let schema_path = manifest_dir.join("api/v1.toml");
    let text = fs::read_to_string(&schema_path).expect("read api/v1.toml");
    let schema: Schema = toml::from_str(&text).expect("parse api/v1.toml");
    let generated = generate(&schema);

    let doc_path = repo_root().join("docs/plugin-api/v1.md");

    if std::env::var("MODPLAYER_UPDATE_API_REFERENCE").as_deref() == Ok("1") {
        if let Some(parent) = doc_path.parent() {
            fs::create_dir_all(parent).expect("create docs dir");
        }
        fs::write(&doc_path, &generated).expect("write reference");
        return;
    }

    let committed = fs::read_to_string(&doc_path).unwrap_or_default();
    assert_eq!(
        committed, generated,
        "docs/plugin-api/v1.md is stale — regenerate with \
         MODPLAYER_UPDATE_API_REFERENCE=1 cargo test -p modplayer-capability-gateway \
         --test api_reference"
    );
}

/// 013-key-and-tempo-plugin (research R1, Constitution IX): the schema
/// bump that adds `NodeInfo.params`/`auto_switched`, the `set_param`/
/// `schedule_param` argument widening, and the documentary `[[node_kind]]`
/// table (superseded 012's `api_version_is_1_3`).
#[test]
fn api_version_is_1_4() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let schema_path = manifest_dir.join("api/v1.toml");
    let text = fs::read_to_string(&schema_path).expect("read api/v1.toml");
    let schema: Schema = toml::from_str(&text).expect("parse api/v1.toml");
    assert_eq!(schema.api_version.major, 1);
    assert_eq!(schema.api_version.minor, 4);

    // Reference regenerated and matches (folded in here rather than only
    // in `reference_is_current`, per the named-test list in
    // contracts/plugin-api-v1.4.md §8).
    let generated = generate(&schema);
    let doc_path = repo_root().join("docs/plugin-api/v1.md");
    let committed = fs::read_to_string(&doc_path).unwrap_or_default();
    assert_eq!(
        committed, generated,
        "docs/plugin-api/v1.md is stale — regenerate with \
         MODPLAYER_UPDATE_API_REFERENCE=1 cargo test -p modplayer-capability-gateway \
         --test api_reference"
    );
}

/// 013-key-and-tempo-plugin (research R1): the "Node parameters" section
/// is rendered from `[[node_kind]]` and lists every wire name the catalog
/// exposes.
#[test]
fn reference_lists_node_kind_params() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let schema_path = manifest_dir.join("api/v1.toml");
    let text = fs::read_to_string(&schema_path).expect("read api/v1.toml");
    let schema: Schema = toml::from_str(&text).expect("parse api/v1.toml");
    assert_eq!(schema.node_kind.len(), 6, "six built-in node kinds");
    let generated = generate(&schema);
    assert!(generated.contains("## Node parameters"));
    assert!(generated.contains("`pitch_shift`"));
    assert!(generated.contains("`semitones`"));
    assert!(generated.contains("`quality_mode`"));
    assert!(generated.contains("performance\\|quality"));
}
