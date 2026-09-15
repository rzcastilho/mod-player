<!--
Sync Impact Report
==================
Version change: TEMPLATE (unfilled) → 1.0.0
Rationale: Initial ratification of the ModPlayer constitution. MAJOR because
this establishes the founding principle set (not a mere expansion of a prior
version).

Modified principles: N/A (initial version)

Added sections:
- Core Principles I-X (Real-Time Path Is Sacred; Plugins Are Guests;
  Host Primitives, Plugin Behaviors; The Audio Source Is Replaceable and
  Isolated; No Audio Ever Leaves the Engine; Security and Privacy by
  Default; Rust Quality Gates; Test What the NFRs Promise; One Plugin API
  Definition, Semantically Versioned; Simplicity, Portability, and the
  User's Override)
- Governance

Removed sections: none (template placeholders replaced)

Templates requiring updates:
- .specify/templates/plan-template.md: ✅ no change needed — Constitution
  Check gate already derives from this file at plan time; no hardcoded
  principle list to desync.
- .specify/templates/spec-template.md: ✅ no change needed — generic,
  no constitution-specific references.
- .specify/templates/tasks-template.md: ✅ no change needed — generic,
  no constitution-specific references.
- .specify/templates/commands/*.md: N/A — directory does not exist in
  this project.
- README / docs: ✅ no change needed — no root README; docs/ModPlayer-
  Software-Specification.md is a separate source document, not a
  constitution-dependent artifact.

Follow-up TODOs:
- TODO(MSRV): Principle VII references MSRV [1.xx] — pin exact Rust
  version once toolchain is selected.
- TODO(WASM_RUNTIME_DECISION): Principle II defers the plugin runtime
  choice (wasmtime+fuel vs. Rhai vs. mlua/Lua) to an Architecture
  Decision Record; record it there and link it back here when decided.
-->

# ModPlayer Constitution

## Core Principles

### I. Real-Time Path Is Sacred (NON-NEGOTIABLE)

Everything that touches audio samples runs on the audio thread; everything
else (UI, plugins, sync, cache, network) is off it. Code on the real-time
path MUST NOT allocate, lock, block, perform I/O, log, or call into plugin
scripts. Communication into and out of the real-time path goes only through
lock-free SPSC/MPSC queues and atomics. Effect parameter changes and effect
chain reorders MUST apply only at buffer boundaries, never mid-buffer. Any
PR touching the engine crate MUST include a "real-time safety" note in its
description and MUST pass the latency and loop-seam tests (NFR-1.1, NFR-1.2,
NFR-1.3, NFR-2.3, AR guiding decision 1).

**Rationale**: This is a live-performance instrument. A single allocation,
lock, or blocking call on the audio thread produces an audible dropout —
unacceptable mid-performance for a musician or DJ. This principle is
non-negotiable because it protects the one guarantee the entire product
exists to make.

### II. Plugins Are Guests: Fail-Isolated, Least-Privilege, Budgeted

Every plugin runs in an isolated context with no ambient authority. All
capabilities MUST go through a single Capability Gateway that enforces
declared permissions and per-plugin CPU, memory, storage, network, and
notification budgets (PL-1.3, PL-1.4, PL-8, NFR-4.3). A plugin fault
(panic, hang, over-budget, memory exhaustion) MUST NEVER cause an audio
dropout, a host crash, or another plugin's failure. Refusals from the
gateway are ordinary `Err` values, never panics. The plugin runtime MUST
support execution budgets and memory caps natively — the sanctioned
choice (WASM via wasmtime with fuel metering, vs. Rhai, vs. Lua via mlua)
MUST be recorded with rationale in an Architecture Decision Record before
the plugin-runtime crate is implemented.

**Rationale**: Plugins are third-party, scripted, and untrusted by
default. A live performer cannot afford a community plugin taking down
their set. Least-privilege plus hard budgets make plugin misbehavior a
contained, recoverable event instead of an incident.

### III. Host Primitives, Plugin Behaviors

Markers, loop regions, cue points, effect nodes, actions, transport focus,
and per-track state are host concepts owned by core crates; plugins
compose and parameterize them, they do not reimplement them (FR-5,
FR-3.3, FR-6, AR guiding decision 4). DSP (pitch shift, time stretch, EQ,
filters, limiter) is implemented in the host in Rust; the scripting tier
MUST NEVER implement DSP directly, except through the gated, benchmarked
custom-buffer hook (PL-6.7, NFR-4.8).

**Rationale**: Keeping DSP and core transport state in host Rust code
keeps the real-time guarantees of Principle I enforceable and keeps
plugin authors focused on musical behavior, not on re-deriving
correctness- and performance-critical audio math in a sandboxed script.

### IV. The Audio Source Is Replaceable and Isolated

Only one crate (`audio-source-*`) knows the streaming protocol. It sits
behind an `AudioSource` trait with at least two implementations: the
Connect receiver and a synthetic/test source. Every other crate MUST
build, test, and run using only the synthetic source (C-1, NFR-10.1,
GOV-1.2, GOV-6.3). No crate other than the receiver's own consumer path
may depend on the receiver crate directly.

**Rationale**: Isolating the streaming protocol behind a trait keeps the
rest of the system testable without live Spotify Connect sessions, keeps
protocol-specific risk (auth flows, wire format changes) contained to one
crate, and leaves the door open to future sources without a rewrite. The
`AudioSource` trait is the one sanctioned exception to Principle X's
single-implementor rule.

### V. No Audio Ever Leaves the Engine (NON-NEGOTIABLE)

No API, feature flag, debug path, or test helper may write decoded audio
to a user-accessible file, expose sample buffers outside the
engine/effect chain, or decrypt the offline cache for any purpose other
than playback (C-2, FR-6.5, NFR-11.1, GOV-6.4). The offline cache MUST be
encrypted at rest, with its key held in the OS secure store. Pull
requests adding any such capability are rejected on principle, with no
exception process.

**Rationale**: This is a licensing and trust boundary, not a preference.
ModPlayer's right to exist as a Spotify Connect receiver depends on
decoded audio never becoming extractable content. There is no legitimate
feature request that justifies weakening this.

### VI. Security and Privacy by Default

Session credentials live only in the OS credential store (keyring or
platform equivalent); they are never logged, never serialized to disk,
and never reachable by plugins (FR-1.2.2, NFR-4.1). Plugin packages, the
registry index, and client updates MUST be signature- and
digest-verified before use (NFR-4.4, NFR-4.5). Plugin network calls go
only to manifest-declared hosts over TLS and never carry the user's
credential (NFR-4.6). Telemetry is zero by default; crash reports are
opt-in, previewable before sending, and scrubbed of sensitive data
(NFR-5.1, NFR-5.2).

**Rationale**: Musicians and DJs are trusting ModPlayer with the
credentials to their Spotify account and, during a set, with their live
audio path. Defaults must protect that trust without requiring the user
to understand the threat model.

### VII. Rust Quality Gates

Stable toolchain, MSRV `TODO(MSRV): pin exact version`, one Cargo
workspace with one crate per architectural component (engine, effects,
audio-source, plugin-runtime, capability-gateway, core services, ui,
cli). Required before merge: `cargo fmt --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace`, and `cargo deny check` (licenses, advisories,
duplicates). `#![forbid(unsafe_code)]` applies to every crate except the
explicitly listed FFI/real-time crates (audio I/O, MIDI, keychain,
plugin runtime); inside those, every `unsafe` block carries a `// SAFETY:`
comment and is reviewed by the designated area maintainer (GOV-3.2). No
`unwrap()`/`expect()` outside tests and `main`; library crates use
`thiserror`, binaries use `anyhow` with context. Public items carry doc
comments with examples that run under `cargo test --doc`.

**Rationale**: A crate-per-component workspace keeps the boundaries in
Principles I, II, and IV structurally enforced by the compiler, not just
by convention. The unsafe/unwrap/lint gates are the cheapest available
defense against the exact class of bug (panic, UB, silent lock) that
Principle I forbids on the real-time path.

### VIII. Test What the NFRs Promise

Test-first for public behavior; every bug fix ships with a regression
test. The suite MUST include automated tests for: loop-seam sample
accuracy and click-free seams, control-to-audio latency budget, plugin
isolation (crash, hang, memory), permission enforcement, cache eviction,
and offline sync conflict resolution (NFR-10.3). Real-time crates
additionally require criterion benchmarks and a 24-hour soak test for
unbounded growth (NFR-2.7). Property-based tests (proptest) are required
for manifest parsing, marker/loop arithmetic, and state serialization.
CI builds and tests on all three target platforms (GOV-3.3).

**Rationale**: The constitution's non-negotiables (I, II, V) are only
real if they are continuously verified. Each NFR named above corresponds
to a way the product can silently fail a live performer; the tests exist
to catch regressions before a user does, on stage.

### IX. One Plugin API Definition, Semantically Versioned

The plugin API is defined exactly once, in a machine-readable schema
that generates the manifest validator, the runtime call checker, and the
API reference; documentation and runtime behavior MUST NOT diverge
(PL-9.5, NFR-10.2, GOV-2.5). The API is versioned independently of the
host: minor versions only add; removal requires a major version, a
6-month deprecation window with console warnings, a migration guide, and
a compatibility layer for the previous major (PL-9, GOV-2). Any change to
the API requires a written change request included in the PR.

**Rationale**: A single generated source of truth makes "the docs say X
but the runtime does Y" structurally impossible. The versioning and
deprecation policy protects the plugin ecosystem — the product's core
value proposition — from breaking on every host release.

### X. Simplicity, Portability, and the User's Override

YAGNI: no trait with a single implementor (the `AudioSource` trait, per
Principle IV, is the sanctioned exception); no feature flag without a
concrete second consumer; adding a new crate requires stating in the PR
why `std` or an existing dependency is insufficient. Behavior, keyboard
shortcuts, and the plugin API are identical across macOS, Windows, and
Linux; platform differences are confined to adapter crates (C-3,
NFR-9.2). Every host function is keyboard-operable and every UI element
has an accessible name (NFR-6.1, NFR-6.2). All host strings are
externalized; English and pt-BR ship first (NFR-7.1). The user can
always take transport focus back from any plugin, and can disable any
plugin, in one action (FR-3.3.2, JTBD-15).

**Rationale**: Complexity and platform divergence are long-term
maintenance debt that a small open-source project cannot afford. The
user-override guarantee exists because a live performer must be able to
regain control instantly if a plugin misbehaves onstage, regardless of
what Principle II's isolation already provides in the background.

## Governance

This constitution supersedes all other project practices, style guides,
and prior conventions. Every `/speckit.plan` MUST include a Constitution
Check section that names which principles the feature touches and
justifies any deviation with a linked requirement ID. Deviations from
Principles I, II, V, and VI are never accepted, regardless of
justification. Specs and tasks MUST reference the requirement IDs
(C-n, FR-, PL-, NFR-, GOV-) they implement, to keep traceability from
constitution to spec to plan to task intact.

Changes touching the engine crate, the Capability Gateway, or the
Plugin Runtime require review and sign-off by the designated area
maintainer (GOV-3.2) before merge, in addition to normal code review.

Amendments to this constitution are made via pull request, carrying a
rationale and a version bump under semantic versioning:

- **MAJOR**: backward-incompatible principle removal or redefinition.
- **MINOR**: a new principle added, or an existing principle materially
  expanded.
- **PATCH**: wording, clarification, or typo fixes with no semantic
  change.

Every amendment PR MUST update the version line below and regenerate the
Sync Impact Report at the top of this file. All other project templates
(plan, spec, tasks, checklist) MUST be reviewed for consistency with the
amended principles as part of the same PR.

**Version**: 1.0.0 | **Ratified**: 2026-09-14 | **Last Amended**: 2026-09-14
