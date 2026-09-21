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
    use std::time::Duration;

    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};

    fn filetime_100ns(ft: &FILETIME) -> u64 {
        (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
    }

    pub fn thread_cpu_time() -> Duration {
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut creation, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);
        // SAFETY: `GetCurrentThread` returns a pseudo-handle that needs no
        // closing; the four out-pointers are valid `FILETIME`s that
        // `GetThreadTimes` only writes into. Non-zero return means success.
        let ok = unsafe {
            GetThreadTimes(
                GetCurrentThread(),
                &raw mut creation,
                &raw mut exit,
                &raw mut kernel,
                &raw mut user,
            )
        };
        if ok == 0 {
            return Duration::ZERO;
        }
        let total_100ns = filetime_100ns(&kernel).saturating_add(filetime_100ns(&user));
        Duration::from_nanos(total_100ns.saturating_mul(100))
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
