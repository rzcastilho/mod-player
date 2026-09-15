// SPDX-License-Identifier: MIT OR Apache-2.0

//! `AccountStateStore`: load/save/delete `account.toml`
//! (contracts/account-session.md "account.toml"). Same directory as
//! `settings.toml`; written via tmp + `sync_all` + `rename`, matching
//! 001's `SettingsStore`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::registry::{AccountScopedStore, ClearError};
use crate::session::Tier;

/// Current on-disk schema version.
pub const SCHEMA_VERSION: u32 = 1;

const FILE_NAME: &str = "account.toml";
const TMP_FILE_NAME: &str = "account.toml.tmp";

/// The non-secret part of `AccountSession` as persisted to `account.toml`
/// (data-model.md §1.3; `state` is derived at load time and never
/// persisted).
#[derive(Debug, Clone, PartialEq)]
pub struct PersistedAccount {
    pub account_id: String,
    pub display_name: String,
    pub tier: Tier,
    /// Always the secure-store entry name `"session-credential"`.
    pub credential_ref: String,
    pub expires_at: OffsetDateTime,
    pub authorized_at: OffsetDateTime,
    pub last_validated_at: Option<OffsetDateTime>,
}

/// The result of `AccountStateStore::load()` — distinguishes "no file"
/// from "file present but unreadable/unparseable" so the caller can apply
/// data-model.md §4's rebuild rule (credential present → rebuild with tier
/// `Unknown`; credential absent → treat as no session). That branching is
/// `AccountService::launch()`'s job (US2/US4); this store only reports
/// what it found.
#[derive(Debug)]
pub enum LoadOutcome {
    Absent,
    Loaded(PersistedAccount),
    Unreadable,
}

/// Errors saving `account.toml`. Loading never returns an error: every
/// failure mode is reported through `LoadOutcome::Unreadable`.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("could not write account state: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not serialize account state: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("could not format an account timestamp: {0}")]
    Timestamp(#[from] time::error::Format),
}

/// Loads and saves `account.toml` at a directory resolved the same way as
/// 001's `SettingsStore` (`MODPLAYER_CONFIG_DIR` override, or platform
/// config dir — resolution itself stays the caller's job, mirroring
/// `SettingsStore::new()`/`with_path()`).
#[derive(Debug, Clone)]
pub struct AccountStateStore {
    path: PathBuf,
}

impl AccountStateStore {
    /// Point the store at `dir/account.toml`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            path: dir.into().join(FILE_NAME),
        }
    }

    /// Point the store directly at `path` (tests).
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The resolved `account.toml` path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load `account.toml` (contracts/account-session.md read rules).
    pub fn load(&self) -> LoadOutcome {
        if !self.path.exists() {
            return LoadOutcome::Absent;
        }
        let Ok(content) = fs::read_to_string(&self.path) else {
            return LoadOutcome::Unreadable;
        };
        let Ok(raw) = toml::from_str::<RawAccountFile>(&content) else {
            return LoadOutcome::Unreadable;
        };
        match raw.into_account() {
            Some(account) => LoadOutcome::Loaded(account),
            None => LoadOutcome::Unreadable,
        }
    }

    /// Save `account`, replacing the file atomically (tmp + `sync_all` +
    /// `rename`). On error the previous file is kept — callers raise
    /// `settings-save-failed` (contracts/account-session.md).
    pub fn save(&self, account: &PersistedAccount) -> Result<(), SaveError> {
        let raw = RawAccountFile::from_account(account)?;
        let serialized = toml::to_string_pretty(&raw)?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = self.path.with_file_name(TMP_FILE_NAME);
        {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(serialized.as_bytes())?;
            file.sync_all()?;
        }
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    /// Delete `account.toml`. `Ok(())` when it did not exist (idempotent,
    /// FR-015).
    pub fn delete(&self) -> std::io::Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }
}

