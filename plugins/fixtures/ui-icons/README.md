# UI Icons fixture

011-plugin-ui-contributions (US3 T091, FR-014a), holds `ui.overlay`.
Declares a manifest `icon` (`icon.png`) and two `[glyphs]` keys: `ok` (a
valid, in-limit PNG) and `big` (a 64x64 PNG, over the 32px `GLYPH_MAX_PX`
cap). Both keys are valid *manifest* entries — Rule 3 only rejects a
malformed `[glyphs]` table shape, never checks the files themselves —
but `big`'s *asset* fails to decode at discovery: a console warning
names `glyphs/big.png`, `PluginAssets.glyphs` omits it, and the overlay
painter substitutes the generic glyph for it at paint time.

On `ready_ack`, draws one `glyph` overlay per key (`ok_glyph`,
`big_glyph`) so a manual run (quickstart.md M10) can see `ok` render its
own icon and `big` render the generic fallback side by side, with the
manifest `icon` itself showing in the header.

Only used when `MODPLAYER_PLUGIN_FIXTURES=1` is set at launch.
