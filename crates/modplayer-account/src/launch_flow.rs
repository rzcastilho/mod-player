// SPDX-License-Identifier: MIT OR Apache-2.0

//! `LaunchStep`, `next_step` (data-model.md §2.4): the pure function that
//! decides which gate `App` shows this frame. Evaluated every frame, so
//! acknowledging, signing in, and confirming the device each advance the
//! flow without a restart (FR-001, FR-010, FR-012).

use crate::disclosure::DISCLOSURE_BUNDLE_VERSION;
use crate::session::{SessionState, Tier};

/// Which gate (if any) `App` must show this frame (data-model.md §2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchStep {
    Welcome,
    SignIn,
    DeviceCheck,
    Main,
}

/// Decide the launch step, first matching condition wins (data-model.md
/// §2.4). `device_check_needed` is 001's
/// `PlaybackController::should_show_device_check()`.
pub fn next_step(
    acknowledged_version: u32,
    session: &SessionState,
    tier: Tier,
    device_check_needed: bool,
) -> LaunchStep {
    if acknowledged_version != DISCLOSURE_BUNDLE_VERSION {
        return LaunchStep::Welcome;
    }
    if session.is_signing_in() {
        return LaunchStep::SignIn;
    }
    if matches!(session, SessionState::Active | SessionState::Expired)
        && tier == Tier::Premium
        && device_check_needed
    {
        return LaunchStep::DeviceCheck;
    }
    LaunchStep::Main
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first-match-wins precedence (data-model.md §2.4 table) in one
    /// place: Welcome beats every other condition, SignIn beats
    /// DeviceCheck/Main, and DeviceCheck only wins over Main when every one
    /// of its conditions holds (US1 T035).
    #[test]
    fn launch_flow_orders_welcome_signin_devicecheck_main() {
        // Welcome wins even when every other condition would otherwise
        // point past it (signed out is irrelevant; an unacknowledged
        // version always wins).
        assert_eq!(
            next_step(
                0,
                &SessionState::SignedOut { note: None },
                Tier::Unknown,
                false
            ),
            LaunchStep::Welcome
        );

        // Once acknowledged, SignIn wins over DeviceCheck/Main for every
        // "still signing in" state, even one that would satisfy
        // DeviceCheck's tier/flag conditions once Active.
        assert_eq!(
            next_step(
                DISCLOSURE_BUNDLE_VERSION,
                &SessionState::Checking { attempt_id: 7 },
                Tier::Premium,
                true
            ),
            LaunchStep::SignIn
        );

        // Acknowledged + signed in (not signing in) + Premium + device
        // check needed: DeviceCheck wins over Main.
        assert_eq!(
            next_step(
                DISCLOSURE_BUNDLE_VERSION,
                &SessionState::Active,
                Tier::Premium,
                true
            ),
            LaunchStep::DeviceCheck
        );

        // Same, but any single DeviceCheck condition failing (here: not
        // Premium) falls through to Main.
        assert_eq!(
            next_step(
                DISCLOSURE_BUNDLE_VERSION,
                &SessionState::Active,
                Tier::Free,
                true
            ),
            LaunchStep::Main
        );

        // Acknowledged, signed in, device already confirmed: Main.
        assert_eq!(
            next_step(
                DISCLOSURE_BUNDLE_VERSION,
                &SessionState::Active,
                Tier::Premium,
                false
            ),
            LaunchStep::Main
        );
    }

    #[test]
    fn unacknowledged_disclosure_always_shows_welcome_first() {
        assert_eq!(
            next_step(0, &SessionState::Active, Tier::Premium, true),
            LaunchStep::Welcome
        );
        assert_eq!(
            next_step(
                DISCLOSURE_BUNDLE_VERSION + 5,
                &SessionState::SignedOut { note: None },
                Tier::Unknown,
                false
            ),
            LaunchStep::Welcome,
            "a version mismatch in either direction still shows Welcome"
        );
    }

    #[test]
    fn signed_out_after_ack_goes_to_sign_in() {
        assert_eq!(
            next_step(
                DISCLOSURE_BUNDLE_VERSION,
                &SessionState::SignedOut { note: None },
                Tier::Unknown,
                false
            ),
            LaunchStep::SignIn
        );
    }

    #[test]
    fn authorizing_and_checking_also_go_to_sign_in() {
        assert_eq!(
            next_step(
                DISCLOSURE_BUNDLE_VERSION,
                &SessionState::Authorizing {
                    attempt_id: 1,
                    resumed: false
                },
                Tier::Unknown,
                false
            ),
            LaunchStep::SignIn
        );
        assert_eq!(
            next_step(
                DISCLOSURE_BUNDLE_VERSION,
                &SessionState::Checking { attempt_id: 1 },
                Tier::Unknown,
                false
            ),
            LaunchStep::SignIn
        );
    }

    #[test]
    fn active_premium_needing_device_check_goes_to_device_check() {
        assert_eq!(
            next_step(
                DISCLOSURE_BUNDLE_VERSION,
                &SessionState::Active,
                Tier::Premium,
                true
            ),
            LaunchStep::DeviceCheck
        );
    }

    #[test]
    fn active_premium_with_device_confirmed_goes_to_main() {
        assert_eq!(
            next_step(
                DISCLOSURE_BUNDLE_VERSION,
                &SessionState::Active,
                Tier::Premium,
                false
            ),
            LaunchStep::Main
        );
    }

    #[test]
    fn active_free_or_unknown_skips_device_check_and_goes_to_main() {
        assert_eq!(
            next_step(
                DISCLOSURE_BUNDLE_VERSION,
                &SessionState::Active,
                Tier::Free,
                true
            ),
            LaunchStep::Main
        );
        assert_eq!(
            next_step(
                DISCLOSURE_BUNDLE_VERSION,
                &SessionState::Expired,
                Tier::Unknown,
                true
            ),
            LaunchStep::Main
        );
    }
}
