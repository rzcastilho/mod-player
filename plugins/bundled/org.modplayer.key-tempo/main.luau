-- org.modplayer.key-tempo -- the second bundled plugin: transpose a song
-- by semitones (with fine cents) or change its tempo (25-200%) without
-- touching the other, and remember a track's settings when asked
-- (spec.md US1/US2/US3, contracts/key-tempo-plugin.md). Every
-- `api.<ns>.<method>(` call below is public plugin API surface (SC-008)
-- -- this file is valid, unmodified example code for a third-party
-- plugin author.
--
-- Architecture (research R5): the panel and the stored `settings` entry
-- both only ever *mirror* the host's own two effect nodes -- this script
-- holds no DSP and no parameter truth of its own (Constitution III). The
-- host's `effect_chain_changed` event (API 1.4, `params`/`auto_switched`)
-- is what drives every widget update; a plugin action only ever calls
-- `api.effects.set_param` and otherwise changes nothing until the host
-- confirms it (contract A12).
--
-- 013-key-and-tempo-plugin Phase 2 (Foundational) implements this
-- skeleton: action/panel registration (G1), node adoption/creation (G2),
-- and the E1 (node re-find + tempo/formant/quality mirror) and E2
-- (badge reset + entry lookup) skeletons. Phases 3-5 (User Stories 1-3)
-- fill in every action handler, the key/cents split (FR-006), the
-- `settings` read/write cycle and the full `track_changed` apply logic.

local IDENTIFIER = "org.modplayer.key-tempo"

-- Session-only state (data-model.md §3.2) -- never persisted; only the
-- `settings` entry (via `api.state.track`) crosses a restart.
local S = {
    pitch = nil, -- this plugin's own pitch_shift node id, or nil
    stretch = nil, -- this plugin's own time_stretch node id, or nil
    key = 0, -- -12..12
    cents = 0, -- -50..50
    tempo = 100, -- 25..200, mirrors the stretch node's ratio * 100
    formant = false,
    quality = false, -- OR of both nodes' quality_mode
    sent = nil, -- { semitones, key, cents } last composed and sent, or nil
    step = 10, -- 1..50, configured tempo step
    remember = false, -- current track has a `settings` entry
    keep_across = false, -- session-wide; never persisted (FR-011)
    restored = false, -- badge condition for the current track
    last_written = nil, -- last `settings` table written, or nil
    node_removed = false, -- a node was removed by the user while Active
}

local function log_error(where, err)
    api.log.error(where .. ": " .. err.code .. "/" .. err.reason .. " " .. err.message)
end

-- "Restored" = the panel's `restored` text widget -- doubles as the
-- badge (contract A9/A12) and the node-removed notice (A11).
local function set_restored(text)
    local ok, err = api.ui.update_widget("main", "restored", text)
    if not ok then
        log_error("update_widget(restored)", err)
    end
end

local function clamp(v, lo, hi)
    if v < lo then
        return lo
    elseif v > hi then
        return hi
    end
    return v
end

-- FR-006, contract E1 step 3: round-half-up (ties round towards +inf),
-- used only when decomposing a `semitones` value this script did not
-- itself just send (not matched by `S.sent`).
local function round_half_up(v)
    return math.floor(v + 0.5)
end

-- Contract M1/M3: the one `settings` table, built only from the values
-- this script has already mirrored from the host (never from an
-- unconfirmed request) -- data-model.md §3.3's field order.
local function current_table()
    return {
        key = S.key,
        cents = S.cents,
        tempo = S.tempo,
        formant = S.formant,
        quality = S.quality and "quality" or "performance",
    }
end

local function tables_equal(a, b)
    if not b then
        return false
    end
    return a.key == b.key
        and a.cents == b.cents
        and a.tempo == b.tempo
        and a.formant == b.formant
        and a.quality == b.quality
end

-- M4 (T046): field-by-field defaulting on read -- an out-of-range,
-- wrong-typed or missing field is read as its default; the entry is
-- never refused or removed by the plugin for being malformed (the next
-- E1 step 5 rewrite, on any subsequent mirrored change, makes the
-- stored table whole again).
local function valid_int(v, lo, hi)
    return type(v) == "number" and v == math.floor(v) and v >= lo and v <= hi
