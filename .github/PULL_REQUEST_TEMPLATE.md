## Summary

<!-- What does this PR do and why? -->

## Real-time safety

<!--
Mandatory for any change touching crates/modplayer-engine (processor.rs, limiter.rs,
output_stage.rs, shared.rs) or anything called from Processor::render.
If this PR does not touch the real-time path, write "N/A — no real-time path changes."
-->

- [ ] `render()` (and everything it calls) performs no heap allocation, lock, blocking
      call, I/O, or logging.
- [ ] No new `unsafe` code was introduced (`#![forbid(unsafe_code)]` holds).
- [ ] Commands are only applied at buffer boundaries via the `Command` queue (no other
      path into the engine).
- [ ] `assert_no_alloc`-gated tests (`render_never_allocates` and similar) still pass.
- [ ] Engine maintainer (CODEOWNERS) has reviewed and approved.

## Test plan

<!-- How was this verified? cargo test output, manual scenarios, etc. -->

## Checklist

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo deny check`
- [ ] `scripts/check-license-headers.sh`
