# Contract: Plugin actions in the Action & Binding service

**Feature**: 011-plugin-ui-contributions | **Extends**: [007 action-registry.md](../../007-keyboard-actions-and-shortcuts/contracts/action-registry.md) (rules G1–G9 stay in force), [007 keymap-settings.md](../../007-keyboard-actions-and-shortcuts/contracts/keymap-settings.md), [007 ui-actions.md](../../007-keyboard-actions-and-shortcuts/contracts/ui-actions.md) | **Spec**: FR-008, FR-010–FR-013

## 1. Identity and ownership

- **R1** A plugin action's id is `<plugin identifier>.<name>`, `name` matching `[a-z][a-z0-9_]{0,63}`; `PluginActionId::parse` splits on the **last** `.`. `host.` remains reserved for the catalog.
- **R2** `ActionOwner::Plugin { id, tier }` with `tier ∈ { Bundled, Community }`; `OwnerTier::Host` is implied for catalog actions. Ordering `Host > Bundled > Community`. Today every plugin record is `Source::Bundled` ⇒ `Bundled`.
- **R3** Scope is always `App`; `repeats_while_held` defaults `false`; kind `Trigger` | `Continuous`.

## 2. Registry rules

- **G10 (register)** `register_plugin_action(def)`: the 65th distinct id for one owner ⇒ `Err(ActionLimit)`; re-registering replaces `label/kind/repeats_while_held/default_binding`, keeps any user override, then `rebuild()`. A default binding that `Chord::parse` rejects, or that 007 FR-007's capture would reject (`Tab`, `Shift+Tab`, `Esc`, lone modifier, macOS `Control` chord), yields `default_binding = None` (the caller logs the console warning naming the binding).
- **G11 (enabled tracks health)** `set_plugin_enabled(owner, false)` on `Suspend`/`Disable`; `true` on `Ready`. Disabled plugin actions are out of `index` (so out of conflict evaluation) exactly as 007 FR-012's `enabled = false`.
- **G12 (unregister)** `unregister_plugin_actions(owner)` on `Disable`/`Shutdown`/uninstall: removes defs, moves each id's override to `KeymapOverrides::dormant` (never dropped), `rebuild()`.
- **G13 (tiered conflicts, FR-011)** In `rebuild`, for each chord with ≥ 2 enabled candidates whose scopes `can_coexist`: `top` = max tier among them. Exactly one candidate at `top` ⇒ flag every other candidate only; ≥ 2 at `top` ⇒ flag all of them (and every lower one). `resolve` returns the first unflagged live candidate (host catalog order first, then plugin ids sorted). Consequences: host `L` + plugin `L` ⇒ plugin flagged, host fires (SC-002); two bundled plugins on `K` ⇒ both flagged, neither fires (EC-6.8); two host actions ⇒ 007's rule unchanged (same-tier tie).
- **G14 (invocable)** `is_invocable(id)` = registered ∧ enabled ∧ no chord of it is flagged. `invoke_plugin_action` from a panel button or keyboard checks it; a non-invocable action delivers nothing and (for the button path) logs a console line.
- **G15 (rows)** `rows()` yields the 46 host rows in catalog order, then one group per owning plugin sorted by plugin name, each action sorted by label; a plugin row carries `label: RowLabel::Literal(resolved string)`, `category: RowCategory::Plugin { name }`, `owner_tier`, `enabled`, `bindings`, `conflicts`. A row whose plugin is not Active renders greyed; conflict text names the partner's tier ("conflicts with the host's Toggle loop").

## 3. Dispatch (FR-008, FR-012)

- **D1** `modplayer-ui::actions::dispatch` returns `Invocation { action: ActionId, repeat }`; `push_invocation` reads `repeats_while_held` from the catalog or the plugin def.
- **D2** `invoke()`: `ActionId::Host(a)` → the 007 match unchanged; `ActionId::Plugin(id)` → `controller.invoke_plugin_action(&id, ActionSource::Keyboard)`.
- **D3** A panel button with `action = "<name>"` resolves `<identifier>.<name>` against the registry: registered ⇒ `invoke_plugin_action(&id, ActionSource::Ui)`; unregistered ⇒ inert + console entry.
- **D4** `invoke_plugin_action` delivers `HostEvent::ActionInvoked { action: id.id(), source, value: (kind == Continuous).then_some(1.0) }` directly to the owning handle (never via fan-out). An action whose owner has no running handle delivers nothing.

## 4. Persistence (FR-010a)

- **K1** `[keybindings]` entries whose id is **not** `host.`-namespaced and whose value decodes are loaded into `dormant` and re-serialised verbatim; they never appear in `keybindings-invalid-entries`.
- **K2** `host.`-namespaced unknown ids and undecodable values behave exactly as 007 (dropped + warning).
- **K3** On `register_plugin_action`, `adopt_dormant(id)` moves a matching dormant entry into the live override map before `rebuild()`; on unregister, `park(id)` moves it back. Both are pure map moves — nothing is written to disk until the user next changes a binding (007's write rule).

## 5. Named tests

- core `tests/actions.rs::{plugin_action_id_parse_roundtrip, plugin_action_id_rejects_bad_name, host_beats_bundled_on_same_chord, two_bundled_both_flagged, host_vs_host_unchanged, disabled_plugin_action_excluded_from_conflicts, reenable_reevaluates_conflicts, sixty_fifth_action_refused, reregister_keeps_override, rejected_default_registers_unbound, dormant_override_adopted_on_register, dormant_parked_on_unregister, rows_group_plugins_after_host, is_invocable_gate}`
- core `tests/settings.rs::{non_host_keybinding_retained_dormant, host_unknown_still_warned}`
- core `tests/controller_plugin_ui.rs::{keyboard_invokes_plugin_action, continuous_value_one, inactive_action_not_dispatched, button_names_unknown_action_inert}`
- ui `tests/actions.rs::{dispatch_returns_plugin_action_id}`; `tests/controls.rs::{plugin_group_rendered, tier_conflict_text, greyed_when_suspended}`
