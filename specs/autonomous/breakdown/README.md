# ModPlayer — Wave Breakdown

## Summary

ModPlayer is an open-source desktop music player that plays a user's Spotify Premium catalog by acting as an unofficial Connect receiver, decodes the audio in-client, and lets small sandboxed scripted plugins act on the music while it plays: loop a bar, transpose to a singer's key, slow a solo without changing pitch, mark cue points. Two bundled plugins (Section Loop, Key & Tempo) prove the plugin API; an offline cache, MIDI mapping, and Performance Mode serve DJs; Developer Mode and a community registry serve plugin authors. The product is desktop-only on three OSes, exports no audio, and runs no project server beyond a static registry index.

## Source documents

| Document | Waves | Covers |
|---|---|---|
| [ModPlayer-Software-Specification.md](../ModPlayer-Software-Specification.md) | 001–007 | Consolidated software specification v0.1 (draft, 2026-09-15): overview, personas, journeys, functional requirements, plugin API, data model, integrations, NFRs, architecture, edge cases, governance |

## Wave sequence

### 001 — MVP

The smallest coherent product: a signed-in Premium user plays their catalog, sees a waveform, loops a passage gaplessly, changes key and tempo, and does it all from the keyboard — with the plugin runtime already underneath so the two bundled plugins are real plugins built on the public API rather than host features to be rewritten later. Wave 1 is deliberately large (13 files) because the plugin runtime, transport focus, and UI contributions cannot be cut without turning the bundled plugins into throwaway host code. The first file is a walking skeleton with a synthetic audio source so the engine and plugin tiers are testable before the unofficial receiver protocol lands.

1. [001 — Walking Skeleton: App Shell, Synthetic Source, and Audio Output](001-mvp/001-walking-skeleton.md) — three-OS app shell, real-time engine with synthetic source, limiter, device check, CI
2. [002 — First Launch Disclosure and Sign-In](001-mvp/002-first-launch-and-sign-in.md) — versioned disclosure, browser auth, secure store, Premium check, sign-out
3. [003 — Streaming Playback, Transport, and Queue](001-mvp/003-streaming-playback-and-queue.md) — Connect receiver Audio Source, decode, transport, queue, remote transfer
4. [004 — Catalog Search and Library Browsing](001-mvp/004-search-and-library-browse.md) — search grouped by type, library and playlist views, recently played
5. [005 — Now-Playing View with Waveform](001-mvp/005-now-playing-waveform.md) — locally generated waveform, progressive fill, click/drag seek, zoom
6. [006 — Markers, Loop Regions, and Cue Points](001-mvp/006-markers-loops-and-cues.md) — host primitives, gapless seam on the real-time path, per-track persistence
7. [007 — Named Actions and Keyboard Shortcuts](001-mvp/007-keyboard-actions-and-shortcuts.md) — action registry, default shortcut set, rebinding, conflict detection
8. [008 — Effect Chain and Built-In Effect Nodes](001-mvp/008-effect-chain-and-built-in-nodes.md) — ordered chain UI, pitch/stretch/gain/EQ/filter/stereo nodes, metering, budget bypass
9. [009 — Plugin Runtime, Sandbox, and Permission Gateway](001-mvp/009-plugin-runtime-and-permissions.md) — manifest, lifecycle, isolated contexts, budgets, suspension, core capabilities
10. [010 — Transport Focus Arbitration](001-mvp/010-transport-focus.md) — one holder at a time, three policies, user always wins
11. [011 — Plugin UI Contributions](001-mvp/011-plugin-ui-contributions.md) — declarative panels, waveform overlays, plugin actions, settings pages, notifications
12. [012 — Section Loop Bundled Plugin](001-mvp/012-section-loop-plugin.md) — A/B loop, cues, marker list, overlays, persisted via host state
13. [013 — Key & Tempo Bundled Plugin and Getting Started Panel](001-mvp/013-key-and-tempo-plugin.md) — pitch and stretch nodes, per-track memory, restored badge, onboarding panel

### 002 — Developer Mode

The plugin runtime exists; this wave opens it to authors. It comes right after the MVP because "write or install a small plugin" is a P0 job, the delta over the runtime is small, and every later plugin-facing wave benefits from real third-party plugins existing early.

