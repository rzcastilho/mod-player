# 011-browse-and-manage-polish / 005 — Keyboard Shortcut Reference

**Source:** [§ 3.7 Settings](../../ModPlayer-UI-UX-Review.md#37-settings) (UX-32), [§ 3.1 First launch](../../ModPlayer-UI-UX-Review.md#31-first-launch) (UX-10), [§ 4.4 Typography and text](../../ModPlayer-UI-UX-Review.md#44-typography-and-text), [§ 4.6 Accessibility versus the spec](../../ModPlayer-UI-UX-Review.md#46-accessibility-versus-the-spec)

**Prerequisites:** Assumes the action catalog and rebinding from 001-mvp/007-keyboard-actions-and-shortcuts; uses the card and row components from wave 008. Related: MIDI mappings from 004-performance/001-midi-mapping-and-profiles share the same action catalog.

## Prompt

> Turn the app's only shortcut reference into something a musician can actually read. The controls settings page lists every action on one line each — name, trigger, current binding, an add-binding control and a per-action reset — roughly sixty rows deep, and the key symbols for space, digits and arrows render as empty boxes, so the binding a user most wants to check is the one they cannot read.

> Render every key as its platform name or symbol — Space, ⌘, ⇧, ←, the digits — so a binding is legible at a glance, and lay the page out in two columns grouped by the action categories the catalog already defines: transport, markers, loop, cues, navigation, panels. Each row shows its action name, its bindings as key chips, and its per-action controls revealed on hover or focus rather than occupying width permanently. The existing filter stays and now matches on key names as well as action names, so a user can ask what the L key does.

> Add a read-only overview the user can reach without opening settings: a shortcut sheet, opened from a menu item and from the getting-started card, that lists the same grouped bindings for reading and printing, and that reflects any rebinding immediately. The getting-started card stops inlining its shortcuts as prose and links to the sheet instead, and the sheet remains reachable after the card has been dismissed.

> Acceptance: when the controls page is shown, the play/pause binding reads "Space" rather than an empty box. When the filter is given a key name, the actions bound to that key are listed. When a binding is changed and the shortcut sheet is opened, the sheet shows the new binding. When the getting-started card has been dismissed, the shortcut sheet is still reachable from the application menu.

## Scope boundary

Covers reading and presenting bindings; the binding model, conflict detection and capture behaviour remain with 001-mvp/007-keyboard-actions-and-shortcuts.
