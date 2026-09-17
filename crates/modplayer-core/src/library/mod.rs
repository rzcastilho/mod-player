// SPDX-License-Identifier: MIT OR Apache-2.0

//! `library`: the host-owned library index, sync scheduler, play log,
//! persistence and connectivity fact (data-model.md §3, contracts/
//! library-and-search-core.md §1, research R4-R6/R11). Every sub-module is
//! pure, testable state — `PlaybackController`'s catalog surface (US2
//! T060) is the only caller.

pub mod connectivity;
pub mod index;
pub mod persist;
pub mod play_log;
pub mod sync;

pub use connectivity::Connectivity;
pub use index::{HydrateBatch, LibraryIndex, SavedEntry, SyncMeta, SyncOutcome};
pub use persist::{LibraryPaths, LoadIndexOutcome, LoadPlayLogOutcome, LoadWarning, PersistJob};
pub use play_log::PlayLog;
pub use sync::{SchedulerState, SyncScheduler};

/// The Library view's derived state flags (contracts/library-and-search-
/// core.md §1, contracts/ui-surface.md §3): `loading` while the index
/// hasn't yet been read from disk, `refreshing` while the sync scheduler is
/// backed off after a rate limit, `first_sync_failed` for FR-021's inline
/// state, and the connectivity fact every surface that gates on "online"
/// reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibraryStatus {
    pub loading: bool,
    pub refreshing: bool,
    pub first_sync_failed: bool,
    pub connectivity: Connectivity,
}
