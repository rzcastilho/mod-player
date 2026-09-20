// SPDX-License-Identifier: MIT OR Apache-2.0

//! Refusals: every failure a plugin's request can hit is a value, never a
//! panic (G3, FR-008, data-model.md §1.4).

/// The closed six-code refusal set (FR-008, `api/v1.toml` `refusal_codes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefusalCode {
    PermissionDenied,
    NoFocus,
    InvalidState,
    NotFound,
    RateLimited,
    BudgetExceeded,
}

impl RefusalCode {
    /// The Lua-facing snake_case code string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            RefusalCode::PermissionDenied => "permission_denied",
            RefusalCode::NoFocus => "no_focus",
            RefusalCode::InvalidState => "invalid_state",
            RefusalCode::NotFound => "not_found",
            RefusalCode::RateLimited => "rate_limited",
            RefusalCode::BudgetExceeded => "budget_exceeded",
        }
    }
}

/// A refused request or event delivery: `{ code, reason, message }`
/// (contract §2). Never thrown to Lua — always returned as `nil, refusal`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: RefusalCode,
    /// A closed, snake_case detail (data-model.md §1.4's `reason` set),
    /// e.g. `"not_granted"`, `"not_owner"`, `"chain_full"`.
    pub reason: &'static str,
    /// A one-sentence, human-readable explanation (never localized —
    /// plugin-facing, not user-facing).
    pub message: String,
}

impl Refusal {
    #[must_use]
    pub fn new(code: RefusalCode, reason: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            reason,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn permission_denied() -> Self {
        Self::new(
            RefusalCode::PermissionDenied,
            "not_granted",
            "This plugin was not granted the permission this call requires.",
        )
    }

    #[must_use]
    pub fn not_owner() -> Self {
        Self::new(
            RefusalCode::PermissionDenied,
            "not_owner",
            "This plugin does not own that marker, region or node.",
        )
    }

    #[must_use]
    pub fn no_focus() -> Self {
        Self::new(
            RefusalCode::NoFocus,
            "no_focus",
            "This plugin does not currently hold transport focus.",
        )
    }

    #[must_use]
    pub fn focus_held() -> Self {
        Self::new(
            RefusalCode::InvalidState,
            "focus_held",
            "Another plugin currently holds transport focus.",
        )
    }

    #[must_use]
    pub fn rate_limited() -> Self {
        Self::new(
            RefusalCode::RateLimited,
            "rate_limited",
            "This plugin is calling this category of request too often.",
        )
    }

    #[must_use]
    pub fn not_found() -> Self {
        Self::new(
            RefusalCode::NotFound,
            "unknown_id",
            "That identifier does not exist.",
        )
    }

    #[must_use]
    pub fn no_track() -> Self {
        Self::new(
            RefusalCode::InvalidState,
            "no_track",
            "There is no current track.",
        )
    }

    #[must_use]
    pub fn invalid_state(reason: &'static str, message: impl Into<String>) -> Self {
        Self::new(RefusalCode::InvalidState, reason, message)
    }

    #[must_use]
    pub fn budget_exceeded(reason: &'static str, message: impl Into<String>) -> Self {
        Self::new(RefusalCode::BudgetExceeded, reason, message)
    }

    #[must_use]
    pub fn host_busy() -> Self {
        Self::new(
            RefusalCode::InvalidState,
            "host_busy",
            "The host did not reply in time.",
        )
    }
}

/// The result of any request application (data-model.md §1.4).
pub type PluginResult<T> = Result<T, Refusal>;
