// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Manual live round-trip against the real OS secure store
//! (contracts/secure-store.md, research R5). Not run in CI: requires an
//! unlocked keychain/credential manager/secret service. Run explicitly
//! with `cargo test -p modplayer-secure-store -- --ignored`.

use modplayer_secure_store::{EntryName, KeyringSecureStore, SecureStore};

#[test]
#[ignore = "manual"]
fn keyring_round_trip() {
    let store = KeyringSecureStore::new();
    let entry = EntryName::Probe(std::process::id());
    let payload = b"modplayer-live-round-trip";

    store.put(entry, payload).expect("put");
    assert_eq!(store.get(entry).expect("get"), Some(payload.to_vec()));
    store.delete(entry).expect("delete");
    assert_eq!(store.get(entry).expect("get after delete"), None);
}
