# 011-browse-and-manage-polish / 006 — Onboarding and Sign-In Clarity

**Source:** [§ 3.1 First launch](../../ModPlayer-UI-UX-Review.md#31-first-launch) (UX-07, UX-08, UX-09, UX-10), [§ 3.2 Sign-in](../../ModPlayer-UI-UX-Review.md#32-sign-in) (UX-11, UX-12), [§ 3.10 States reviewed from source](../../ModPlayer-UI-UX-Review.md#310-states-reviewed-from-source-not-captured), [§ 4.5 Feedback and affordance](../../ModPlayer-UI-UX-Review.md#45-feedback-and-affordance)

**Prerequisites:** Assumes the disclosure, sign-in and device check from 001-mvp/002-first-launch-and-sign-in and 001-mvp/001-walking-skeleton, the gate presentation from 009-window-and-shell/003-shell-navigation-and-gates, and the shortcut sheet from 011-browse-and-manage-polish/005.

## Prompt

> Make the first five minutes of ModPlayer read like a guided setup instead of four screens of undifferentiated paragraphs.

> The welcome screen gains a hierarchy: a heading, one short lead paragraph that says what the app does, and the legal and Premium facts below it as a compact block rather than four competing paragraphs. Its body text is held to a readable measure instead of running the full window width, "I understand, continue" is the single primary action with "Decline" beside it as an ordinary one, and the privacy notice opens as a step whose back control names where it returns to.

> Sign-in gains progress and error clarity. While authorization is pending, the screen shows an activity indicator, says which browser page should have opened and how long it has been waiting, and offers reopening the page as the primary action with cancel beside it. The line explaining why sign-in is being asked for again — an unfinished previous attempt, an unreadable credential store, an account without Premium — is styled as a status message distinct from the instructions, so the reason is legible at a glance.

> The device check gains a way back to the previous step, a label and a short explanation for the buffer-size choice, and keeps its test tone and its three answers. The getting-started card, once the user reaches the library, becomes a short scannable card: one line per bundled plugin, a link to the shortcut sheet instead of inlined prose, and a way to bring the card back after it has been dismissed.

> Acceptance: when the welcome screen is shown at the minimum window size, no paragraph exceeds a readable measure and exactly one action is styled as primary. When authorization is pending, an activity indicator and an elapsed hint are visible. When the credential store cannot be read, the reason appears as a distinct status message above the retry action. When the device check is shown, a back control returns to the previous step and the buffer choice carries an explanation. When the getting-started card has been dismissed, it can be shown again from settings.

## Scope boundary

Covers the presentation, copy and feedback of the launch flow; the authorization protocol, disclosure versioning and Premium check remain with 001-mvp/002-first-launch-and-sign-in, and translation of the new strings with 007-operations-and-polish/003-localization-pt-br.
