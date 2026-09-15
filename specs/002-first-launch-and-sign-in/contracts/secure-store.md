# Contract: Secure Store (`modplayer-secure-store`)

Platform adapter over the OS credential store (Constitution VI, X). The only
crate in the workspace that depends on `keyring`.

## Trait

```rust
pub trait SecureStore: Send + Sync {
    /// Human-readable, platform-specific store name key for messages
    /// (Fluent key, see ui-surface.md): "store-name-keychain" (macOS),
    /// "store-name-credential-manager" (Windows), "store-name-secret-service" (Linux/*BSD).
    fn platform_name_key(&self) -> &'static str;
    /// Write-read-delete round-trip of a throw-away entry `probe-<pid>`.
    /// Ok(()) iff all three succeed and the read bytes equal the written bytes.
    fn probe(&self) -> Result<(), SecureStoreError>;
    fn put(&self, entry: EntryName, secret: &[u8]) -> Result<(), SecureStoreError>;
    /// Ok(None) when the entry does not exist.
    fn get(&self, entry: EntryName) -> Result<Option<Vec<u8>>, SecureStoreError>;
    /// Ok(()) when the entry did not exist (idempotent).
    fn delete(&self, entry: EntryName) -> Result<(), SecureStoreError>;
}

/// Closed set of entry names — no free-form strings reach the OS store.
pub enum EntryName { SessionCredential, PendingAuthorization, Probe(u32 /* pid */),
                     /// Reserved for 003's reusable receiver credential.
                     ReceiverCredential }
```

Service string is always `"ModPlayer"`; user string is the `EntryName`'s
kebab-case name (`session-credential`, `pending-authorization`,
`probe-<pid>`, `receiver-credential`).

## Errors

```rust
#[derive(thiserror::Error, Debug)]
pub enum SecureStoreError {
    /// Store is locked or refuses access (keyring NoStorageAccess, NoDefaultStore).
    #[error("secure store unavailable")] Unavailable,
    /// Platform API failed for another reason (keyring PlatformFailure, BadEncoding, …).
    #[error("secure store failure")] Platform,
    /// Payload exceeds the platform limit (keyring TooLong).
    #[error("secret too long: {0} > {1} bytes")] TooLong(usize, u32),
}
```

Error `Display` strings never include the secret or the entry payload. The
UI maps both `Unavailable` and `Platform` to the same guidance message with
the platform store name (FR-017); `TooLong` is a programming error surfaced as
`Platform` in release builds and asserted against by the size test.

## Implementors

| Type | Backing | Notes |
|---|---|---|
| `KeyringSecureStore` | `keyring 4.2` `Entry::new("ModPlayer", user)`, `set_secret/get_secret/delete_credential` | `NoEntry` → `Ok(None)` on get / `Ok(())` on delete. `#![forbid(unsafe_code)]` in this crate; FFI is inside keyring's platform store crates. |
| `MemorySecureStore` | `Mutex<HashMap<EntryName, Vec<u8>>>` | Test double. `set_unavailable(bool)` makes every call return `Unavailable`; `set_fail_writes(bool)` makes `put` return `Platform`; `entries()` snapshot for assertions; `written_bytes_log()` for the FR-020 leak test. |

## Guarantees

1. No method logs, prints, or formats the secret bytes.
2. `probe()` leaves no entry behind on success or failure (delete is attempted in a `finally`-style block).
3. All calls are synchronous and may block for OS prompts; callers run them off the UI thread except `probe()` at the moment the user presses **Sign in** (bounded by the OS; acceptable per FR-017 ordering "before opening the browser").
4. The crate exposes no API to enumerate entries or to export a secret to a file.
