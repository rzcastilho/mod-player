// SPDX-License-Identifier: MIT OR Apache-2.0

//! `api.ui.*` (contracts/plugin-api-v1.2.md §3): every call here builds a
//! [`Request`] from its Lua table arguments and sends it through
//! [`call`] exactly like every other namespace — `get_settings` is no
//! exception (research R4): `bindings::dispatch`'s own `RequestKind::
//! GetSettings` arm is what serves it locally instead of routing to
//! `rpc(..)`, so this module never special-cases it.
//!
//! Every closed vocabulary (`kind`, `color`, `icon`, field `kind`, notify
//! `level`) is a plain Lua string here — parsed into the gateway crate's
//! typed enum before the [`Request`] is built. An unparseable string
//! (not one of the closed set) is refused locally with `invalid_state`/
//! `invalid_argument`, *before* the call ever reaches the gateway's own
//! admission — the one exception is `notify`'s `level` (contract §3.6:
//! a bad level still consumes the notify window's slot), which US5's own
//! task (T111) tightens once `core::plugins::apply` actually validates
//! it; this Foundational phase already routes every `ui.*` shape through
//! admission correctly for every request whose fields all parse.

use std::collections::BTreeMap;

use mlua::{Lua, Table, Value};

use modplayer_capability_gateway::api::RequestKind;
use modplayer_capability_gateway::refusal::Refusal;
use modplayer_capability_gateway::request::{Request, Response};
use modplayer_capability_gateway::state::Scope;
use modplayer_capability_gateway::ui::{
    ActionKindSpec, ActionSpec, FieldKind, GlyphRef, HostGlyph, ListItem, NotifyLevel,
    OverlayColor, OverlayPrimitive, SettingsField, UiId, WidgetKind, WidgetSpec, WidgetValue,
    validate_stored,
};

use super::{SharedHandle, call, lock, lua_err};

/// `dispatch`'s local handler for `RequestKind::GetSettings` (research
/// R4): reads the schema this plugin's last successful `register_settings`
/// RPC left in `Shared.settings_schema` (`None` — nothing registered yet
/// — returns `{}`) and, for each field, the stored `Scope::Settings`
/// value if still valid for that field's kind/range, else the field's own
/// `default` (console warning, storage untouched).
pub fn get_settings_local(shared: &SharedHandle) -> Result<Response, Refusal> {
    let guard = lock(shared);
    let Some(schema) = &guard.settings_schema else {
        return Ok(Response::Settings(BTreeMap::new()));
    };
    let mut values = BTreeMap::new();
    for field in schema {
        let stored = guard.store.get(Scope::Settings, field.id.as_str());
        let value = match stored {
            Some(v) if validate_stored(field, v) => v.clone(),
            Some(_) => {
                log::warn!(
                    target: "plugin",
                    "[{}] stored settings value for '{}' is invalid; using its default",
                    guard.identifier,
                    field.id
                );
                field.default.clone()
            }
            None => field.default.clone(),
        };
        values.insert(field.id.as_str().to_string(), value);
    }
    Ok(Response::Settings(values))
}

fn bad_argument(detail: impl Into<String>) -> Refusal {
    Refusal::invalid_state("invalid_argument", detail)
}

fn parse_id(s: &str) -> Result<UiId, Refusal> {
    UiId::parse(s).ok_or_else(|| bad_argument(format!("'{s}' is not a valid id.")))
}

fn parse_list_items(table: &Option<Table>) -> Result<Vec<ListItem>, Refusal> {
    let Some(table) = table else {
        return Ok(Vec::new());
    };
    let mut items = Vec::new();
    for pair in table.clone().sequence_values::<Table>() {
        let row = pair.map_err(|e| bad_argument(e.to_string()))?;
        let id: String = row
            .get("id")
            .map_err(|_| bad_argument("item.id is required."))?;
        let label: String = row.get("label").unwrap_or_default();
        items.push(ListItem {
            id: parse_id(&id)?,
            label,
        });
    }
    Ok(items)
}