end

local function validate_entry(entry)
    local quality = entry.quality
    if quality ~= "performance" and quality ~= "quality" then
        quality = "performance"
    end
    return {
        key = valid_int(entry.key, -12, 12) and entry.key or 0,
        cents = valid_int(entry.cents, -50, 50) and entry.cents or 0,
        tempo = valid_int(entry.tempo, 25, 200) and entry.tempo or 100,
        formant = type(entry.formant) == "boolean" and entry.formant or false,
        quality = quality,
    }
end

-- §4 E1 (skeleton): re-find this plugin's own nodes by `owner`/`kind`
-- from a `list_chain()`/`effect_chain_changed` node list, mirror
-- `S.tempo`/`S.formant`/`S.quality` and push only the widgets whose
-- mirrored value actually changed. A previously-known node that is now
-- missing sets `S.node_removed` (A11); one that reappears clears it.
--
-- The semitones -> key/cents tie-break (FR-006, contract E1 step 3) and
-- the `settings` rewrite (E1 step 5) are later Phase 3/5 tasks (T031,
-- T044) -- this Foundational skeleton mirrors only the three fields
-- data-model.md's task list names.
local function refresh_from_nodes(nodes)
    local pitch, stretch
    for _, node in ipairs(nodes) do
        if node.owner == IDENTIFIER then
            if node.kind == "pitch_shift" and not pitch then
                pitch = node
            elseif node.kind == "time_stretch" and not stretch then
                stretch = node
            end
        end
    end

    local had_pitch = S.pitch ~= nil
    local had_stretch = S.stretch ~= nil
    S.pitch = pitch and pitch.id or nil
    S.stretch = stretch and stretch.id or nil

    if (had_pitch and not pitch) or (had_stretch and not stretch) then
        S.node_removed = true
        set_restored("@node_removed_text")
    elseif pitch and stretch and S.node_removed then
        S.node_removed = false
    end

    if stretch and stretch.params.ratio ~= nil then
        local new_tempo = math.floor(stretch.params.ratio * 100 + 0.5)
        if new_tempo ~= S.tempo then
            S.tempo = new_tempo
            local ok, err = api.ui.update_widget("main", "tempo", S.tempo)
            if not ok then
                log_error("update_widget(tempo)", err)
            end
        end
    end

    if pitch and pitch.params.formant ~= nil then
        if pitch.params.formant ~= S.formant then
            S.formant = pitch.params.formant
            local ok, err = api.ui.update_widget("main", "formant", S.formant)
            if not ok then
                log_error("update_widget(formant)", err)
            end
        end
    end

    local pitch_quality = pitch ~= nil and pitch.params.quality_mode == "quality"
    local stretch_quality = stretch ~= nil and stretch.params.quality_mode == "quality"
    local new_quality = pitch_quality or stretch_quality
    if new_quality ~= S.quality then
        S.quality = new_quality
        local ok, err = api.ui.update_widget("main", "quality", S.quality)
        if not ok then
            log_error("update_widget(quality)", err)
        end
    end

    -- E1 step 3 (FR-006, contract E1, T031): tell the plugin's own echo
    -- of its last composed send from an independent host/other-actor
    -- edit. Within 1e-6 of `S.sent.semitones` -> keep the user's own
    -- key/cents split; otherwise round-half-up decompose and forget
    -- `S.sent` (the next echo, if any, starts a fresh comparison).
    if pitch and pitch.params.semitones ~= nil then
        local semitones = pitch.params.semitones
        local key, cents
        if S.sent and math.abs(semitones - S.sent.semitones) <= 1e-6 then
            key, cents = S.sent.key, S.sent.cents
        else
            key = round_half_up(semitones)
            cents = round_half_up((semitones - key) * 100)
            S.sent = nil
        end
        if key ~= S.key then
            S.key = key
            local ok, err = api.ui.update_widget("main", "key", S.key)
            if not ok then
                log_error("update_widget(key)", err)
            end
        end
        if cents ~= S.cents then
            S.cents = cents
            local ok, err = api.ui.update_widget("main", "cents", S.cents)
            if not ok then
                log_error("update_widget(cents)", err)
            end
        end
    end

    -- E1 step 5 (T044): every mirrored change, by any actor, keeps the
    -- remembered entry in sync -- written only while `S.remember`, and
    -- only when it actually differs from what was last written.
    if S.remember then
        local table_ = current_table()
        if not tables_equal(table_, S.last_written) then
            local ok, err = api.state.track.set("settings", table_)
            if ok then
                S.last_written = table_
            else
                log_error("state.track.set(settings)", err)
            end
        end
    end

    return pitch, stretch
