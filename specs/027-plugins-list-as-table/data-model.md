# Data Model: Plugins List as a Real Table (027)

**Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

Presentation-only feature. One core model struct is extended; everything else is UI-local value types. No persistence, no runtime/lifecycle change.

## 1. `PluginRow` (core, `crates/modplayer-core/src/plugins/view.rs`) — extended

| Field | Type | Status | Source / rule |
|-------|------|--------|---------------|
| `id`, `identifier`, `name`, `version`, `source`, `enabled`, `health`, `invalid_reason`, `permissions`, `cpu_pct_of_share`, `memory_bytes`, `can_uninstall`, `panels` | — | unchanged | 009 / 011 |
| `suspend_cause` | `Option<SuspendCause>` | **new** | `Some(cause)` iff `record.lifecycle == Lifecycle::Suspended { cause }`; else `None` (R6). Invariant: `suspend_cause.is_some() ⇒ health == Some(Health::Suspended)`. |
| `cpu_budget_pct` | `f32` | **new** | `record.budgets.share / record.budgets.window × 100` (10.0 for `Budgets::DEFAULT`); `0.0` if `window` is zero (R7). |
| `memory_budget_bytes` | `u64` | **new** | `record.budgets.memory as u64` (67 108 864 for `DEFAULT`) (R7). |

Unchanged invariants: rows sorted by `name.to_lowercase()`; `cpu_pct_of_share`/`memory_bytes` are `None` unless `Active` with gauges; `permissions` in catalog order.

## 2. `Health` display mapping (UI, pure)

| `Health` | Word key (en-US value) | Theme role | Restart shown |
|----------|-----------------------|------------|---------------|
| `Ok` | `plugins-health-ok` ("healthy") | `roles.positive` | no |
| `Warning` | `plugins-health-warning` ("degraded") | `roles.warning` | no |
| `Suspended` | `plugins-health-suspended` ("suspended") | `roles.danger` | **yes** |
| `None` (invalid) | — (invalid-manifest span) | — | no |

`health_color(roles, health)` unchanged. `suspension_reason(health, cause) -> Option<String>`: `(Some(Suspended), Some(c))` → `plugin-suspended-cause-{hang|cpu-share|memory|did-not-start}`; `(Some(Suspended), None)` → `plugins-suspended-reason-unknown`; else `None`.

## 3. `PluginsColumns` (UI-local, `plugins_view.rs`, pure)

```text
enum Column { Name, Source, Enabled, Health, Permissions, Resource, Actions }   // display order = FR-001
struct ColumnExtent { min_x: f32, max_x: f32 }                                    // relative to table left
struct PluginsColumns { extents: [ColumnExtent; 7], gap: f32 }
fn PluginsColumns::layout(available_width: f32) -> PluginsColumns
fn PluginsColumns::span(from: Column, to: Column) -> ColumnExtent                 // invalid-manifest span
```

Constants (R2): `GAP = theme::space::XS (4)`; `MIN = [104, 56, 44, 80, 48, 128, 76]` (Σ 536 + 6×4 = 560); `WEIGHT = [3, 1, 0, 2, 0, 1, 1]`.

Rules:
- L1 `available ≥ 560`: width_i = MIN_i + extra × WEIGHT_i / ΣWEIGHT, extra = available − 560.
- L2 `available < 560` (defensive): width_i = MIN_i × (available − 24) / 536.
- L3 extents are contiguous left→right separated by exactly `GAP`; last `max_x ≤ available` (float tolerance 0.5 pt).
- L4 every `width_i ≥ 0`; for `available ≥ 560`, `width_i ≥ MIN_i`.

## 4. `PluginsViewState` (UI-local, owned by `App`)

| Field | Type | Rule |
|-------|------|------|
| `permissions_open` | `HashSet<PluginId>` | toggled by the permissions disclosure; only toggleable when `permissions.len() > 0` |
| `panels_open` | `HashSet<PluginId>` | toggled by "Panels (N)"; only exists when `panels.len() ≥ 2` |

Lifecycle: created empty at app start; never persisted; after each `plugins_view()` read, ids absent from `view.rows` are removed (`retain`). Health/enable changes do not touch it (keyed by `PluginId`, R8). Toggles are independent; many rows may be open.

## 5. Row rendering states (derived, no stored state)

| Row condition | Health cell | Permissions cell | Resource cell | Actions cell |
|---------------|-------------|------------------|---------------|--------------|
| invalid manifest | span Health→Resource: `plugins-invalid-manifest` (truncating) | (spanned) | (spanned) | empty |
| Active, gauges | ● word | count [+ disclosure] | `CPU u % / b %` / `Mem u MB / b MB` (mono, right-aligned) [+ "over budget"] | panel controls |
| Loading / Disabled / no gauges | ● word (if health) | count [+ disclosure] | `CPU —` / `Mem —` | panel controls |
| Suspended | ● "suspended" + reason (truncating) | count [+ disclosure] | `—` lines | panel controls + **Restart** |

Panel controls: 0 panels → nothing; 1 panel → inline Show/Hide + Enable/Disable; ≥ 2 → "Panels (N)" disclosure, lines in the expansion area.

State transition via Restart: `Suspended --plugin_restart(id)--> Disabled → spawn → Loading → Active` (existing controller behaviour; the row reflects it on the next repaint).