1. [001 — Development Folder and Hot Reload](002-developer-mode/001-dev-folder-and-hot-reload.md) — Local · Dev plugins, sub-second reload preserving playback, state hand-off
2. [002 — Plugin Console and Condition Simulation](002-developer-mode/002-plugin-console-and-simulation.md) — per-plugin logs, permission checks, budgets, simulated offline/denial/pending/focus loss
3. [003 — Plugin Template, Bundled Tutorial, and Source Viewing](002-developer-mode/003-template-and-tutorial.md) — create-from-template, "hello loop" tutorial, generated API reference

### 003 — Offline and Library

Théo's P0 job — a bad connection must not end a set — and the assumed-in-scope library management, which shares the offline write queue. Ordered before MIDI and Performance Mode because the Performance Mode checklist reports cache state and grace period, so those must exist first.

1. [001 — Encrypted Offline Cache and Pinning](003-offline-and-library/001-offline-cache-and-pinning.md) — encrypted stream cache, pin track/album/playlist, eviction, cache view
2. [002 — Offline Session Start and Playback](003-offline-and-library/002-offline-session-and-playback.md) — grace period, offline indicator, cached-metadata library and search
3. [003 — Library and Playlist Management with Offline Sync](003-offline-and-library/003-library-and-playlist-management.md) — save/follow/playlist edits, queued sync, conflict rules, `library.*` permissions

### 004 — Performance

Everything a DJ needs to trust the player on stage: physical control, OS integration, a mode that suppresses interruptions, and layouts for a dark room and a second screen.

1. [001 — MIDI Learn, Mapping, and Device Profiles](004-performance/001-midi-mapping-and-profiles.md) — learn mode, continuous controls with pickup, named profiles, hot-plug, `midi.*` permissions
2. [002 — Global Shortcuts, Media Keys, and OS Now-Playing Integration](004-performance/002-global-shortcuts-and-os-integration.md) — app-unfocused shortcuts, media keys, OS now-playing surface, notification routing
3. [003 — Performance Mode and Crash-State Restore](004-performance/003-performance-mode.md) — pre-set checklist, suppression rules, silent fault handling, exit summary, state restore
4. [004 — Detachable Panels and Full-Screen Performance Layout](004-performance/004-detachable-and-full-screen-layouts.md) — detached now-playing and plugin panels, simplified full-screen layout

### 005 — Community Registry

Turns the plugin system into an ecosystem: discover and install with verified integrity and a permission sheet, manage and audit grants, give plugins the web, publish through an automated pipeline, keep plugins updated, and unlock the High-risk capabilities that need this trust model in place.

1. [001 — Registry Browser, Verified Install, and Permission Approval](005-community-registry/001-registry-browse-and-install.md) — signed index, digest-verified packages, approval sheet with justifications
2. [002 — Runtime Permission Management, Usage Log, and Uninstall](005-community-registry/002-permission-management-and-usage-log.md) — per-permission grants, revoke with `permission_changed`, sensitive-usage log, data retention on uninstall
3. [003 — Plugin Network Access through the Host Proxy](005-community-registry/003-plugin-network-access.md) — declared hosts only, credential stripping, rate and byte caps
4. [004 — Package for Registry and Submission Pipeline](005-community-registry/004-package-and-publish.md) — packaging validation, signed descriptor, repository submission checks, High-risk review queue
5. [005 — Plugin Updates, Sideloading, Extra Registries, and Delisting](005-community-registry/005-plugin-updates-sideload-and-delisting.md) — update checks, new-permission approval, sideload warnings, security delisting
6. [006 — Custom Buffer Processor (`audio.process`)](005-community-registry/006-custom-buffer-processor.md) — restricted-subset compile, benchmark gate, runtime bypass
7. [007 — File Picker and Clipboard Permissions](005-community-registry/007-file-and-clipboard-permissions.md) — opaque file handles via picker, gesture-gated clipboard

### 006 — Practice Depth

