// SPDX-License-Identifier: MIT OR Apache-2.0

//! Build-time constant links opened only from the UI
//! (contracts/transport-and-queue.md §6; 002 design note 10:
//! `egui::Context::open_url`, never from a core/background thread).

/// The status page linked from `stream-source-unavailable`/
/// `stream-source-update-required` notifications. Overridable at compile
/// time via `MODPLAYER_STATUS_PAGE_URL`; falls back to the issue tracker
/// until a dedicated status page exists (spec assumption).
pub const STATUS_PAGE_URL: &str = match option_env!("MODPLAYER_STATUS_PAGE_URL") {
    Some(url) => url,
    None => "https://github.com/rzcastilho/modplayer/issues",
};

/// The placeholder plugin tutorial linked from the Getting Started card
/// (013-key-and-tempo-plugin, contracts/getting-started-card.md C1). A
/// single constant, no compile-time override (design note: the spec names
/// one constant, replaced when the tutorial ships).
pub const GETTING_STARTED_TUTORIAL_URL: &str =
    "https://github.com/rzcastilho/mod-player/blob/main/docs/plugin-tutorial.md";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_page_url_is_non_empty() {
        assert!(!STATUS_PAGE_URL.is_empty());
    }

    #[test]
    fn getting_started_tutorial_url_is_non_empty() {
        assert!(!GETTING_STARTED_TUTORIAL_URL.is_empty());
    }
}
