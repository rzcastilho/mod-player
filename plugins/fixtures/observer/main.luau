-- org.modplayer.fixture.observer (US2 T081, spec.md acceptance 1):
-- holds only `playback.observe` and, on `ready_ack`, calls
-- `api.transport.seek(0)` — a `transport.control`-only request. The
-- gateway refuses it before it ever reaches the host (G2: permission
-- checked first), with no side effect, and the refusal is logged here
-- for the manual scenario (quickstart.md M7) to read back.

api.on("ready_ack", function(_event)
    local ok, err = api.transport.seek(0)
    if ok then
        api.log.error("seek unexpectedly succeeded without transport.control")
    else
        api.log.info("seek refused: " .. err.code .. "/" .. err.reason)
    end
end)

api.ready()
