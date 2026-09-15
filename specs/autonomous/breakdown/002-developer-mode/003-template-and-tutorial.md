# 002-developer-mode / 003 — Plugin Template, Bundled Tutorial, and Source Viewing

**Source:** [§ 12 Developer Mode and plugin tooling](../../ModPlayer-Software-Specification.md#12-developer-mode-and-plugin-tooling) (FR-12.1.1 template action, 12.1.6, 12.1.8), [J-5 — Write and publish a plugin (S-4)](../../ModPlayer-Software-Specification.md#j-5--write-and-publish-a-plugin-s-4) (step 2), [Part 5 § 9 Versioning and compatibility](../../ModPlayer-Software-Specification.md#9-versioning-and-compatibility) (PL-9.5 generated reference), [GOV § 7 Community and support](../../ModPlayer-Software-Specification.md#7-community-and-support) (GOV-7.2, 7.3), [NFR § 10 Maintainability and quality](../../ModPlayer-Software-Specification.md#10-maintainability-and-quality) (NFR-10.2), [§ 6 Success metrics](../../ModPlayer-Software-Specification.md#6-success-metrics) ("hello loop" under 30 minutes), [§ 1 Primary personas](../../ModPlayer-Software-Specification.md#1-primary-personas) (Dani)

**Prerequisites:** Assumes Developer Mode from 002-developer-mode/001-dev-folder-and-hot-reload and Section Loop from 001-mvp/012-section-loop-plugin.

## Prompt

> Get a first-time plugin author from "Create plugin" to a working, hot-reloaded "hello loop" in under thirty minutes using only what ships in the client.
>
> Developer Mode offers "Create plugin from template". It asks for a name and a reverse-domain identifier, then creates a folder in the development folder containing a valid manifest (name, version 0.1.0, the current API version range, empty permission lists with a comment explaining justifications, no network hosts), a minimal entry script that calls `ready()` and logs a message, and a readme. The new plugin appears immediately in the plugin list as "Local · Dev".
>
> A bundled tutorial, reachable from Getting Started, Settings → Developer, and the template's readme, walks through building a section-loop plugin end to end: declaring permissions, registering a panel and shortcuts, creating markers, requesting transport focus, arming a loop, persisting per-track state, and handling `permission_denied` and `no_focus`. The tutorial's finished plugin is itself installable from within the tutorial and its source is viewable inside the client. Every bundled plugin's detail page has a "View source" action, so Section Loop and Key & Tempo serve as worked examples.
>
> The API reference the tutorial links to is generated from the same definition the host uses to validate manifests and calls, versioned alongside the API, so documentation and behavior cannot diverge.
>
> Acceptance: when a new author chooses the template and saves it unchanged, the plugin loads, calls `ready()`, and its log line appears in the console. When the author follows the tutorial to the "arm loop" step, pressing the tutorial's default shortcut loops the current track. When the author opens Section Loop's detail page, the full source is readable and copyable. When the plugin API version changes, the generated reference shows the new version and marks deprecated capabilities.

## Scope boundary

Does not cover packaging or publishing to the registry, which lands with the community-registry wave.
