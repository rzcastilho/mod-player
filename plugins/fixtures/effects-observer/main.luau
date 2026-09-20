-- org.modplayer.fixture.effects-observer (013-key-and-tempo-plugin,
-- Phase 2 Foundational, contract plugin-api-v1.4.md §4.1): holds only
-- `audio.effects` and logs a compact, deterministic summary of every
-- `effect_chain_changed` it receives — sorted param names, one line per
-- delivery, `#N` sequence prefix so a test can tell "one coalesced
-- event" from "several". Lets host-side tests (`controller_effects.rs`)
-- prove the API 1.4 fan-out mechanism (research R1/R3, R4's fix) without
-- needing the bundled Key & Tempo package, which a later phase adds.

local function summarize_node(node)
    local names = {}
    for name, _ in pairs(node.params) do
        table.insert(names, name)
    end
    table.sort(names)
    local parts = {}
    for _, name in ipairs(names) do
        table.insert(parts, name .. "=" .. tostring(node.params[name]))
    end
    return node.kind
        .. "["
        .. table.concat(parts, ",")
        .. "]auto_switched="
        .. tostring(node.auto_switched)
end

local seq = 0
api.on("effect_chain_changed", function(event)
    seq = seq + 1
    local parts = {}
    for _, node in ipairs(event.nodes) do
        table.insert(parts, summarize_node(node))
    end
    api.log.info(
        "effect_chain_changed#"
            .. tostring(seq)
            .. ":"
            .. tostring(#event.nodes)
            .. ":"
            .. table.concat(parts, "|")
    )
end)

api.ready()
