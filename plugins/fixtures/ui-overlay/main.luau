-- org.modplayer.fixture.ui-overlay (US3 T090, contracts/overlays-
-- settings-notify.md §1.1): draws one primitive of every kind
-- (`line`/`region`/`label`/`glyph`) on every `track_changed` — the host
-- has already cleared this plugin's previous set before this handler
-- ever runs (O3, FR-016), so re-adding the same ids on each call is the
-- intended, ordinary usage pattern, not a workaround.
--
-- `debug_probe`:
--   * `"add_501st"`  — a single `add_overlays` call with 501 primitives,
--     proving the whole-batch `overlay_limit` refusal (O1) end to end.
--   * `"bad_region"` — `add_overlays` with `from >= to`, proving
--     `invalid_value` (O1's shape checks, `validate_primitives`).
--   * `"bad_icon"`   — `add_overlays` naming a `glyph` icon that is
--     neither a host glyph nor a declared manifest `[glyphs]` key (this
--     fixture declares none), proving `invalid_value` for an unknown
--     glyph reference.
-- Every probe returns `{ ok = true }` or `{ ok = false, code, reason,
-- message }`, mirroring the `ui-panel` fixture's own `register_bad`.

local function draw_overlay_set()
    return api.ui.add_overlays({
        { id = "line1", kind = "line", at = 500 },
        { id = "region1", kind = "region", from = 1000, to = 1500, color = "secondary" },
        { id = "label1", kind = "label", at = 1000, text = "Verse 1" },
        { id = "glyph1", kind = "glyph", at = 1500, icon = "note" },
    })
end

local function reply(ok, err)
    if ok then
        return { ok = true }
    end
    return { ok = false, code = err.code, reason = err.reason, message = err.message }
end

api.on("track_changed", function(_event)
    local ok, err = draw_overlay_set()
    if not ok then
        api.log.error("add_overlays: " .. err.code .. "/" .. err.reason .. " " .. err.message)
    end
end)

api.on("debug_probe", function(request)
    if request.name == "add_501st" then
        local primitives = {}
        for i = 1, 501 do
            primitives[i] = { id = "p" .. tostring(i), kind = "line", at = i }
        end
        local ok, err = api.ui.add_overlays(primitives)
        return reply(ok, err)
    elseif request.name == "bad_region" then
        local ok, err = api.ui.add_overlays({
            { id = "badregion", kind = "region", from = 2000, to = 1000 },
        })
        return reply(ok, err)
    elseif request.name == "bad_icon" then
        local ok, err = api.ui.add_overlays({
            { id = "badicon", kind = "glyph", at = 500, icon = "not_a_real_icon" },
        })
        return reply(ok, err)
    end
    return nil
end)

api.ready()
