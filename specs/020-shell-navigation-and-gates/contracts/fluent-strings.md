# Contract: Fluent Strings

**Covers**: FR-002, FR-009 (externalized strings, NFR-7.1) · **Tests**: `crates/modplayer-ui/tests/fluent_keys.rs`

en-US keys are added in this feature. pt-BR has **no bundle yet** (`locales/` holds only
`en-US/`; `tr` resolves `en-US` only — research.md R8), so the pt-BR column is the reviewed draft
the pt-BR slice must ship verbatim. No `locales/pt-BR/` directory is created here.

## `locales/en-US/app.ftl`

```ftl
## 020-shell-navigation-and-gates: launch gate step indicator
gate-step-welcome = Welcome
gate-step-sign-in = Sign in
gate-step-audio-output-check = Audio output check
# $current, $total: integers; $label: one of the three step labels above
gate-step-progress = Step { $current } of { $total }: { $label }
```

## `locales/en-US/settings.ftl`

```ftl
## 020-shell-navigation-and-gates: settings category row overflow control
settings-more = More
settings-more-a11y = More settings categories
```

## pt-BR drafts (for the pt-BR slice)

| Key | pt-BR |
|---|---|
| `gate-step-welcome` | Boas-vindas |
| `gate-step-sign-in` | Entrar |
| `gate-step-audio-output-check` | Verificação da saída de áudio |
| `gate-step-progress` | Etapa { $current } de { $total }: { $label } |
| `settings-more` | Mais |
| `settings-more-a11y` | Mais categorias de configurações |

## Clauses

| # | Clause |
|---|---|
| F1 | Every key above resolves (not the raw-key fallback) in en-US; `gate-step-progress` resolves with all three args and renders "Step 2 of 3: Sign in" for `(2, 3, tr("gate-step-sign-in"))`. |
| F2 | `fluent_keys.rs`'s "no unexercised key in `settings.ftl`" check lists the two new settings keys. |
| F3 | No new user-visible literal in Rust sources (`design_token_literals.rs`/code review). Numerals in the progress string are passed as Fluent numbers, not pre-formatted. |