Deepens the practicing musician's experience once the core loop and ecosystem exist: beat-aware markers and plugins, correctable grids, named sections, and saved rigs. These are SHOULD-level and P1/P2 jobs, so they follow the P0 waves.

1. [001 — Beat Grid, Key, and Loudness Analysis with Beat Snapping](006-practice-depth/001-beat-grid-key-and-loudness-analysis.md) — background analysis, provider fallback, snap to beat, `analysis.read`, loop last N beats, detected key
2. [002 — Beat Grid Correction and Song Sections](006-practice-depth/002-beat-grid-correction-and-sections.md) — tap tempo, downbeat nudge, sections with jump actions, `analysis.write`
3. [003 — Named Presets for Chain, Focus, and Enabled Plugins](006-practice-depth/003-presets.md) — save/recall/export, bindable recall, missing-node handling
4. [004 — Per-Track State Inspection, Clearing, and Export](006-practice-depth/004-per-track-state-tools.md) — Track memory panel, per-group clear, export/import, corruption handling

### 007 — Operations and Polish

What the project needs to run for a long time and reach everyone: crash diagnostics, safe updates with API compatibility, the first non-English locale, an accessibility pass, and the remaining playback conveniences.

1. [001 — Diagnostic Bundles and Opt-In Crash Reporting](007-operations-and-polish/001-diagnostics-and-crash-reporting.md) — bundle generation, consent-gated reports with plugin attribution, no telemetry by default
2. [002 — Client Updates with Plugin API Compatibility](007-operations-and-polish/002-client-updates-and-api-compatibility.md) — verified updates, channels, rollback, compatibility preview and mode, independent Audio Source updates
3. [003 — Localization and Portuguese (Brazil)](007-operations-and-polish/003-localization-pt-br.md) — externalized strings, pt-BR, solfège naming, plugin string tables
4. [004 — Accessibility Hardening](007-operations-and-polish/004-accessibility-hardening.md) — keyboard-only operation, accessible names, contrast and high-contrast, flashing flag, CI checks
5. [005 — Playback Polish: Crossfade, Quality Tier, Library Sorting](007-operations-and-polish/005-playback-polish.md) — crossfade (off while looping), quality tier, sortable library

## How to use this folder

Work through waves in order, and files in order within a wave. Open a file, copy the block under **Prompt**, and paste it after `/speckit.specify`. Each wave is shippable on its own — finish one before starting the next.

## Coverage map

### ModPlayer-Software-Specification.md