end

-- A2/A7 (T026): the one place that turns `S.key`/`S.cents` into the
-- pitch node's `semitones` target; tracks what was sent so E1 step 3
-- can tell the plugin's own echo from an independent host/other-actor
-- edit.
local function send_composed()
    if not S.pitch then
        return
    end
    local composed = S.key + S.cents / 100
    local ok, err = api.effects.set_param(S.pitch, "semitones", composed)
    if ok then
        S.sent = { semitones = composed, key = S.key, cents = S.cents }
    else
        log_error("set_param(semitones)", err)
    end
end

-- A button click, unlike a slider commit, carries no optimistic value
-- for the "key"/"cents" *slider* widgets themselves (R6's plain button
-- only ever stores its own inert value) -- and since `S.key`/`S.cents`
-- are set directly by these handlers *before* the host round trip, E1's
-- own `key ~= S.key` diff would find nothing to push (T031's mirror
-- compares against a value the handler already applied). So `key_step`/
-- `reset_key` push the two slider widgets themselves, right after
-- setting the state that now belongs to them.
local function push_key_widgets()
    local ok, err = api.ui.update_widget("main", "key", S.key)
    if not ok then
        log_error("update_widget(key)", err)
    end
    local ok2, err2 = api.ui.update_widget("main", "cents", S.cents)
    if not ok2 then
        log_error("update_widget(cents)", err2)
    end
end

-- A1 (T027), A11 (T047): key_up/key_down -- Fine tune (`S.cents`)
-- untouched; a no-op while the pitch node is gone (a user removal).
local function key_step(delta)
    if not S.pitch then
        return
    end
    S.key = clamp(S.key + delta, -12, 12)
    push_key_widgets()
    send_composed()
end

-- A8 first half (T028), A11 (T047): Key and Fine tune both back to 0;
-- a no-op while the pitch node is gone.
local function reset_key()
    if not S.pitch then
        return
    end
    S.key = 0
    S.cents = 0
    push_key_widgets()
    send_composed()
end

-- A3 (T035): tempo slider commit -- a direct send, no optimistic local
-- update (unlike key/cents there is no decomposition ambiguity to
-- guard against, so E1's own mirror is what pushes the `tempo` widget
-- once the host confirms).
local function send_tempo(ratio)
    if not S.stretch then
        return
    end
    local ok, err = api.effects.set_param(S.stretch, "ratio", ratio)
    if not ok then
        log_error("set_param(ratio)", err)
    end
end

-- A4 (T037): tempo_up/tempo_down -- the clamped target is always sent,
-- even at a bound (a host-side no-op; nothing is refused).
local function tempo_step(delta)
    local target = clamp(S.tempo + delta, 25, 200)
    send_tempo(target / 100)
end

-- A8 second half (T038): ratio back to 1.0.
local function reset_tempo()
    send_tempo(1.0)
end

-- A5 (T036), FR-013: `S.step` is local only, never sent to the host;
-- the step_note text row explains a non-default step.
local function set_step(v)
    S.step = v
    local ok, err = api.ui.update_widget("main", "step_note", v == 10 and "" or "@step_note_text")
    if not ok then
        log_error("update_widget(step_note)", err)
    end
end

-- A6 first half (T029): Formant preservation -- pitch node only.
local function set_formant(v)
    if not S.pitch then
        return
    end
    local ok, err = api.effects.set_param(S.pitch, "formant", v)
    if not ok then
        log_error("set_param(formant)", err)
    end
