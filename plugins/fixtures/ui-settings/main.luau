-- org.modplayer.fixture.ui-settings (011-plugin-ui-contributions US4 T105,
-- contracts/overlays-settings-notify.md §2): registers a 4-field settings
-- schema on `ready_ack` —
--   * "snap"  (boolean) "Snap to beat", default false
--   * "shift" (number, -12..12, step 1) "Semitone shift", default 0 —
--             the field M8 sets to 3 and checks after a restart
--   * "note"  (string) "Label", default ""
--   * "mode"  (choice: a/b) "Mode", default "a"
-- Every `settings_changed` delivery increments a counter and records the
-- changed keys, so a test can prove exactly one delivery per host-applied
-- edit (S4). `debug_probe("get")` returns `api.ui.get_settings()` verbatim
-- — M8's own evidence that a value survives a restart: the runtime loads
-- `settings.json` fresh on every spawn (R5), so this reflects whatever
-- last reached disk, not anything this session remembers.

local settings_changed_count = 0
local last_changes = nil

api.on("ready_ack", function(_event)
    local ok, err = api.ui.register_settings({
        { id = "snap", kind = "boolean", label = "Snap to beat", default = false },
        {
            id = "shift",
            kind = "number",
            label = "Semitone shift",
            description = "Transpose by this many semitones.",
            min = -12,
            max = 12,
            step = 1,
            default = 0,
        },
        { id = "note", kind = "string", label = "Label", default = "" },
        {
            id = "mode",
            kind = "choice",
            label = "Mode",
            options = {
                { id = "a", label = "Mode A" },
                { id = "b", label = "Mode B" },
            },
            default = "a",
        },
    })
    if not ok then
        api.log.error("register_settings: " .. err.code .. "/" .. err.reason .. " " .. err.message)
    end
end)

api.on("settings_changed", function(event)
    settings_changed_count = settings_changed_count + 1
    last_changes = event.changes
end)

api.on("debug_probe", function(request)
    if request.name == "get" then
        -- `api.ui.get_settings()` returns the values table itself as its
        -- first result (never an "ok, value" pair — `get_settings` never
        -- fails, R4), so this is the whole reply.
        return api.ui.get_settings()
    elseif request.name == "settings_changed_count" then
        return settings_changed_count
    elseif request.name == "last_changes" then
        return last_changes
    elseif request.name == "state_shift" then
        -- FR-018 (T095 settings_not_visible_via_state_plugin): `Scope::
        -- Settings` and `Scope::Plugin` are separate stores — a "shift"
        -- key set on the settings page must never be visible through
        -- `state.plugin`, which only ever reaches `Scope::Plugin`. Like
        -- `get_settings`, the stored value (or `nil`) is the first result.
        local value = api.state.plugin.get("shift")
        return { found = value ~= nil, value = value }
    end
    return nil
end)

api.ready()