| Source section | Covered by |
|---|---|
| About this document | Not sliced — document preface |
| Table of contents | Not sliced — navigation |
| Part 1 § 1 Vision | Not sliced — context; summarized above |
| Part 1 § 2 Problem statement | Not sliced — context |
| Part 1 § 3 Why now | Not sliced — context |
| Part 1 § 4 Target users | Not sliced — persona context, developed in Part 2 |
| Part 1 § 5 Scope | In-scope items mapped through Parts 4–5 below; out-of-scope and deferred items (social, podcasts, mobile, audio export, controller mode, multi-account, marketplace, native plugins, local files, other services) deliberately not sliced |
| Part 1 § 6 Success metrics | Not sliced — targets; referenced by 001-mvp/013 and 002-developer-mode/003 |
| Part 1 § 7 Hard constraints | 001-mvp/001 (C-3, C-5), 001-mvp/002 (C-1, C-8), 001-mvp/008 (C-2, C-4), 003-offline-and-library/001 (C-2, C-6), 007-operations-and-polish/002 (C-7) |
| Part 1 § 8 Key risks | 001-mvp/001 (risk 4), 001-mvp/002 (risk 2), 001-mvp/003 (risk 1), 001-mvp/009 (risk 3) |
| Part 2 § 1 Primary personas | Not sliced — persona context; cited by 001-mvp/007, 002-developer-mode/001, 004-performance/001 |
| Part 2 § 2 Secondary personas | 005-community-registry/004 (registry maintainer), 007-operations-and-polish/001–002 (project maintainer), 001-mvp/004 (occasional listener) |
| Part 2 § 3 Edge users | 007-operations-and-polish/004 (screen reader, hearing protection), 003-offline-and-library/002 (low bandwidth), 004-performance/004 (multi-monitor), 007-operations-and-polish/003 (non-English) |
| Part 2 § 4 Jobs-to-be-done | JTBD-1 → 001-mvp/006, 012; JTBD-2/3 → 001-mvp/008, 013; JTBD-4 → 001-mvp/009, 002-developer-mode/001; JTBD-5 → 004-performance/001; JTBD-6 → 003-offline-and-library/002; JTBD-7 → 001-mvp/006, 013, 006-practice-depth/004; JTBD-8 → 005-community-registry/001–002; JTBD-9 → 002-developer-mode/001; JTBD-10 → 005-community-registry/001, 004; JTBD-11 → 004-performance/003; JTBD-12 → 006-practice-depth/002; JTBD-13 → 001-mvp/010, 006-practice-depth/003; JTBD-14 → 001-mvp/004; JTBD-15 → 001-mvp/009; JTBD-16 → 006-practice-depth/001; JTBD-17 → 005-community-registry/003 |
| Part 2 § 5 Key scenarios | S-1 → 001-mvp/012; S-2 → 001-mvp/013; S-3 → 003-offline-and-library/001, 004-performance/001, 003; S-4 → 002-developer-mode/001–003, 005-community-registry/004; S-5 → 005-community-registry/001–003; S-6 → 001-mvp/009 |
| J-1 First launch and sign-in | 001-mvp/002; audio check in 001-mvp/001 |
| J-2 Drill a solo with Section Loop | 001-mvp/012, 001-mvp/013; beat snapping in 006-practice-depth/001 |
| J-3 Transpose a set | 001-mvp/013 |
| J-4 Prepare and play a bar set | 003-offline-and-library/001–002, 004-performance/001, 004-performance/003 |
| J-5 Write and publish a plugin | 002-developer-mode/001–003, 005-community-registry/004 |
| J-6 Install a community plugin and manage its permissions | 005-community-registry/001–003 |
| J-7 A plugin misbehaves during practice | 001-mvp/009; report in 007-operations-and-polish/001 |
| J-8 Reorder and resolve effect chain and transport focus | 001-mvp/008, 001-mvp/010, 006-practice-depth/003 |
| J-9 Offline session start | 003-offline-and-library/002 |
| J-10 Update the client with a plugin API change | 007-operations-and-polish/002 |
| Part 4 § 1 Onboarding and account | 001-mvp/001 (FR-1.3), 001-mvp/002 (FR-1.1, 1.2), 001-mvp/013 (FR-1.4.1), 001-mvp/009 (FR-1.4.2) |
| Part 4 § 2 Catalog, search, library, and playlists | 001-mvp/004; 003-offline-and-library/001 (pinning), 002 (offline search), 003 (edits); 006-practice-depth/001 (FR-2.4.2); 007-operations-and-polish/005 (sorting) |
| Part 4 § 3 Playback and transport | 001-mvp/003 (FR-3.1, 3.2), 001-mvp/010 (FR-3.3), 001-mvp/001 (FR-3.4), 004-performance/002 (FR-3.1.6), 007-operations-and-polish/005 (FR-3.1.5) |
| Part 4 § 4 Now-playing view and waveform | 001-mvp/005; overlays in 001-mvp/011; beat grid in 006-practice-depth/001; detach in 004-performance/004 |
| Part 4 § 5 Markers, loops, and cue points | 001-mvp/006; snapping in 006-practice-depth/001; sections (FR-5.4) in 006-practice-depth/002 |
| Part 4 § 6 Audio engine and effect chain | 001-mvp/008; clock/loop evaluation in 001-mvp/006; custom processor (FR-6.3.7) in 005-community-registry/006; local analysis (FR-6.6) in 001-mvp/005, 006-practice-depth/001–002 |
| Part 4 § 7 Plugin management | 001-mvp/009 (FR-7.1.1, 7.3), 001-mvp/011 (FR-7.4.1–2), 001-mvp/010 (FR-7.4.3), 005-community-registry/001 (FR-7.1.2–3, 7.1.5, 7.2.1–3), 002 (FR-7.2.4–5, 7.3.2), 005 (FR-7.1.4, 7.3.3), 004-performance/003 (FR-7.2.6) |
| Part 4 § 8 Offline cache and connectivity | 003-offline-and-library/001 (FR-8.1), 002 (FR-8.2), 003 (FR-8.2.5); 001-mvp/003 (FR-8.3); 005-community-registry/001 (FR-8.2.6); 007-operations-and-polish/005 (FR-8.3.3) |
| Part 4 § 9 Controls: keyboard and MIDI | 001-mvp/007 (FR-9.1, 9.2.1–2), 004-performance/002 (FR-9.2.3), 004-performance/001 (FR-9.3) |
| Part 4 § 10 Performance Mode | 004-performance/003; layout (FR-10.1.6) in 004-performance/004 |
| Part 4 § 11 Settings, presets, and per-track state | 001-mvp/001 (FR-11.1.1–2), 001-mvp/011 (FR-11.1.3), 006-practice-depth/003 (FR-11.2), 001-mvp/006 (FR-11.3.1–2), 006-practice-depth/004 (FR-11.3.3–4) |
| Part 4 § 12 Developer Mode and plugin tooling | 002-developer-mode/001 (FR-12.1.1–3), 002 (FR-12.1.4, 12.1.7), 003 (FR-12.1.6, 12.1.8); packaging (FR-12.1.5) in 005-community-registry/004 |
| Part 4 § 13 Bundled reference plugins | 001-mvp/012, 001-mvp/013; analysis-dependent items (FR-13.1.6, 13.2.5) in 006-practice-depth/001 |
| Part 4 § 14 Notifications, diagnostics, and updates | 001-mvp/001 (FR-14.1, 14.4.1), 001-mvp/011 (FR-14.1.3), 007-operations-and-polish/001 (FR-14.2), 002 (FR-14.3), 003 (FR-14.4.2) |
| Part 5 § 1 Design principles | 001-mvp/009 |
| Part 5 § 2 Plugin package | 001-mvp/009; network_hosts in 005-community-registry/003; PL-2.4 in 005-community-registry/004; string tables in 007-operations-and-polish/003 |
| Part 5 § 3 Lifecycle | 001-mvp/009; hot reload (PL-3.5) in 002-developer-mode/001 |
| Part 5 § 4 Permission catalog | 001-mvp/009 (playback, transport, queue, markers, audio.effects/meter, state), 001-mvp/011 (ui.*), 003-offline-and-library/003 (library.*), 004-performance/001 (midi.*), 005-community-registry/001 (sheet, risk classes), 002 (PL-4.5), 003 (network), 006 (audio.process), 007 (files.*, clipboard), 006-practice-depth/001–002 (analysis.*) |
| Part 5 § 5 Events (host → plugin) | 001-mvp/009 (core), 001-mvp/010 (focus), 001-mvp/011 (action_invoked, settings_changed), 003-offline-and-library/002 (connectivity_changed), 004-performance/001 (midi_message), 004-performance/003 (performance_mode_changed), 006-practice-depth/001 (analysis_ready) |
| Part 5 § 6 Requests (plugin → host) | 001-mvp/009 (6.1–6.3, 6.6, 6.10), 001-mvp/010 (focus), 001-mvp/011 (6.5), 003-offline-and-library/003 (6.9), 005-community-registry/003 (6.8), 006 (6.7), 006-practice-depth/001 (6.4 read), 002 (6.4 write, define_section) |
| Part 5 § 7 UI contribution rules | 001-mvp/011; PL-7.3 in 007-operations-and-polish/003; PL-7.2 in 007-operations-and-polish/004 |
| Part 5 § 8 Sandboxing and budgets | 001-mvp/009; visibility (PL-8.2) in 002-developer-mode/002; raised budgets at install in 005-community-registry/001; network caps in 005-community-registry/003 |
| Part 5 § 9 Versioning and compatibility | 007-operations-and-polish/002; deprecation warnings in 002-developer-mode/002; PL-9.5 in 002-developer-mode/003; PL-9.2 in 005-community-registry/005 |
| Part 5 § 10 Registry publishing contract | 005-community-registry/004; PL-10.4 in 005-community-registry/001 |
| Part 6 § 1 Entity overview | Not sliced — diagram; entities mapped individually below |
| Part 6 § 2 Account and session | 001-mvp/002 (DM-1) |
| Part 6 § 3 Catalog references | 001-mvp/003–004 (DM-2, DM-3), 003-offline-and-library/001 (DM-4), 001-mvp/005 and 006-practice-depth/001 (DM-5) |
| Part 6 § 4 Per-track user state | 001-mvp/006 (DM-6, 7, 9), 006-practice-depth/002 (DM-8) |
| Part 6 § 5 Plugins | 001-mvp/009 (DM-10–12), 005-community-registry/002 (DM-11, 13), 001-mvp/007 (DM-14, 15), 004-performance/001 (DM-15, 16) |
| Part 6 § 6 Audio configuration | 001-mvp/008 (DM-17, 18, 20), 006-practice-depth/003 (DM-19) |
| Part 6 § 7 Sync and connectivity | 003-offline-and-library/003 (DM-21), 002 (DM-22) |
| Part 6 § 8 Registry | 005-community-registry/001 (DM-23, 24) |
| Part 6 § 9 Diagnostics | 007-operations-and-polish/001 (DM-25, 26), 001-mvp/002 (DM-27) |
| Part 6 § 10 Retention summary | 001-mvp/002 (sign-out), 003-offline-and-library/001 (cache), 005-community-registry/002 (plugin data, usage logs), 006-practice-depth/003–004 (presets, per-track), 007-operations-and-polish/001 (crash reports) |
| INT-1 Streaming service — authorization | 001-mvp/002 |
| INT-2 Streaming service — Connect receiver protocol | 001-mvp/003; INT-2.6 in 003-offline-and-library/002; INT-2.7 in 007-operations-and-polish/002 |
| INT-3 Streaming service — catalog and library | 001-mvp/004 (reads), 003-offline-and-library/003 (writes, conflicts) |
| INT-4 Plugin registry | 005-community-registry/001, 004, 005 |
| INT-5 Update channel | 007-operations-and-polish/002 |
| INT-6 Operating system services | 001-mvp/001 (INT-6.1), 004-performance/001 (6.2), 002 (6.3, 6.5), 003 (6.4), 005-community-registry/007 (6.6) |
| INT-7 Plugin-declared network hosts | 005-community-registry/003 |
| INT-8 Crash and diagnostic reporting | 007-operations-and-polish/001 |
| Part 7 Data flow summary | Not sliced — diagram summarizing INT-1..8 |
| NFR § 1 Performance and latency | 001-mvp/001 (1.10, 1.13), 003 (1.4–1.7), 004 (1.12), 006 (1.2, 1.3), 008 (1.1, 1.8, 1.14, 1.15), 009 (1.9); 002-developer-mode/001 (1.11) |
| NFR § 2 Reliability and availability | 001-mvp/001 (2.4), 003 (2.1, 2.5), 006 (2.8), 008 (2.7), 009 (2.3); 004-performance/003 (2.6); 2.2 measured via 007-operations-and-polish/001 |
| NFR § 3 Scalability (local) | 001-mvp/004 (3.1), 003-offline-and-library/001 (3.2), 001-mvp/009 (3.3), 006-practice-depth/004 (3.4), 001-mvp/008 (3.5) |
| NFR § 4 Security | 001-mvp/002 (4.1), 003-offline-and-library/001 (4.2), 001-mvp/009 (4.3), 005-community-registry/001 (4.4), 007-operations-and-polish/002 (4.5), 005-community-registry/003 (4.6), 001-mvp/011 (4.7), 005-community-registry/006 (4.8), 004-performance/002 (4.9); 4.10 not sliced — governance process (GOV § 5) |
| NFR § 5 Privacy | 007-operations-and-polish/001 (5.1, 5.2), 005-community-registry/002 (5.3), 001-mvp/002 (5.4, 5.5), 005-community-registry/001 (5.6) |
| NFR § 6 Accessibility | 007-operations-and-polish/004; NFR-6.1 in 001-mvp/007; NFR-6.3 in 001-mvp/011 |
| NFR § 7 Internationalization and localization | 007-operations-and-polish/003 |
| NFR § 8 Observability (local) | 002-developer-mode/002 (8.1), 004-performance/003 (8.2, 8.3), 007-operations-and-polish/001 (8.4) |
| NFR § 9 Compatibility and portability | 001-mvp/001 (9.1, 9.4), 004-performance/002 (9.2), 001-mvp/009 (9.3) |
| NFR § 10 Maintainability and quality | 001-mvp/001 (10.1, 10.3 CI suite), 002-developer-mode/003 (10.2); 10.4 not sliced — test protocol document (A-16) |
| NFR § 11 Legal and compliance | 001-mvp/008 (11.1), 001-mvp/002 (11.3, 11.4); 11.2, 11.5 not sliced — governance policy |
| Part 9 § 1 Guiding decisions | 001-mvp/001, 001-mvp/009 |
| Part 9 § 2 Component diagram | Not sliced — diagram; components mapped in § 3 |
| Part 9 § 3 Components and responsibilities | Real-time path → 001-mvp/001, 003, 008; AR-6 → 001-mvp/003; AR-7 → 001-mvp/006; AR-8 → 001-mvp/007; AR-9 → 003-offline-and-library/001; AR-10 → 003-offline-and-library/003; AR-11 → 001-mvp/005, 006-practice-depth/001; AR-12, 16, 17 → 001-mvp/009; AR-13 → 004-performance/003; AR-14 → 007-operations-and-polish/001; AR-15 → 007-operations-and-polish/002; AR-18 → 005-community-registry/003; AR-19 → 001-mvp/011 |
| Part 9 § 4 Communication patterns | 001-mvp/001; continuous-control path in 004-performance/001 |
| Part 9 § 5 Key flows | 001-mvp/006 (5.1), 008 (5.2), 010 (5.3), 009 (5.4) |
| Part 9 § 6 Trust boundaries | 001-mvp/009, 005-community-registry/003 |
| Part 9 § 7 Deployment shape | 001-mvp/001 |
| EC § 1 Account and session | 001-mvp/002; EC-1.2 in 003-offline-and-library/002 |
| EC § 2 Catalog, library, playlists | 001-mvp/004; EC-2.3, 2.4 in 003-offline-and-library/003 |
| EC § 3 Playback and transport | 001-mvp/003; EC-3.8, 3.9 in 001-mvp/001; EC-3.4, 3.6 in 001-mvp/010; EC-3.10 in 004-performance/002; EC-3.7 in 007-operations-and-polish/005 |
| EC § 4 Markers, loops, cues | 001-mvp/006; EC-4.4 in 001-mvp/010; EC-4.5 in 001-mvp/009; EC-4.7 in 006-practice-depth/001; EC-4.8 in 006-practice-depth/004 |
| EC § 5 Audio engine and effects | 001-mvp/008; EC-5.2 in 005-community-registry/006; EC-5.7 in 006-practice-depth/003; EC-5.9 in 006-practice-depth/001 |
| EC § 6 Plugins | 001-mvp/009; EC-6.8, 6.9, 6.19 in 001-mvp/011; EC-6.16, 6.17 in 002-developer-mode/001; EC-6.4, 6.7 in 002-developer-mode/002; EC-6.10, 6.11 in 005-community-registry/001; EC-6.13, 6.14 in 005-community-registry/002; EC-6.2, 6.12, 6.15 in 005-community-registry/005 |
| EC § 7 Offline cache and connectivity | 003-offline-and-library/001 (7.1, 7.2, 7.6, 7.7), 002 (7.3, 7.4, 7.5, 7.8); EC-7.9 in 005-community-registry/001 |
| EC § 8 Controls | 004-performance/001 (8.1–8.4), 002 (8.5); EC-8.6 in 001-mvp/007 |
| EC § 9 Performance Mode | 004-performance/003 |
| EC § 10 Settings, presets, per-track state | 006-practice-depth/003 (10.1), 004 (10.2, 10.3); EC-10.4 in 003-offline-and-library/003 |
| EC § 11 Updates | 007-operations-and-polish/002 |
| EC § 12 Empty, loading, and first-run states | 001-mvp/004 (library, playlists), 005 (now playing, waveform), 006 (markers), 008 (effect chain); 003-offline-and-library/001 (cache); 004-performance/001 (MIDI); 005-community-registry/001 (plugin list, registry) |
| GOV § 1 Licensing | GOV-1.3 in 001-mvp/012; GOV-1.6 in 007-operations-and-polish/001; rest not sliced — licensing policy decision (Q-4) |
| GOV § 2 Plugin API stability | GOV-2.4, 2.6 in 007-operations-and-polish/002; rest not sliced — change-request process |
| GOV § 3 Contribution process | GOV-3.3 CI in 001-mvp/001; PR template accessibility item in 007-operations-and-polish/004; rest not sliced — repository process docs |
| GOV § 4 Registry governance | 005-community-registry/004; GOV-4.5 in 005-community-registry/005 |
| GOV § 5 Security disclosure | Not sliced — process document |
| GOV § 6 Legal posture and takedown response | GOV-6.1 disclosure in 001-mvp/002; rest not sliced — process document |
| GOV § 7 Community and support | GOV-7.2, 7.3 in 002-developer-mode/003; GOV-7.4 in 007-operations-and-polish/003; GOV-7.1 not sliced — community infrastructure |
| GOV § 8 Success signals for governance | Not sliced — targets; referenced by 005-community-registry/004 |
| Part 12 A. Assumptions | Carried into per-file "Open questions" sections and the list below |
| Part 12 B. Open questions | Carried into per-file "Open questions" sections and the list below |
| Part 13 Glossary | Not sliced — reference material |

