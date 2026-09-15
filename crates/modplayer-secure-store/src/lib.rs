// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

//! OS credential store adapter. The only crate that imports `keyring`
//! (Constitution X: platform differences confined to adapter crates).
//! `SecureStore` trait, `KeyringSecureStore`, and `MemorySecureStore`
//! (contracts/secure-store.md).

mod keyring_store;
mod memory_store;
mod store;

pub use keyring_store::KeyringSecureStore;
pub use memory_store::MemorySecureStore;
pub use store::{EntryName, SERVICE, SecureStore, SecureStoreError};
