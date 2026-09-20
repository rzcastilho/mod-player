// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `PluginStateStore`/`StateWriter` tests (Constitution VIII, contracts/
//! gateway-and-runtime.md §4 — G6, G7).

use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::sync_channel;
use std::time::Duration;

use modplayer_capability_gateway::state::store::{MAX_KEY_BYTES, MAX_VALUE_BYTES, Scope};
use modplayer_capability_gateway::state::{PluginStateStore, StateWriter, WriteJob};
use proptest::prelude::*;
use serde_json::json;

#[test]
fn set_rejects_long_key() {
    let mut store = PluginStateStore::new();
    let key = "a".repeat(MAX_KEY_BYTES + 1);
    assert!(store.set(Scope::Plugin, &key, json!(1)).is_err());
    assert_eq!(store.used_bytes(), 0);
}

#[test]
fn set_rejects_large_value() {
    let mut store = PluginStateStore::new();
    let value = json!("x".repeat(MAX_VALUE_BYTES + 1));
    assert!(store.set(Scope::Plugin, "k", value).is_err());
    assert_eq!(store.used_bytes(), 0);
}

#[test]
fn cap_is_atomic() {
    let mut store = PluginStateStore::new();
    // Each value serializes to exactly `MAX_VALUE_BYTES` (a
    // `MAX_VALUE_BYTES - 2`-byte string plus its two quote bytes), so it
    // passes the per-value check on its own; with a 2-byte key, 9 of them
    // (9 * (MAX_VALUE_BYTES + 2) = 9,437,202 bytes) fit under
    // `STORAGE_CAP` (10,485,760) but a 10th does not.
    let chunk = json!("x".repeat(MAX_VALUE_BYTES - 2));
    for i in 0..9 {
        store
            .set(Scope::Plugin, &format!("k{i}"), chunk.clone())
            .expect("under cap");
    }
    let before = store.used_bytes();
    let result = store.set(Scope::Plugin, "k9", chunk.clone());
    assert!(
        result.is_err(),
        "expected the 10th max-size value to exceed the storage cap"
    );
    assert_eq!(store.used_bytes(), before);
}

#[test]
fn encode_is_deterministic() {
    let mut store = PluginStateStore::new();
    store.set(Scope::Plugin, "b", json!(2)).expect("set");
    store.set(Scope::Plugin, "a", json!(1)).expect("set");
    let first = store.encode(Scope::Plugin);
    let second = store.encode(Scope::Plugin);
    assert_eq!(first, second);
    let text = String::from_utf8(first).expect("utf8");
    assert!(text.find("\"a\"").expect("a") < text.find("\"b\"").expect("b"));
}

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "modplayer-plugin-state-store-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn writer_is_atomic_and_acks() {
    let dir = tempdir();
    let writer = StateWriter::spawn();
    let path = dir.join("plugin.json");
    let (ack_tx, ack_rx) = sync_channel(1);
    writer.send(WriteJob::Save {
        path: path.clone(),
        bytes: b"{\"a\":1}".to_vec(),
        ack: Some(ack_tx),
    });
    ack_rx.recv_timeout(Duration::from_secs(2)).expect("ack");
    assert_eq!(fs::read(&path).expect("read"), b"{\"a\":1}");
    assert!(!path.with_extension("json.tmp").exists());
}

// -- 011-plugin-ui-contributions: `Scope::Settings` (R5, FR-018) ----------

#[test]
fn settings_scope_round_trip() {
    let mut store = PluginStateStore::new();
    store
        .set(Scope::Settings, "snap", json!(true))
        .expect("under budget");
    let bytes = store.encode(Scope::Settings);

    let mut reloaded = PluginStateStore::new();
    reloaded.load_settings(&bytes).expect("reload");
    assert_eq!(reloaded.get(Scope::Settings, "snap"), Some(&json!(true)));
    assert_eq!(reloaded.settings_snapshot().get("snap"), Some(&json!(true)));
}

#[test]
fn settings_scope_counts_toward_cap() {
    let mut store = PluginStateStore::new();
    assert_eq!(store.used_bytes(), 0);
    store
        .set(Scope::Settings, "k", json!("value"))
        .expect("under budget");
    assert!(store.used_bytes() > 0);
    let after_settings = store.used_bytes();
    store
        .set(Scope::Plugin, "k2", json!("value2"))
        .expect("under budget");
    assert!(store.used_bytes() > after_settings);
}

proptest! {
    #[test]
    fn state_roundtrip(pairs in proptest::collection::vec(("[a-z]{1,10}", 0i64..1000), 0..20)) {
        let mut store = PluginStateStore::new();
        // A key may repeat with a later, different value — the store's
        // (and this test's) source of truth is the *last* value set for
        // that key, not every pair in arrival order.
        let mut expected = std::collections::BTreeMap::new();
        for (k, v) in &pairs {
            store.set(Scope::Plugin, k, json!(v)).expect("under every budget");
            expected.insert(k.clone(), *v);
        }
        let bytes = store.encode(Scope::Plugin);
        let mut reloaded = PluginStateStore::new();
        reloaded.load_plugin(&bytes).expect("reload");
        for (k, v) in &expected {
            prop_assert_eq!(reloaded.get(Scope::Plugin, k), Some(&json!(v)));
        }
    }
}
