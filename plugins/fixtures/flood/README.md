# Flood fixture

US2 rate-limit fixture (spec.md priority P2, G5). Requires
`playback.observe` (its one-shot `play_state_changed(playing)` trigger)
and `transport.control`. On that first event it requests transport focus
then issues 1 000 `seek` calls back to back, well past the rolling
1 s / 100-call `transport` category cap (`RateLimiter`, G5): the 101st and
later calls within that window are refused `rate_limited` without ever
reaching the host, and consume no further quota. The fixture logs how
many calls were refused for that reason.

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.
