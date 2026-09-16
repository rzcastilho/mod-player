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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_page_url_is_non_empty() {
        assert!(!STATUS_PAGE_URL.is_empty());
    }
}
