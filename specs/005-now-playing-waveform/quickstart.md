# Quickstart: validating the Now-Playing View with Waveform

**Feature**: 005-now-playing-waveform | **Plan**: [plan.md](plan.md)

## Prerequisites

- Rust 1.95.0 (pinned by `rust-toolchain.toml`; if the shell exports
  `RUSTUP_TOOLCHAIN`, run with `RUSTUP_TOOLCHAIN=1.95.0`); `cargo deny`
  installed.
- For manual scenarios: a Spotify **Premium** account signed in through
  002's flow; network access; macOS host for the constitution's
  Quartz-driven recipe. Automated gates need none of these.

## Automated gates (run from the repository root)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
scripts/check-license-headers.sh
```

Named tests that must exist and pass (details in the *Tests pinning this
contract* sections of [contracts/](contracts/)):

| Requirement | Test |
|---|---|
| FR-001 artwork/title/artists/album/labels, slider + position label removed | `modplayer-ui tests/now_playing.rs`, `tests/accessibility.rs`, `tests/fluent_keys.rs` |
| FR-002 overview + detail, 30 s initial window, highlight | `modplayer-ui tests/now_playing.rs`, `src/waveform` unit tests `detail_window_*` |
| FR-003 / SC-009 analysis off the RT path, below-normal thread | `modplayer-core tests/analysis.rs` (A11/A12), receiver `tests/rt_no_alloc.rs` (extended) |
| FR-004 waveform only | `analysis.rs` (no other sections written: `only_wave_section_is_written`) |
| FR-005 / SC-002 progressive fill, placeholders per bucket | `analysis.rs::progressive_store_publishes_partial_then_complete`, `now_playing.rs::placeholder_columns_for_uncovered_buckets` |
| FR-006 / SC-004 cache hit instant, only `Complete` written, ≤ 1 MB | `analysis.rs::cache_hit_publishes_complete_without_a_store`, `::only_complete_entries_reach_disk`, `::ten_minute_entry_fits_one_megabyte` |
| FR-007 / SC-005 analyzer version invalidation | `analysis.rs::stale_analyzer_version_is_unlinked_and_recomputed` |
| FR-008 / Constitution V peaks only, `read_frames` boundary | `modplayer tests/decoded_store_boundary.rs`, `modplayer-audio-source` `store_debug_is_redacted` |
| FR-009 click / drag / Esc / paused / stopped | `now_playing.rs::click_on_overview_seeks_to_exact_frame`, `::drag_previews_without_seeking_and_esc_cancels`, `::seek_while_paused_stays_paused`, `::seek_while_stopped_enters_paused` |
| FR-010 / SC-003 sample-exact seek, ≤ 10 ms | receiver `rt_seek_into_store_is_sample_exact`, `modplayer-core tests/transport_reducer.rs::seek_frames_carries_exact_frame_to_engine`, `modplayer-engine tests/seek.rs` (unchanged), receiver `rt_seek_latency_within_one_buffer` |
| FR-011 zoom/pan bounds | `waveform::detail_window_*` |
| FR-012 follow / recenter / suspend | `waveform::detail_window_follow_*` |
| FR-013 / SC-006 keyboard table | `now_playing.rs::keyboard_table_matches_pointer_results`, `accessibility.rs` |
| FR-014 coordinate space | proptest `waveform::time_space_round_trips_within_one_pixel` |
| FR-015 / SC-007 empty state | `now_playing.rs::empty_state_shows_pick_a_track` |
| FR-016 / SC-008 failure: unavailable, partial kept, no retry, not persisted | `analysis.rs::silent_track_fails_and_writes_nothing`, `::decoder_failure_keeps_partial_peaks`, `::failed_track_is_not_retried_this_session`, `now_playing.rs::analysis_unavailable_label_when_failed_without_peaks` |
| FR-017 current track only, abandon on change | `modplayer-core tests/controller_streaming.rs::track_change_detaches_then_attaches_analysis`, `analysis.rs::detach_drops_partial_without_writing` |
| FR-018 accessible names/roles/values | `accessibility.rs` |
| FR-019 strings externalised | `fluent_keys.rs` |
| FR-020 no markers/loops/overlays/re-analyze UI | `accessibility.rs` (enumeration has no such controls), `fluent_keys.rs` (no such keys) |
| FR-021 retained store: bound, drop discipline, feed rules | `modplayer-audio-source` `store_cap_never_allocates_past_bound`, receiver `rt_track_start_swaps_store_retires_old`, `store_drop_never_on_rt` (three consecutive track changes), `rt_retire_never_full`, `rt_store_feed_drains_ring_in_lockstep`, `rt_store_rescues_ring_underrun`, `rt_store_exhaustion_falls_back_to_ring` |
| SC-001 fill begins ≤ 1 s | `analysis.rs::first_partial_within_one_second` (scripted `Progressive`, injected clock) |
| SC-012 volume-independent peaks | `analysis.rs::peaks_independent_of_master_volume` |
| Constitution IV guard | `modplayer tests/single_dependent.rs` (unchanged, must stay green; `symphonia` and `thread-priority` are permitted in the receiver only — the guard is extended for `symphonia`) |
| Constitution VIII serialization proptests | `analysis.rs::cache_round_trips_any_peaks`, `::cache_rejects_any_truncation` |
| 003 regression | every 003 test in `modplayer-core`, `modplayer-engine`, receiver, `modplayer-ui` stays green; `now_playing::seek_slider_commits_once_per_release` retargeted at the overview |

## Live probe (manual, receiver crate)

```bash
RUSTUP_TOOLCHAIN=1.95.0 cargo test -p modplayer-audio-source-connect --test live \
  decode_ahead_fills_store_faster_than_playback -- --ignored --nocapture
