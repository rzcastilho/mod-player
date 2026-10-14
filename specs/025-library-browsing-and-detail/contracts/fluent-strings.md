# Contract: Fluent Strings (025)

en-US in `locales/en-US/library.ftl`; pt-BR in new `locales/pt-BR/library.ftl` (this feature's keys only; header comment as in `pt-BR/effects.ftl`). `tests/fluent_keys.rs`: every key resolves via `tr`/`tr_args`; `library_025_keys_have_en_us_and_pt_br_parity` asserts each key below exists in both files with the same placeholders.

## Changed

| Key | en-US | pt-BR |
|-----|-------|-------|
| `detail-back` | `‹ Library` | `‹ Biblioteca` |

## New

| Key | Args | en-US | pt-BR |
|-----|------|-------|-------|
| `detail-play` | — | `Play` | `Tocar` |
| `detail-play-name` | `name` | `Play { $name }` | `Tocar { $name }` |
| `detail-no-tracks-hint` | — | `This collection has no tracks` | `Esta coleção não tem faixas` |
| `detail-owner` | `name` | `by { $name }` | `por { $name }` |
| `detail-kind-artist` | — | `Artist` | `Artista` |
| `detail-top-track-count` | `count` | `{ $count -> [one] 1 top track *[other] { $count } top tracks }` | `{ $count -> [one] 1 faixa em destaque *[other] { $count } faixas em destaque }` |
| `detail-runtime-minutes` | `minutes` | `{ $minutes } min` | `{ $minutes } min` |
| `detail-runtime-hours` | `hours`, `minutes` | `{ $hours } hr { $minutes } min` | `{ $hours } h { $minutes } min` |
| `playlist-track-count` (pt-BR only; en-US unchanged) | `count` | — | `{ $count -> [one] 1 faixa *[other] { $count } faixas }` |
| `row-actions` (pt-BR only; en-US unchanged) | `name` | — | `Ações para { $name }` |
| `playlist-no-tracks` (pt-BR only) | — | — | `Esta playlist não tem faixas` |

The last three rows add pt-BR values for existing en-US keys the header now renders, so the whole header is covered in pt-BR.

## Unchanged, reused

`playlist-owner` (rows only), `coming-soon`, `action-*` (six menu labels), `loading`, `library-empty*`, `action-search`, `action-create`, `library-retry`.

Facts separator `" · "` is a code constant (`FACTS_SEPARATOR`), not a Fluent key (research R2).
