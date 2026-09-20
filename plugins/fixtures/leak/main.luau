-- org.modplayer.fixture.leak (research R18): appends a 1 MB string to a
-- growing table on every `position` event to exercise `Lua::
-- set_memory_limit`'s cap (RT4, SuspendCause::Memory) — the plugin's own
-- heap fills and is suspended; the host process is never touched
-- (Constitution I).

local bag = {}
local position_count = 0

api.on("position", function(_event)
    position_count = position_count + 1
    -- A per-call prefix keeps every appended string's *content* unique —
    -- Luau interns identical strings, so appending the same literal
    -- string object every time would never actually grow live memory.
    bag[#bag + 1] = tostring(position_count) .. string.rep("x", 1024 * 1024)
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
