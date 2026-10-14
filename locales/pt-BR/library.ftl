## 025-library-browsing-and-detail (contracts/fluent-strings.md): pt-BR
## values for this feature's changed and new keys, plus pt-BR values for
## three existing en-US keys the detail header now renders (FR-012,
## NFR-7.1). Not yet a shipped locale (`tr`/`tr_args` still resolve against
## en-US only) — this bundle exists so the `fluent_keys` parity test proves
## every key here has a matching pt-BR translation, ready for when a locale
## switch ships.

## Changed values.

detail-back = ‹ Biblioteca

## New keys.

detail-play = Tocar
detail-play-name = Tocar { $name }
detail-no-tracks-hint = Esta coleção não tem faixas
detail-owner = por { $name }
detail-kind-artist = Artista
detail-top-track-count = { $count ->
    [one] 1 faixa em destaque
   *[other] { $count } faixas em destaque
}
detail-runtime-minutes = { $minutes } min
detail-runtime-hours = { $hours } h { $minutes } min

## pt-BR values for existing en-US keys the header renders.

playlist-track-count = { $count ->
    [one] 1 faixa
   *[other] { $count } faixas
}
row-actions = Ações para { $name }
playlist-no-tracks = Esta playlist não tem faixas

## 026-search-results-structure (contracts/fluent-strings.md): pt-BR values
## for the new search keys and the existing keys the Search view renders.

search-field-label = Pesquisar no catálogo
search-hint = Faixas, álbuns, artistas, playlists
search-clear = Limpar pesquisa
search-in-flight = Pesquisando…
search-result-count = { $count ->
    [one] 1 resultado
   *[other] { $count } resultados
}
search-group-header = { $group }, { $count ->
    [one] 1 resultado
   *[other] { $count } resultados
}
search-stale = Mostrando resultados anteriores — a pesquisa está ocupada, atualizando em breve
search-rate-limited = A pesquisa está ocupada — tentando novamente em breve
search-offline = A pesquisa precisa de conexão — você está offline
search-no-results = Nenhum resultado para “{ $query }” — verifique a ortografia ou talvez você esteja offline.
search-group-tracks = Faixas
search-group-albums = Álbuns
search-group-artists = Artistas
search-group-playlists = Playlists
search-show-more = Mostrar mais { $group }
