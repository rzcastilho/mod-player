# 008-design-foundation / 002 — Button, Toggle, and Meter Variants

**Source:** [§ 1 Summary](../../ModPlayer-UI-UX-Review.md#1-summary) (UX-04), [§ 4.5 Feedback and affordance](../../ModPlayer-UI-UX-Review.md#45-feedback-and-affordance), [§ 5.4 Component rules](../../ModPlayer-UI-UX-Review.md#54-component-rules), [§ 3.5 Now Playing](../../ModPlayer-UI-UX-Review.md#35-now-playing) (UX-22), [§ 3.6 Plugins](../../ModPlayer-UI-UX-Review.md#36-plugins) (UX-28)

**Prerequisites:** Builds on the tokens from 008-design-foundation/001-design-tokens-and-type-scale.

## Prompt

> Make the difference between "continue", "remove everything" and "show more" visible before the user clicks, and make a toggle look like a toggle.

> Introduce four button variants — a filled primary used at most once per view, a default, a quiet text-only variant for row-level actions, and a destructive variant that carries the danger colour and an outline. Apply them across the app: the welcome screen's "I understand, continue" becomes primary while "Decline" does not; "Clear all markers", "Remove", "Disable" and "Sign out" become destructive; per-row actions such as "Move up" or "Play next" become quiet. A destructive action never sits immediately beside its harmless neighbour without a spacer between them.

> Separate toggles from buttons. The Queue, Effects and Transport controls in the now-playing view, the plugin enable checkboxes, and the bypass controls in the effect chain all render as toggles with an unmistakable on state, rather than as labels that happen to highlight. Add the interaction feedback the app has never had: a hover fill on interactive rows and controls, a focus ring distinct from the selected state, and a pressed state. Level and peak meters pick up the positive, warning and danger colours with scale marks at −6 and 0 dB and monospace numerals, and plugin health takes its colour from the same roles while continuing to spell the state out in words.

> Acceptance: when a view containing a destructive action is displayed, that action is visually distinct from every other button in the view. When the pointer rests on a list row or a toggle, the surface changes. When focus moves by keyboard onto a control that is also selected, the focus ring and the selection remain separately identifiable. When the output level exceeds the limiter ceiling, the meter shows the danger colour and the numeric readout stays aligned with the readouts above it.

## Scope boundary

Styles and re-typed controls only; it does not move, group or re-lay-out any screen, and it does not change what any control does.
