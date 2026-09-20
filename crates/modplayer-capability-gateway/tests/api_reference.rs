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
