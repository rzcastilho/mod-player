# SPDX-License-Identifier: MIT OR Apache-2.0
# Transport, queue, transfer banner, status line, notification, and
# device-name keys (003-streaming-playback-and-queue).
#
# Scaffold only (Phase 1 Setup, T006) — keys land with each user story
# (contracts/ui-surface.md).

## Transport (Now Playing full transport — Phase 3, US1)

transport-skip-back = Skip back
transport-skip-forward = Skip forward
transport-seek = Seek
now-playing-title = { $title }
now-playing-artist = { $artist }
now-playing-empty = No track is playing.
now-playing-pick-a-track = Pick a track to start playing.
now-playing-album = { $album }
waveform-unavailable = Waveform unavailable
waveform-detail = Waveform detail
waveform-detail-window = { $start } to { $end }
time-elapsed = { $time }
time-remaining = -{ $time }
waveform-overview-desc = Whole-track waveform. Click or use arrow keys to seek.

## Status line (buffering / reconnecting / unavailable — Phase 3/6, US1/US4)

status-buffering = Buffering…
status-no-device = No output device is available. Connect a device to enable playback.
status-premium-required = A Spotify Premium subscription is required to play.
status-subscription-not-verified = Your subscription could not be verified. Try again from Settings › Account.
status-signed-out = Sign in to play from your account.
status-not-registered = Playback is not available right now.
status-source-unavailable = The audio source is unavailable right now.
status-reconnecting = Reconnecting…

## Queue (Queue panel — Phase 4, US2)

queue-toggle = Queue
queue-shuffle = Shuffle
queue-repeat-off = Repeat off
queue-repeat-one = Repeat one
queue-repeat-all = Repeat all
queue-row = { $title }
queue-current = Now playing:
queue-badge-play-next = In play next
queue-badge-unavailable = Unavailable
queue-move-up = Move up
queue-move-down = Move down
queue-play-next = Play next
queue-remove = Remove
queue-empty = The queue is empty.

## Transfer banner ("Playing on <device>" / Play here — Phase 5, US3)

banner-playing-elsewhere = Playing on { $device }
banner-play-here = Play here
banner-unknown-device = another device

## Notifications (stream/session/subscription — Phase 3/6, US1/US4)

transfer-request-failed = Couldn't take over playback here. Try again.
stream-reconnect-warning = Still trying to reconnect. Playback may be interrupted soon.
stream-source-unavailable = Streaming stopped unexpectedly. The audio source is unavailable.
stream-source-update-required = ModPlayer needs to be updated before streaming can continue.
subscription-downgraded = Your account no longer has Premium. Streaming has been turned off.
action-status-page = Status page
action-retry = Retry
action-open-upgrade-page = Upgrade

## Markers and loop regions (waveform lane/overlay, Markers panel — Phase 8 (006), US1)

markers-panel = Markers
markers-empty = No markers — press I to set A
markers-status = Marker action unavailable.
marker-limit-reached = The track already has the maximum number of markers.
loop-region-incomplete = Set both A and B before arming the loop.
loop-region-too-short = The region is too short to loop.
marker-glyph = { $role } { $name } { $time }
marker-role-a = A
marker-role-b = B
loop-arm = Arm loop
loop-disarm = Disarm loop
loop-repeat = Repeat
loop-crossfade = Crossfade
loop-wraps-remaining = { $count } wraps remaining
loop-wraps-infinite = Looping indefinitely
loop-armed-inactive = Armed (waiting for the playhead)

## Markers panel — empty/clear-all chrome and persistence warnings (Phase 8 (006), US2)

markers-new-loop = New loop region
markers-clear-all = Clear all markers
markers-clear-confirm = Clear { $count } markers?
markers-clear-yes = Yes
markers-clear-no = No
marker-clamped-desc = This marker was beyond the track's current length and was pulled back.
track-state-unreadable = This track's saved markers could not be read and have been reset.
track-state-newer-version = This track's saved markers were made by a newer version of the app and could not be loaded.
track-state-save-failed = Saving this track's markers failed. Your changes may be lost.

## Markers panel — precise editing (Phase 8 (006), US3)

marker-role-point = Marker
marker-default-name = Marker { $n }
markers-rename = Rename
markers-color = Colour { $index }

## Markers panel — cue points (Phase 8 (006), US4)

marker-role-cue = Cue { $slot }

## Settings > Playback (device name) — Phase 3, US1
