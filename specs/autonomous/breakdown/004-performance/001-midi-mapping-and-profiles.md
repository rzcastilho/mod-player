# 004-performance / 001 — MIDI Learn, Mapping, and Device Profiles

**Source:** [FR-9.3 MIDI](../../ModPlayer-Software-Specification.md#fr-93-midi), [FR-9.1 Actions](../../ModPlayer-Software-Specification.md#fr-91-actions) (MIDI bindings), [DM-15 Binding](../../ModPlayer-Software-Specification.md#dm-15-binding), [DM-16 MidiProfile](../../ModPlayer-Software-Specification.md#dm-16-midiprofile), [INT-6 Operating system services](../../ModPlayer-Software-Specification.md#int-6-operating-system-services) (INT-6.2, MIDI removal), [Part 5 § 4 Permission catalog](../../ModPlayer-Software-Specification.md#4-permission-catalog) (`midi.observe`, `midi.output`), [Part 9 § 4 Communication patterns](../../ModPlayer-Software-Specification.md#4-communication-patterns) (continuous controls to parameter queues), [J-4 — Prepare and play a bar set (S-3)](../../ModPlayer-Software-Specification.md#j-4--prepare-and-play-a-bar-set-s-3) (step 4, 9–10), [EC § 8 Controls](../../ModPlayer-Software-Specification.md#8-controls) (EC-8.1–8.4), [EC § 12 Empty, loading, and first-run states](../../ModPlayer-Software-Specification.md#12-empty-loading-and-first-run-states) (MIDI), [NFR § 1 Performance and latency](../../ModPlayer-Software-Specification.md#1-performance-and-latency) (NFR-1.1), [§ 1 Primary personas](../../ModPlayer-Software-Specification.md#1-primary-personas) (Théo), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-5)

**Prerequisites:** Assumes the action registry from 001-mvp/007-keyboard-actions-and-shortcuts and plugin-registered actions from 001-mvp/011-plugin-ui-contributions.

## Prompt

> Let a DJ map every action they use — loop in, loop out, loop toggle, cue 1–4, tempo nudge, key shift — to a physical control on their controller, so they never look at the screen during a set.
>
> Settings → Controls → MIDI lists connected MIDI input devices with hot-plug detection; the empty state reads "No MIDI devices detected" with a refresh action. Learn mode works by selecting an action, moving a control, and having the app bind the message (note, control change, or program change) with its channel; a jittery control is captured by its first stable message type and number, ignoring value noise. Any host or plugin action can be bound, and one action can carry several bindings alongside its keyboard ones.
>
> Continuous controls bind to continuous parameters — volume, tempo ratio, pitch semitones, any effect parameter — with a configurable range, curve, and pickup (soft-takeover) behavior, so a parameter does not jump until the physical control passes its current value. Continuous control changes go straight to the engine's parameter queue for latency. Foot-pedal inputs arriving as keyboard or MIDI messages are treated like any other binding.
>
> Mappings save as named profiles. A profile can be tied to a device identity and auto-load when that device connects. When a device disconnects mid-set, bindings are kept, the profile is marked disconnected with an indicator and no dialog, and it resumes automatically on reconnect. Two devices sending the same message both trigger the action unless the binding is scoped to a device in the profile. Plugins gain `midi.observe` (receive raw messages) and, where a device profile declares feedback, `midi.output` (LED states for loop on/off and cue set).
>
> Acceptance: when the user binds "tempo nudge" to a knob and turns it during playback, tempo changes within 20 ms at p95 with pitch unchanged. When the knob's physical position is at 30% but tempo is at 100%, nothing changes until the knob passes 100% (pickup). When the controller is unplugged and replugged during a set, the profile reloads and the next cue press works. When the user learns "cue 2" and taps the pad, playback jumps instantly with no gap.

## Scope boundary

Does not cover global (app-unfocused) shortcuts, media keys, or Performance Mode.

## Open questions

- Q-13: whether MIDI output for controller feedback (LEDs) belongs in the first release or is deferred; this prompt includes it as a SHOULD.
