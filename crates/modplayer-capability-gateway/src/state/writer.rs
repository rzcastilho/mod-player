// SPDX-License-Identifier: MIT OR Apache-2.0

//! The background persistence thread for plugin state (G7, research R9):
//! `.tmp -> write_all -> sync_all -> rename`, one thread shared by every
//! plugin, with an `ack` sender so the `unloading` handler can wait for
//! its own write to land within the 200 ms window (RT8).

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, SyncSender};

/// One write (or delete) the writer thread must perform.
pub enum WriteJob {
    Save {
        path: PathBuf,
        bytes: Vec<u8>,
        /// Signalled once the write has been fsync'd and renamed into
        /// place (RT8's 200 ms unloading wait).
        ack: Option<SyncSender<()>>,
    },
    Delete {
        path: PathBuf,
    },
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// The writer thread's handle: a job sender plus the `JoinHandle` the
/// controller joins on shutdown (mirrors `markers::store::spawn_writer`).
pub struct StateWriter {
    tx: Sender<WriteJob>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl StateWriter {
    /// Spawn the writer thread. The loop ends (and the thread exits) once
    /// every clone of the returned sender is dropped.
    #[must_use]
    pub fn spawn() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<WriteJob>();
        let thread = std::thread::Builder::new()
            .name("plugin-state-writer".to_string())
            .spawn(move || {
                for job in rx {
                    match job {
                        WriteJob::Save { path, bytes, ack } => {
                            let _ = write_atomic(&path, &bytes);
                            if let Some(ack) = ack {
                                let _ = ack.send(());
                            }
                        }
                        WriteJob::Delete { path } => {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            })
            .ok();
        Self { tx, thread }
    }

    /// Queue `job` (best-effort: a closed receiver, only possible after
    /// `join`, silently drops it).
    pub fn send(&self, job: WriteJob) {
        let _ = self.tx.send(job);
    }

    /// A cloneable sender for handing to plugin threads directly.
    #[must_use]
    pub fn sender(&self) -> Sender<WriteJob> {
        self.tx.clone()
    }

    /// Join the writer thread (shutdown). `self.tx` — this `StateWriter`'s
    /// own sender clone, as opposed to any clone already handed to a
    /// plugin thread via [`Self::sender`] — must be dropped *before*
    /// blocking on the join: the writer thread's loop only ends once
    /// every clone of the channel's sender is gone, and `self.tx` would
    /// otherwise stay alive for this whole call's duration (a field of
    /// `self`, not dropped until the function returns), deadlocking
    /// against the very thread being joined.
    pub fn join(self) {
        let Self { tx, thread } = self;
        drop(tx);
        if let Some(thread) = thread {
            let _ = thread.join();
        }
    }
}

// `writer_is_atomic_and_acks` lives in `tests/state_store.rs` (Constitution
// VIII, contracts/gateway-and-runtime.md §4) so it exercises the crate's
// public API only.
