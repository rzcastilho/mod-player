# Key & Tempo

The second bundled plugin (`org.modplayer.key-tempo`): transpose a song
by semitones — with fine cents — or slow it down / speed it up (25-200%)
without touching the other, independently, and remember a track's
chosen key and tempo when you ask it to. It ships with ModPlayer,
enabled by default, and is written only against the public plugin API,
so its source (`main.luau`) is also valid, unmodified example code for
anyone writing their own plugin.

## What it does

- **Key** — the panel's Key slider (and `key_up`/`key_down`, once you
  resolve any keyboard conflict below) transposes the track ±12
  semitones. **Fine tune** adds up to ±50 cents on top, composed with
  Key into one pitch-shift request.
- **Tempo** — the panel's Tempo slider changes playback speed 25-200%
  with the track's pitch held fixed; `tempo_up`/`tempo_down` (default
  `+`/`-`) step it by the configured **Tempo step** (1-50%, default
  10%).
- **Formant** — toggles formant preservation on the pitch-shift node, so
  a large transpose doesn't leave voices sounding unnaturally
  chipmunked or deepened.
- **High quality** — a single toggle that sets both nodes' quality mode
  together; the host may also auto-switch a node to Quality on its own
  for a large excursion, which this plugin only ever mirrors, never
  overrides on its own.
- **Remember for this track** / **Keep across tracks** — when
  "Remember" is on, every change (from this plugin, the host's Effect
  Chain panel, or the host's own `+`/`-`) is saved as that track's one
  stored entry and restored the next time you return to it, with a
  status badge confirming the restore. "Keep across tracks" is a
  session-only mode: with it on, a track that has never been
  remembered simply carries over the values already in effect, instead
  of resetting.
- **Reset key** / **Reset tempo** — separate buttons return each side to
  its neutral value (0 semitones / 100% tempo) without touching the
  other.

Every control is reachable and operable by keyboard alone, with an
accessible name (contracts/key-tempo-plugin.md, spec.md SC-006).

## How it uses the API

Key & Tempo is built only on `api.*` — the same surface any third-party
plugin sees, never a private host interface. It declares:
`audio.effects`, `ui.panel`, `ui.shortcuts`, `state.track`,
`playback.observe` — all required.

It creates and owns exactly two adjacent effect nodes — one
`pitch_shift`, one `time_stretch` — and drives them only through
`api.effects.set_param`, widened in plugin API **1.4** to accept a
parameter's wire name (`"semitones"`, `"ratio"`, `"formant"`,
`"quality_mode"`, …) and a boolean or enum-name value alongside the
older numeric form. It never guesses or computes a node's value itself:
every widget and the stored `settings` entry are rewritten only from
`NodeInfo.params` — the same 1.4 addition that lets it see the host's
own `+`/`-` action, an Effect Chain panel edit, or an auto-switch, and
mirror it within one host tick, coalesced to at most one
`effect_chain_changed` delivery per tick. This mirror-only discipline
(never re-interpreting, never fighting a host edit) is Constitution
III's whole reason for existing. See `contracts/plugin-api-v1.4.md` and
`contracts/key-tempo-plugin.md` in the feature's spec folder for the
full contract this script honours.

## Persistence guarantee

The chosen key and tempo are stored, one entry per track, only in the
**host's** per-track plugin-state store — under this plugin's own
`state.track` scope, keyed `"settings"` — and only while "Remember for
this track" is on for that track. That means:

- They survive closing and reopening ModPlayer, and switching away and
  back to the same track, whenever remember was left on for it.
- A track that was never remembered always starts at 0 semitones /
  100% tempo, with no badge — unless "Keep across tracks" is on, in
  which case the values already in effect simply carry over instead.
- Disabling, suspending or restarting the plugin never touches either
  node's parameters: on the next enable/restart it adopts the two nodes
  it finds exactly as they are, with no `set_param` call of its own.
- If you remove either node yourself in the host's Effect Chain panel,
  Key & Tempo notices, says so in its status line, and never recreates
  it while still Active — only the next disable/re-enable does.

Every other piece of this plugin's own script state — which node ids it
currently owns, the last composed semitone split it sent, the
configured tempo step, "keep across tracks" — is session-only and never
persisted; it starts fresh every time the plugin (re)starts, and the
very next `effect_chain_changed`/`list_chain()` rebuilds everything
else from the host's own record.

## A note on the default `+` / `-` keys

The host already binds `+`/`-` as its own default tempo-step action
(008). Key & Tempo registers the same two keys as `tempo_up`/
`tempo_down`'s own defaults — but on a fresh install, the host's
bindings win the conflict, and Key & Tempo's identical ones are flagged
inactive until you rebind one side in Settings › Controls. This is
expected: the panel's own Tempo Up/Down buttons work regardless, from
the very first launch, and the panel always mirrors whichever side
actually changed the tempo — the host's own `+`/`-` included — within
one tick, whether or not the plugin's own shortcut is currently active.
