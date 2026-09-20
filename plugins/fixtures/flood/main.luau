-- org.modplayer.fixture.flood (US2 T083, contracts/gateway-and-runtime.md
-- G5): on its first `play_state_changed(playing)`, requests transport
-- focus, then issues 1000 `seek` calls in small batches on a 1ms
-- interval timer — well past the rolling 1s/100-call `transport`
-- category rate limit. Batching keeps every single handler invocation
-- (this one and each timer tick) comfortably inside the 4 ms per-handler
-- budget: even a *locally* refused `rate_limited` call still costs real
-- Lua/host round-trip time, so 1000 of them back to back in one handler
-- would itself blow the budget and abort the handler before it ever
-- logged its count. Spreading them across many timer ticks — all still
-- well inside the rate limiter's rolling 1 s window — issues the same
-- 1000 calls without that self-inflicted abort. The count of calls
-- refused specifically `rate_limited` is logged for the host-side
-- test/manual scenario (M9) to read back.

local flooded = false
local issued = 0
local rate_limited = 0
local BATCH = 20
local TOTAL = 1000

api.on("play_state_changed", function(event)
    api.log.info("flood: handler fired state=" .. tostring(event.state))
    if flooded or event.state ~= "playing" then
        return
    end
    flooded = true

    local fok, ferr = api.transport.request_focus()
    api.log.info("flood: request_focus ok=" .. tostring(fok))

    local handle = api.timers.set_interval(1)

    api.on("timer", function(timer_event)
        if timer_event.handle ~= handle then
            return
        end
        local remaining = TOTAL - issued
        local batch = remaining < BATCH and remaining or BATCH
        for _ = 1, batch do
            issued = issued + 1
            local ok, err = api.transport.seek(0)
            if not ok and err.reason == "rate_limited" then
                rate_limited = rate_limited + 1
            end
        end
        if issued >= TOTAL then
            api.timers.clear(handle)
            api.log.info("flood: " .. tostring(rate_limited) .. " rate_limited")
        end
    end)
end)

api.ready()
