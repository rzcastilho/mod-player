synthetic-track-title = Synthetic test track
transport-play = Play
transport-pause = Pause
transport-stop = Stop
master-volume = Master volume
peak-meter = Peak meter
transport-disabled-no-device = No output device is available. Connect a device to enable playback.

device-lost = { $device } disconnected. Now playing through { $fallback }.
device-missing-at-launch = { $device } isn't connected. Playing through { $fallback } instead.
device-available-again = { $device } is available again. You can switch back in Settings › Audio.
no-output-devices = No output devices are available. Connect a device to enable playback.
device-appeared = An output device is now available.
settings-unreadable = Your settings file could not be read and default settings were used instead.
settings-newer-version = Your settings file was saved by a newer version of ModPlayer; default settings were used instead and your file was left unchanged.
settings-invalid-value = One or more settings in your settings file had an unrecognised value and were reset to their defaults.
settings-save-failed = Your settings could not be saved.
sample-notification-critical = This is a sample critical notification.
sample-notification-warning = This is a sample warning notification.
sample-notification-info = This is a sample info notification.

nav-library = Library
nav-search = Search
nav-now-playing = Now Playing
nav-plugins = Plugins
nav-settings = Settings

severity-critical = Critical
severity-warning = Warning
severity-info = Info
notification-dismiss = Dismiss
notification-action-sign-in = Sign in

# 019-notification-presentation (US2, contract fluent-strings.md): the
# collapsed stack's overflow control ("{N} more") and its expanded-state
# counterpart ("Show fewer").
notification-more = { $count ->
    [one] { $count } more
   *[other] { $count } more
}
notification-show-fewer = Show fewer

# 019-notification-presentation (US4, contract fluent-strings.md): the
# per-card truncation toggle, the Details toggle, and the two generic
# device phrases resolved at raise time (research R9).
notification-show-more = Show more
notification-show-less = Show less
notification-details = Details
notification-hide-details = Hide details
notification-device-unknown = Your saved output device
notification-device-fallback-default = the system default output

# 002-first-launch-and-sign-in: placeholder launch-gate content until the
# real Welcome (US1) and Sign-in (US2) screens land.
launch-gate-placeholder-welcome = Welcome screen coming soon.
launch-gate-placeholder-sign-in = Sign-in screen coming soon.

# 013-key-and-tempo-plugin (US4, contracts/getting-started-card.md §2):
# the dismissible Getting Started card at the top of the Library view.
getting-started-title = Getting started
getting-started-section-loop = Section Loop — drop A and B around a passage and drill it hands-free. Shortcuts: I set A, O set B, L loop, [ / ] nudge.
getting-started-key-tempo = Key & Tempo — transpose a song or slow it down without changing the rest. Shortcuts: + / - tempo step; key controls in the panel.
getting-started-tutorial = Open plugin tutorial
getting-started-dismiss = Dismiss

## 020-shell-navigation-and-gates: launch gate step indicator
gate-step-welcome = Welcome
gate-step-sign-in = Sign in
gate-step-audio-output-check = Audio output check
# $current, $total: integers; $label: one of the three step labels above
gate-step-progress = Step { $current } of { $total }: { $label }
