## Effect Chain panel (008-effect-chain-and-built-in-nodes). Keys land
## incrementally as each phase's UI controls are built; see tasks.md.

## Phase 4 (US2): panel toggle, header, rows, add/remove/bypass/reorder,
## and the kinds/params for pitch shift, time stretch and gain (the only
## kinds this phase's panel draws controls for — the rest land in
## Phase 5's T076).

effects-toggle = Effects
effects-panel-title = Effect chain
effects-chain-cpu = Chain CPU load: { $pct } %
effects-add-node = Add node…
effects-add = Add
effects-remove = Remove
effects-bypass = Bypass
effects-reorder-handle = Reorder
effects-cpu = CPU { $pct } %
effects-mode-note = Quality mode auto-switched
effects-chain-full = Chain is full — remove a node first
effects-owner-host = host
effects-owner-plugin = plugin
effects-kind-pitch-shift = Pitch shift
effects-kind-time-stretch = Time stretch
effects-kind-gain = Gain
effects-param-semitones = Semitones
effects-param-formant = Formant
effects-param-mode = Mode
effects-param-ratio = Tempo
effects-param-level = Level
effects-param-mute = Mute
effects-mode-performance = Performance
effects-mode-quality = Quality

## Phase 5 (US3): the remaining three kinds' labels/params — equalizer,
## filter, stereo tools — completing the six-kind catalog.

effects-kind-equalizer = Equalizer
effects-kind-filter = Filter
effects-kind-stereo-tools = Stereo tools
effects-param-band-freq = Frequency
effects-param-band-gain = Gain
effects-param-band-q = Q
effects-param-band-type = Type
effects-param-filter-mode = Mode
effects-param-cutoff = Cutoff
effects-param-resonance = Resonance
effects-param-width = Width
effects-param-balance = Balance
effects-param-mono-sum = Mono sum
effects-param-phase-invert = Phase invert
effects-param-channel-swap = Channel swap
effects-band-type-peak = Peak
effects-band-type-low-shelf = Low shelf
effects-band-type-high-shelf = High shelf
effects-filter-high-pass = High-pass
effects-filter-low-pass = Low-pass

## Phase 6 (US4): meters, spectrum, overload/auto-bypass — pre-/post-chain
## level pairs, the 64-band spectrum, the overload counter and over-budget
## badge, the per-row auto-bypassed label, and the three notification keys
## (`tempo_step`'s `effects-no-time-stretch` was raised as early as Phase 3
## but its string landed here, alongside the rest of this feature's
## notifications).

effects-overloads = Overloads: { $count }
effects-over-budget-badge = Effect chain over budget
effects-pre = Pre-chain level
effects-post = Post-chain level
effects-peak = peak
effects-rms = rms
effects-spectrum = Spectrum
effects-auto-bypassed = Auto-bypassed (over budget)
effect-chain-over-budget = Effect chain over budget: { $node } ({ $owner }) is the costliest node
effect-chain-auto-bypassed = { $node } was auto-bypassed to keep audio playing (over budget)
effects-no-time-stretch = Add a Time Stretch node to change tempo with the + / - keys
