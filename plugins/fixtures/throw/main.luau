-- org.modplayer.fixture.throw (research R18): raises a Lua error on
-- every `position` event to exercise the scheduler's per-handler
-- exception abort (RT3, AbortCause::Exception) — the plugin itself keeps
-- running (a thrown handler aborts only that one call).

local position_count = 0

api.on("position", function(_event)
    position_count = position_count + 1
    error("boom")
end)

-- Fixture-only (L10): lets automated tests read this fixture's counters
-- without a UI (research R18).
api.on("debug_probe", function(request)
    if request.name == "position_count" then
        return position_count
    end
    return nil
end)

api.ready()