fn parse_widget_spec(row: &Table) -> Result<WidgetSpec, Refusal> {
    let id: String = row
        .get("id")
        .map_err(|_| bad_argument("widget.id is required."))?;
    let kind_str: String = row
        .get("kind")
        .map_err(|_| bad_argument("widget.kind is required."))?;
    let kind = WidgetKind::parse(&kind_str)
        .ok_or_else(|| bad_argument(format!("'{kind_str}' is not a widget kind.")))?;
    let label: String = row.get("label").unwrap_or_default();
    let items_table: Option<Table> = row.get("items").unwrap_or(None);
    let selected: Option<String> = row.get("selected").unwrap_or(None);
    Ok(WidgetSpec {
        id: parse_id(&id)?,
        kind,
        label,
        min: row.get("min").unwrap_or(None),
        max: row.get("max").unwrap_or(None),
        step: row.get("step").unwrap_or(None),
        value: row.get("value").unwrap_or(None),
        items: parse_list_items(&items_table)?,
        selected: selected.map(|s| parse_id(&s)).transpose()?,
        action: row.get("action").unwrap_or(None),
        text: row.get("text").unwrap_or(None),
    })
}

fn parse_widgets(table: Table) -> Result<Vec<WidgetSpec>, Refusal> {
    let mut widgets = Vec::new();
    for pair in table.sequence_values::<Table>() {
        let row = pair.map_err(|e| bad_argument(e.to_string()))?;
        widgets.push(parse_widget_spec(&row)?);
    }
    Ok(widgets)
}

/// `update_widget`'s value has no widget-kind context on this thread (the
/// registry that knows it lives in core) — a boolean/number/table maps
/// unambiguously, and a bare string is a list item id when it parses as
/// one, else free text.
fn parse_widget_value(value: Value) -> Result<WidgetValue, Refusal> {
    match value {
        Value::Boolean(b) => Ok(WidgetValue::Bool(b)),
        Value::Integer(n) => Ok(WidgetValue::Number(n as f64)),
        Value::Number(n) => Ok(WidgetValue::Number(n)),
        Value::String(s) => {
            let s = s.to_string_lossy();
            match UiId::parse(&s) {
                Some(id) => Ok(WidgetValue::Item(id)),
                None => Ok(WidgetValue::Text(s)),
            }
        }
        Value::Table(t) => {
            let items_table: Option<Table> = t.get("items").unwrap_or(None);
            let selected: Option<String> = t.get("selected").unwrap_or(None);
            Ok(WidgetValue::Items {
                items: parse_list_items(&items_table)?,
                selected: selected.map(|s| parse_id(&s)).transpose()?,
            })
        }
        _ => Err(bad_argument("Unsupported widget value shape.")),
    }
}

fn parse_color(row: &Table) -> Result<OverlayColor, Refusal> {
    let color: Option<String> = row.get("color").unwrap_or(None);
    match color {
        None => Ok(OverlayColor::default()),
        Some(s) => {
            OverlayColor::parse(&s).ok_or_else(|| bad_argument(format!("'{s}' is not a color.")))
        }
    }
}

fn parse_glyph_ref(icon: &str) -> Result<GlyphRef, Refusal> {
    Ok(match HostGlyph::parse(icon) {
        Some(host) => GlyphRef::Host(host),
        None => GlyphRef::Package(icon.to_string()),
    })
}

fn parse_primitive(row: &Table) -> Result<OverlayPrimitive, Refusal> {
    let id: String = row
        .get("id")
        .map_err(|_| bad_argument("primitive.id is required."))?;
    let id = parse_id(&id)?;
    let kind: String = row
        .get("kind")
        .map_err(|_| bad_argument("primitive.kind is required."))?;
    let color = parse_color(row)?;
    Ok(match kind.as_str() {
        "line" => OverlayPrimitive::Line {
            id,
            at_ms: row.get("at").map_err(|_| bad_argument("at is required."))?,
            color,
        },
        "region" => OverlayPrimitive::Region {
            id,
            from_ms: row
                .get("from")
                .map_err(|_| bad_argument("from is required."))?,
            to_ms: row.get("to").map_err(|_| bad_argument("to is required."))?,
            color,
        },
        "label" => OverlayPrimitive::Label {
            id,
            at_ms: row.get("at").map_err(|_| bad_argument("at is required."))?,
            text: row.get("text").unwrap_or_default(),
            color,
        },
        "glyph" => {
            let icon: String = row
                .get("icon")
                .map_err(|_| bad_argument("icon is required."))?;
            OverlayPrimitive::Glyph {
                id,
                at_ms: row.get("at").map_err(|_| bad_argument("at is required."))?,
                icon: parse_glyph_ref(&icon)?,
                color,
            }
        }
        other => return Err(bad_argument(format!("'{other}' is not a primitive kind."))),
    })
}

