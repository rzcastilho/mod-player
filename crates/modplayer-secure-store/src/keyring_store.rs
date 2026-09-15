// SPDX-License-Identifier: MIT OR Apache-2.0

//! `KeyringSecureStore`: the `SecureStore` adapter over `keyring 4.2`
//! (contracts/secure-store.md). The only file in the workspace that
//! constructs a `keyring::Entry`.

use keyring::Entry;

use crate::store::{EntryName, SERVICE, SecureStore, SecureStoreError};

/// Platform-specific store name Fluent key (`cfg`-gated constant, no
/// platform call needed to resolve it).
#[cfg(target_os = "macos")]
const PLATFORM_NAME_KEY: &str = "store-name-keychain";
#[cfg(target_os = "windows")]
const PLATFORM_NAME_KEY: &str = "store-name-credential-manager";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const PLATFORM_NAME_KEY: &str = "store-name-secret-service";

/// `SecureStore` backed by the OS credential store via `keyring 4.2`
/// (macOS Keychain, Windows Credential Manager, Linux/*BSD Secret Service
/// via zbus).
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringSecureStore;

impl KeyringSecureStore {
    pub fn new() -> Self {
        Self
    }

    fn entry(&self, name: EntryName) -> Result<Entry, SecureStoreError> {
        Entry::new(SERVICE, &name.user_name()).map_err(map_error)
    }
}

impl SecureStore for KeyringSecureStore {
    fn platform_name_key(&self) -> &'static str {
        PLATFORM_NAME_KEY
    }

    fn probe(&self) -> Result<(), SecureStoreError> {
        let entry = self.entry(EntryName::Probe(std::process::id()))?;
        let payload = b"modplayer-probe";
        let result = entry
            .set_secret(payload)
            .map_err(map_error)
            .and_then(|()| entry.get_secret().map_err(map_error))
            .map(|read_back| read_back == payload);
        // Always attempt the delete, even on failure above, so no probe
        // entry is ever left behind (contracts/secure-store.md guarantee 2).
        let _ = entry.delete_credential();
        match result {
            Ok(true) => Ok(()),
            Ok(false) => Err(SecureStoreError::Platform),
            Err(err) => Err(err),
        }
    }

    fn put(&self, entry: EntryName, secret: &[u8]) -> Result<(), SecureStoreError> {
        self.entry(entry)?
            .set_secret(secret)
            .map_err(|err| map_put_error(err, secret.len()))
    }

    fn get(&self, entry: EntryName) -> Result<Option<Vec<u8>>, SecureStoreError> {
        match self.entry(entry)?.get_secret() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(map_error(err)),
        }
    }

    fn delete(&self, entry: EntryName) -> Result<(), SecureStoreError> {
        match self.entry(entry)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(map_error(err)),
        }
    }
}

/// Map a `keyring::Error` to our closed error set. `Display` on the source
/// error is never forwarded (it may echo platform-specific detail, but
/// never the secret) — callers get one of three coarse reasons per the
/// contract.
fn map_error(err: keyring::Error) -> SecureStoreError {
    match err {
        keyring::Error::NoStorageAccess(_) | keyring::Error::NoDefaultStore => {
            SecureStoreError::Unavailable
        }
        keyring::Error::TooLong(_, limit) => SecureStoreError::TooLong(0, limit),
        _ => SecureStoreError::Platform,
    }
}

/// Same as `map_error`, but `TooLong` carries the actual secret length we
/// tried to write (the platform error only names the attribute, not the
/// length).
fn map_put_error(err: keyring::Error, secret_len: usize) -> SecureStoreError {
    match err {
        keyring::Error::TooLong(_, limit) => SecureStoreError::TooLong(secret_len, limit),
        other => map_error(other),
    }
}
