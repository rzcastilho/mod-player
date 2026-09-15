# 006-practice-depth / 003 — Named Presets for Chain, Focus, and Enabled Plugins

**Source:** [FR-11.2 Presets](../../ModPlayer-Software-Specification.md#fr-112-presets), [FR-6.2 Effect chain](../../ModPlayer-Software-Specification.md#fr-62-effect-chain) (FR-6.2.7), [DM-19 Preset](../../ModPlayer-Software-Specification.md#dm-19-preset), [J-8 — Reorder and resolve effect chain and transport focus](../../ModPlayer-Software-Specification.md#j-8--reorder-and-resolve-effect-chain-and-transport-focus-jtbd-13) (step 4), [EC § 5 Audio engine and effects](../../ModPlayer-Software-Specification.md#5-audio-engine-and-effects) (EC-5.7), [EC § 10 Settings, presets, per-track state](../../ModPlayer-Software-Specification.md#10-settings-presets-per-track-state) (EC-10.1), [§ 10 Retention summary](../../ModPlayer-Software-Specification.md#10-retention-summary) (presets not cleared by sign-out), [§ 4 Jobs-to-be-done](../../ModPlayer-Software-Specification.md#4-jobs-to-be-done) (JTBD-13)

**Prerequisites:** Assumes the effect chain from 001-mvp/008-effect-chain-and-built-in-nodes, transport focus from 001-mvp/010-transport-focus, and the plugin list from 001-mvp/009-plugin-runtime-and-permissions.

## Prompt

> Let a DJ save the whole rig — effect order, parameters, which plugin drives the transport, which plugins are on — as "Bar set" and recall it in one action before the next gig.
>
> A preset captures the effect chain (nodes with type, owner, parameters, bypass state, and order), the transport focus policy, the focus holder, and the set of enabled plugins. The user saves the current state as a named preset from the Effect Chain panel or Settings → Playback, and recalls it from the same places or through a bindable action, so a MIDI pad can switch rigs. Recall applies chain changes at buffer boundaries with crossfades, so switching presets mid-playback does not glitch. Presets contain no audio and no credentials, live outside account-scoped data (they survive sign-out), and can be exported to and imported from a file for backup.
>
> When a recalled preset references a node type owned by a plugin that is no longer installed, the rest of the preset applies, the node is skipped, and a notice lists what was skipped. When a preset was exported from a newer app version, import applies what it understands and lists unknown fields.
>
> Acceptance: when the user saves "Bar set" with EQ before time stretch and Section Loop holding focus, then changes everything, recalling "Bar set" restores the order, parameters, focus holder, and enabled plugins within a second with no audible glitch. When "Bar set" includes a node from a plugin the user has since uninstalled, the notice reads "Skipped: <node> (plugin not installed)" and everything else applies. When the user binds "Recall preset: Bar set" to a MIDI pad and taps it during playback, the switch is glitch-free. When the user signs out and back in, presets are still listed.

## Scope boundary

Does not cover per-track state export, which is the next slice, or plugin-internal settings, which each plugin persists itself.
