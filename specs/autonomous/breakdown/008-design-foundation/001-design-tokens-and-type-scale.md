# 008-design-foundation / 001 — Design Tokens, Type Scale, and Spacing

**Source:** [§ 4.1 No design tokens](../../ModPlayer-UI-UX-Review.md#41-no-design-tokens), [§ 4.2 Measured contrast](../../ModPlayer-UI-UX-Review.md#42-measured-contrast-wcag-2x-sampled-from-the-captures), [§ 4.4 Typography and text](../../ModPlayer-UI-UX-Review.md#44-typography-and-text), [§ 5.1 Type scale](../../ModPlayer-UI-UX-Review.md#51-type-scale-single-family-platform-default-textstyle-overrides), [§ 5.2 Spacing and radius](../../ModPlayer-UI-UX-Review.md#52-spacing-and-radius), [§ 5.3 Semantic colour](../../ModPlayer-UI-UX-Review.md#53-semantic-colour-both-themes-contrast-verified), [§ 3.9 Cross-theme](../../ModPlayer-UI-UX-Review.md#39-cross-theme)

**Prerequisites:** Assumes the full MVP UI through 001-mvp/013-key-and-tempo-plugin. Every wave from 008 onwards builds on the tokens defined here.

## Prompt

> Give ModPlayer a single source of visual truth, so the app stops looking like an unstyled prototype and no screen can invent its own colour or size again.

> Define a named set of design tokens covering typography, spacing, radius and colour, applied once per frame to the whole application rather than per widget. Typography is six roles on the platform's default family: a display role for the now-playing track title and the welcome heading, a title role for screen headings, a small uppercase section role for panel and group headers, a body role for row titles and field labels, a muted secondary role for artist/album lines, help text and metadata, and a tabular monospace role for every number a user compares — timestamps such as `0:35.204`, decibel readings, CPU and memory figures, and durations. Spacing is a 4-pixel scale (4, 8, 12, 16, 24, 32) and radius has three steps; panels are separated by space rather than hairline rules, and body copy is capped at roughly 72 characters so paragraphs stop running the full window width.

> Colour becomes semantic — primary, secondary and disabled text, a base and a raised surface, an accent, and positive, warning and danger roles — with a light and a dark value for every role. Secondary text must reach at least a 4.5:1 contrast ratio against its surface in both themes; today the light theme measures 2.96:1 on the artist, album, duration and owner lines, which fails the product's stated accessibility minimum. Every colour and font-size literal currently scattered through the views — plugin health dots, the over-ceiling meter red, opaque white marker labels, ad-hoc 9-point monospace — moves into the token layer, restoring the project's existing rule that no view names a colour.

> Acceptance: when the app runs in the light theme and a secondary text sample is measured against its background, the contrast ratio is at least 4.5:1. When a list row is displayed, its title and its secondary line render at visibly different size and weight. When the source tree is searched for colour literals outside the theme module, nothing is found. When a marker's timestamp and a plugin's CPU figure are shown in adjacent rows, their digits align in a fixed-width column.

## Scope boundary

Defines and applies the tokens; it does not restructure any layout, change any component's behaviour, or add the high-contrast variant (008-design-foundation/004).

## Open questions

- Whether a custom font family is in scope, or the type scale stays on each platform's default family (source § 7, open question 1). The slice assumes the platform default.
