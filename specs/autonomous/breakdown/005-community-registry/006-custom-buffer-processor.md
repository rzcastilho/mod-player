# 005-community-registry / 006 — Custom Buffer Processor (`audio.process`)

**Source:** [FR-6.3 Built-in effect nodes](../../ModPlayer-Software-Specification.md#fr-63-built-in-effect-nodes) (FR-6.3.7), [FR-6.5 Audio access boundary](../../ModPlayer-Software-Specification.md#fr-65-audio-access-boundary) (FR-6.5.2), [Part 5 § 6.7 Custom buffer processor](../../ModPlayer-Software-Specification.md#67-custom-buffer-processor-audioprocess), [Part 5 § 1 Design principles](../../ModPlayer-Software-Specification.md#1-design-principles) (PL-1.2), [Part 9 § 3 Plugin subsystem](../../ModPlayer-Software-Specification.md#plugin-subsystem) (AR-16 compile and benchmark), [EC § 5 Audio engine and effects](../../ModPlayer-Software-Specification.md#5-audio-engine-and-effects) (EC-5.2), [NFR § 4 Security](../../ModPlayer-Software-Specification.md#4-security) (NFR-4.8), [B. Open questions](../../ModPlayer-Software-Specification.md#b-open-questions) (Q-6)

**Prerequisites:** Assumes the effect chain from 001-mvp/008-effect-chain-and-built-in-nodes, the plugin runtime from 001-mvp/009-plugin-runtime-and-permissions, and High-risk permission handling from 005-community-registry/001-registry-browse-and-install.

## Prompt

> Let an advanced plugin author run their own audio processing on the sound — a custom filter, a bit-crusher, a stereo trick the built-in nodes do not offer — without ever letting script code onto the real-time path unchecked.
>
> A plugin with the High-risk `audio.process` permission supplies a processing function written in a restricted subset of the scripting language: no allocation, no I/O, no host calls, bounded loops, and a fixed-size state block. The host compiles it ahead of time. Before inserting the resulting node into the effect chain, the host benchmarks the compiled function on a reference buffer set; if its worst-case time exceeds the per-node budget, insertion is refused with `budget_exceeded` and the measured figures so the author can optimize. At runtime the node is monitored; three consecutive over-budget callbacks bypass it, notify the user, and inform the plugin with `budget_exceeded`.
>
> The function sees the input samples and writes the output samples for the current buffer only, plus its own state block; it cannot access previous buffers except through that state, cannot retain buffers beyond the callback, and has no path to files or the network. The node appears in the Effect Chain panel labeled "Custom (<plugin>)" with the same bypass, reorder, remove, and cost indicator as any node. The permission's approval sheet carries the High-risk warning, and the registry requires human review for plugins that request it.
>
> Acceptance: when a processor's function contains an unbounded loop, compilation is refused and the console names the construct. When a compiled processor's benchmark worst case is 1.8× the per-node budget, insertion fails with `budget_exceeded` and the measured time. When a processor overruns its budget on three consecutive callbacks during playback, it is bypassed, audio continues, and a warning names the plugin. When the owning plugin is disabled, the node is orphaned and keeps running with its last state until the user removes it.

## Scope boundary

Does not cover built-in node behavior or the general plugin sandbox, which already exist.

## Open questions

- Q-6: whether the chosen scripting runtime can compile a restricted subset ahead of time; if not, this slice is deferred and `audio.process` is withheld from the first stable release.
