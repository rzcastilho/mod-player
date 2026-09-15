# 005-community-registry / 002 — Runtime Permission Management, Usage Log, and Uninstall

**Source:** [FR-7.2 Permissions](../../ModPlayer-Software-Specification.md#fr-72-permissions) (FR-7.2.4, 7.2.5), [FR-7.3 Lifecycle](../../ModPlayer-Software-Specification.md#fr-73-lifecycle) (FR-7.3.2), [Part 5 § 4 Permission catalog](../../ModPlayer-Software-Specification.md#4-permission-catalog) (PL-4.5), [DM-11 PermissionGrant](../../ModPlayer-Software-Specification.md#dm-11-permissiongrant), [DM-13 UsageLogEntry](../../ModPlayer-Software-Specification.md#dm-13-usagelogentry), [J-6 — Install a community plugin and manage its permissions (S-5)](../../ModPlayer-Software-Specification.md#j-6--install-a-community-plugin-and-manage-its-permissions-s-5) (steps 5–7), [EC § 6 Plugins](../../ModPlayer-Software-Specification.md#6-plugins) (EC-6.13, 6.14), [NFR § 5 Privacy](../../ModPlayer-Software-Specification.md#5-privacy) (NFR-5.3), [§ 10 Retention summary](../../ModPlayer-Software-Specification.md#10-retention-summary), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-8)

**Prerequisites:** Assumes registry install from 005-community-registry/001-registry-browse-and-install.

## Prompt

> Let a user look at what a plugin has actually been doing with the access they granted, change their mind about any permission at any time, and remove the plugin cleanly.
>
> Settings → Plugins → <plugin> → Permissions lists every permission in the catalog with its state for that plugin (granted, denied, not requested), when and how it was granted (at install or changed later), and for network the declared hosts. The user can grant or revoke any permission; the change takes effect immediately, the plugin receives `permission_changed`, and subsequent calls fail cleanly with `permission_denied`. The host provides fallback text for a plugin panel that does not handle the loss, such as "Network access is off for this plugin".
>
> For sensitive permissions — network and file access — the host keeps a per-plugin usage log showing time, target (host, or a file handle label), byte volume, and outcome (allowed or denied), never content. A request to an undeclared host is denied, logged, and counted as a policy violation visible in the log. Entries are retained 30 days, rotated by size, and stay local.
>
> Uninstall removes the plugin, its UI, its bindings, and its effect nodes, and asks whether to also delete its stored plugin-scoped and per-track data; the default keeps that data for 30 days and then purges it. Bundled plugins can be disabled but not uninstalled.
>
> Acceptance: when the user revokes network access from Chord Chart, the plugin's next fetch fails with `permission_denied` and its panel shows the fallback text. When a plugin requests `https://other.example` while declared for `chords.example` only, the request is denied and appears in the usage log marked denied. When a request is in flight during revocation, it completes or is cancelled and no later request succeeds. When the user uninstalls and chooses "keep data", reinstalling within 30 days restores the plugin's settings.

## Scope boundary

Does not cover the network proxy's request execution, rate limits, or transport security — only the grant model, usage log, and uninstall.