end

-- A6 second half (T030), FR-009: a user flip sets **both** nodes; the
-- plugin only ever calls `set_param(quality_mode)` from here -- never
-- for a mode change it merely observed via E1.
local function set_quality(v)
    local name = v and "quality" or "performance"
    if S.pitch then
        local ok, err = api.effects.set_param(S.pitch, "quality_mode", name)
        if not ok then
            log_error("set_param(pitch quality_mode)", err)
        end
    end
    if S.stretch then
        local ok, err = api.effects.set_param(S.stretch, "quality_mode", name)
        if not ok then
            log_error("set_param(stretch quality_mode)", err)
        end
    end
end

-- A9 (T042): write/remove the one `settings` entry. `v == true` writes
-- the full current table; a refusal reverts the toggle and shows the
-- refusal message as the badge, storing nothing. `v == false` removes
-- the entry; the badge itself is left untouched (A9's own wording --
-- it describes how the current values were seeded, not remember's
-- on/off state). Both the panel toggle (already-applied `event.value`)
-- and the `toggle_remember` action (`not S.remember`) call this same
-- function, so the widget is always explicitly mirrored here rather
-- than assumed already shown.
local function toggle_remember(v)
    if v then
        local table_ = current_table()
        local ok, err = api.state.track.set("settings", table_)
        if ok then
            S.remember = true
            S.last_written = table_
            local ok2, err2 = api.ui.update_widget("main", "remember", true)
            if not ok2 then
                log_error("update_widget(remember)", err2)
            end
        else
            log_error("state.track.set(settings)", err)
            local ok2, err2 = api.ui.update_widget("main", "remember", false)
            if not ok2 then
                log_error("update_widget(remember)", err2)
            end
            local ok3, err3 = api.ui.update_widget("main", "restored", err.message)
            if not ok3 then
                log_error("update_widget(restored)", err3)
            end
        end
    else
        local ok, err = api.state.track.remove("settings")
        if not ok then
            log_error("state.track.remove(settings)", err)
        end
        S.remember = false
        S.last_written = nil
        local ok2, err2 = api.ui.update_widget("main", "remember", false)
        if not ok2 then
            log_error("update_widget(remember)", err2)
        end
    end
end

-- A10 (T043): session-only, never persisted (FR-011, M6); the widget is
-- always mirrored since the `toggle_keep_across_tracks` action has no
-- panel commit of its own to already show the flip.
local function set_keep_across(v)
    S.keep_across = v
    local ok, err = api.ui.update_widget("main", "keep_across", S.keep_across)
    if not ok then
        log_error("update_widget(keep_across)", err)
    end
end

-- §4 E2 (T045): badge reset, entry lookup, then one of three cases --
-- a stored entry is applied (validated field-by-field, M4/T046); with
-- no entry, either reset to defaults or, with `S.keep_across`, leave
-- the values as they are. Every `set_param` call below already goes
-- through a helper that no-ops when its node is `nil` (A11), so a
-- track switch after a user removed a node changes nothing it cannot.
local function track_changed(track)
    S.restored = false
    set_restored("")
    local entry = track and api.state.track.get("settings")

    if entry then
        local validated = validate_entry(entry)
        S.key, S.cents = validated.key, validated.cents
        send_composed()
        set_formant(validated.formant)
        set_quality(validated.quality == "quality")
        send_tempo(validated.tempo / 100)
        S.remember = true
        S.last_written = validated
        S.restored = true
        local ok, err = api.ui.update_widget("main", "remember", true)
        if not ok then
            log_error("update_widget(remember)", err)
        end
        set_restored("@restored_text")
    elseif S.keep_across then
        S.remember = false
        local ok, err = api.ui.update_widget("main", "remember", false)
        if not ok then
            log_error("update_widget(remember)", err)
        end
    else
        S.key, S.cents = 0, 0
        send_composed()
        set_formant(false)
        set_quality(false)
        send_tempo(1.0)
        S.remember = false
        local ok, err = api.ui.update_widget("main", "remember", false)
        if not ok then
            log_error("update_widget(remember)", err)
        end
    end
