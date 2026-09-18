// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SourceCommand::PrefetchHint` forwarding to the decode-ahead (006-
//! markers-loops-and-cues, contracts/engine-loop.md §1). `#[path]`-
//! includes `program.rs`/`rt.rs`/`subfile.rs`/`decode_ahead.rs` directly
//! (like `tests/rt_feed.rs`) so `decode_ahead::forward_prefetch_hint` — a
//! `pub(crate)` free function, deliberately not exercised through a live
//! `Spirc` session — can be driven directly without a network connection.

use std::sync::{Arc, Mutex};

#[path = "../src/decode_ahead.rs"]
#[allow(dead_code)]
mod decode_ahead;
#[path = "../src/program.rs"]
#[allow(dead_code)]
mod program;
#[path = "../src/rt.rs"]
#[allow(dead_code)]
mod rt;
#[path = "../src/subfile.rs"]
#[allow(dead_code)]
mod subfile;

use decode_ahead::{DecodeAheadHandle, SharedDecodeAhead, forward_prefetch_hint};

/// The forwarded frame reaches the running decode-ahead's `seek_hint`.
#[test]
fn prefetch_hint_forwards_to_decode_ahead() {
    let handle = DecodeAheadHandle::for_test();
    let decode_ahead: SharedDecodeAhead = Arc::new(Mutex::new(Some(handle)));

    forward_prefetch_hint(&decode_ahead, 12_345);

    let lock = decode_ahead
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let hint = lock
        .as_ref()
        .unwrap_or_else(|| unreachable!("handle was just set"))
        .pending_seek_hint();
    assert_eq!(hint, 12_345);
}

/// With no decode-ahead running (synthetic/scripted hosts, or between
/// tracks), the hint is silently ignored — never panics, never touches
/// anything else.
#[test]
fn prefetch_hint_is_ignored_with_no_decode_ahead() {
    let decode_ahead: SharedDecodeAhead = Arc::new(Mutex::new(None));

    forward_prefetch_hint(&decode_ahead, 999);

    assert!(
        decode_ahead
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none()
    );
}
