// SPDX-License-Identifier: MIT OR Apache-2.0

//! CPU/memory budget accounting: the interrupt-check closure `PluginContext`
//! installs, and the aggregate-share evaluator the scheduler runs after
//! every handler (RT3, RT4; research R4).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use mlua::{Lua, Result as LuaResult, VmState};

/// The `mlua::Error::RuntimeError` message the interrupt callback raises
/// when a handler's deadline has passed. The scheduler distinguishes a
/// budget abort from a genuine script error by this marker (RT3) rather
/// than by an mlua error variant, since Luau surfaces both as
/// `RuntimeError`.
pub const DEADLINE_MARKER: &str = "__modplayer_plugin_deadline_exceeded__";

/// Per-plugin CPU/memory gauges the list reads lock-free (RT13,
/// data-model.md §2).
#[derive(Debug, Default)]
pub struct PluginGauges {
    /// Trailing-1s CPU sum, as permille (0..=1000) of the 100 ms/1 s
    /// share (so 1000 = fully at the aggregate cap).
    cpu_permille_of_share: AtomicU16,
    used_bytes: AtomicU64,
    pending_timers: AtomicU16,
    /// 011-plugin-ui-contributions (R5): the plugin's whole `PluginStateStore`
    /// (all three scopes) serialized-size total, updated by the store on
    /// every `set`/`remove`/load so a `Scope::Settings` cap check never
    /// needs to lock the store from another thread.
    storage_used: AtomicUsize,
}

impl PluginGauges {
    #[must_use]
    pub fn cpu_permille_of_share(&self) -> u16 {
        self.cpu_permille_of_share.load(Ordering::Relaxed)
    }

    pub fn set_cpu_permille_of_share(&self, value: u16) {
        self.cpu_permille_of_share.store(value, Ordering::Relaxed);
    }

    #[must_use]
    pub fn used_bytes(&self) -> u64 {
        self.used_bytes.load(Ordering::Relaxed)
    }

    pub fn set_used_bytes(&self, value: u64) {
        self.used_bytes.store(value, Ordering::Relaxed);
    }

    #[must_use]
    pub fn pending_timers(&self) -> u16 {
        self.pending_timers.load(Ordering::Relaxed)
    }

    pub fn set_pending_timers(&self, value: u16) {
        self.pending_timers.store(value, Ordering::Relaxed);
    }

    #[must_use]
    pub fn storage_used_bytes(&self) -> usize {
        self.storage_used.load(Ordering::Relaxed)
    }

    pub fn set_storage_used_bytes(&self, value: usize) {
        self.storage_used.store(value, Ordering::Relaxed);
    }
}

/// The interrupt/aggregate-share state for one plugin's Lua state (RT3,
/// RT4). `deadline_ns` is nanoseconds since `epoch`, checked from the
/// `set_interrupt` callback on (approximately) every Luau loop
/// back-edge/call — cheap enough (one `Instant::now()`, ~25 ns) to run at
/// that frequency.
pub struct BudgetState {
    deadline_ns: AtomicU64,
    epoch: Instant,
    /// Handler durations within the trailing 1 s window, *excluding* time
    /// spent blocked on a Gateway RPC reply (R3, R4). Guarded by a
    /// `Mutex` since it is touched only between handler invocations on
    /// the plugin's own thread — never a real contention point, just the
    /// `Sync` mlua's `send` feature requires of anything an installed
    /// binding closure captures.
    samples: std::sync::Mutex<VecDeque<(Instant, Duration)>>,
    /// Time spent blocked on Gateway RPC replies during the handler
    /// currently running (or most recently finished) — accumulated by
    /// binding closures via [`Self::extend_deadline_for_wait`]'s sibling
    /// [`Self::add_rpc_wait`], drained by the scheduler after each
    /// handler so it can exclude the wait from the aggregate CPU sample
    /// (R3, R4, RT7).
    rpc_wait: std::sync::Mutex<Duration>,
    pub gauges: Arc<PluginGauges>,
}

impl BudgetState {
    #[must_use]
    pub fn new(gauges: Arc<PluginGauges>) -> Arc<Self> {
        Arc::new(Self {
            deadline_ns: AtomicU64::new(u64::MAX),
            epoch: Instant::now(),
            samples: std::sync::Mutex::new(VecDeque::new()),
            rpc_wait: std::sync::Mutex::new(Duration::ZERO),
            gauges,
        })
    }

