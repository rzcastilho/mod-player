-- org.modplayer.fixture.focus-b (010-transport-focus, data-model.md §5):
-- requests transport focus on ready_ack; on every focus_granted, seeks
-- to 2000ms; re-requests focus on every track_changed. While it does
-- not currently hold focus, attempts seek(0) on every play_state_changed
-- and logs the resulting no_focus refusal — the single-holder
-- contention proof (quickstart.md M2, SC-001) when paired with
-- focus-a. debug_probe("log") returns the accumulated log.

local log = {}
local is_holder = false

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
    is_holder = true
    record("focus_granted: holder=" .. tostring(event.holder))

    local seek_ok, seek_err = api.transport.seek(2000)
    if not seek_ok then
        record("seek: " .. seek_err.code .. "/" .. seek_err.reason)
    end
end)

api.on("focus_revoked", function(event)
    is_holder = false
    record("focus_revoked: holder=" .. tostring(event.holder))
end)

api.on("track_changed", function(_event)
    request_focus()
end)

api.on("play_state_changed", function(event)
    record("play_state_changed: state=" .. tostring(event.state))
    if not is_holder then
        local ok, err = api.transport.seek(0)
        if ok then
            record("seek: ok")
        else
            record("seek: " .. err.code .. "/" .. err.reason)
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
