# 007-operations-and-polish / 001 — Diagnostic Bundles and Opt-In Crash Reporting

**Source:** [FR-14.2 Diagnostics](../../ModPlayer-Software-Specification.md#fr-142-diagnostics), [DM-25 LogEntry](../../ModPlayer-Software-Specification.md#dm-25-logentry), [DM-26 CrashReport](../../ModPlayer-Software-Specification.md#dm-26-crashreport), [INT-8 Crash and diagnostic reporting (optional, opt-in)](../../ModPlayer-Software-Specification.md#int-8-crash-and-diagnostic-reporting-optional-opt-in), [Part 9 § 3 Core services](../../ModPlayer-Software-Specification.md#core-services) (AR-14), [J-7 — A plugin misbehaves during practice (S-6)](../../ModPlayer-Software-Specification.md#j-7--a-plugin-misbehaves-during-practice-s-6) (step 6 report), [NFR § 8 Observability (local)](../../ModPlayer-Software-Specification.md#8-observability-local) (NFR-8.1, 8.4), [NFR § 5 Privacy](../../ModPlayer-Software-Specification.md#5-privacy) (NFR-5.1, 5.2), [GOV § 1 Licensing](../../ModPlayer-Software-Specification.md#1-licensing) (GOV-1.6 About screen), [§ 2 Secondary personas](../../ModPlayer-Software-Specification.md#2-secondary-personas) (project maintainer), [§ 10 Retention summary](../../ModPlayer-Software-Specification.md#10-retention-summary)

**Prerequisites:** Assumes structured logging from 002-developer-mode/002-plugin-console-and-simulation and the plugin health model from 001-mvp/009-plugin-runtime-and-permissions.

## Prompt

> Let a user hand maintainers what they need to fix a crash — and let maintainers see which plugin was involved — without the user ever giving up their credentials, listening history, or audio, and without anything leaving the machine unasked.
>
> Settings → Privacy & diagnostics offers "Generate diagnostic bundle": app version, OS, audio device and buffer settings, plugin list with versions and health, recent structured logs, and recent crash reports, assembled in under 10 seconds and under 20 MB. It excludes credentials, track history, search queries, and any audio. The user chooses where to save it.
>
> Crashes are captured locally as reports with timestamp, app version, OS, stack summary, the implicated plugin when one is involved, and an attached log excerpt, with a consent state of pending, sent, or discarded. Nothing is sent without explicit consent per report or an opt-in setting; every report is previewable before sending; the endpoint is operated by the project and named in the privacy notice. Pending reports are kept at most 90 days. When a plugin is auto-disabled after three suspensions, the user is offered an optional report pre-filled with that plugin's console output and no personal data. The About screen lists third-party components with their licenses and links to the privacy notice.
>
> The app sends no telemetry by default: the only outbound traffic is to the streaming service, anonymous registry and update fetches, plugin-declared hosts under permission, and this consent-gated reporter.
>
> Acceptance: when the user generates a bundle after a session with three plugins, the bundle lists them with health and contains no track titles. When the app crashes and is relaunched, a non-blocking notice offers to preview and send the report; declining discards or keeps it pending, never sends. When a crash occurred inside a plugin's handler, the report's implicated-plugin field names it. When opt-in reporting is on, reports send without a prompt but remain viewable in Settings with their sent state.

## Scope boundary

Does not cover the developer console, client updates, or security disclosure handling.

## Open questions

- Q-12: who operates the crash-reporting endpoint and under what retention.
