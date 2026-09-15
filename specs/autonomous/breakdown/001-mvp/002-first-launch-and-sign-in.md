# 001-mvp / 002 — First Launch Disclosure and Sign-In

**Source:** [J-1 — First launch and sign-in](../../ModPlayer-Software-Specification.md#j-1--first-launch-and-sign-in), [FR-1.1 First launch](../../ModPlayer-Software-Specification.md#fr-11-first-launch), [FR-1.2 Sign-in](../../ModPlayer-Software-Specification.md#fr-12-sign-in), [INT-1 Streaming service — authorization](../../ModPlayer-Software-Specification.md#int-1-streaming-service--authorization), [DM-1 AccountSession](../../ModPlayer-Software-Specification.md#dm-1-accountsession-account-scoped), [DM-27 DisclosureAcknowledgement](../../ModPlayer-Software-Specification.md#dm-27-disclosureacknowledgement), [EC § 1 Account and session](../../ModPlayer-Software-Specification.md#1-account-and-session), [NFR § 4 Security](../../ModPlayer-Software-Specification.md#4-security), [NFR § 5 Privacy](../../ModPlayer-Software-Specification.md#5-privacy), [NFR § 11 Legal and compliance](../../ModPlayer-Software-Specification.md#11-legal-and-compliance), [GOV § 6 Legal posture and takedown response](../../ModPlayer-Software-Specification.md#6-legal-posture-and-takedown-response)

**Prerequisites:** Assumes the app shell and settings screen from 001-mvp/001-walking-skeleton.

## Prompt

> Let a new user go from a fresh install to a signed-in account, with the legal position stated plainly before anything else happens. ModPlayer connects to the user's Spotify Premium account through an unofficial receiver protocol; the user must understand that before they sign in.
>
> On first launch the app shows a welcome screen with a one-paragraph description of the product, a plain-language disclosure that the client connects to the streaming service in an unofficial way, that a Premium subscription is required, and that the user's account is subject to the service's terms, plus an explicit "I understand, continue" action. Declining closes the app with a short explanation. The acknowledgement is recorded locally together with the version of the disclosure text, and the disclosure is shown again whenever that text changes in a later release. A plain-language privacy notice is reachable from the welcome screen and from About. The product name, icon, and branding never use the streaming service's trademarks.
>
> Sign-in uses the service's own browser-based authorization flow opened in the system default browser; the app never presents a password field. The resulting session credential is stored only in the operating system's secure credential store. After authorization the app shows a "checking your account" state, then verifies the tier: a Premium account proceeds to the audio device check; a non-Premium account sees an explanation with a link to the service's upgrade page and can stay signed in to browse but not play. Only one account is signed in at a time. While online the credential is refreshed before expiry; if refresh keeps failing, a non-blocking notice offers "sign in again" and playback continues on the existing credential until it expires.
>
> Sign-out clears the session credential, the offline cache, and all account-scoped state within seconds, after a confirmation that lists what is deleted; plugin installs and non-account settings remain.
>
> Acceptance: when the user cancels the browser authorization or it times out, the app returns to the sign-in step with a retry action and no partial state. When the OS secure store is locked or unavailable, sign-in is refused with platform-specific guidance and never falls back to plain-file storage. When authorization completes in the browser after the app was closed, the next launch picks up the result if still valid, otherwise restarts sign-in. When the service revokes the session remotely, the app shows a critical notice, clears the credential, and offers sign-in.

## Scope boundary

Does not cover the Connect receiver, playback, library browsing, or the offline grace period — only disclosure, authorization, tier check, credential storage, and sign-out.

## Open questions

- A-2: the service's browser-based authorization must yield a credential a Connect receiver can use; a spike should confirm before this slice is planned.
- Q-5: whether the disclosure needs more than acknowledgement (waiting period, link to the service's terms).
- Q-4 / Q-16: legal review of the project's exposure and the "ModPlayer" name.