/// `account.toml` (T081, contracts/account-session.md `sign_out()`):
/// `clear()` deletes it, retaining nothing account-scoped on disk.
impl AccountScopedStore for AccountStateStore {
    fn category_key(&self) -> &'static str {
        "signout-category-account-details"
    }

    fn clear(&mut self) -> Result<(), ClearError> {
        self.delete().map_err(|_| ClearError)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawAccountFile {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    account: RawAccount,
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawAccount {
    id: String,
    display_name: String,
    tier: String,
    credential_ref: String,
    expires_at: String,
    authorized_at: String,
    #[serde(default)]
    last_validated_at: Option<String>,
}

impl RawAccountFile {
    fn from_account(account: &PersistedAccount) -> Result<Self, SaveError> {
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            account: RawAccount {
                id: account.account_id.clone(),
                display_name: account.display_name.clone(),
                tier: tier_to_str(account.tier).to_string(),
                credential_ref: account.credential_ref.clone(),
                expires_at: account.expires_at.format(&Rfc3339)?,
                authorized_at: account.authorized_at.format(&Rfc3339)?,
                last_validated_at: account
                    .last_validated_at
                    .map(|at| at.format(&Rfc3339))
                    .transpose()?,
            },
        })
    }

    /// `None` when a required field cannot be parsed at all (a malformed
    /// file — the caller sees `LoadOutcome::Unreadable`). An unrecognised
    /// `tier` string silently falls back to `Unknown`
    /// (contracts/account-session.md read rules); it is not a reason to
    /// reject the whole file.
    fn into_account(self) -> Option<PersistedAccount> {
        let expires_at = parse_timestamp(&self.account.expires_at)?;
        let authorized_at = parse_timestamp(&self.account.authorized_at)?;
        let last_validated_at = match self.account.last_validated_at {
            Some(raw) => Some(parse_timestamp(&raw)?),
            None => None,
        };
        Some(PersistedAccount {
            account_id: self.account.id,
            display_name: self.account.display_name,
            tier: tier_from_str(&self.account.tier),
            credential_ref: self.account.credential_ref,
            expires_at,
            authorized_at,
            last_validated_at,
        })
    }
}

fn tier_to_str(tier: Tier) -> &'static str {
    match tier {
        Tier::Premium => "premium",
        Tier::Free => "free",
        Tier::Unknown => "unknown",
    }
}

fn tier_from_str(raw: &str) -> Tier {
    match raw {
        "premium" => Tier::Premium,
        "free" => Tier::Free,
        _ => Tier::Unknown,
    }
}

