# 002-developer-mode / 001 — Development Folder and Hot Reload

**Source:** [§ 12 Developer Mode and plugin tooling](../../ModPlayer-Software-Specification.md#12-developer-mode-and-plugin-tooling) (FR-12.1.1–12.1.3), [Part 5 § 3 Lifecycle](../../ModPlayer-Software-Specification.md#3-lifecycle) (PL-3.5), [J-5 — Write and publish a plugin (S-4)](../../ModPlayer-Software-Specification.md#j-5--write-and-publish-a-plugin-s-4) (steps 1, 3, 4), [FR-7.1 Sources and installation](../../ModPlayer-Software-Specification.md#fr-71-sources-and-installation) (dev source), [EC § 6 Plugins](../../ModPlayer-Software-Specification.md#6-plugins) (EC-6.16, 6.17), [NFR § 1 Performance and latency](../../ModPlayer-Software-Specification.md#1-performance-and-latency) (NFR-1.11), [§ 1 Primary personas](../../ModPlayer-Software-Specification.md#1-primary-personas) (Dani), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-4, JTBD-9)

**Prerequisites:** Assumes the plugin runtime from 001-mvp/009-plugin-runtime-and-permissions and UI contributions from 001-mvp/011-plugin-ui-contributions.

## Prompt

> Let a hobbyist developer write a plugin in an evening and see each edit in the player within a second, without restarting or losing their place in the song.
>
> Developer Mode is a toggle under Settings → Plugins → Developer. Turning it on reveals the path of a local development folder and the current plugin API version. Every subfolder of that folder containing a valid manifest is loaded as a plugin labeled "Local · Dev" in the plugin list. Developer plugins go through the same permission model as any other: when a manifest's permission set changes on save, the host prompts the developer to approve the new permissions once, so authors see exactly what users will see.
>
> Saving any file in a dev plugin's folder triggers a hot reload within one second. The reload re-runs the plugin lifecycle from Loaded while preserving the current track, playback position, play state, markers, and effect chain; only the plugin's own in-memory state resets, unless the plugin implements `serialize_state()` / `restore_state()` to hand its state across the reload. If the plugin held transport focus, it keeps focus when it re-requests it during `ready()`, otherwise focus returns to the host. A failed reload from a syntax or manifest error keeps the last good version running and reports the error with a file and line reference.
>
> A dev-folder plugin whose identifier matches an installed plugin shadows the installed one while Developer Mode is on and is labeled "Local · Dev (shadowing installed vX)". Hot reload is suppressed in Performance Mode once that mode exists.
>
> Acceptance: when the developer saves a script change while a track is playing at 1:45 with a loop armed, the plugin reloads within one second, audio never interrupts, the playhead is still at the same position, and the loop stays armed. When the saved script has a syntax error, the previous version keeps running and the error names the file and line. When the developer adds `transport.control` to the manifest and saves, a permission prompt appears for that permission only. When Developer Mode is turned off, dev plugins unload and any shadowed installed plugin returns.

## Scope boundary

Does not cover the plugin console, condition simulation, the template, the tutorial, or packaging for the registry.
