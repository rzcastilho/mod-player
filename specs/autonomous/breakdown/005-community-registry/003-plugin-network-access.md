# 005-community-registry / 003 — Plugin Network Access through the Host Proxy

**Source:** [INT-7 Plugin-declared network hosts](../../ModPlayer-Software-Specification.md#int-7-plugin-declared-network-hosts), [Part 5 § 6.8 Network](../../ModPlayer-Software-Specification.md#68-network-network), [Part 5 § 4 Permission catalog](../../ModPlayer-Software-Specification.md#4-permission-catalog) (PL-4.2, 4.3), [Part 5 § 2 Plugin package](../../ModPlayer-Software-Specification.md#2-plugin-package) (PL-2.3 network_hosts), [Part 5 § 8 Sandboxing and budgets](../../ModPlayer-Software-Specification.md#8-sandboxing-and-budgets) (network caps), [Part 9 § 3 Plugin subsystem](../../ModPlayer-Software-Specification.md#plugin-subsystem) (AR-18), [Part 9 § 6 Trust boundaries](../../ModPlayer-Software-Specification.md#6-trust-boundaries), [NFR § 4 Security](../../ModPlayer-Software-Specification.md#4-security) (NFR-4.6), [§ 5 Key scenarios](../../ModPlayer-Software-Specification.md#5-key-scenarios) (S-5), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-17)

**Prerequisites:** Assumes the permission model from 005-community-registry/002-permission-management-and-usage-log.

## Prompt

> Let a plugin such as a chord-chart or lyrics tool fetch data from the web, while guaranteeing it can only reach the hosts it declared, can never carry the user's streaming credential, and cannot exceed a budget the user can see.
>
> A plugin with the `network` permission calls `request(host, path, method, headers, body)`. The host's Network Proxy performs the request on the plugin's behalf: it enforces that the host is in the manifest's declared list (anything else returns `permission_denied` and is logged as a violation), permits only secure transport, strips any credential material the plugin might attempt to include for the streaming service, and never attaches the user's session credential. Responses are delivered to the plugin and never cached by the host. Every request is logged to the usage log with host, time, and byte counts.
>
> Per-plugin caps default to 5 MB per minute and 60 requests per minute; exceeding them returns `rate_limited` or `budget_exceeded` with the measured figures. A manifest may declare higher caps with a justification, shown at install for the user to accept. `network` is never a required permission — a manifest listing it as required is invalid — and a wildcard host is refused for registry plugins.
>
> Acceptance: when Chord Chart requests `chords.example/song/123` over secure transport, the proxy performs it, delivers the response, and logs the host and bytes. When the same plugin requests a plain-text (insecure) URL, the request is refused. When a plugin sets a header containing the user's session credential, the proxy strips it before sending. When a plugin makes its 61st request within a minute, it receives `rate_limited` and the usage log shows the refusal.

## Scope boundary

Does not cover file access, clipboard, or the registry's human review of High-risk plugins.