```

Uses the app's own Keychain credential (constitution recipe); expected:
`covered_frames` reaches the full length before 25 % is consumed; a seek
hint at 75 % is covered within 3 s. Filter `bearer|access_token` from any
captured output.

## Manual scenarios (signed-in Premium account; executed by the implementing agent per Constitution › Manual Scenario Sign-Off)

Launch: `RUSTUP_TOOLCHAIN=1.95.0 cargo build -p modplayer && ./target/debug/modplayer`
(background). Fresh cache: `MODPLAYER_ANALYSIS_DIR=$(mktemp -d)`. Drive
with Quartz `CGEventPost`; capture with `screencapture -x -o -l <id>`.

| # | Scenario | Steps | Expected |
|---|---|---|---|
| M1 | First play fills the waveform (US3, SC-001/002/011) | fresh `MODPLAYER_ANALYSIS_DIR`; play a never-analysed 3–5 min track from Library | within 1 s the overview starts showing bars from the left; the undecoded part is visibly hatched; the fill completes well before the track does (≈ download time); no dropout in the audio |
| M2 | Now-playing metadata (US1 AS1/AS2) | while M1 plays, screenshot | artwork (or initials), title, artists, album, elapsed `m:ss` and remaining `-m:ss` both ticking; no 003 slider, no `position / duration` label |
| M3 | Click seek is exact and fast (US1 AS3, SC-003) | click the overview at ≈ 1:23 on the M1 track once the region is filled | audio resumes from there without audible glitch; elapsed jumps to 1:23; app log shows `seek_frames` with the exact frame and the engine `Seek` applied on the next buffer |
| M4 | Drag preview + Esc (US1 AS4) | press on the overview, move right 100 px, hold; then press `Esc`; repeat and release | while held: playhead and labels follow the pointer, audio position unchanged; `Esc` → reverts; release → seeks to the released position |
| M5 | Keyboard seek/zoom/pan (US1 AS5, US2 AS4, SC-006) | `Tab` to the overview; `→`, `Shift+→`, `Home`, `End`; `Tab` to the detail; `+`, `+`, `-`, `Alt+→`, `Alt+Shift+←`, `0` | each key produces the FR-013 effect; the overview highlight tracks the detail window |
| M6 | Pointer zoom/pan + follow (US2 AS1/3/5/6) | pinch/scroll to zoom the detail to ≈ 1 s; let it play past the right edge; then pan away with `Alt+←` and wait; then click the overview elsewhere | window pages forward with the playhead at the left edge; after the pan the window stays put; the seek re-centres and follow resumes |
| M7 | Cached replay (US4 AS1, SC-004) | after M1 completes, skip to another track, then play the M1 track again; also relaunch and play it | full waveform appears immediately, no hatched region; `analysis/<hash>.mpwf` exists and is < 1 MB |
| M8 | Analyzer version bump (US4 AS2, SC-005) | rebuild with `ANALYZER_VERSION` bumped; play the M1 track | waveform recomputes progressively; the old file is replaced |
| M9 | Two cached tracks never mix (US4 AS3) | with two cached tracks, alternate between them | each shows its own waveform instantly |
| M10 | Seek-ahead into an undecoded region (US3 AS4) | on a fresh track, within 2 s of starting, click at 90 % on the overview | playback lands there within ≈ 50 ms of the click position once buffered; the skipped middle stays hatched and fills later; `covers(90 %)` region fills within a few seconds |
| M11 | Empty state (SC-007) | launch signed in with nothing loaded; open Now Playing | "pick a track" hint, no waveform area, transport disabled as in 003 |
| M12 | Volume independence (SC-012) | play a fresh track; move master volume 20 % → 100 % during the fill; wait for `Complete`; `shasum` the `.mpwf`; delete it, replay at a fixed volume, compare | identical hashes |
| M13 | RT safety under analysis (SC-009) | play 10 minutes of fresh tracks at the Balanced preset with Activity Monitor open | no dropouts; the `analysis` and `decode-ahead` threads show below-normal priority; `assert_no_alloc` suite green |
| M14 | Paused / stopped seek (US1 AS7) | pause, click the overview; stop, click the overview | position moves, transport shows paused both times, no audio starts |
| M15 | Analysis failure (EC-5.9) | `MODPLAYER_DECODE_FORCE_FAIL=1` relaunch; play a track | audio plays normally; the waveform area shows "analysis unavailable" (or the partial computed before the injected failure); switching away and back does not retry; no file written |

Record each result (pass / deviation with evidence) on the scenario task
in `tasks.md`, and any behaviour deviation here and in `research.md`.
