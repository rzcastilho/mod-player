# 007-operations-and-polish / 004 — Accessibility Hardening

**Source:** [NFR § 6 Accessibility](../../ModPlayer-Software-Specification.md#6-accessibility), [Part 5 § 7 UI contribution rules](../../ModPlayer-Software-Specification.md#7-ui-contribution-rules) (PL-7.2), [§ 3 Edge users](../../ModPlayer-Software-Specification.md#3-edge-users) (screen-reader user, hearing-protection-conscious user), [FR-14.4 Appearance and language](../../ModPlayer-Software-Specification.md#fr-144-appearance-and-language) (themes), [GOV § 3 Contribution process](../../ModPlayer-Software-Specification.md#3-contribution-process) (PR template accessibility impact)

**Prerequisites:** Assumes the full MVP UI through 001-mvp/013-key-and-tempo-plugin and the performance layout from 004-performance/004-detachable-and-full-screen-layouts.

## Prompt

> Make ModPlayer fully usable by a screen-reader user and safe for someone protecting their hearing, and lock that in so later features cannot regress it.
>
> Every host function — including waveform seeking, marker placement via nudge actions, effect-chain reordering, plugin management, and the registry browser — is operable by keyboard alone with a logical focus order. All host UI elements expose accessible names, roles, and states to platform assistive technologies; plugin widgets inherit this from the host widget set, and registration without a label remains refused. Color is never the only carrier of meaning: markers have names and shapes, plugin health states have icons and text, the offline indicator has a label. Contrast meets recognized guideline minimums in both light and dark themes, and a high-contrast option is added under Appearance. Text scales with the platform text-size setting up to 200% without loss of function in any view.
>
> The host never flashes content above 3 Hz; a plugin that declares the flashing-content flag shows a warning at install, and a plugin that flashes without declaring it is a registry policy violation. The output limiter cannot be bypassed and "safe volume on startup" remains available, so a resonant filter or gain node cannot produce a dangerous transient.
>
> Automated accessibility checks run in continuous integration against every view, and the pull request template asks about accessibility impact.
>
> Acceptance: when a screen-reader user tabs through the now-playing view, they hear the playhead position, each marker's name and position, and the loop state, and can move marker A by 10 ms with the nudge shortcut. When the user switches to high contrast, every text element meets the contrast minimum and the waveform's markers remain distinguishable. When a visualizer plugin without the flashing flag is submitted to the registry, the automated check rejects it. When text size is 200%, the registry browser's detail page scrolls rather than truncating.

## Scope boundary

Does not cover localization or theme design beyond contrast and the high-contrast option.
