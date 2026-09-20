// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ReceiverCredentials`: the only path this crate has to the OAuth access
//! token (contracts/connect-source.md §1, design note 5). Implemented by
//! the `modplayer` binary over `SecureStore` + `SessionCredential`; this
//! crate never stores the token beyond the `Credentials` value handed to
//! librespot for a single (re)connect.

use std::fmt;

/// `ReceiverCredentials::access_token` failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialError {
    /// No credential available (signed out, store unreadable, ...). Maps
    /// to `SourceHealth::Transient` (contracts/connect-source.md §4).
    Unavailable,
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("no receiver credential available")
    }
}

impl std::error::Error for CredentialError {}

/// The current OAuth access token (scope `streaming`), read on demand.
/// Called on every (re)connect (contracts/connect-source.md §1).
pub trait ReceiverCredentials: Send + Sync + 'static {
    fn access_token(&self) -> Result<String, CredentialError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysFails;
    impl ReceiverCredentials for AlwaysFails {
        fn access_token(&self) -> Result<String, CredentialError> {
            Err(CredentialError::Unavailable)
        }
    }

    #[test]
    fn credential_error_display_never_panics() {
        assert!(!CredentialError::Unavailable.to_string().is_empty());
    }

    #[test]
    fn trait_object_is_object_safe() {
        let boxed: Box<dyn ReceiverCredentials> = Box::new(AlwaysFails);
        assert!(boxed.access_token().is_err());
    }
}