    fn now_ns(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.epoch).as_nanos() as u64
    }

    /// Set the deadline for the handler about to run (RT3): `now +
    /// handler_budget`.
    pub fn start_handler(&self, now: Instant, handler_budget: Duration) {
        let deadline = now_ns(self, now) + handler_budget.as_nanos() as u64;
        self.deadline_ns.store(deadline, Ordering::Release);
    }

    /// RT7: an admitted RPC blocks the handler; push the deadline forward
    /// by the wait so the interrupt never fires purely because the host
    /// was slow to reply, and record the wait so the scheduler can
    /// exclude it from the aggregate CPU sample (R4).
    pub fn extend_deadline_for_wait(&self, wait: Duration) {
        let extend = wait.as_nanos() as u64;
        self.deadline_ns.fetch_add(extend, Ordering::AcqRel);
        self.add_rpc_wait(wait);
    }

    fn add_rpc_wait(&self, wait: Duration) {
        if let Ok(mut total) = self.rpc_wait.lock() {
            *total += wait;
        }
    }

    /// Read and reset the accumulated RPC wait for the handler that just
    /// finished (RT7).
    pub fn take_rpc_wait(&self) -> Duration {
        let mut total = self.rpc_wait.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *total)
    }

    /// Disable the interrupt between handlers (no handler running).
    pub fn clear_deadline(&self) {
        self.deadline_ns.store(u64::MAX, Ordering::Release);
    }

    /// The closure installed via `Lua::set_interrupt` (RT3): compares
    /// `Instant::now()` against the current deadline and raises the
    /// deadline marker once it has passed. Takes an owned `Arc` (rather
    /// than a method on `&self`) since `self: &Arc<Self>` receivers
    /// aren't stable — callers pass `Arc::clone(&budget)`.
    pub fn interrupt_check(
        state: Arc<BudgetState>,
    ) -> impl Fn(&Lua) -> LuaResult<VmState> + Send + 'static {
        move |_lua: &Lua| -> LuaResult<VmState> {
            let now_ns = now_ns(&state, Instant::now());
            if now_ns >= state.deadline_ns.load(Ordering::Acquire) {
                return Err(mlua::Error::RuntimeError(DEADLINE_MARKER.to_string()));
            }
            Ok(VmState::Continue)
        }
    }

    /// RT4: record one handler's duration (RPC waits already excluded by
    /// the caller), evict samples older than `window`, and report whether
    /// the sum now exceeds `share`.
    pub fn record_sample(&self, duration: Duration, now: Instant, window: Duration) -> Duration {
        let mut samples = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        samples.push_back((now, duration));
        while let Some(&(at, _)) = samples.front() {
            if now.saturating_duration_since(at) >= window {
                samples.pop_front();
            } else {
                break;
            }
        }
        samples.iter().map(|(_, d)| *d).sum()
    }

    /// Publish the aggregate sum as permille of `share` into the gauges
    /// (RT13).
    pub fn publish_share_gauge(&self, sum: Duration, share: Duration) {
        let permille = if share.is_zero() {
            0
        } else {
            ((sum.as_secs_f64() / share.as_secs_f64()) * 1000.0).min(u16::MAX as f64) as u16
        };
        self.gauges.set_cpu_permille_of_share(permille);
    }
}

fn now_ns(state: &BudgetState, now: Instant) -> u64 {
    state.now_ns(now)
}

/// Whether an `mlua::Error` is the deadline marker this module raises
/// from `interrupt_check` (RT3).
#[must_use]
pub fn is_deadline_error(err: &mlua::Error) -> bool {
    match err {
        mlua::Error::RuntimeError(msg) => msg.contains(DEADLINE_MARKER),
        mlua::Error::CallbackError { cause, .. } => is_deadline_error(cause),
        _ => false,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn deadline_defaults_to_never() {
        let gauges = Arc::new(PluginGauges::default());
        let budget = BudgetState::new(gauges);
        let check = BudgetState::interrupt_check(Arc::clone(&budget));
        let lua = Lua::new();
        assert!(matches!(check(&lua), Ok(VmState::Continue)));
    }

    #[test]
    fn interrupt_fires_after_deadline() {
        let gauges = Arc::new(PluginGauges::default());
        let budget = BudgetState::new(gauges);
        budget.start_handler(Instant::now(), Duration::from_millis(0));
        std::thread::sleep(Duration::from_millis(1));
        let check = BudgetState::interrupt_check(Arc::clone(&budget));
        let lua = Lua::new();
        match check(&lua) {
            Err(err) => assert!(is_deadline_error(&err)),
            Ok(_) => panic!("expected the interrupt to fire once the deadline had passed"),
        }
    }

    #[test]
    fn record_sample_evicts_old_entries() {
        let gauges = Arc::new(PluginGauges::default());
        let budget = BudgetState::new(gauges);
        let t0 = Instant::now();
        let sum = budget.record_sample(Duration::from_millis(50), t0, Duration::from_secs(1));
        assert_eq!(sum, Duration::from_millis(50));
        let sum2 = budget.record_sample(
            Duration::from_millis(10),
            t0 + Duration::from_millis(1_100),
            Duration::from_secs(1),
        );
        // The first 50ms sample is now outside the 1s window.
        assert_eq!(sum2, Duration::from_millis(10));
    }
}
