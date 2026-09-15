# 005-community-registry / 005 — Plugin Updates, Sideloading, Extra Registries, and Delisting

**Source:** [FR-7.1 Sources and installation](../../ModPlayer-Software-Specification.md#fr-71-sources-and-installation) (FR-7.1.4), [FR-7.3 Lifecycle](../../ModPlayer-Software-Specification.md#fr-73-lifecycle) (FR-7.3.3), [FR-7.2 Permissions](../../ModPlayer-Software-Specification.md#fr-72-permissions) (FR-7.2.2 updates, 7.2.3 sideload wildcard), [INT-4 Plugin registry](../../ModPlayer-Software-Specification.md#int-4-plugin-registry) (INT-4.4, 4.6), [GOV § 4 Registry governance](../../ModPlayer-Software-Specification.md#4-registry-governance) (GOV-4.5), [Part 5 § 9 Versioning and compatibility](../../ModPlayer-Software-Specification.md#9-versioning-and-compatibility) (PL-9.2), [EC § 6 Plugins](../../ModPlayer-Software-Specification.md#6-plugins) (EC-6.2, 6.12, 6.15), [DM-10 Plugin](../../ModPlayer-Software-Specification.md#dm-10-plugin) (source, health)

**Prerequisites:** Assumes registry install from 005-community-registry/001-registry-browse-and-install and permission management from 005-community-registry/002-permission-management-and-usage-log.

## Prompt

> Keep installed community plugins current and safe over time, and let power users install from outside the main registry without losing the safeguards.
>
> Plugin updates from the registry are checked on launch and daily, outside Performance Mode, and shown as available on the plugin list and detail page. An update applies only on user action, unless the user opts into auto-update per plugin. When an update requests new permissions, it downloads but does not apply until the user approves a sheet listing only the new permissions. A plugin whose declared API version range no longer includes the host's API major is disabled with "Incompatible — needs API vX" and a registry update offered if one exists.
>
> The user can install from a local package file; such plugins are labeled "sideloaded", always show the full permission sheet, and may declare a wildcard network host only after a strong warning. Users can add additional registry sources (private or organizational) with the same signature and digest requirements; plugins from them are labeled with their registry.
>
> When the main index marks an installed plugin as removed, the plugin shows "removed from registry" with the stated reason category. If the reason is security, the plugin is disabled immediately, pending the user's explicit confirmation to re-enable.
>
> Acceptance: when an installed plugin's new version adds `library.read`, the update waits and the approval sheet shows that one permission; declining leaves the old version running. When the user sideloads a package declaring a wildcard host, the sheet shows a strong warning before the user can proceed. When an added registry's index fails signature verification, it is refused and its plugins are not listed. When a plugin is delisted for security while the user is playing, it is disabled at once, audio continues, and a warning names it with the reason.

## Scope boundary

Does not cover client updates or the plugin API compatibility layer for major version changes.
