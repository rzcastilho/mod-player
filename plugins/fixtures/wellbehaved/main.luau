-- org.modplayer.fixture.wellbehaved (US3 T091, spec.md priority P3):
-- holds every one of this slice's 9 operable permissions and drives one
-- full observe/act cycle on `ready_ack` — subscribe to position, request
-- focus, arm its own loop region, create and parameterize an effect
-- node at a suggested position, read a meter tick, persist per-track
-- state, and schedule a position timer — logging each step's outcome so
-- a manual run (quickstart.md M8) or a test reading `plugin_log()` can
-- confirm every one of them worked exactly as declared.

local function step(name, ok, err)
    if ok then
        api.log.info(name .. ": ok")
    else
        api.log.error(name .. ": " .. err.code .. "/" .. err.reason)
    end
end

local meter_logged = false
api.on("meter", function(_event)
    if not meter_logged then
        meter_logged = true
        api.log.info("meter: received")
    end
end)

-- Observability only (US3 T100): logs every `marker_changed` this
-- plugin is fanned (markers.read), including the host's own edits to a
-- marker this plugin owns (contracts/plugin-api-v1.md §6: "the plugin
-- then sees marker_changed { actor = 'host' }"). No effect on the
-- ready_ack cycle above.
api.on("marker_changed", function(event)
    api.log.info("marker_changed: actor=" .. tostring(event.actor))
end)

api.on("position_reached", function(event)
    api.log.info("position_reached: handle=" .. tostring(event.handle) .. " position_ms=" .. tostring(event.position_ms))
end)

api.on("ready_ack", function(_event)
    -- 1. position/state observation (playback.observe)
    local sub_ok, sub_err = api.playback.subscribe_position(30)
    step("subscribe_position", sub_ok, sub_err)

    local state_ok = api.playback.state()
    step("playback.state", state_ok ~= nil, { code = "invalid_state", reason = "unexpected_nil" })

    -- 2. queue (queue.write)
    local queue_ok, queue_err = api.queue.list()
    step("queue.list", queue_ok ~= nil, queue_err)

    -- 3. transport focus (transport.control)
    local focus_ok, focus_err = api.transport.request_focus()
    step("request_focus", focus_ok, focus_err)

    -- 4. create and arm its own loop region (markers.write, then
    -- transport.control's arm_loop; markers.read reads it back)
    local region_id, region_err = api.markers.create_loop(1000, 5000, { transient = true })
    step("markers.create_loop", region_id ~= nil, region_err)
    if region_id then
        local arm_ok, arm_err = api.transport.arm_loop(region_id)
        step("arm_loop", arm_ok, arm_err)
    end
    local list_ok, list_err = api.markers.list()
    step("markers.list", list_ok ~= nil, list_err)

    -- 5. effect chain (audio.effects): time_stretch first, then
    -- pitch_shift suggested `before` it — proves the suggestion actually
    -- resolves against a real neighbour.
    local ts_id, ts_err = api.effects.create_node("time_stretch")
    step("create_node(time_stretch)", ts_id ~= nil, ts_err)
    local ps_id, ps_err = api.effects.create_node("pitch_shift", { before = "time_stretch" })
    step("create_node(pitch_shift, before=time_stretch)", ps_id ~= nil, ps_err)
    if ps_id then
        local param_ok, param_err = api.effects.set_param(ps_id, 0, 3.0)
        step("effects.set_param", param_ok, param_err)
    end

    -- 6. per-track state (state.track)
    local set_ok, set_err = api.state.track.set("wellbehaved_seen", true)
    step("state.track.set", set_ok, set_err)

    -- 7. plugin-scoped state (state.plugin)
    local pset_ok, pset_err = api.state.plugin.set("cycles_completed", 1)
    step("state.plugin.set", pset_ok, pset_err)

    -- 8. a position timer (no permission gate)
    local handle, timer_err = api.timers.schedule_at_position(100)
    step("schedule_at_position", handle ~= nil, timer_err)

    api.log.info("wellbehaved: cycle complete")
end)

api.ready()
