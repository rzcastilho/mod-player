-- org.modplayer.fixture.focus-a (010-transport-focus, data-model.md §5):
-- requests transport focus on ready_ack; on every focus_granted, seeks
-- to 1000ms, creates a transient loop region and arms it; re-requests
-- focus on every track_changed (US2's per-track re-arbitration). Logs
-- every focus_granted/focus_revoked/play_state_changed/loop_armed/
-- loop_disarmed event it receives, and deliberately hangs (a busy loop)
-- on the third play_state_changed it receives, so the "holder suspended
-- -> host within a second" manual scenario (quickstart.md M5b) is
-- reproducible from the transport rather than depending on a
-- machine-dependent CPU-share suspension. debug_probe("log") returns
-- the accumulated log.

local log = {}
local play_state_changed_count = 0

local function record(entry)
    log[#log + 1] = entry
end

local function request_focus()
    local ok, err = api.transport.request_focus()
    if ok then
        record("request_focus: ok")
    else
        record("request_focus: " .. err.code .. "/" .. err.reason)
    end
end

api.on("ready_ack", function(_event)
    request_focus()
end)

api.on("focus_granted", function(event)
    record("focus_granted: holder=" .. tostring(event.holder))

    local seek_ok, seek_err = api.transport.seek(1000)
    if not seek_ok then
        record("seek: " .. seek_err.code .. "/" .. seek_err.reason)
    end

    local region_id, region_err = api.markers.create_loop(1000, 3000, { transient = true })
    if region_id then
        local arm_ok, arm_err = api.transport.arm_loop(region_id)
        if not arm_ok then
            record("arm_loop: " .. arm_err.code .. "/" .. arm_err.reason)
        end
    else
        record("markers.create_loop: " .. region_err.code .. "/" .. region_err.reason)
    end
end)

api.on("focus_revoked", function(event)
    record("focus_revoked: holder=" .. tostring(event.holder))
end)

api.on("track_changed", function(_event)
    request_focus()
end)

api.on("loop_armed", function(event)
    record("loop_armed: region=" .. tostring(event.region) .. " by=" .. tostring(event.by))
end)

api.on("loop_disarmed", function(event)
    record("loop_disarmed: by=" .. tostring(event.by))
end)

api.on("play_state_changed", function(event)
    play_state_changed_count = play_state_changed_count + 1
    record("play_state_changed: state=" .. tostring(event.state))
    if play_state_changed_count == 3 then
        -- Deliberate hang (research R18-style containment exercise):
        -- the scheduler's per-handler budget interrupt (RT3) aborts this
        -- call, and the aggregate 1s CPU share the resulting retries
        -- blow suspends this plugin (RT4) — the manual/test-driven
        -- trigger for "holder suspended -> host" (SC-003).
        while true do
        end
    end
end)

api.on("debug_probe", function(request)
    if request.name == "log" then
        return log
    end
    return nil
end)

api.ready()
