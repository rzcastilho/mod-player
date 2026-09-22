# 008-design-foundation / 004 — High-Contrast Appearance Option

**Source:** [§ 4.2 Measured contrast](../../ModPlayer-UI-UX-Review.md#42-measured-contrast-wcag-2x-sampled-from-the-captures) (UX-39), [§ 4.6 Accessibility versus the spec](../../ModPlayer-UI-UX-Review.md#46-accessibility-versus-the-spec), [§ 5.3 Semantic colour](../../ModPlayer-UI-UX-Review.md#53-semantic-colour-both-themes-contrast-verified), [§ 3.7 Settings](../../ModPlayer-UI-UX-Review.md#37-settings)

**Prerequisites:** Requires the semantic colour roles from 008-design-foundation/001-design-tokens-and-type-scale. Extends the accessibility work planned in 007-operations-and-polish/004-accessibility-hardening.

## Prompt

> Offer a high-contrast appearance for users who cannot read the default palette, satisfying the product's accessibility requirement that such an option exists alongside light and dark.

> Add a high-contrast choice to the appearance settings, orthogonal to the light/dark/system selection, so a user can run high-contrast light or high-contrast dark. In this mode secondary text is promoted to the primary text colour, every border and divider reaches at least a 3:1 ratio against its surface, focus rings thicken, and the accent, positive, warning and danger roles are replaced with variants that reach at least 7:1 against their surface. Elements whose colour is data rather than decoration — marker colours, plugin overlay colours — keep their hue but gain an outline so they stay distinguishable, since a marker's colour is meant to survive a theme change.

> The choice persists across restarts alongside the existing theme preference, applies live without relaunching, and reaches plugin-contributed panels and overlays through the same tokens the host uses, so a plugin panel never becomes the one unreadable area of the window.

> Acceptance: when high contrast is enabled, every text sample in every view measures at least 7:1 against its background. When a track has markers placed and high contrast is enabled, each marker remains individually identifiable on the waveform. When high contrast is enabled and a plugin panel is docked, that panel's text and controls follow the same palette. When the app restarts, the high-contrast choice and the light/dark choice are both restored.

## Scope boundary

Covers the palette variant and its setting only; keyboard operability, accessible names and the 200 % text-scale requirement remain with 007-operations-and-polish/004-accessibility-hardening.

## Open questions

- Whether high contrast should also follow an operating-system high-contrast preference automatically, or stay a manual choice.
