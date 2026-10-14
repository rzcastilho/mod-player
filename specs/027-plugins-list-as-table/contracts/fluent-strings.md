# Contract: Fluent strings (027)

en-US in `locales/en-US/plugins.ftl`; pt-BR in a **new** `locales/pt-BR/plugins.ftl` carrying only the keys below (staged, not yet a runtime locale — same pattern as `locales/pt-BR/library.ftl`, 025). `crates/modplayer-ui/tests/fluent_keys.rs`: add every new key to the key table, remove the removed keys, and add a parity test asserting each key below exists in both files with the same variables.

## Changed values (key unchanged)

| Key | en-US | pt-BR |
|-----|-------|-------|
| `plugins-health-ok` | healthy | saudável |
| `plugins-health-warning` | degraded | degradado |
| `plugins-health-suspended` | suspended | suspenso |

## New keys

| Key | Vars | en-US | pt-BR |
|-----|------|-------|-------|
| `plugins-col-resource` | — | Resource use | Uso de recursos |
| `plugins-col-actions` | — | Actions | Ações |
| `plugins-resource-cpu` | `$used`, `$budget` | CPU { $used } % / { $budget } % | CPU { $used } % / { $budget } % |
| `plugins-resource-memory` | `$used`, `$budget` | Mem { $used } MB / { $budget } MB | Mem { $used } MB / { $budget } MB |
| `plugins-resource-cpu-none` | — | CPU — | CPU — |
| `plugins-resource-memory-none` | — | Mem — | Mem — |
| `plugins-over-budget` | — | over budget | acima do limite |
| `plugins-suspended-reason-unknown` | — | reason unavailable | motivo indisponível |
| `plugins-permissions-show` | `$count`, `$plugin` | Show { $count } permissions of { $plugin } | Mostrar { $count } permissões de { $plugin } |
| `plugins-permissions-hide` | `$count`, `$plugin` | Hide { $count } permissions of { $plugin } | Ocultar { $count } permissões de { $plugin } |
| `plugins-panels-count` | `$count` | Panels ({ $count }) | Painéis ({ $count }) |
| `plugins-panels-show` | `$plugin` | Show panels of { $plugin } | Mostrar painéis de { $plugin } |
| `plugins-panels-hide` | `$plugin` | Hide panels of { $plugin } | Ocultar painéis de { $plugin } |
| `plugins-panel-show-a11y` | `$title`, `$plugin` | Show { $title } panel of { $plugin } | Mostrar painel { $title } de { $plugin } |
| `plugins-panel-hide-a11y` | `$title`, `$plugin` | Hide { $title } panel of { $plugin } | Ocultar painel { $title } de { $plugin } |
| `plugins-panel-enable-a11y` | `$title`, `$plugin` | Enable { $title } panel of { $plugin } | Ativar painel { $title } de { $plugin } |
| `plugins-panel-disable-a11y` | `$title`, `$plugin` | Disable { $title } panel of { $plugin } | Desativar painel { $title } de { $plugin } |
| `plugins-restart-a11y` | `$plugin` | Restart { $plugin } | Reiniciar { $plugin } |

## Removed keys

`plugins-col-version`, `plugins-col-cpu`, `plugins-col-memory`, `plugins-cpu`, `plugins-memory` (the literal "64 MB" goes with it), `plugins-list-separator` (permissions are no longer comma-joined). Implementation greps the workspace to confirm no other caller before removal; any remaining caller keeps its key.

## Reused unchanged

`plugins-title`, `plugins-empty`, `plugins-col-name/-source/-enabled/-health/-permissions`, `plugins-source-bundled`, `plugins-enable-toggle`, `plugins-invalid-manifest`, `plugins-dash`, `permission-*` (25), `plugin-suspended-cause-*` (4), `plugin-panel-show/-hide/-enable/-disable/-restart`.

Pseudo-locale expansion (`apply_pseudo_expansion`) must not break the 960 pt fit: covered because every cell truncates (T6).
