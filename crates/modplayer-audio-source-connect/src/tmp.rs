// SPDX-License-Identifier: MIT OR Apache-2.0

//! Private temp directory for librespot's encrypted `NamedTempFile`s
//! (contracts/connect-source.md §6, research R6). Never contains decoded
//! audio (Constitution V) — only the AES-encrypted stream librespot reads
//! from disk while decrypting for playback.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Siblings older than this are purged at `Initialize` (contract §6).
const STALE_AGE: Duration = Duration::from_secs(24 * 60 * 60);

const PREFIX: &str = "modplayer-stream-";

/// Build (and create) this process's private stream directory under the OS
/// temp directory, purging stale siblings from a previous run first.
/// `0o700` on Unix; best-effort on platforms without that permission model.
pub fn prepare(pid: u32) -> PathBuf {
    let base = std::env::temp_dir();
    purge_stale_siblings(&base, pid);
    let dir = base.join(format!("{PREFIX}{pid}"));
    let _ = fs::create_dir_all(&dir);
    set_private(&dir);
    dir
}

/// Remove `dir` entirely (`Initialize` and `Shutdown`, contract §6).
/// Best-effort: a failure to remove is not fatal (the OS reclaims temp
/// directories eventually either way).
pub fn purge(dir: &Path) {
    let _ = fs::remove_dir_all(dir);
}

#[cfg(unix)]
fn set_private(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn set_private(_dir: &Path) {
    // Windows/other: directory ACLs default to the owning user only for a
    // path under the per-user temp directory; nothing further to do here.
}

/// Remove any `modplayer-stream-<pid>` sibling other than our own that is
/// older than [`STALE_AGE`] (contract §6) — a previous run that crashed
/// before its own `Shutdown` purge ran.
fn purge_stale_siblings(base: &Path, own_pid: u32) {
    let Ok(entries) = fs::read_dir(base) else {
        return;
    };
    let now = SystemTime::now();
    let own_name = format!("{PREFIX}{own_pid}");
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name == own_name || !name.starts_with(PREFIX) {
            continue;
        }
        let is_stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= STALE_AGE);
        if is_stale {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn unique_pid() -> u32 {
        // Real pids are never this large; keeps parallel tests from
        // colliding on the same directory.
        1_000_000_000 + COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    fn prepare_creates_a_fresh_private_directory() {
        let pid = unique_pid();
        let dir = prepare(pid);
        assert!(dir.exists());
        let file_name = dir.file_name().map(|n| n.to_string_lossy().into_owned());
        assert_eq!(
            file_name.as_deref(),
            Some(format!("{PREFIX}{pid}").as_str())
        );
        purge(&dir);
        assert!(!dir.exists());
    }

    #[test]
    fn purge_is_idempotent_on_a_missing_directory() {
        let dir = std::env::temp_dir().join(format!("{PREFIX}{}", unique_pid()));
        purge(&dir); // never created; must not panic
        assert!(!dir.exists());
    }

    #[test]
    fn stale_siblings_older_than_24h_are_removed() {
        let base = std::env::temp_dir();
        let stale_pid = unique_pid();
        let stale_dir = base.join(format!("{PREFIX}{stale_pid}"));
        let _ = fs::create_dir_all(&stale_dir);
        let old = SystemTime::now() - Duration::from_secs(25 * 60 * 60);
        let _ = filetime_backdate(&stale_dir, old);

        let own_pid = unique_pid();
        let dir = prepare(own_pid);

        assert!(!stale_dir.exists(), "stale sibling must be purged");
        purge(&dir);
    }

    /// Best-effort mtime backdate without an extra dependency: on
    /// platforms where this fails, the stale-purge test still exercises
    /// the code path, it just may not observe removal (accepted — this is
    /// a best-effort hygiene feature, not a correctness one).
    fn filetime_backdate(path: &Path, when: SystemTime) -> std::io::Result<()> {
        let file = fs::File::open(path)?;
        file.set_modified(when)
    }
}
