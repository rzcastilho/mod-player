# 010-now-playing-workbench / 004 — Effect Chain Rows and Meters

**Source:** [§ 3.5 Now Playing](../../ModPlayer-UI-UX-Review.md#35-now-playing) (UX-25), [§ 5.4 Component rules](../../ModPlayer-UI-UX-Review.md#54-component-rules), [§ 4.4 Typography and text](../../ModPlayer-UI-UX-Review.md#44-typography-and-text)

**Prerequisites:** Assumes the effect chain from 001-mvp/008-effect-chain-and-built-in-nodes and the control variants and meter styling from 008-design-foundation/002-control-variants.

## Prompt

> Make an effect node's row readable as four things rather than one run-on line. A node currently renders as an unbroken sequence — position number, name, source, bypass, CPU figure, an unlabelled checkbox, a value, a unit, two mode words, a dropdown and a remove action — with nothing separating what the node *is* from what it is *set to*.

> Restructure each row into four zones: identity on the left (drag handle, position, node name, and whether it came from the host or a plugin), state next (enabled, bypassed, and the node's share of the chain's CPU budget), the node's parameters in the middle with each value carrying its unit inside the control, and the node's actions on the right. Reordering gains a visible drag affordance in addition to the existing keyboard reordering. The chain header keeps the total CPU load and overload count, and labels them as the budget figures they are.

> Give the chain's meters a scale: the pre-chain and post-chain level pairs show peak and RMS with their numbers in tabular figures, and the spectrum display gains frequency ticks and an amplitude reference so it reads as a measurement rather than decoration. When the chain is empty, the panel says what an effect node does and offers the add control as its primary action instead of showing a bare dropdown.

> Acceptance: when two nodes are in the chain, each row shows its identity, state, parameters and actions as four distinct groups. When a pitch-shift node is set to two semitones, the unit appears with the value in the control. When a node is dragged onto a new position, the chain reorders and the position numbers update. When the chain is empty, the panel shows an explanatory empty state with a single primary action.

## Scope boundary

Covers the presentation of the chain, its rows and its meters; node behaviour, budget enforcement and the custom buffer processor remain with 001-mvp/008-effect-chain-and-built-in-nodes and 005-community-registry/006-custom-buffer-processor.
