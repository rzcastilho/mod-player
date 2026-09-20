## Plugins section (009-plugin-runtime-and-permissions, contracts/ui-plugins.md).
## Seeded by Phase 1 (Setup, T010) with every key this feature needs: the
## list/columns/health/permission-summary strings, all 25 permission
## catalog explanations (Part 5 §4), all 8 manifest validation error
## sentences (contracts/manifest.md §3), and the suspension/auto-disable
## notification strings. Wired to real UI in later phases; see tasks.md.

## List (contracts/ui-plugins.md §2)

plugins-title = Plugins
plugins-empty = No plugins installed. Launch with plugin fixtures enabled to see the sample plugins.
plugins-col-name = Name
plugins-col-version = Version
plugins-col-source = Source
plugins-col-enabled = Enabled
plugins-col-health = Health
plugins-col-permissions = Permissions
plugins-col-cpu = CPU
plugins-col-memory = Memory
plugins-source-bundled = bundled
plugins-health-ok = ok
plugins-health-warning = warning
plugins-health-suspended = suspended
plugins-enable-toggle = Enable { $plugin }
plugins-invalid-manifest = invalid manifest: { $reason }
plugins-cpu = { $pct } %
plugins-memory = { $used } MB / 64 MB
plugins-dash = —
plugins-list-separator = ,{" "}

## Permission catalog (Part 5 §4 of docs/ModPlayer-Software-Specification.md;
## data-model.md §1.1 `Permission::explanation_key`). All 25 entries; the
## 9 this slice makes operable are playback.observe, transport.control,
## queue.write, markers.read, markers.write, audio.effects, audio.meter,
## state.plugin, state.track (FR-015) — the rest are recognized catalog
## strings with no capability behind them yet.

permission-playback-observe = See what is playing and where it is
permission-transport-control = Control playback (play, pause, jump around)
permission-queue-write = Change what plays next
permission-markers-read = See markers and loops
permission-markers-write = Create and move markers and loops
permission-audio-effects = Change how the music sounds (pitch, tempo, EQ, …)
permission-audio-meter = See volume levels and a spectrum
permission-audio-process = Run its own audio processing code on the sound
permission-analysis-read = Read the song's beats, key, and waveform
permission-analysis-write = Edit the beat grid and song sections
permission-library-read = Browse your library and search
permission-library-write = Edit your playlists and saved tracks
permission-ui-panel = Add a panel to the window
permission-ui-overlay = Draw on the waveform
permission-ui-shortcuts = Add keyboard shortcuts
permission-ui-notify = Show notifications
permission-ui-settings = Add a settings page
permission-state-plugin = Remember its own settings
permission-state-track = Remember settings for each song
permission-network = Connect to the internet (only to its declared hosts)
permission-files-read = Open files you choose
permission-files-write = Save files where you choose
permission-midi-observe = See MIDI controller input
permission-midi-output = Send messages to MIDI devices
permission-clipboard = Use the clipboard

## Manifest validation errors (contracts/manifest.md §3, FR-002).
## `ManifestError` renders to exactly one of these sentences.

manifest-error-unreadable = The manifest could not be read: { $detail }.
manifest-error-missing-field = The manifest is missing the required field '{ $field }'.
manifest-error-malformed-field = The field '{ $field }' is not valid: { $detail }.
manifest-error-unknown-permission = The { $list } permission '{ $permission }' is not in the permission catalog.
manifest-error-network-required = The 'network' permission may only be requested as optional.
manifest-error-network-hosts = The 'network' permission requires at least one declared network host.
manifest-error-duplicate-permission = The permission '{ $permission }' is listed more than once.
manifest-error-entry-missing = The entry script '{ $path }' is not in the package.

## Suspension / auto-disable notifications (FR-010, FR-011; contracts/ui-plugins.md §3).

plugin-suspended = { $plugin } was suspended ({ $cause }).
plugin-suspended-cause-hang = it stopped responding
plugin-suspended-cause-cpu-share = it used too much CPU
plugin-suspended-cause-memory = it used too much memory
plugin-suspended-cause-did-not-start = it did not start in time
plugin-auto-disabled = { $plugin } was suspended three times this session and has been disabled.
notification-action-restart-plugin = Restart plugin
notification-action-disable-plugin = Disable plugin