## Open questions

1. Q-1 / A-3 — offline grace period the service's credentials allow, and whether cached streams play without a live session → gates 003-offline-and-library/002; spike before planning that wave.
2. A-2 — browser authorization yields a receiver-usable credential → gates 001-mvp/002.
3. Q-2 — seeking into unbuffered offsets → affects 001-mvp/003 latency target.
4. Q-3 — quality tiers, formats, gapless implications → 001-mvp/003, 007-operations-and-polish/005.
5. Q-4 / Q-16 — legal exposure, distribution of the Audio Source, "ModPlayer" name → 001-mvp/002, 001-mvp/003; may change the distribution model.
6. Q-5 — whether the disclosure needs more than acknowledgement → 001-mvp/002.
7. Q-6 — scripting runtime and ahead-of-time compilation of a restricted subset → 001-mvp/009; decides whether 005-community-registry/006 ships in the first release.
8. Q-7 — transport focus per-track or persistent → 001-mvp/010.
9. Q-9 — Key & Tempo per-track memory default → 001-mvp/013 (assumed off).
10. Q-10 / Q-17 — source availability for all plugins; a "reviewed" badge tier → 005-community-registry/004.
11. Q-11 — install counts from static hosting → 005-community-registry/001.
12. Q-12 — crash-reporting endpoint operator and retention → 007-operations-and-polish/001.
13. Q-13 — MIDI output feedback in first release → 004-performance/001.
14. Q-14 — auto-detected sections → 006-practice-depth/002 (assumed user/plugin-defined only).
15. Q-15 — app footprint limits → 001-mvp/001.
16. Q-8 — a "session" concept beyond presets and playlists → not sliced; would add a file to 006-practice-depth if adopted.
17. A-1 — library management in scope → 003-offline-and-library/003.
18. A-5 — bundled plugins enabled with pre-approved permissions → 001-mvp/009, 013.
19. A-6 / A-10 — cache default limit; analysis cleared with cache → 003-offline-and-library/001.
20. A-9 — per-track state account-scoped → 006-practice-depth/004.
21. A-11 — independently versioned Audio Source module → 001-mvp/003, 007-operations-and-polish/002.
22. A-12 — plugin budget defaults → 001-mvp/009.
23. A-13 — pt-BR first locale, solfège → 007-operations-and-polish/003.
24. A-15 — loop stays armed on seek-outside → 001-mvp/006.
25. A-16 — reference hardware and track set → 001-mvp/001 and every latency/quality acceptance target.
26. A-19 — provider audio features available → 006-practice-depth/001.
