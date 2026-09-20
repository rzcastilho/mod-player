-- org.modplayer.fixture.hang (research R18): loops forever inside its
-- play_state_changed handler to exercise the scheduler's per-handler
-- budget interrupt (RT3) and, once the aggregate 1 s CPU share is blown
-- by the resulting abort, the Hang suspension (RT4).

api.on("play_state_changed", function(_event)
    while true do
    end
end)

api.ready()
