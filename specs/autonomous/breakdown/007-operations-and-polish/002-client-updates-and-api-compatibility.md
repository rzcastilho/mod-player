# 007-operations-and-polish / 002 — Client Updates with Plugin API Compatibility

**Source:** [FR-14.3 Updates](../../ModPlayer-Software-Specification.md#fr-143-updates), [J-10 — Update the client with a plugin API change](../../ModPlayer-Software-Specification.md#j-10--update-the-client-with-a-plugin-api-change), [INT-5 Update channel](../../ModPlayer-Software-Specification.md#int-5-update-channel), [Part 5 § 9 Versioning and compatibility](../../ModPlayer-Software-Specification.md#9-versioning-and-compatibility), [GOV § 2 Plugin API stability](../../ModPlayer-Software-Specification.md#2-plugin-api-stability) (GOV-2.4, 2.6), [Part 9 § 3 Core services](../../ModPlayer-Software-Specification.md#core-services) (AR-15), [DM-10 Plugin](../../ModPlayer-Software-Specification.md#dm-10-plugin) (compatibility mode), [EC § 11 Updates](../../ModPlayer-Software-Specification.md#11-updates), [INT-2 Streaming service — Connect receiver protocol](../../ModPlayer-Software-Specification.md#int-2-streaming-service--connect-receiver-protocol) (INT-2.7 independent module), [NFR § 4 Security](../../ModPlayer-Software-Specification.md#4-security) (NFR-4.5), [§ 2 Secondary personas](../../ModPlayer-Software-Specification.md#2-secondary-personas) (project maintainer)

**Prerequisites:** Assumes Performance Mode suppression from 004-performance/003-performance-mode and plugin update handling from 005-community-registry/005-plugin-updates-sideload-and-delisting.

## Prompt

> Let a user update ModPlayer without losing their plugins, and let a protocol fix for the Audio Source ship without waiting for a full client release.
>
> The client checks a release manifest — version, changelog, plugin API version, download location, signature — on launch and daily, outside Performance Mode, and shows the changelog; the update installs only on user action. The user can choose the stable or pre-release channel. Every update is signature-verified before install; an unverifiable update is refused with a notice. A failed install rolls back to the previous version and reports it. The Audio Source module is versioned independently and updates through the same verified path, listed as a "playback module update".
>
> Before updating, the client evaluates each installed plugin's declared API version range against the new plugin API version and shows counts and names for compatible, needs update, and incompatible. After a minor API bump, compatible plugins load and those with a registry update available are offered it. After a major bump, plugins declaring only the previous major run on the host's compatibility layer, labeled "running in compatibility mode until updated", for at least one major host release cycle; plugins outside any supported range are disabled with an explanation and a link to their registry entry. Deprecated capabilities emit console warnings for at least six months before removal, and a public compatibility table lists supported plugin API versions per host version.
>
> Acceptance: when an update bumps the API from 1.4 to 1.5, the preview shows "compatible (6), needs update (1), incompatible (0)" and the one plugin gets an update offer afterwards. When an update bumps the API major from 1 to 2, a plugin declaring "1.x" loads in compatibility mode with the label visible in the plugin list. When the download's signature does not verify, install is refused and the previous version keeps running. When install fails halfway, the app relaunches on the previous version with "update failed, previous version restored".

## Scope boundary

Does not cover the plugin registry update flow or the governance process for approving API changes.

## Open questions

- A-11: the Audio Source module can be versioned and updated independently of the host.