end

-- G1: 8 trigger actions (data-model.md §3.5); `tempo_up`/`tempo_down`
-- default to `Plus`/`Minus` (research R7 -- these conflict with the
-- host's own fixed-step action on a fresh install, and lose on purpose).
local ACTION_SPECS = {
    { id = "key_up", label = "@action_key_up", kind = "trigger" },
    { id = "key_down", label = "@action_key_down", kind = "trigger" },
    { id = "tempo_up", label = "@action_tempo_up", kind = "trigger", default_binding = "Plus" },
    { id = "tempo_down", label = "@action_tempo_down", kind = "trigger", default_binding = "Minus" },
    { id = "reset_key", label = "@action_reset_key", kind = "trigger" },
    { id = "reset_tempo", label = "@action_reset_tempo", kind = "trigger" },
    { id = "toggle_remember", label = "@action_toggle_remember", kind = "trigger" },
    {
        id = "toggle_keep_across_tracks",
        label = "@action_toggle_keep_across",
        kind = "trigger",
    },
}

local function register_actions()
    for _, spec in ipairs(ACTION_SPECS) do
        local ok, err = api.ui.register_action(spec)
        if not ok then
            log_error("register_action(" .. spec.id .. ")", err)
        end
    end
end

-- G1: one panel, the 16 widgets in data-model.md §3.4's order. The six
-- buttons are plain (no `action` field, research R6) so they stay
-- operable while `tempo_up`/`tempo_down`'s chords are flagged
-- conflicting on a fresh install.
local function register_panel()
    return api.ui.register_panel("main", "@panel_title", {
        { id = "key", kind = "slider", label = "@key", min = -12, max = 12, step = 1, value = 0 },
        { id = "cents", kind = "slider", label = "@cents", min = -50, max = 50, step = 1, value = 0 },
        { id = "tempo", kind = "slider", label = "@tempo", min = 25, max = 200, step = 1, value = 100 },
        { id = "step", kind = "slider", label = "@step", min = 1, max = 50, step = 1, value = 10 },
        { id = "formant", kind = "toggle", label = "@formant", value = false },
        { id = "quality", kind = "toggle", label = "@quality", value = false },
        { id = "remember", kind = "toggle", label = "@remember", value = false },
        { id = "keep_across", kind = "toggle", label = "@keep_across", value = false },
        { id = "restored", kind = "text", label = "@restored", text = "" },
        { id = "step_note", kind = "text", label = "@step_note", text = "" },
        { id = "key_up", kind = "button", label = "@key_up" },
        { id = "key_down", kind = "button", label = "@key_down" },
        { id = "tempo_up", kind = "button", label = "@tempo_up" },
        { id = "tempo_down", kind = "button", label = "@tempo_down" },
        { id = "reset_key", kind = "button", label = "@reset_key" },
        { id = "reset_tempo", kind = "button", label = "@reset_tempo" },
    })
end

-- G2 "both present" / "exactly one present" (T047): adopt, no
-- `set_param`, mirror from `params`, `remember` seeded from the store
-- (or `false` with no track) and mirrored to the `remember` widget so
-- a restart/re-enable shows the same state the track already had.
local function seed_remember()
    local state, serr = api.playback.state()
    if not state then
        log_error("playback.state", serr)
        S.remember = false
    elseif not state.track then
        S.remember = false
    else
        local entry = api.state.track.get("settings")
        S.remember = entry ~= nil
    end
    local ok, err = api.ui.update_widget("main", "remember", S.remember)
    if not ok then
        log_error("update_widget(remember)", err)
    end
end

-- G2 (research R14): adopt both own nodes if present, create exactly the
-- missing one if only one is present, or create both (then run E2's
-- `track_changed` logic) if neither is present -- in every case with no
-- stray `set_param` on adopt.
local function adopt_or_create()
    local result, err = api.effects.list_chain()
    if not result then
        log_error("list_chain", err)
        return
    end

    local pitch, stretch = refresh_from_nodes(result.nodes)

    if pitch and stretch then
        seed_remember()
        set_restored("")
        return
    end

    if pitch and not stretch then
        local id, cerr = api.effects.create_node("time_stretch", { after = "pitch_shift" })
        if not id then
            log_error("create_node(time_stretch)", cerr)
            return
        end
        S.stretch = id
        seed_remember()
        set_restored("")
        return
    end

    if stretch and not pitch then
        local id, cerr = api.effects.create_node("pitch_shift", { before = "time_stretch" })
        if not id then
            log_error("create_node(pitch_shift)", cerr)
            return
        end
        S.pitch = id
        seed_remember()
        set_restored("")
        return
    end

    -- Neither present: create both, adjacent, pitch before stretch.
    local pid, perr = api.effects.create_node("pitch_shift")
    if not pid then
        log_error("create_node(pitch_shift)", perr)
        return
    end
    S.pitch = pid
    local sid, serr = api.effects.create_node("time_stretch", { after = "pitch_shift" })
    if not sid then
        log_error("create_node(time_stretch)", serr)
        return
    end
    S.stretch = sid

    -- G2's own wording: run E2's `track_changed` logic with
    -- `api.playback.state().track` (`nil` == no entry).
    local state = api.playback.state()
    track_changed(state and state.track)
end

-- G4/L2: every field below is already this fresh script's own initial
-- value -- `S.keep_across == false`, `S.step == 10`, and the `step_note`
-- widget's initial `text == ""` (declared in `register_panel` above) --
-- so "after every `ready()`" holds with no extra code: a plugin thread
-- restart always starts this file from scratch.
api.on("ready_ack", function(_event)
    register_actions()
    local ok, err = register_panel()
    if not ok then
        log_error("register_panel", err)
    end
    adopt_or_create()
end)

api.on("effect_chain_changed", function(event)
    refresh_from_nodes(event.nodes)
end)

api.on("track_changed", function(event)
    track_changed(event.track)
end)

-- Panel interaction dispatch (research R6): the six buttons are plain
-- (no `action` field) and routed here, same as a slider/toggle commit.
-- A11 (T047): the "key"/"cents" sliders are a no-op while the pitch
-- node is gone, same as the button handlers they share `send_composed`
-- with.
api.on("panel_interaction", function(event)
    if event.widget == "key" then
        if S.pitch then
            S.key = event.value
            send_composed()
        end
    elseif event.widget == "cents" then
        if S.pitch then
            S.cents = event.value
            send_composed()
        end
    elseif event.widget == "formant" then
        set_formant(event.value == true)
    elseif event.widget == "quality" then
        set_quality(event.value == true)
    elseif event.widget == "remember" then
        toggle_remember(event.value == true)
    elseif event.widget == "keep_across" then
        set_keep_across(event.value == true)
    elseif event.widget == "key_up" then
        key_step(1)
    elseif event.widget == "key_down" then
        key_step(-1)
    elseif event.widget == "reset_key" then
        reset_key()
    elseif event.widget == "tempo" then
        send_tempo(event.value / 100)
    elseif event.widget == "step" then
        set_step(event.value)
    elseif event.widget == "tempo_up" then
        tempo_step(S.step)
    elseif event.widget == "tempo_down" then
        tempo_step(-S.step)
    elseif event.widget == "reset_tempo" then
        reset_tempo()
    end
end)

-- Action dispatch, keyed by the action's own short id.
local ACTION_HANDLERS = {
    key_up = function(_value)
        key_step(1)
    end,
    key_down = function(_value)
        key_step(-1)
    end,
    reset_key = function(_value)
        reset_key()
    end,
    tempo_up = function(_value)
        tempo_step(S.step)
    end,
    tempo_down = function(_value)
        tempo_step(-S.step)
    end,
    reset_tempo = function(_value)
        reset_tempo()
    end,
    toggle_remember = function(_value)
        toggle_remember(not S.remember)
    end,
    toggle_keep_across_tracks = function(_value)
        set_keep_across(not S.keep_across)
    end,
}

api.on("action_invoked", function(event)
    local name = event.action:match("%.([^.]+)$")
    local handler = name and ACTION_HANDLERS[name]
    if handler then
        handler(event.value)
    end
end)

api.ready()
