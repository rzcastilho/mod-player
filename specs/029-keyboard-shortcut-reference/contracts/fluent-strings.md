# Contract: Fluent strings (F1–F5)

Covers FR-013, NFR-7.1 and constitution X. All **new** keys live in `locales/en-US/shortcuts.ftl` and `locales/pt-BR/shortcuts.ftl`, which are new files picked up automatically by `static_loader!`. **Changed** getting-started keys stay in `locales/en-US/app.ftl`. Their pt-BR versions go in a new `locales/pt-BR/app.ftl` that holds only the getting-started keys. `tr` still resolves en-US; pt-BR is shipped for parity (as in 024 and 028).

| # | Rule | Test (`tests/fluent_keys.rs`) |
|---|---|---|
| F1 | Every key below resolves under en-US (`tr(key) != key`) | `shortcuts_029_keys_resolve` |
| F2 | Every key below is defined in both en-US and pt-BR | `shortcuts_029_keys_have_en_us_and_pt_br_parity` |
| F3 | Every `KeyWord::fluent_key()` and `KeyWord::spoken_key()` is in the table (no orphan variant) | `every_key_word_has_a_string` |
| F4 | No `getting-started-*` value contains `Shortcuts:` (en-US) or `Atalhos:` (pt-BR) | `getting_started_has_no_shortcut_prose` |
| F5 | The new UI modules contain no literal user-facing strings. Only `+`, the mac modifier symbols and punctuation characters are allowed as typographic constants | existing literal scan extended to the new modules |

## New keys — `shortcuts.ftl`

| Key | en-US | pt-BR |
|---|---|---|
| `nav-keyboard-shortcuts` | Keyboard shortcuts | Atalhos de teclado |
| `shortcuts-sheet-title` | Keyboard shortcuts | Atalhos de teclado |
| `shortcuts-not-bound` | Not bound | Sem atalho |
| `shortcuts-open-printable` | Open printable version | Abrir versão para impressão |
| `shortcuts-printable-error` | Couldn't create the printable version. Check that there is free disk space and try again. | Não foi possível criar a versão para impressão. Verifique se há espaço livre em disco e tente novamente. |
| `shortcuts-printable-generated` | Generated { $date } | Gerado em { $date } |
| `shortcuts-printable-col-action` | Action | Ação |
| `shortcuts-printable-col-shortcut` | Shortcut | Atalho |
| `shortcuts-conflict-marker` | Conflict | Conflito |
| `controls-row` | { $action }: { $bindings } | { $action }: { $bindings } |
| `key-word-space` | Space | Espaço |
| `key-word-return` | Return | Return |
| `key-word-enter` | Enter | Enter |
| `key-word-esc` | Esc | Esc |
| `key-word-tab` | Tab | Tab |
| `key-word-backspace` | Backspace | Backspace |
| `key-word-delete` | Delete | Delete |
| `key-word-insert` | Insert | Insert |
| `key-word-home` | Home | Home |
| `key-word-end` | End | End |
| `key-word-page-up` | Page Up | Page Up |
| `key-word-page-down` | Page Down | Page Down |
| `key-word-copy` | Copy | Copiar |
| `key-word-cut` | Cut | Recortar |
| `key-word-paste` | Paste | Colar |
| `key-word-left` | Left | Esquerda |
| `key-word-right` | Right | Direita |
| `key-word-up` | Up | Cima |
| `key-word-down` | Down | Baixo |
| `key-word-ctrl` | Ctrl | Ctrl |
| `key-word-shift` | Shift | Shift |
| `key-word-alt` | Alt | Alt |
| `key-word-browser-back` | Browser Back | Voltar (navegador) |
| `key-word-shift-left` | Left Shift | Shift esquerdo |
| `key-word-shift-right` | Right Shift | Shift direito |
| `key-word-control-left` | Left Ctrl | Ctrl esquerdo |
| `key-word-control-right` | Right Ctrl | Ctrl direito |
| `key-word-alt-left` | Left Alt | Alt esquerdo |
| `key-word-alt-right` | Right Alt | Alt direito |
| `key-word-super-left` | Left Super | Super esquerdo |
| `key-word-super-right` | Right Super | Super direito |
| `key-word-intl-backslash` | Intl Backslash | Barra invertida internacional |
| `key-spoken-command` | Command | Command |
| `key-spoken-option` | Option | Option |
| `key-spoken-arrow-left` | Left Arrow | Seta para a esquerda |
| `key-spoken-arrow-right` | Right Arrow | Seta para a direita |
| `key-spoken-arrow-up` | Up Arrow | Seta para cima |
| `key-spoken-arrow-down` | Down Arrow | Seta para baixo |
| `key-spoken-colon` | Colon | Dois-pontos |
| `key-spoken-comma` | Comma | Vírgula |
| `key-spoken-minus` | Minus | Menos |
| `key-spoken-period` | Period | Ponto |
| `key-spoken-plus` | Plus | Mais |
| `key-spoken-equals` | Equals | Igual |
| `key-spoken-semicolon` | Semicolon | Ponto e vírgula |
| `key-spoken-backslash` | Backslash | Barra invertida |
| `key-spoken-slash` | Slash | Barra |
| `key-spoken-pipe` | Pipe | Barra vertical |
| `key-spoken-questionmark` | Question Mark | Ponto de interrogação |
| `key-spoken-exclamationmark` | Exclamation Mark | Ponto de exclamação |
| `key-spoken-open-bracket` | Left Bracket | Colchete esquerdo |
| `key-spoken-close-bracket` | Right Bracket | Colchete direito |
| `key-spoken-open-curly` | Left Brace | Chave esquerda |
| `key-spoken-close-curly` | Right Brace | Chave direita |
| `key-spoken-backtick` | Backtick | Acento grave |
| `key-spoken-quote` | Quote | Aspas simples |

The spoken form of a `Word` key reuses its `key-word-*` value. The spoken forms of Shift, Ctrl and Alt reuse `key-word-shift`, `key-word-ctrl` and `key-word-alt`.

## Changed keys

| Key | File | en-US (new) | pt-BR (new file `pt-BR/app.ftl`) |
|---|---|---|---|
| `getting-started-title` | app.ftl | (unchanged) Getting started | Primeiros passos |
| `getting-started-section-loop` | app.ftl | Section Loop — drop A and B around a passage and drill it hands-free. | Section Loop — marque A e B em volta de um trecho e repita-o sem usar as mãos. |
| `getting-started-key-tempo` | app.ftl | Key & Tempo — transpose a song or slow it down without changing the rest. | Key & Tempo — transponha uma música ou diminua o andamento sem alterar o resto. |
| `getting-started-tutorial` | app.ftl | (unchanged) Open plugin tutorial | Abrir tutorial de plugins |
| `getting-started-dismiss` | app.ftl | (unchanged) Dismiss | Dispensar |
| `getting-started-shortcuts` (new) | app.ftl | Keyboard shortcuts | Atalhos de teclado |
| `controls-binding-chip` | controls.ftl | (unchanged) Binding { $binding }. **`$binding` is now the spoken form, and the value is used only as the accessible name, not painted** | n/a (controls.ftl has no pt-BR file today; out of scope) |
| `controls-remove-binding` | controls.ftl | (unchanged text) Remove binding { $binding }. `$binding` = spoken form | n/a |

`controls-kind-trigger` stays defined (it is still referenced by the kind mapping for exhaustiveness), but it is no longer painted (C7).
