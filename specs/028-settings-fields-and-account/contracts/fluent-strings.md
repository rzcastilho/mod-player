# Contract: Fluent Strings

**Feature**: 028-settings-fields-and-account | Covers FR-015, Constitution X (NFR-7.1).

en-US values go into the existing `locales/en-US/settings.ftl` / `account.ftl`. pt-BR values go into **new** `locales/pt-BR/settings.ftl` / `locales/pt-BR/account.ftl`, holding only this feature's new/changed keys (025/027 precedent; pt-BR is staged, not yet a runtime locale). Every key below must be listed in `crates/modplayer-ui/tests/fluent_keys.rs` tables and covered by the new pt-BR parity test.

## New keys — `settings.ftl`

| Key | en-US | pt-BR |
|---|---|---|
| `settings-group-output` | Output | Saída |
| `settings-group-level-protection` | Level protection | Proteção de nível |
| `settings-group-connect-device` | Connect device | Dispositivo Connect |
| `settings-group-markers` | Markers | Marcadores |
| `settings-group-theme` | Theme | Tema |
| `settings-group-language` | Language | Idioma |
| `setting-value-dbfs` | `{ $value } dBFS` | `{ $value } dBFS` |
| `setting-value-percent` | `{ $value }%` | `{ $value }%` |
| `setting-value-ms` | `{ $value } ms` | `{ $value } ms` |
| `setting-range-dbfs` | `{ $min } to { $max } dBFS` | `{ $min } a { $max } dBFS` |
| `setting-range-percent` | `{ $min } to { $max }%` | `{ $min } a { $max }%` |
| `setting-range-ms` | `{ $min } to { $max } ms` | `{ $min } a { $max } ms` |
| `settings-reset` | Reset | Redefinir |
| `settings-reset-a11y` | `Reset { $field } to default` | `Redefinir { $field } para o padrão` |
| `settings-coming-soon` | Coming soon | Em breve |
| `settings-category-coming-soon-a11y` | `{ $category }, coming soon` | `{ $category }, em breve` |
| `settings-unavailable-offline` | Offline settings aren't available yet. They'll arrive in a later update. | As configurações offline ainda não estão disponíveis. Elas chegarão em uma atualização futura. |
| `settings-unavailable-privacy-diagnostics` | Privacy and diagnostics settings aren't available yet. They'll arrive in a later update. | As configurações de privacidade e diagnóstico ainda não estão disponíveis. Elas chegarão em uma atualização futura. |

## New keys — `account.ftl`

| Key | en-US | pt-BR |
|---|---|---|
| `account-summary-title` | Signed-in account | Conta conectada |
| `account-identity-unavailable` | Name unavailable | Nome indisponível |
| `account-tier-unverified` | Not verified yet | Ainda não verificado |
| `account-last-verified` | Last verified | Última verificação |
| `account-last-verified-at` | `{ $day } { $month } { $year }, { $time }` | `{ $day } { $month } { $year }, { $time }` |
| `account-never-verified` | Never verified online | Nunca verificado online |
| `date-month-short-1` … `date-month-short-12` | Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec | jan fev mar abr mai jun jul ago set out nov dez |

## Unchanged keys reused

`account-tier` (label "Subscription tier"), `tier-premium`, `tier-free`, `account-recheck`, `account-recheck-desc`, `account-sign-out`, `signout-*`, `settings-cat-*`, every existing `setting-*` title/desc key. `tier-unknown` stays (used by tier-gate screens). `placeholder-settings-category` stays (Plugins management placeholder still uses it).

## Removed keys (only after workspace grep shows no caller)

| Key | Replaced by |
|---|---|
| `account-display-name` ("Signed in as { $name }") | identity line + `account-summary-title` |
| `account-last-validated` ("Last checked { $when }") | `account-last-verified` + `account-last-verified-at` |
| `account-never-validated` ("Never checked online") | `account-never-verified` |

The corresponding entries in `fluent_keys.rs` (`ACCOUNT_KEYS` etc.) move to the new keys in the same change.

## Rules

- No new user-visible literal in Rust source (checked by review + existing `trademark.rs`/`fluent_keys.rs` conventions); the `" ms"` literal suffix in `playback.rs` is removed.
- Fluent placeholders use `tr_args`; numbers are passed pre-formatted as strings (one decimal for dBFS) so Fluent's own number formatting does not alter them.
