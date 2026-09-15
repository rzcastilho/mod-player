# 002-developer-mode / 002 — Plugin Console and Condition Simulation

**Source:** [§ 12 Developer Mode and plugin tooling](../../ModPlayer-Software-Specification.md#12-developer-mode-and-plugin-tooling) (FR-12.1.4, 12.1.7), [Part 5 § 8 Sandboxing and budgets](../../ModPlayer-Software-Specification.md#8-sandboxing-and-budgets) (PL-8.2), [Part 5 § 9 Versioning and compatibility](../../ModPlayer-Software-Specification.md#9-versioning-and-compatibility) (PL-9.3 deprecation warnings), [DM-25 LogEntry](../../ModPlayer-Software-Specification.md#dm-25-logentry), [J-5 — Write and publish a plugin (S-4)](../../ModPlayer-Software-Specification.md#j-5--write-and-publish-a-plugin-s-4) (steps 5–6), [EC § 6 Plugins](../../ModPlayer-Software-Specification.md#6-plugins) (EC-6.4, 6.7), [NFR § 8 Observability (local)](../../ModPlayer-Software-Specification.md#8-observability-local) (NFR-8.1)

**Prerequisites:** Assumes Developer Mode from 002-developer-mode/001-dev-folder-and-hot-reload.

## Prompt

> Give a plugin author a window into what their plugin is doing and a way to provoke the failure paths they cannot easily reproduce, so plugins are debugged in minutes rather than by guesswork.
>
> The plugin console is a Developer Mode view that shows, per plugin and filterable by plugin and level: structured log output the plugin emits, errors with stack traces, host lifecycle events (loaded, ready, suspended, restarted, unloading), every permission check with its outcome (granted or denied), CPU and memory usage over time against the plugin's budgets, and every effect-parameter change the plugin issued. Deprecated API usage appears as a console warning naming the capability and its removal window. Logs follow the host's structured local logging with levels, per-plugin sources, and rotation by size and age. An event handler that throws is aborted and logged while the plugin keeps running; repeated errors move its health to "warning". A write that exceeds the plugin's 10 MB storage cap fails with `budget_exceeded` and a console entry.
>
> A simulation panel lets the author impose conditions on a chosen plugin without changing the machine's real state: offline (the plugin receives `connectivity_changed` and network requests fail), slow network, denial of a specific permission (calls return `permission_denied` and `permission_changed` fires), analysis pending (analysis requests return `pending` until the author releases them, then `analysis_ready` fires), and transport focus loss (`focus_revoked`). Each simulation is visibly flagged in the plugin list while active and cleared when Developer Mode turns off.
>
> Acceptance: when a plugin calls a transport request without focus, the console shows the request, the `no_focus` result, and the timestamp within the same second. When the author enables "simulate permission denied: network" for a plugin, its next request fails with `permission_denied` and the usage entry is marked simulated. When a plugin's handler runs 6 ms against a 4 ms budget, the console shows the aborted handler and the measured time. When the console is filtered to one plugin, host lifecycle events for other plugins are hidden.

## Scope boundary

Does not cover user-facing diagnostics bundles or crash reports — only the developer-facing console and simulation.
