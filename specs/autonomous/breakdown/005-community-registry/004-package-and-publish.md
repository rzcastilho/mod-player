# 005-community-registry / 004 — Package for Registry and Submission Pipeline

**Source:** [§ 12 Developer Mode and plugin tooling](../../ModPlayer-Software-Specification.md#12-developer-mode-and-plugin-tooling) (FR-12.1.5), [Part 5 § 10 Registry publishing contract](../../ModPlayer-Software-Specification.md#10-registry-publishing-contract), [Part 5 § 2 Plugin package](../../ModPlayer-Software-Specification.md#2-plugin-package) (PL-2.4), [INT-4 Plugin registry](../../ModPlayer-Software-Specification.md#int-4-plugin-registry) (outbound submissions), [GOV § 4 Registry governance](../../ModPlayer-Software-Specification.md#4-registry-governance) (GOV-4.1–4.4, 4.6, 4.7, 4.8), [GOV § 8 Success signals for governance](../../ModPlayer-Software-Specification.md#8-success-signals-for-governance), [J-5 — Write and publish a plugin (S-4)](../../ModPlayer-Software-Specification.md#j-5--write-and-publish-a-plugin-s-4) (steps 7–8), [§ 2 Secondary personas](../../ModPlayer-Software-Specification.md#2-secondary-personas) (registry maintainer), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-10)

**Prerequisites:** Assumes Developer Mode from 002-developer-mode/001-dev-folder-and-hot-reload and the registry index from 005-community-registry/001-registry-browse-and-install.

## Prompt

> Let a plugin author go from a working dev-folder plugin to a listed registry entry within a day, with most of the checks automated, and give registry maintainers a queue they can review and an audit trail.
>
> In Developer Mode, "Package for registry" validates the manifest against the current API version, checks that every permission carries a justification, refuses forbidden combinations (required `network`, wildcard hosts, permissions outside the catalog), runs the plugin in a clean sandbox to confirm it loads and calls `ready()`, and produces a package plus a submission descriptor: identifier, version, integrity digest, source link, and changelog, signed with the author's publishing key. Problems are listed with the manifest field or file involved.
>
> The registry is a signed, versioned static index in a public repository; submitting is a pull request carrying the descriptor, with the package hosted at the location the descriptor references. Automated checks on every submission: manifest validity, API version compatibility, permission policy, package size cap, digest match, signature validity, and identifier ownership (the first publisher owns an identifier; transfers need both parties' signatures or a documented maintainer decision). An identifier-plus-version that already exists is rejected. Plugins requesting High-risk permissions (`network`, `audio.process`) are held for human review of the justification, declared hosts, and source, which must be available; others list after automated checks pass. Listing and delisting decisions live in repository history with a reason category (security, policy, author request, abandonment), and users have a way to flag a plugin from its detail page.
>
> Acceptance: when an author packages a plugin whose manifest lists `network` under required permissions, packaging fails naming the field. When a valid non-High-risk submission passes checks and is merged, it appears in the in-app browser after the next index refresh. When a second author submits a package under an identifier owned by someone else, the check fails with identifier ownership. When a High-risk submission arrives, it enters the review queue and is not listed until a maintainer approves.

## Scope boundary

Does not cover the client-side update or delisting behavior, or the general contributor process for the host itself.

## Open questions

- Q-10: whether source availability should be required for all plugins or only High-risk ones.
- Q-17: whether a lightweight "reviewed" badge tier beyond High-risk review is worth adding.
