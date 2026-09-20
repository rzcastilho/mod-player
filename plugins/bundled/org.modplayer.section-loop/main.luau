-- org.modplayer.section-loop -- the reference bundled plugin: drop A and
-- B around a passage and drill it hands-free (spec.md US1/US2/US3,
-- contracts/section-loop-plugin.md). Every `api.<ns>.<method>(` call
-- below is public plugin API surface (SC-008) -- this file is valid,
-- unmodified example code for a third-party plugin author.
--
-- Architecture (research R6): one session-only state table `S`, filled
-- only by `relist()` from `api.markers.list()` -- every mutating call is
-- followed by nothing; the host's own `marker_changed`/`loop_armed`/
-- `loop_disarmed` fan-out is what drives the next `relist()`/widget
-- update, so a refusal never leaves this script "guessing" what the host
-- did (contract A12).
--
-- 012-section-loop-plugin Phase 3 (User Story 1) implements mark/loop
-- drilling: `set_a`/`set_b`, `toggle_loop`, `repeat`, `nudge_earlier`/
-- `nudge_later`, and every event handler `relist()` needs. Phase 5 (User
-- Story 3) adds cue set/jump, `clear_markers` and the inert `snap`
-- toggle -- focus arbitration for the loop toggle itself needed no new
-- code (contract A11's `request_focus()`-first shape already covers it).

local IDENTIFIER = "org.modplayer.section-loop"

-- Session-only state (data-model.md §3.2) -- never persisted; the
-- manifest declares no `state.*` permission.
local S = {
    region = nil, -- this plugin's own region id, or nil
    a = nil, -- A endpoint marker id, or nil
    b = nil, -- B endpoint marker id, or nil
    a_ms = nil, -- A's position, or nil
    b_ms = nil, -- B's position, or nil
    cues = {}, -- [1..8] = { id, ms, owner_is_me } for any owner's cue
    active = nil, -- "a" | "b" | nil (FR-006)
    armed = false, -- mirrors loop_armed/loop_disarmed for S.region
    pending_repeat = nil, -- a slider value committed before any region exists
}

local function log_error(where, err)
    api.log.error(where .. ": " .. err.code .. "/" .. err.reason .. " " .. err.message)
end

-- "Status" = the panel's `status` text widget (contract §3).
local function set_status(text)
    local ok, err = api.ui.update_widget("main", "status", text)
    if not ok then
        log_error("update_widget(status)", err)
    end
end

local function clear_status()
    set_status("")
end

-- §4 E1: rebuild every piece of session state -- and this plugin's
-- overlays and mirrored widgets -- from `markers.list()`. The only
-- reader of `markers.list()` in this script; called on `ready_ack`,
-- `track_changed` and `marker_changed`.
local function relist()
    local result, err = api.markers.list()
    if not result then
        log_error("markers.list", err)
        return
    end

    local had_region = S.region ~= nil

    -- This plugin's own region: at most one is expected (spec.md); if
    -- several exist (e.g. after a host-side edit sequence), the lowest
    -- id is used and the rest are logged (contract E1).
    local mine = {}
    for _, region in ipairs(result.regions) do
        if region.owner == IDENTIFIER then
            table.insert(mine, region)
        end
    end
    table.sort(mine, function(x, y)
        return x.id < y.id
    end)
    if #mine > 1 then
        api.log.warn("relist: " .. tostring(#mine) .. " own regions found; using the lowest id")
    end
    local region = mine[1]

    if region then
        S.region = region.id
        S.a = region.a
        S.b = region.b
        S.armed = region.armed
    else
        S.region = nil
        S.a = nil
        S.b = nil
        S.armed = false
    end
    S.a_ms = nil
    S.b_ms = nil

    -- Endpoint positions and every cue slot (any owner) come from the
    -- flat marker list.
    local cues = {}
    for _, marker in ipairs(result.markers) do
        if S.a and marker.id == S.a then
            S.a_ms = marker.position_ms
        end
        if S.b and marker.id == S.b then
            S.b_ms = marker.position_ms
        end
        if marker.kind == "cue" and marker.slot then
            cues[marker.slot] = {
                id = marker.id,
                ms = marker.position_ms,
                owner_is_me = marker.owner == IDENTIFIER,
            }
        end
    end
    S.cues = cues

    -- FR-006: drop the active pointer once its own endpoint is gone.
    if S.active == "a" and not S.a then
        S.active = nil
    elseif S.active == "b" and not S.b then
        S.active = nil
    end

    -- T034/A9: a repeat value committed while no region existed is
    -- applied the moment this plugin's own region first appears.
    if region and not had_region and S.pending_repeat ~= nil then
        local rok, rerr = api.markers.set_loop_repeat(S.region, S.pending_repeat)
        if not rok then
            log_error("set_loop_repeat(pending)", rerr)
        end
        S.pending_repeat = nil
    end

    -- Widgets: the Loop toggle and Repeat slider always mirror the
    -- region -- never the other way around (contract §3.3).
    local loop_ok, loop_err = api.ui.update_widget("main", "loop", S.armed)
    if not loop_ok then
        log_error("update_widget(loop)", loop_err)
    end
    local repeat_value = 0
    if region and region["repeat"] ~= "infinite" then
        repeat_value = region["repeat"]
    end
    local repeat_ok, repeat_err = api.ui.update_widget("main", "repeat", repeat_value)
    if not repeat_ok then
        log_error("update_widget(repeat)", repeat_err)
    end

    -- Overlays: rebuilt from scratch every time (§5 O1/O2) -- at most
    -- 21 primitives (Phase 5 adds the per-slot cue dot/label pair).
    local primitives = {}
    if S.a_ms then
        table.insert(primitives, { id = "a_line", kind = "line", at = S.a_ms })
        table.insert(primitives, { id = "a_label", kind = "label", at = S.a_ms, text = "@label_a" })
    end
    if S.b_ms then
        table.insert(primitives, { id = "b_line", kind = "line", at = S.b_ms })
        table.insert(primitives, { id = "b_label", kind = "label", at = S.b_ms, text = "@label_b" })
    end
    if S.a_ms and S.b_ms and S.a_ms < S.b_ms then
        table.insert(primitives, {
            id = "ab_region",
            kind = "region",
            from = S.a_ms,
            to = S.b_ms,
            color = "accent",
        })
    end
    -- T057: a dot + label per set cue slot (any owner) -- never more
    -- than 21 primitives total (contract O1).
    for n = 1, 8 do
        local cue = S.cues[n]
        if cue then
            table.insert(primitives, { id = "cue_" .. n .. "_dot", kind = "glyph", at = cue.ms, icon = "dot" })
            table.insert(primitives, {
                id = "cue_" .. n .. "_label",
                kind = "label",
                at = cue.ms,
                text = "@cue_label_" .. n,
            })
        end
    end

    local clear_ok, clear_err = api.ui.clear_overlays()
    if not clear_ok then
        log_error("clear_overlays", clear_err)
    end
    if #primitives > 0 then
        local add_ok, add_err = api.ui.add_overlays(primitives)
        if not add_ok then
            log_error("add_overlays", add_err)
        end
    end
end

-- A1/A2: `set_a`/`set_b` read the playhead and set/move the named
-- endpoint of this plugin's own region (creating it on the first call).
local function set_a()
    local state, serr = api.playback.state()
    if not state then
        log_error("playback.state", serr)
        return
    end
    local ok, err = api.markers.set_loop_endpoint(S.region, "a", state.position_ms)
    if not ok then
        set_status(err.message)
        return
    end
    S.active = "a"
    clear_status()
end

local function set_b()
    local state, serr = api.playback.state()
    if not state then
        log_error("playback.state", serr)
        return
    end
    local ok, err = api.markers.set_loop_endpoint(S.region, "b", state.position_ms)
    if not ok then
        set_status(err.message)
        return
    end
    S.active = "b"
    clear_status()
end

-- A3/A4/A11: `request_focus()` is always the first call; a later
-- `focus_granted` never arms/disarms by itself (FR-008).
local function toggle_loop(value)
    if value then
        if not S.region then
            set_status("@status_no_region")
            local uok, uerr = api.ui.update_widget("main", "loop", false)
            if not uok then
                log_error("update_widget(loop)", uerr)
            end
            return
        end
        local fok, ferr = api.transport.request_focus()
        if not fok then
            log_error("request_focus", ferr)
        end
        local ok, err = api.transport.arm_loop(S.region)
        if not ok then
            local uok, uerr = api.ui.update_widget("main", "loop", false)
            if not uok then
                log_error("update_widget(loop)", uerr)
            end
            if err.code == "no_focus" then
                set_status("@status_no_focus")
            else
                set_status(err.message)
            end
            return
        end
        clear_status()
    else
        local fok, ferr = api.transport.request_focus()
        if not fok then
            log_error("request_focus", ferr)
        end
        local ok, err = api.transport.disarm_loop()
        if not ok then
            local uok, uerr = api.ui.update_widget("main", "loop", true)
            if not uok then
                log_error("update_widget(loop)", uerr)
            end
            if err.code == "no_focus" then
                set_status("@status_no_focus")
            else
                set_status(err.message)
            end
            return
        end
        clear_status()
    end
end

-- A9: the slider commits `v`; `0` means infinite.
local function set_repeat(v)
    local arg
    if v == 0 then
        arg = "infinite"
    else
        arg = v
    end
    if S.region then
        local ok, err = api.markers.set_loop_repeat(S.region, arg)
        if not ok then
            set_status(err.message)
            return
        end
        clear_status()
    else
        S.pending_repeat = arg
        clear_status()
    end
end

-- A5/FR-006: nudge the active marker (or B, or A) by `delta` ms, floored
-- at 0 -- a no-op with no endpoint at all.
local function nudge(delta)
    local target_id, target_which
    if S.active == "a" and S.a then
        target_id, target_which = S.a, "a"
    elseif S.active == "b" and S.b then
        target_id, target_which = S.b, "b"
    elseif S.b then
        target_id, target_which = S.b, "b"
    elseif S.a then
        target_id, target_which = S.a, "a"
    end
    if not target_id then
        return
    end
    local base_ms = target_which == "a" and S.a_ms or S.b_ms
    if not base_ms then
        return
    end
    local new_ms = base_ms + delta
    if new_ms < 0 then
        new_ms = 0
    end
    local ok, err = api.markers.move(target_id, new_ms)
    if not ok then
        set_status(err.message)
        return
    end
    S.active = target_which
    clear_status()
end

-- A6: `set_cue_n` sets/moves this plugin's own cue in slot `n` to the
-- playhead. A slot another owner (usually the host) already holds is
-- refused `not_owner` -- shown as the per-slot hint, never a generic
-- message (contract A6).
local function set_cue(n)
    local state, serr = api.playback.state()
    if not state then
        log_error("playback.state", serr)
        return
    end
    local ok, err = api.markers.set_cue(n, state.position_ms)
    if not ok then
        if err.reason == "not_owner" then
            set_status("@status_cue_owned_" .. n)
        else
            set_status(err.message)
        end
        return
    end
    clear_status()
end

-- A7/A11: `jump_cue_n` is a silent no-op on an empty slot; otherwise
-- `request_focus()` is always the first call, then the seek -- any
-- owner's cue is a valid jump target (FR-010).
local function jump_cue(n)
    local cue = S.cues[n]
    if not cue then
        return
    end
    local fok, ferr = api.transport.request_focus()
    if not fok then
        log_error("request_focus", ferr)
    end
    local ok, err = api.transport.seek(cue.ms)
    if not ok then
        if err.code == "no_focus" then
            set_status("@status_no_focus")
        else
            set_status(err.message)
        end
        return
    end
    clear_status()
end

-- A8: deletes this plugin's own A, B and every own cue -- never a
-- host-owned (or other plugin's) cue, no `disarm_loop`, no focus call.
-- Deleting an armed region's own endpoint disarms it host-side
-- (`delete_marker`'s own `LoopDisarm` push) -- this script never calls
-- `disarm_loop` itself here.
local function clear_markers()
    local first_err = nil
    local function try_delete(id)
        local ok, err = api.markers.delete(id)
        if not ok and not first_err then
            first_err = err
        end
    end
    if S.a then
        try_delete(S.a)
    end
    if S.b then
        try_delete(S.b)
    end
    for n = 1, 8 do
        local cue = S.cues[n]
        if cue and cue.owner_is_me then
            try_delete(cue.id)
        end
    end
    if first_err then
        set_status(first_err.message)
    else
        clear_status()
    end
end

-- A10: the `snap` toggle and `toggle_snap` action are permanently
-- inert -- the widget always reverts to `false` (FR-005; beat snapping
-- needs `analysis.read`, reserved but unused this slice).
local function toggle_snap()
    local ok, err = api.ui.update_widget("main", "snap", false)
    if not ok then
        log_error("update_widget(snap)", err)
    end
end

-- G1/G2: 23 trigger actions (data-model.md §3.4). Phase 3 (US1)
-- implements the first five handlers below; the cue set/jump,
-- `clear_markers` and `toggle_snap` handlers are the Phase 5 (US3)
-- additions just above.
local ACTION_SPECS = {
    { id = "set_a", label = "@action_set_a", kind = "trigger", default_binding = "I" },
    { id = "set_b", label = "@action_set_b", kind = "trigger", default_binding = "O" },
    { id = "toggle_loop", label = "@action_toggle_loop", kind = "trigger", default_binding = "L" },
    {
        id = "nudge_earlier",
        label = "@action_nudge_earlier",
        kind = "trigger",
        default_binding = "OpenBracket",
    },
    {
        id = "nudge_later",
        label = "@action_nudge_later",
        kind = "trigger",
        default_binding = "CloseBracket",
    },
}
for n = 1, 8 do
    table.insert(ACTION_SPECS, {
        id = "set_cue_" .. n,
        label = "@action_set_cue_" .. n,
        kind = "trigger",
    })
end
for n = 1, 8 do
    table.insert(ACTION_SPECS, {
        id = "jump_cue_" .. n,
        label = "@action_jump_cue_" .. n,
        kind = "trigger",
    })
end
table.insert(ACTION_SPECS, { id = "toggle_snap", label = "@action_toggle_snap", kind = "trigger" })
table.insert(ACTION_SPECS, { id = "clear_markers", label = "@action_clear_markers", kind = "trigger" })

local function register_actions()
    for _, spec in ipairs(ACTION_SPECS) do
        local ok, err = api.ui.register_action(spec)
        if not ok then
            log_error("register_action(" .. spec.id .. ")", err)
        end
    end
end

-- G1: one panel, widgets in data-model.md §3.3's order.
local function register_panel()
    return api.ui.register_panel("main", "@panel_title", {
        { id = "set_a", kind = "button", label = "@set_a" },
        { id = "set_b", kind = "button", label = "@set_b" },
        { id = "loop", kind = "toggle", label = "@loop" },
        { id = "repeat", kind = "slider", label = "@repeat", min = 0, max = 1000, step = 1, value = 0 },
        { id = "markers", kind = "marker_list", label = "@markers" },
        { id = "snap", kind = "toggle", label = "@snap" },
        { id = "snap_note", kind = "label", label = "@snap_note", text = "@snap_note_text" },
        { id = "status", kind = "text", label = "@status", text = "" },
    })
end

-- G4/L2/FR-015: register everything, then relist -- a plugin enabled or
-- restarted mid-track shows the current track's markers immediately.
api.on("ready_ack", function(_event)
    register_actions()
    local ok, err = register_panel()
    if not ok then
        log_error("register_panel", err)
    end
    relist()
end)

-- E2/E3: the host has already restored/cleared everything for the new
-- track (or coalesced the marker edit) before either event reaches this
-- handler.
api.on("track_changed", function(_event)
    S.active = nil
    relist()
end)

api.on("marker_changed", function(_event)
    relist()
end)

-- E4: covers finite-repeat release, host `L`, endpoint deletion and a
-- focus-loss disarm alike -- this plugin never counts wraps itself.
api.on("loop_armed", function(event)
    S.armed = event.region == S.region
    local ok, err = api.ui.update_widget("main", "loop", S.armed)
    if not ok then
        log_error("update_widget(loop)", err)
    end
end)

api.on("loop_disarmed", function(_event)
    S.armed = false
    local ok, err = api.ui.update_widget("main", "loop", false)
    if not ok then
        log_error("update_widget(loop)", err)
    end
end)

-- E5/FR-008: log only -- no deferred arm/seek.
api.on("focus_granted", function(event)
    api.log.info("focus_granted: " .. tostring(event.holder))
end)

api.on("focus_revoked", function(event)
    api.log.info("focus_revoked: " .. tostring(event.holder))
end)

-- Panel interaction dispatch (research R6). "Set A"/"Set B" are plain
-- buttons (R11) routed here rather than through an `action` field, so
-- they stay operable even while their keyboard chord is flagged on a
-- fresh install (contract G3).
api.on("panel_interaction", function(event)
    if event.widget == "set_a" then
        set_a()
    elseif event.widget == "set_b" then
        set_b()
    elseif event.widget == "loop" then
        toggle_loop(event.value == true)
    elseif event.widget == "repeat" then
        set_repeat(event.value)
    elseif event.widget == "snap" then
        toggle_snap()
    end
end)

-- Action dispatch, keyed by the action's own short id (the part after
-- the last `.` in its fully-namespaced id).
local ACTION_HANDLERS = {
    set_a = set_a,
    set_b = set_b,
    toggle_loop = function(_value)
        toggle_loop(not S.armed)
    end,
    nudge_earlier = function(_value)
        nudge(-10)
    end,
    nudge_later = function(_value)
        nudge(10)
    end,
    toggle_snap = function(_value)
        toggle_snap()
    end,
    clear_markers = function(_value)
        clear_markers()
    end,
}
for n = 1, 8 do
    ACTION_HANDLERS["set_cue_" .. n] = function(_value)
        set_cue(n)
    end
    ACTION_HANDLERS["jump_cue_" .. n] = function(_value)
        jump_cue(n)
    end
end

api.on("action_invoked", function(event)
    local name = event.action:match("%.([^.]+)$")
    local handler = name and ACTION_HANDLERS[name]
    if handler then
        handler(event.value)
    end
end)

api.ready()
