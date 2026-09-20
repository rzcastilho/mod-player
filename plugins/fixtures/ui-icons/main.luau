-- org.modplayer.fixture.ui-icons (US3 T091, FR-014a): declares a
-- manifest `icon` and two `[glyphs]` keys — `ok` (a valid, in-limit PNG)
-- and `big` (a 64x64 PNG, over the 32px `GLYPH_MAX_PX` cap). Both keys
-- are valid manifest entries either way (Rule 3 only checks the
-- `[glyphs]` *table*'s shape, never the files themselves, FR-014a); only
-- `big`'s *asset* fails to decode at discovery — a console warning
-- naming `glyphs/big.png`, `PluginAssets.glyphs` simply omits it, and
-- `modplayer-ui`'s overlay painter substitutes the generic glyph for it
-- at paint time (M10). On `ready_ack`, draws one glyph overlay per key
-- so a manual run can see `ok` render and `big` degrade side by side.

api.on("ready_ack", function(_event)
    local ok, err = api.ui.add_overlays({
        { id = "ok_glyph", kind = "glyph", at = 500, icon = "ok" },
        { id = "big_glyph", kind = "glyph", at = 1500, icon = "big" },
    })
    if not ok then
        api.log.error("add_overlays: " .. err.code .. "/" .. err.reason .. " " .. err.message)
    end
end)

api.ready()
