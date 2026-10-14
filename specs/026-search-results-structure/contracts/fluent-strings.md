# Contract: Fluent Strings

**Covers**: FR-007–FR-012a, Constitution X (NFR-7.1) | **Research**: R13

Files: `locales/en-US/library.ftl` (en-US, shipped) and `locales/pt-BR/library.ftl` (pt-BR parity bundle established by 025; `tests/fluent_keys.rs` parity test proves every key has a pt-BR value). Every key below is added to `tests/fluent_keys.rs`' exercised list; `$`-args resolved via `tr_args`.

## New keys

| Key | en-US | pt-BR | Args | Used by |
|---|---|---|---|---|
| `search-field-label` | Search the catalog | Pesquisar no catálogo | — | field accessible name (F1) |
| `search-hint` | Tracks, albums, artists, playlists | Faixas, álbuns, artistas, playlists | — | field placeholder (F2) |
| `search-clear` | Clear search | Limpar pesquisa | — | trailing × (C2) and empty-state button (E2) |
| `search-in-flight` | Searching… | Pesquisando… | — | spinner (S3) |
| `search-result-count` | `{ $count -> [one] 1 result *[other] { $count } results }` | `{ $count -> [one] 1 resultado *[other] { $count } resultados }` | `count` | count line (N2) |
| `search-group-header` | `{ $group }, { $count -> [one] 1 result *[other] { $count } results }` | `{ $group }, { $count -> [one] 1 resultado *[other] { $count } resultados }` | `group`, `count` | header accessible name (RL8) |
| `search-stale` | Showing earlier results — search is busy, refreshing shortly | Mostrando resultados anteriores — a pesquisa está ocupada, atualizando em breve | — | status strip (V2) |
| `search-rate-limited` | Search is busy — retrying shortly | A pesquisa está ocupada — tentando novamente em breve | — | status strip (V2) |

## pt-BR values for existing keys the view renders

Added to `locales/pt-BR/library.ftl` so the whole Search view has parity: `search-offline`, `search-no-results`, `search-group-tracks`, `search-group-albums`, `search-group-artists`, `search-group-playlists`, `search-show-more` (translations authored during implementation; parity test enforces presence).

## Removed keys

| Key | Why |
|---|---|
| `search-placeholder` | Only consumer was the removed visible label + hint (research R6). Removed from `library.ftl` and from `tests/fluent_keys.rs` / `tests/accessibility.rs`. |

## Unchanged

`refreshing` (still used by `library_view.rs`), `nav-search`, `action-nav-search`.
