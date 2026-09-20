-- org.modplayer.fixture.ui-panel (011-plugin-ui-contributions US1 T059,
-- US2 T078, contracts/ui-panels.md, contracts/action-registry-plugins.md
-- D3): registers panel "Controls" with one of every widget kind, then
-- drives its interaction probes from `panel_interaction` (`hang`/`throw`)
-- and `action_invoked` (`take_over`, D3 coverage: the "Take Over" button
-- names `action = "take_over"`, so it never produces `panel_interaction`
-- at all — the host resolves and invokes the action instead):
--   * "take_over"    -> api.transport.request_focus() from *inside* the
--                       action_invoked handler (R16/FR-026: counts as a
--                       user interaction, auto-granted under the default
--                       AutoOnInteraction policy).
--   * "hang"         -> a deliberate busy loop (RT3's per-handler budget
--                       interrupt aborts it; a single abort never
--                       suspends the plugin outright, L5).
--   * "throw"         -> a deliberate Lua error (RT3's own
--                       AbortCause::Exception; FR-023/SC-007: contained,
--                       the plugin keeps running).
--   * "notify7"       -> posts 7 `api.ui.notify("info", ..)` calls in a
--                       row (US5 T114, manual scenario M9): the notify
--                       bucket admits only 6 per 60s, so the 7th is
--                       refused `rate_limited` and never shown.
-- Every other panel widget id just increments `interaction_count` so a
-- test can prove `panel_interaction` really was delivered exactly once
-- per commit.
--
-- `debug_probe("register_bad")` calls `api.ui.register_panel` directly
-- with a deliberately unlabeled widget (M2: proven refused, with the
-- offending widget's path, and the plugin's real "Controls" panel left
-- untouched) — the plumbing FR-001/FR-002a's refusal message names.
--
-- On `ready_ack`, also registers two `ui.shortcuts` actions — "take_over"
-- (unbound; only ever invoked via its own panel button, D3) and
-- "focus_me" (default `Shift+K`, deliberately colliding with the
-- `ui-shortcuts` fixture's own `nudge` for M6, T078) — then schedules a
-- 20ms timer; its own handler calls request_focus() *outside* any
-- interaction handler, so the request is merely recorded (never
-- auto-granted) — the R16 contrast case.

local interaction_count = 0
local last_interaction = nil
local action_invoked_count = 0
local last_action_invoked = nil

local function register_controls()
    return api.ui.register_panel("main", "Controls", {
        { id = "info", kind = "label", label = "Info", text = "Fixture panel" },
        { id = "take_over", kind = "button", label = "Take Over", action = "take_over" },
        { id = "hang", kind = "button", label = "Hang" },
        { id = "throw", kind = "button", label = "Throw" },
        { id = "notify7", kind = "button", label = "Notify ×7" },
        { id = "power", kind = "toggle", label = "Power" },
        { id = "tempo", kind = "slider", label = "Tempo", min = 0, max = 200, step = 1, value = 120 },
        { id = "gain", kind = "knob", label = "Gain", min = 0, max = 10, step = 0.1, value = 5 },
        {
            id = "mode",
            kind = "list",
            label = "Mode",
            items = {
                { id = "a", label = "Mode A" },
                { id = "b", label = "Mode B" },
            },
            selected = "a",
        },
        { id = "status", kind = "text", label = "Status", text = "Idle" },
        { id = "markers", kind = "marker_list", label = "Markers" },
        { id = "level", kind = "meter", label = "Level" },
    })
end

local function register_actions()
    local ok1, err1 = api.ui.register_action({ id = "take_over", label = "Take over transport", kind = "trigger" })
    if not ok1 then
        api.log.error("register_action(take_over): " .. err1.code .. "/" .. err1.reason .. " " .. err1.message)
    end
    local ok2, err2 = api.ui.register_action({
        id = "focus_me",
        label = "Focus me",
        kind = "trigger",
        default_binding = "Shift+K",
    })
    if not ok2 then
        api.log.error("register_action(focus_me): " .. err2.code .. "/" .. err2.reason .. " " .. err2.message)
    end
end

api.on("ready_ack", function(_event)
    local ok, err = register_controls()
    if not ok then
        api.log.error("register_panel: " .. err.code .. "/" .. err.reason .. " " .. err.message)
    end
    register_actions()
    api.timers.set_timeout(20)
end)

api.on("panel_interaction", function(event)
    interaction_count = interaction_count + 1
    last_interaction = event

    if event.widget == "hang" then
        while true do
        end
    elseif event.widget == "throw" then
        error("boom")
    elseif event.widget == "notify7" then
        for i = 1, 7 do
            local ok, err = api.ui.notify("info", "notify " .. tostring(i))
            if not ok then
                api.log.error("notify (panel notify7): " .. err.code .. "/" .. err.reason)
            end
        end
    end
end)

api.on("action_invoked", function(event)
    action_invoked_count = action_invoked_count + 1
    last_action_invoked = event

    if event.action == "org.modplayer.fixture.ui-panel.take_over" then
        local ok, err = api.transport.request_focus()
        if not ok then
            api.log.error("request_focus (action_invoked): " .. err.code .. "/" .. err.reason)
        end
    end
end)

-- R16/FR-026 contrast case: a `request_focus()` made from a timer handler
-- (never an interaction handler) is only ever recorded, never
-- auto-granted, regardless of policy.
api.on("timer", function(_event)
    local ok, err = api.transport.request_focus()
    if not ok then
        api.log.error("request_focus (timer): " .. err.code .. "/" .. err.reason)
    end
end)

api.on("debug_probe", function(request)
    if request.name == "interaction_count" then
        return interaction_count
    elseif request.name == "last_interaction" then
        return last_interaction
    elseif request.name == "action_invoked_count" then
        return action_invoked_count
    elseif request.name == "last_action_invoked" then
        return last_action_invoked
    elseif request.name == "register_bad" then
        local ok, err = api.ui.register_panel("bad", "Bad panel", {
            { id = "unlabeled", kind = "label", label = "" },
        })
        if ok then
            return { ok = true }
        end
        return { ok = false, code = err.code, reason = err.reason, message = err.message }
    end
    return nil
end)

api.ready()
