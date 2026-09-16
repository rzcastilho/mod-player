# Quickstart: validating Streaming Playback, Transport, and Queue

**Feature**: 003-streaming-playback-and-queue | Branch `003-streaming-playback-and-queue`

This is a validation guide, not an implementation guide. Contracts are in
[contracts/](contracts/), entities in [data-model.md](data-model.md).

## Prerequisites

- Rust 1.95.0 (`rust-toolchain.toml`), `cargo-deny`; Linux additionally
  `libasound2-dev libxkbcommon-dev libwayland-dev pkg-config` (CI list).
- For manual scenarios: a Spotify **Premium** account already signed in
  through 002's flow (Welcome → Sign in → tier verified), a second Connect
  controller on the same account (the official desktop/mobile app), and a
  way to throttle/cut the network (e.g. Network Link Conditioner, `tc`,
  or toggling Wi-Fi).
- Optional: `MODPLAYER_STATUS_PAGE_URL=<url>` at build time (research R13);
  `MODPLAYER_TEST_ACCESS_TOKEN=<token>` for the ignored live test.

## Automated gates (must all pass; same CI matrix as 001/002)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
scripts/check-license-headers.sh
```

Named tests that prove each requirement (all run without network):

| Requirement | Crate / test |
|---|---|
| FR-002 additive seam, 001 tests unchanged | `modplayer-audio-source-synthetic` (all 001 tests) + `synthetic_host_attach_restores_position` |
| Constitution I on the new RT half | `modplayer-audio-source-connect::rt::fill_and_seek_never_allocate` (`assert_no_alloc`) |
| FR-006 / SC-003 position ≥ 60 Hz, ≤ 5 ms jitter | `modplayer-engine::position_clock_60hz_jitter`, `position_clock_frozen_when_paused` |
| FR-005 seek command | `modplayer-engine::seek_command_applies_at_boundary` |
| Engine leftover carry | `modplayer-engine::leftover_carry_is_bit_exact_with_001` |
| FR-003 transport table | `modplayer-core::transport_reducer::t01_…t24_…` |
| FR-004 / FR-011–014 / SC-011 | `modplayer-core::queue::*` and `queue_proptest::advance_is_consistent_with_fr013_fr014` |
| SC-001 buffered play < 50 ms | `modplayer-core::controller_streaming::buffered_play_is_audible_within_50ms` |
| SC-002 / FR-010 readiness | `controller_streaming::first_play_buffers_until_two_seconds_then_plays` |
| SC-004 / FR-007 gapless | `controller_streaming::adjacent_buffered_tracks_transition_without_gap` |
| SC-005 degrade / dry buffer | `controller_streaming::{continues_from_buffer_when_throttled, dry_buffer_shows_buffering_and_resumes_without_skip}` |
| SC-006 seek past end | `controller_streaming::seek_past_end_advances_per_fr014` |
| SC-007 / SC-008 transfer | `controller_streaming::{transfer_in_replaces_queue_and_plays, transfer_away_freezes_and_shows_banner, play_here_requests_transfer}` |
| FR-018 5 s timeout | `controller_streaming::transfer_request_times_out_with_warning` |
| FR-021 / SC-009 transient vs. unavailable | `controller_streaming::{transient_30s_raises_warning_and_clears, five_minute_transient_never_unavailable, unavailable_disables_transport_and_keeps_navigation}`, `modplayer-audio-source-connect::health::classification_table` |
| FR-026 unavailable item | `controller_streaming::unavailable_item_marked_skipped_and_notified` |
| FR-027 / SC-013 session events | `controller_streaming::{sign_out_stops_clears_and_deregisters, downgrade_finishes_track_then_disables}` |
| FR-001 device name | `modplayer-core::settings::playback_section_round_trips`, `device_name_validation` |
| FR-022 Play from account | `modplayer-account::reads::*`, `modplayer-ui::developer::play_from_account_replaces_queue` |
| FR-023 accessibility | `modplayer-ui::accessibility::every_new_control_has_a_name` |
| FR-024 strings | `modplayer-ui::fluent_keys` |
| Constitution VI leak check | `modplayer-ui::credential_leak` (now includes the connect crate's `Debug` output) |
| Constitution IV single dependent | `modplayer::receiver_crate_has_single_dependent` |

Live (manual, ignored): `cargo test -p modplayer-audio-source-connect -- --ignored plays_five_seconds_of_a_real_track`.

## Manual scenarios (reference hardware, Premium account)

Launch with `cargo run -p modplayer` (or the release binary). Each scenario
lists the expected outcome; record pass/fail per platform.

**M1 — Hear real music (US1, SC-001/SC-002)**
1. Settings › Developer › **Play from account**. Expected: queue replaced
   with up to 20 tracks; Now Playing shows title/artist; status `Buffering…`
   briefly; audio within 1.5 s on a 10 Mbit/s link.
2. Pause, wait 5 s, Play. Expected: resumes at the same position instantly.
3. Stop, then Play. Expected: track restarts from 0:00; queue unchanged.
4. Skip back at > 3 s → restarts; skip back twice quickly → previous track.
5. Drag the seek slider to 2:00. Expected: position shows 2:00 at once,
   audio follows (Buffering if not fetched yet, never a skip).
6. Move the volume slider while playing. Expected: smooth, no click.
7. Watch the position readout for 30 s. Expected: smooth, no stutter.

**M2 — Queue (US2)**: open Queue (`Ctrl/Cmd+Q`), use Move up/down with the
keyboard, Remove a non-current and then the current item, add **Play next**
to a later item, toggle shuffle and each repeat mode; verify the next track
after each change matches FR-014 (repeat-one repeats on natural end but a
manual skip moves on; play-next plays before shuffle/repeat-all logic).

**M3 — Transfer (US3, SC-007/SC-008/SC-012)**: from the official app pick
"ModPlayer on <host>" as the playback device. Expected: ModPlayer starts
playing within 2 s, queue shows the transferred track. Move playback back
to the phone/desktop app. Expected: ModPlayer stops, banner "Playing on
<device>" with **Play here**; queue/position unchanged. Press **Play here**
→ ModPlayer resumes as the active device. While ModPlayer is active, pause
/ seek / change volume in the official app and locally — each side reflects
the other within 1 s.

**M4 — Background playback (SC-010)**: play, minimize for 10 min, switch
virtual desktops, focus other apps. Expected: no interruption; position
still advancing on restore; tracks advanced normally.

**M5 — Network trouble (US4, SC-005)**: play, throttle to 256 kbit/s after
~20 s → uninterrupted; cut the network → status `Reconnecting…`, audio
continues from buffer; after 30 s the warning notification appears; restore
→ warning clears. Cut the network right after pressing Play on a new track
→ `Buffering…`, position frozen, no skip; restore → resumes automatically.

**M6 — Protocol failure (SC-009)**: run with
`MODPLAYER_CONNECT_FORCE_UNAVAILABLE=1` (debug-only env honoured by the
connect crate's health classifier) → critical notification with **Status
page** (opens the browser) and **Retry**; app navigable; Settings and Queue
usable. Unset and press **Retry** → streaming resumes without restart.

**M7 — Account events (SC-013)**: sign out during playback → audio stops
immediately, sign-in step shown, no device listed in the official app.
Sign in with a non-Premium account → transport disabled with "Premium
required", no device listed, Play from account hidden.

**M8 — Device name (FR-001)**: Settings › Playback › Device name → "Studio";
the official app shows "Studio" within 5 s; whitespace → default restored;
65+ chars → inline error, previous name kept.

**M9 — Quit (FR-008)**: close the window while playing → process exits,
audio stops, device disappears from the official app's list.

## Expected outcomes summary

All automated gates green on ubuntu/macos/windows; M1–M9 pass on each
platform with the deviations documented in plan.md Complexity Tracking
(encrypted temp file on disk; shuffle flag reported as off; prefetch starts
30 s before track end).
