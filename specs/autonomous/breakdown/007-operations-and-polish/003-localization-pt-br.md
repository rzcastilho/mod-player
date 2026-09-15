# 007-operations-and-polish / 003 — Localization and Portuguese (Brazil)

**Source:** [FR-14.4 Appearance and language](../../ModPlayer-Software-Specification.md#fr-144-appearance-and-language) (FR-14.4.2), [NFR § 7 Internationalization and localization](../../ModPlayer-Software-Specification.md#7-internationalization-and-localization), [Part 5 § 7 UI contribution rules](../../ModPlayer-Software-Specification.md#7-ui-contribution-rules) (PL-7.3), [Part 5 § 2 Plugin package](../../ModPlayer-Software-Specification.md#2-plugin-package) (localizable strings), [GOV § 7 Community and support](../../ModPlayer-Software-Specification.md#7-community-and-support) (GOV-7.4), [§ 3 Edge users](../../ModPlayer-Software-Specification.md#3-edge-users) (non-English speaker), [NFR § 5 Privacy](../../ModPlayer-Software-Specification.md#5-privacy) (NFR-5.5 notice in every locale)

**Prerequisites:** Assumes the settings shell from 001-mvp/001-walking-skeleton and plugin UI contributions from 001-mvp/011-plugin-ui-contributions.

## Prompt

> Let a Portuguese-speaking musician use ModPlayer entirely in their language, and let plugin authors ship translations through the same mechanism the host uses.
>
> All host strings are externalized. The UI language is selectable under Settings → Language, defaulting to the system locale; English and Portuguese (Brazil) ship complete, including the first-launch disclosure, the privacy notice, the Getting Started panel, and the plugin tutorial. Numbers, times, and dates format per locale. Musical terms offer both English note names (C, D, E) and solfège (dó, ré, mi) as a user preference, applied wherever keys are shown, including Key & Tempo's detected and resulting key. Layouts accommodate 40% text expansion without truncation, and nothing in the UI layer precludes a future right-to-left locale.
>
> Plugins ship string tables in their manifest for name, description, panel labels, action labels, and settings; the host selects strings by the active locale and falls back to the plugin's default language. Localization contributions for the host follow the repository's contribution process, with a maintained list of locales and their completeness shown in About.
>
> Acceptance: when the system locale is pt-BR on first launch, the disclosure appears in Portuguese and the acknowledgement records the Portuguese text version. When the user switches to Portuguese while a community plugin with a pt-BR string table is enabled, its panel labels change without a reload; a plugin without pt-BR strings keeps its default language. When solfège is selected and Key & Tempo is at −2 semitones on a track in D, the panel reads "ré → dó". When a translated label is 40% longer than English, the performance layout still shows it in full.

## Scope boundary

Does not cover additional locales beyond English and Portuguese (Brazil), or right-to-left layout.

## Open questions

- A-13: Portuguese (Brazil) as first locale and solfège naming are assumptions to confirm.