fn parse_timestamp(raw: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(raw, &Rfc3339).ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_store() -> AccountStateStore {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-account-state-test-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        AccountStateStore::with_path(dir.join(FILE_NAME))
    }

    fn sample() -> PersistedAccount {
        PersistedAccount {
            account_id: "user-1".to_string(),
            display_name: "Alex".to_string(),
            tier: Tier::Premium,
            credential_ref: "session-credential".to_string(),
            expires_at: OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(3600),
            authorized_at: OffsetDateTime::UNIX_EPOCH,
            last_validated_at: Some(OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(10)),
        }
    }

    #[test]
    fn missing_file_loads_absent() {
        let store = temp_store();
        assert!(matches!(store.load(), LoadOutcome::Absent));
    }

    #[test]
    fn round_trips_a_saved_account() {
        let store = temp_store();
        let account = sample();
        store.save(&account).expect("save");
        match store.load() {
            LoadOutcome::Loaded(loaded) => assert_eq!(loaded, account),
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn round_trips_with_no_last_validated_at() {
        let store = temp_store();
        let account = PersistedAccount {
            last_validated_at: None,
            ..sample()
        };
        store.save(&account).expect("save");
        match store.load() {
            LoadOutcome::Loaded(loaded) => assert_eq!(loaded.last_validated_at, None),
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn unparseable_file_loads_unreadable() {
        let store = temp_store();
        fs::write(store.path(), "not valid toml {{{").expect("write");
        assert!(matches!(store.load(), LoadOutcome::Unreadable));
    }

    #[test]
    fn unknown_tier_string_falls_back_to_unknown() {
        let store = temp_store();
        let content = "schema_version = 1\n\n[account]\nid = \"u\"\ndisplay_name = \"D\"\ntier = \"platinum\"\ncredential_ref = \"session-credential\"\nexpires_at = \"1970-01-01T01:00:00Z\"\nauthorized_at = \"1970-01-01T00:00:00Z\"\n";
        fs::write(store.path(), content).expect("write");
        match store.load() {
            LoadOutcome::Loaded(account) => assert_eq!(account.tier, Tier::Unknown),
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn unknown_top_level_key_is_ignored() {
        let store = temp_store();
        let content = "schema_version = 1\nsome_future_key = true\n\n[account]\nid = \"u\"\ndisplay_name = \"D\"\ntier = \"free\"\ncredential_ref = \"session-credential\"\nexpires_at = \"1970-01-01T01:00:00Z\"\nauthorized_at = \"1970-01-01T00:00:00Z\"\n";
        fs::write(store.path(), content).expect("write");
        assert!(matches!(store.load(), LoadOutcome::Loaded(_)));
    }

    #[test]
    fn delete_is_idempotent() {
        let store = temp_store();
        assert!(store.delete().is_ok());
        store.save(&sample()).expect("save");
        assert!(store.path().exists());
        store.delete().expect("delete");
        assert!(!store.path().exists());
        assert!(store.delete().is_ok());
    }

    #[test]
    fn account_scoped_store_clear_deletes_the_file_and_is_idempotent() {
        let mut store = temp_store();
        store.save(&sample()).expect("save");
        assert!(store.path().exists());

        assert_eq!(store.category_key(), "signout-category-account-details");
        assert!(AccountScopedStore::clear(&mut store).is_ok());
        assert!(!store.path().exists());
        assert!(AccountScopedStore::clear(&mut store).is_ok());
    }

    #[test]
    fn simulated_crash_mid_write_leaves_prior_file_intact() {
        let store = temp_store();
        let original = sample();
        store.save(&original).expect("save");

        let tmp_path = store.path().with_file_name(TMP_FILE_NAME);
        fs::write(&tmp_path, "garbage, never renamed").expect("write tmp");

        match store.load() {
            LoadOutcome::Loaded(loaded) => assert_eq!(loaded, original),
            other => panic!("expected Loaded, got {other:?}"),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_store() -> AccountStateStore {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-account-state-proptest-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        AccountStateStore::with_path(dir.join(FILE_NAME))
    }

    fn tier() -> impl Strategy<Value = Tier> {
        prop_oneof![Just(Tier::Premium), Just(Tier::Free), Just(Tier::Unknown)]
    }

    /// Arbitrary Unicode display names, including empty (a null
    /// `display_name` falls back to `account_id` upstream, but the store
    /// itself round-trips whatever string it is given).
    fn display_name() -> impl Strategy<Value = String> {
        ".{0,64}"
    }

    fn timestamp() -> impl Strategy<Value = OffsetDateTime> {
        (0i64..=4_102_444_800i64) // 1970-01-01 .. 2100-01-01
            .prop_map(|secs| OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(secs))
    }

    proptest! {
        /// Constitution VIII state-serialization proptest (T107): any
        /// `AccountSession` (every `Tier`, an optional `last_validated_at`,
        /// arbitrary RFC 3339 timestamps and Unicode display names)
        /// survives `account.toml` save -> load.
        #[test]
        fn account_toml_round_trips_any_session(
            account_id in "[a-zA-Z0-9_-]{1,64}",
            display_name in display_name(),
            tier in tier(),
            expires_at in timestamp(),
            authorized_at in timestamp(),
            last_validated_at in proptest::option::of(timestamp()),
        ) {
            let store = temp_store();
            let account = PersistedAccount {
                account_id,
                display_name,
                tier,
                credential_ref: "session-credential".to_string(),
                expires_at,
                authorized_at,
                last_validated_at,
            };
            store.save(&account).expect("save");
            match store.load() {
                LoadOutcome::Loaded(loaded) => prop_assert_eq!(loaded, account),
                other => prop_assert!(false, "expected Loaded, got {other:?}"),
            }
        }
    }
}
