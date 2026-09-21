// SPDX-License-Identifier: MIT OR Apache-2.0
// The crate is `#![deny(unsafe_code)]`; this module is its one FFI seam
// (constitution Principle VII: the plugin runtime is an allowed FFI crate).
#![allow(unsafe_code)]

//! The calling thread's consumed CPU time (user + system), used by the
//! budget accounting in [`crate::budget`] (RT3, RT4). FR-009/FR-011 are
//! *CPU* budgets: a plugin thread that is descheduled by the OS or blocked
//! in a syscall has not spent any of its share, and charging it wall-clock
//! time would abort/suspend well-behaved plugins on a loaded machine.

use std::time::Duration;

/// CPU time consumed by the calling thread since it started. Monotonic
/// within one thread; only differences between two readings on the same
/// thread are meaningful. Falls back to `Duration::ZERO` if the platform
/// call fails, which the callers treat as "no CPU spent" (never as an
/// abort).
#[must_use]
pub fn thread_cpu_time() -> Duration {
    imp::thread_cpu_time()
}

#[cfg(unix)]
mod imp {
    use std::time::Duration;

    pub fn thread_cpu_time() -> Duration {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `ts` is a valid, writable `timespec`; `clock_gettime`
        // only writes into it and returns 0 on success.
        let rc = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &raw mut ts) };
        if rc != 0 {
            return Duration::ZERO;
        }
        Duration::new(
            u64::try_from(ts.tv_sec).unwrap_or(0),
            u32::try_from(ts.tv_nsec).unwrap_or(0),
        )
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::OnceLock;
    use std::time::Duration;

    use windows_sys::Win32::System::Performance::{
        QueryPerformanceCounter, QueryPerformanceFrequency,
    };
    use windows_sys::Win32::System::Threading::GetCurrentThread;
    use windows_sys::Win32::System::WindowsProgramming::QueryThreadCycleTime;

    /// Nanoseconds per CPU cycle, measured once. `GetThreadTimes` is only
    /// updated on the ~15.6 ms scheduler tick — useless against a 4 ms
    /// budget — so the thread's cycle counter is used instead and scaled
    /// by a short calibration spin against the performance counter. The
    /// scale drifts with frequency scaling (turbo, power states), which
    /// is fine for a budget guard: the aim is "not charged while off-CPU",
    /// not a metrology-grade clock.
    fn ns_per_cycle() -> f64 {
        static NS_PER_CYCLE: OnceLock<f64> = OnceLock::new();
        *NS_PER_CYCLE.get_or_init(calibrate)
    }

    fn calibrate() -> f64 {
        let mut freq: i64 = 0;
        let mut qpc_start: i64 = 0;
        let mut qpc_now: i64 = 0;
        // SAFETY: valid out-pointers; the calls only write into them.
        let ok = unsafe { QueryPerformanceFrequency(&raw mut freq) != 0 }
            && unsafe { QueryPerformanceCounter(&raw mut qpc_start) != 0 };
        if !ok || freq <= 0 {
            return 0.0;
        }
        let cycles_start = raw_cycles();
        // Spin ~2 ms of real work so the cycle counter moves.
        let target = qpc_start + freq / 500;
        let mut acc = 0u64;
        loop {
            acc = acc.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            // SAFETY: as above.
            if unsafe { QueryPerformanceCounter(&raw mut qpc_now) } == 0 || qpc_now >= target {
                break;
            }
        }
        std::hint::black_box(acc);
        let cycles = raw_cycles().saturating_sub(cycles_start);
        if cycles == 0 {
            return 0.0;
        }
        let elapsed_ns = (qpc_now - qpc_start) as f64 * 1e9 / freq as f64;
        elapsed_ns / cycles as f64
    }

    fn raw_cycles() -> u64 {
        let mut cycles: u64 = 0;
        // SAFETY: `GetCurrentThread` is a pseudo-handle needing no close;
        // `cycles` is a valid out-pointer the call only writes into.
        let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &raw mut cycles) };
        if ok == 0 { 0 } else { cycles }
    }

    pub fn thread_cpu_time() -> Duration {
        let ns = raw_cycles() as f64 * ns_per_cycle();
        Duration::from_nanos(ns as u64)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn busy_work_advances_the_clock_and_sleep_does_not() {
        let before = thread_cpu_time();
        // ~20 ms of real work.
        let start = Instant::now();
        let mut acc = 0u64;
        while start.elapsed() < Duration::from_millis(20) {
            acc = acc.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        }
        std::hint::black_box(acc);
        let after_work = thread_cpu_time();
        assert!(
            after_work > before,
            "spinning must consume thread CPU time ({before:?} -> {after_work:?})"
        );

        std::thread::sleep(Duration::from_millis(30));
        let after_sleep = thread_cpu_time();
        // Sleeping costs a couple of syscalls at most, nowhere near the
        // 30 ms slept.
        assert!(
            after_sleep - after_work < Duration::from_millis(10),
            "sleeping must not be charged as CPU ({after_work:?} -> {after_sleep:?})"
        );
    }
}
