# ADR 0001: Plugin scripting runtime — Luau via `mlua`

**Status**: Accepted
**Date**: 2026-09-19
**Feature**: 009-plugin-runtime-and-permissions
**Constitution**: resolves Principle II's
`TODO(WASM_RUNTIME_DECISION)` ("the sanctioned choice … MUST be recorded
with rationale in an Architecture Decision Record before the
plugin-runtime crate is implemented").

## Context

Constitution II requires every plugin to run in an isolated context with
no ambient authority, and requires the plugin runtime to "support
execution budgets and memory caps natively" so a plugin fault can never
cause an audio dropout, a host crash, or another plugin's failure
(FR-009, FR-011, FR-026). The constitution named three candidates to
choose from before this crate could be implemented: WebAssembly via
`wasmtime` with fuel/epoch interruption, Rhai, and Lua via `mlua`.

The concrete requirements a candidate has to satisfy:

- **Preemptive interruption** (FR-009, FR-011; Clarifications): a
  handler MUST be aborted at its 4 ms CPU budget regardless of whether
  the plugin code cooperates (yields, checks a flag, etc.) — a
  `while true do end` handler has to be contained without relying on the
  script.
- **Native memory cap** (FR-009): an over-cap allocation MUST fail
  *inside the plugin*, never the host, and the live heap figure must be
  queryable for the plugin list's memory gauge.
- **Sandboxing by design**: no ambient access to `io`, `os`, `debug`,
  `package`, or `require` — only the capabilities the manifest declares
  and the Gateway admits.
- **One state per plugin thread, `Send`** (RT1, R3): a `Lua`/engine value
  owned by exactly one OS thread, with two plugin contexts sharing
  nothing.
- **JSON-shaped state and event payloads** (FR-014, FR-021): plugin
  state and event data round-trip through `serde_json::Value`.
- **Text-file plugins**: FR-001/PL-2.1 describe a plugin package as a
  folder with an entry *script*, editable and readable as files — not a
  compiled artifact — and 002-developer-mode's hot reload assumes the
  same.

## Decision

The plugin language is **Luau**, embedded through `mlua = "0.12"` with
features `["luau", "send", "serialize"]` (no `luau-jit`).

Verified against the vendored `mlua-0.12.1/src/state.rs`:

- `Lua::set_interrupt` installs a callback the Luau VM invokes "at any
  function call or at any loop iteration"; returning `Err` from it
  unwinds the running handler as an ordinary `mlua::Error`. A
  `while true do end` handler is therefore aborted within one interrupt
  check of the 4 ms deadline, with no script cooperation — satisfies
  preemptive interruption.
- `Lua::set_memory_limit(bytes)` makes the allocator return
  `Error::MemoryError` at the allocation that would cross the limit;
  `Lua::used_memory()` gives the live heap figure — satisfies the native
  memory cap.
- `Lua::sandbox(true)` (Luau-only) freezes globals and builtins;
  `Lua::new_with(StdLib, LuaOptions)` loads only the safe subset
  (`STRING | TABLE | MATH | BIT32 | UTF8`; no `io`, `os`, `debug`,
  `package`, no `require`) — Luau exists to run untrusted scripts in a
  multi-tenant host, which is exactly this threat model (PL-1.2,
  NFR-4.3).
- One `Lua` state per plugin, `Send` under the `send` feature, owned by
  that plugin's own OS thread; two states share nothing (FR-004, RT1).
- The `serialize` feature gives `serde_json::Value ⇄ LuaValue` for state
  and event payloads (FR-014's "JSON-serializable" requirement falls out
  of this for free).
- MIT licence (`mlua`, `mlua-sys`, `luau0-src`); the `luau` feature
  auto-vendors and compiles Luau's C++ through `cc`, already available on
  the ubuntu/macos/windows CI images `ci.yml` uses; MSRV 1.88 ≤ the
  workspace's 1.95.
- `mlua`'s public API is safe; both new crates
  (`modplayer-capability-gateway`, `modplayer-plugin-runtime`) keep
  `#![forbid(unsafe_code)]` — the constitution's "plugin runtime" unsafe
  allowance is not needed.

Justified under Constitution X (no dependency without stating why `std`
is insufficient): `std` has no scripting interpreter, and the plugin
runtime is a constitution-named architectural component (AR-16).

## Alternatives considered

- **WebAssembly via `wasmtime` with fuel/epoch interruption.** Strongest
  isolation and deterministic fuel-based accounting, but pulls in ≈ 50
  transitive crates plus Cranelift's compile time on every CI run, and
  plugin authors would need a `wasm32` compile toolchain rather than
  editing a text file — at odds with the spec's "small scripted plugins"
  framing and 002-developer-mode's hot-reload-a-script workflow.
  **Retained as the documented fallback** if Luau ever fails to build on
  a target platform: the Gateway crate's request/event/manifest types are
  runtime-agnostic, so swapping the runtime crate's internals would not
  touch the Gateway's public API.
- **Rhai.** Pure Rust (no `cc`/C++ toolchain dependency at all), and
  `on_progress` gives operation-granularity interruption. But it has no
  per-engine heap cap — only element-count limits (`set_max_array_size`
  and friends) — so a global-allocator shim would be needed, and that
  shim would count the *host's* allocations too, not just the plugin's.
  Fails Constitution II's "support execution budgets and memory caps
  natively" for the memory axis.
- **Lua 5.4 via `mlua`.** Same crate, same dependency footprint, but no
  `set_interrupt`; `set_hook` with `HookTriggers::every_nth_instruction`
  can approximate it, but Luau's `sandbox()` and its from-scratch
  allocator-backed memory limit are Luau-only features in `mlua` — Lua
  5.4 would need a hand-rolled sandboxing table and no native memory cap
  at all. Rejected for the same reason as Rhai on the memory axis, plus a
  weaker interruption story.

## Consequences

- One new direct dependency (`mlua`) and its two indirect crates
  (`mlua-sys`, `luau0-src`), all MIT, already covered by the existing
  `deny.toml` licence allow-list (T007).
- CI on all three platforms needs a C++ toolchain for `cc` to vendor-build
  Luau; ubuntu/macos/windows GitHub-hosted runners ship one already, so
  `ci.yml` needs no change.
- If a future slice needs multi-language plugin support or the fallback
  above, the Gateway crate's `Request`/`HostEvent`/`Manifest` types do
  not name Luau anywhere, keeping that door open without an API break.

## Links

- `specs/009-plugin-runtime-and-permissions/research.md` R1 (this
  decision's evidence trail), R3 (one thread per plugin), R4 (CPU budget
  aggregation).
- `.specify/memory/constitution.md` Principle II (PATCH-amended by this
  feature's Setup phase to link here).