fn parse_primitives(table: Table) -> Result<Vec<OverlayPrimitive>, Refusal> {
    let mut primitives = Vec::new();
    for pair in table.sequence_values::<Table>() {
        let row = pair.map_err(|e| bad_argument(e.to_string()))?;
        primitives.push(parse_primitive(&row)?);
    }
    Ok(primitives)
}

fn parse_ids(table: Table) -> Result<Vec<UiId>, Refusal> {
    let mut ids = Vec::new();
    for pair in table.sequence_values::<String>() {
        let s = pair.map_err(|e| bad_argument(e.to_string()))?;
        ids.push(parse_id(&s)?);
    }
    Ok(ids)
}

fn parse_action_spec(row: Table) -> Result<ActionSpec, Refusal> {
    let id: String = row
        .get("id")
        .map_err(|_| bad_argument("action.id is required."))?;
    let label: String = row.get("label").unwrap_or_default();
    let kind_str: String = row.get("kind").unwrap_or_else(|_| "trigger".to_string());
    let kind = ActionKindSpec::parse(&kind_str)
        .ok_or_else(|| bad_argument(format!("'{kind_str}' is not an action kind.")))?;
    Ok(ActionSpec {
        id: parse_id(&id)?,
        label,
        kind,
        default_binding: row.get("default_binding").unwrap_or(None),
        repeats_while_held: row.get("repeats_while_held").unwrap_or(false),
    })
}

fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Nil => serde_json::Value::Null,
        Value::Boolean(b) => serde_json::Value::Bool(*b),
        Value::Integer(i) => serde_json::Value::from(*i),
        Value::Number(n) => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.to_string_lossy()),
        _ => serde_json::Value::Null,
    }
}

fn parse_settings_field(row: Table) -> Result<SettingsField, Refusal> {
    let id: String = row
        .get("id")
        .map_err(|_| bad_argument("field.id is required."))?;
    let label: String = row.get("label").unwrap_or_default();
    let description: Option<String> = row.get("description").unwrap_or(None);
    let kind_str: String = row
        .get("kind")
        .map_err(|_| bad_argument("field.kind is required."))?;
    let default_value: Value = row.get("default").unwrap_or(Value::Nil);
    let kind = match kind_str.as_str() {
        "boolean" => FieldKind::Boolean,
        "number" => FieldKind::Number {
            min: row.get("min").unwrap_or(f64::MIN),
            max: row.get("max").unwrap_or(f64::MAX),
            step: row.get("step").unwrap_or(1.0),
        },
        "string" => FieldKind::String,
        "choice" => {
            let options_table: Option<Table> = row.get("options").unwrap_or(None);
            FieldKind::Choice {
                options: parse_list_items(&options_table)?,
            }
        }
        other => return Err(bad_argument(format!("'{other}' is not a field kind."))),
    };
    Ok(SettingsField {
        id: parse_id(&id)?,
        kind,
        label,
        description,
        default: value_to_json(&default_value),
    })
}

fn parse_settings_fields(table: Table) -> Result<Vec<SettingsField>, Refusal> {
    let mut fields = Vec::new();
    for pair in table.sequence_values::<Table>() {
        let row = pair.map_err(|e| bad_argument(e.to_string()))?;
        fields.push(parse_settings_field(row)?);
    }
    Ok(fields)
}

