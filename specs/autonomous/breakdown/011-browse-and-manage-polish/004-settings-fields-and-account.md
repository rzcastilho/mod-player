# 011-browse-and-manage-polish / 004 — Settings Fields, Placeholders, and Account Summary

**Source:** [§ 3.7 Settings](../../ModPlayer-UI-UX-Review.md#37-settings) (UX-33, UX-34, UX-35, UX-36), [§ 5.4 Component rules](../../ModPlayer-UI-UX-Review.md#54-component-rules), [§ 4.4 Typography and text](../../ModPlayer-UI-UX-Review.md#44-typography-and-text)

**Prerequisites:** Assumes the settings screen from 001-mvp/001-walking-skeleton and the category row from 009-window-and-shell/003-shell-navigation-and-gates; uses the card, row and button components from wave 008.

## Prompt

> Make a settings category readable as a set of decisions rather than a stack of equal lines. Each field currently renders as a label, a help sentence and a control at the same weight, so the five audio settings read as ten identical lines, and sliders show a bare number with no unit — a ceiling of "-1.0" and a volume of "50" with nothing saying decibels or per cent.

> Group related fields into cards with a small group header, indent the help text under its label, and put every unit inside its control so a value is self-describing. Numeric fields show their range, and a field that has been changed from its default offers a per-field reset. The settings search keeps its "category › field" results and now highlights the matched field when the user jumps to it.

> Fix the two categories that ship as placeholders — offline, and privacy and diagnostics — so they either carry their real settings or state plainly that the area is not available yet, with their entries marked in the category row rather than looking identical to complete categories. The account category leads with what matters: the signed-in identity, the subscription tier, and when it was last verified, presented as a summary rather than three equal lines, with the re-check and sign-out actions beneath it. The sign-out confirmation widens enough to show its full consequence list at the minimum window size, keeps cancel as the default action, and keeps sign-out styled as destructive.

> Acceptance: when the audio category is shown, its fields are grouped under headers and every slider's value carries its unit. When a field differs from its default, a reset control for that field is available. When the offline category is opened, it either shows settings or states that it is not available yet, and the category row reflects that. When sign-out is confirmed at the minimum window width, the list of what will be deleted is fully visible before the user commits.

## Scope boundary

Covers the presentation of settings fields, the placeholder categories and the account summary; the shortcut reference is 011-browse-and-manage-polish/005, and the settings that offline and diagnostics will eventually hold belong to 003-offline-and-library/001 and 007-operations-and-polish/001.
