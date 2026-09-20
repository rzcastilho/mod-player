-- org.modplayer.fixture.ui-shortcuts (011-plugin-ui-contributions US2
-- T077, contracts/action-registry-plugins.md): registers three actions
-- on ready_ack:
--   * "take_over" -> default binding "L", deliberately colliding with
--                    the host's own ToggleLoop (host.loop.toggle, also
--                    "L") for M5: the host must win, this binding stays
--                    flagged and never fires.
--   * "nudge"      -> continuous, default binding "Shift+K", colliding
--                    with the ui-panel fixture's own "focus_me" (also
--                    Shift+K, both Bundled tier) for M6: both flagged,
--                    neither fires until the user rebinds one.
--   * "tab_bound"  -> default binding "Tab", which 007 FR-007's own
--                    capture would reject (G10): registers successfully
--                    but unbound, with a console warning.
-- Every invocation increments a per-action counter, probed by name.

local invoked = { take_over = 0, nudge = 0, tab_bound = 0 }
local last_value = nil

api.on("ready_ack", function(_event)
    local specs = {
        { id = "take_over", label = "Take over (collides with host L)", kind = "trigger", default_binding = "L" },
        { id = "nudge", label = "Nudge (continuous)", kind = "continuous", default_binding = "Shift+K" },
        { id = "tab_bound", label = "Tab-bound (rejected default)", kind = "trigger", default_binding = "Tab" },
    }
    for _, spec in ipairs(specs) do
        local ok, err = api.ui.register_action(spec)
        if not ok then
            api.log.error("register_action(" .. spec.id .. "): " .. err.code .. "/" .. err.reason .. " " .. err.message)
        end
    end
end)

api.on("action_invoked", function(event)
    local name = event.action:match("%.([^.]+)$")
    if name and invoked[name] ~= nil then
        invoked[name] = invoked[name] + 1
    end
    last_value = event.value
end)

api.on("debug_probe", function(request)
    if request.name == "invoked_take_over" then
        return invoked.take_over
    elseif request.name == "invoked_nudge" then
        return invoked.nudge
    elseif request.name == "invoked_tab_bound" then
        return invoked.tab_bound
    elseif request.name == "last_value" then
        return last_value
    end
    return nil
end)

api.ready()