pub fn install(lua: &Lua, api: &Table, shared: SharedHandle) -> mlua::Result<()> {
    let ns = lua.create_table()?;

    let shared_panel = shared.clone();
    let register_panel = lua.create_function(
        move |lua, (panel, title, widgets): (String, String, Table)| {
            let panel_id = match parse_id(&panel) {
                Ok(id) => id,
                Err(refusal) => return lua_err(lua, &refusal),
            };
            let widgets = match parse_widgets(widgets) {
                Ok(w) => w,
                Err(refusal) => return lua_err(lua, &refusal),
            };
            call(
                lua,
                &shared_panel,
                RequestKind::RegisterPanel,
                Request::RegisterPanel {
                    panel: panel_id,
                    title,
                    widgets,
                },
            )
        },
    )?;
    ns.set("register_panel", register_panel)?;

    let shared_update = shared.clone();
    let update_widget = lua.create_function(
        move |lua, (panel, widget, value): (String, String, Value)| {
            let panel_id = match parse_id(&panel) {
                Ok(id) => id,
                Err(refusal) => return lua_err(lua, &refusal),
            };
            let widget_id = match parse_id(&widget) {
                Ok(id) => id,
                Err(refusal) => return lua_err(lua, &refusal),
            };
            let value = match parse_widget_value(value) {
                Ok(v) => v,
                Err(refusal) => return lua_err(lua, &refusal),
            };
            call(
                lua,
                &shared_update,
                RequestKind::UpdateWidget,
                Request::UpdateWidget {
                    panel: panel_id,
                    widget: widget_id,
                    value,
                },
            )
        },
    )?;
    ns.set("update_widget", update_widget)?;

    let shared_add = shared.clone();
    let add_overlays = lua.create_function(move |lua, primitives: Table| {
        let primitives = match parse_primitives(primitives) {
            Ok(p) => p,
            Err(refusal) => return lua_err(lua, &refusal),
        };
        call(
            lua,
            &shared_add,
            RequestKind::AddOverlays,
            Request::AddOverlays { primitives },
        )
    })?;
    ns.set("add_overlays", add_overlays)?;

    let shared_remove = shared.clone();
    let remove_overlays = lua.create_function(move |lua, ids: Table| {
        let ids = match parse_ids(ids) {
            Ok(ids) => ids,
            Err(refusal) => return lua_err(lua, &refusal),
        };
        call(
            lua,
            &shared_remove,
            RequestKind::RemoveOverlays,
            Request::RemoveOverlays { ids },
        )
    })?;
    ns.set("remove_overlays", remove_overlays)?;

    let shared_clear = shared.clone();
    let clear_overlays = lua.create_function(move |lua, ()| {
        call(
            lua,
            &shared_clear,
            RequestKind::ClearOverlays,
            Request::ClearOverlays,
        )
    })?;
    ns.set("clear_overlays", clear_overlays)?;

    let shared_action = shared.clone();
    let register_action = lua.create_function(move |lua, spec: Table| {
        let action = match parse_action_spec(spec) {
            Ok(a) => a,
            Err(refusal) => return lua_err(lua, &refusal),
        };
        call(
            lua,
            &shared_action,
            RequestKind::RegisterAction,
            Request::RegisterAction { action },
        )
    })?;
    ns.set("register_action", register_action)?;

    let shared_settings = shared.clone();
    let register_settings = lua.create_function(move |lua, fields: Table| {
        let fields = match parse_settings_fields(fields) {
            Ok(f) => f,
            Err(refusal) => return lua_err(lua, &refusal),
        };
        // R5: the runtime ships the plugin thread's own `Scope::Settings`
        // snapshot inside the RPC so core can render the page without a
        // second round trip; core never opens `settings.json` itself.
        let stored: BTreeMap<String, serde_json::Value> =
            lock(&shared_settings).store.settings_snapshot();
        let schema = fields.clone();
        // R4: dispatched directly (rather than through `call`) so a
        // successful reply can update `Shared.settings_schema` here —
        // the one field `get_settings_local` reads back — before the Lua
        // caller sees the result; a refusal leaves any prior schema
        // untouched.
        match super::dispatch(
            &shared_settings,
            RequestKind::RegisterSettings,
            Request::RegisterSettings { fields, stored },
        ) {
            Ok(response) => {
                lock(&shared_settings).settings_schema = Some(schema);
                match super::response_to_lua(lua, response) {
                    Ok(value) => super::lua_ok(value),
                    Err(err) => Err(err),
                }
            }
            Err(refusal) => lua_err(lua, &refusal),
        }
    })?;
    ns.set("register_settings", register_settings)?;

    let shared_get = shared.clone();
    let get_settings = lua.create_function(move |lua, ()| {
        call(
            lua,
            &shared_get,
            RequestKind::GetSettings,
            Request::GetSettings,
        )
    })?;
    ns.set("get_settings", get_settings)?;

    let shared_notify = shared.clone();
    let notify = lua.create_function(move |lua, (level, text): (String, String)| {
        // Contract §3.6: a bad `level` still reaches admission (and so
        // still consumes the notify window's slot) — only a genuinely
        // malformed *shape* (handled above, for the other calls) is
        // refused before admission. `NotifyLevel::Info` is a harmless
        // placeholder when `level` doesn't parse; `plugins::apply`'s own
        // `notify` arm (US5 T111) re-validates the original string against
        // `crate::ui::validate_notify` and is the refusal of record.
        let parsed = NotifyLevel::parse(&level).unwrap_or(NotifyLevel::Info);
        call(
            lua,
            &shared_notify,
            RequestKind::Notify,
            Request::Notify {
                level: parsed,
                text,
            },
        )
    })?;
    ns.set("notify", notify)?;

    api.set("ui", ns)?;
    Ok(())
}
