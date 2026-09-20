// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `Gateway::admit` tests (Constitution VIII, contracts/gateway-and-
//! runtime.md §4 — G2, G5).

use std::time::{Duration, Instant};

use modplayer_capability_gateway::api::{Permission, RequestKind};
use modplayer_capability_gateway::focus::{FocusToken, PluginId};
use modplayer_capability_gateway::gateway::Gateway;
use modplayer_capability_gateway::grants::Grants;
use modplayer_capability_gateway::limiter::LIMIT;
use modplayer_capability_gateway::manifest;

/// Build a `Gateway` for `PluginId(0)` granting `permission` (if any),
/// sharing `focus` with the caller so tests can acquire/release it.
fn gateway_with(permission: Option<Permission>, focus: FocusToken) -> Gateway {
    let mut grants = Grants::none();
    if let Some(p) = permission {
        let toml = format!(
            r#"
identifier = "org.modplayer.test.gw"
name = "t"
version = "1.0.0"
api = "1.0"
author = "t"
license = "MIT"
source = "bundled"

[[permissions.required]]
permission = "{}"
justification = "test"
"#,
            p.name()
        );
        let dto = manifest::parse(&toml).expect("parse");
        let manifest = manifest::validate(dto, true).expect("validate");
        grants = Grants::from_bundled(&manifest);
    }
    Gateway::new(PluginId(0), grants, focus)
}

#[test]
fn admit_checks_permission_before_focus() {
    let mut gw = gateway_with(None, FocusToken::new());
    let err = gw
        .admit(RequestKind::TransportSeek, Instant::now())
        .unwrap_err();
    assert_eq!(err.reason, "not_granted");
}

#[test]
fn admit_checks_focus_before_rate() {
    let mut gw = gateway_with(Some(Permission::TransportControl), FocusToken::new());
    let err = gw
        .admit(RequestKind::TransportSeek, Instant::now())
        .unwrap_err();
    assert_eq!(err.reason, "no_focus");
}

#[test]
fn refused_calls_consume_no_quota() {
    let focus = FocusToken::new();
    let mut gw = gateway_with(Some(Permission::TransportControl), focus.clone());
    let now = Instant::now();
    // Focus isn't held yet: every one of these is `no_focus` and must not
    // touch the rate limiter's quota (G2: refused calls consume none).
    for _ in 0..200 {
        let err = gw.admit(RequestKind::TransportSeek, now).unwrap_err();
        assert_eq!(err.reason, "no_focus");
    }
    focus.set_holder(Some(gw.plugin()));
    // If any of the 200 refusals above had consumed quota, fewer than
    // `LIMIT` calls would succeed here.
    for _ in 0..LIMIT {
        gw.admit(RequestKind::TransportSeek, now).expect("ok");
    }
}

#[test]
fn rate_limit_101st_in_window() {
    let focus = FocusToken::new();
    let mut gw = gateway_with(Some(Permission::TransportControl), focus.clone());
    focus.set_holder(Some(gw.plugin()));
    let now = Instant::now();
    for _ in 0..LIMIT {
        gw.admit(RequestKind::TransportSeek, now).expect("ok");
    }
    let err = gw.admit(RequestKind::TransportSeek, now).unwrap_err();
    assert_eq!(err.reason, "rate_limited");
}

#[test]
fn rate_limit_window_slides() {
    let focus = FocusToken::new();
    let mut gw = gateway_with(Some(Permission::TransportControl), focus.clone());
    focus.set_holder(Some(gw.plugin()));
    let now = Instant::now();
    for _ in 0..LIMIT {
        gw.admit(RequestKind::TransportSeek, now).expect("ok");
    }
    let later = now + Duration::from_millis(1_001);
    assert!(gw.admit(RequestKind::TransportSeek, later).is_ok());
}
