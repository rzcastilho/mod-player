-- org.modplayer.fixture.ui-notify (011-plugin-ui-contributions US5 T113,
-- contracts/overlays-settings-notify.md §3): holds `ui.notify` only, and
-- posts real `api.ui.notify(level, text)` calls one per timer tick
-- rather than in a burst from `debug_probe` itself.
--
-- This is deliberate, not stylistic: `debug_probe`'s own host-side
-- dispatch (`plugins/apply.rs::Request::DebugProbe`) blocks the *host*
-- thread on a bounded (250ms) wait for this plugin's reply — a `notify`
-- call is a real nested RPC back to the very same host, and the host
-- cannot drain that nested request while it is itself blocked waiting on
-- this one (a single-threaded re-entrancy deadlock, timing out at
-- `host_busy` well before all of `n` calls could complete). A `timer`
-- event, by contrast, is fire-and-forget from the host's side (mirrors
-- `flood`'s own "spread across many timer ticks" precedent) — so posting
-- happens from there, one call per tick, and `debug_probe` is only ever
-- used for the *fast, non-nested* control calls (`arm`/`counts`).
--
-- `debug_probe`:
--   * `"arm:<level>:<n>"` — queues `n` valid `notify(level, "fixture
--     notification")` calls, resetting the counters below to zero.
--   * `"arm_invalid:<n>"` — the same, but with a 201-character text
--     (over `MAX_NOTIFY_TEXT_CHARS`), proving N1's "admitted, then
--     refused, but the slot is already gone" rule.
--   * `"counts"` — `{ posted, refused, last_reason, done }`: `done` is
--     `true` once every queued call has been attempted (poll this until
--     `done` before reading `posted`/`refused`, since they fire one per
--     5ms timer tick, not all at once).

local queue_level = nil
local queue_text = nil
local queue_remaining = 0
local posted = 0
local refused = 0
local last_reason = nil

api.on("ready_ack", function(_event)
    api.timers.set_interval(5)
end)

api.on("timer", function(_event)
    if queue_remaining <= 0 then
        return
    end
    local ok, err = api.ui.notify(queue_level, queue_text)
    if ok then
        posted = posted + 1
    else
        refused = refused + 1
        last_reason = err.reason
    end
    queue_remaining = queue_remaining - 1
end)

api.on("debug_probe", function(request)
    local level, count = string.match(request.name, "^arm:([%a]+):(%d+)$")
    if level then
        queue_level = level
        queue_text = "fixture notification"
        queue_remaining = tonumber(count)
        posted = 0
        refused = 0
        last_reason = nil
        return { ok = true }
    end
    local invalid_count = string.match(request.name, "^arm_invalid:(%d+)$")
    if invalid_count then
        queue_level = "info"
        queue_text = string.rep("x", 201)
        queue_remaining = tonumber(invalid_count)
        posted = 0
        refused = 0
        last_reason = nil
        return { ok = true }
    end
    if request.name == "counts" then
        return {
            posted = posted,
            refused = refused,
            last_reason = last_reason,
            done = queue_remaining <= 0,
        }
    end
    return nil
end)

api.ready()
