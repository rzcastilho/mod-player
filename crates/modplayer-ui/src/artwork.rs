// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ArtworkCache` (contracts/ui-surface.md §6, research R8): fetches the
//! public CDN artwork URL a track/album/artist/playlist ref carries with a
//! small `ureq` worker pool, decodes it with `image`
//! (`default-features = false, features = ["jpeg"]`), and hands back an
//! egui texture — never on the UI thread (design note 2: "a frame must
//! never call `ureq`, `File` I/O or `image::load`"). A failed fetch/decode
//! is cached negatively so a broken URL is not retried every frame it
//! scrolls back into view; `rows::ListRow` falls back to the initials
//! placeholder (`widgets::initials`) on `Failed` or no URL.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;

use egui::{ColorImage, Context, TextureHandle, TextureId, TextureOptions};

/// Worker-pool size (contracts/ui-surface.md §6: "a bounded worker pool (2
/// threads)").
const WORKER_THREADS: usize = 2;
/// Texture LRU bound (contracts/ui-surface.md §6, research R8).
const LRU_CAPACITY: usize = 256;

/// What [`ArtworkCache::get`] reports for one URL (contracts/ui-surface.md
/// §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkState {
    Ready(TextureId),
    Loading,
    Failed,
}

/// Raw decoded pixels handed from a worker thread to the cache — a worker
/// thread has no `egui::Context`, so the `TextureHandle` itself is only
/// ever built on the UI thread, in [`ArtworkCache::poll_results`].
struct DecodedImage {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

enum FetchOutcome {
    Ready(DecodedImage),
    Failed,
}

/// Off-thread artwork fetch + decode + texture cache. One instance lives
/// for the lifetime of the signed-in session (design note 8: cleared on
/// sign-out along with the rest of catalog state).
pub struct ArtworkCache {
    job_tx: Sender<String>,
    result_rx: Receiver<(String, FetchOutcome)>,
    _workers: Vec<thread::JoinHandle<()>>,
    in_flight: HashSet<String>,
    textures: HashMap<String, TextureHandle>,
    /// Recency order for the LRU bound, most-recently-used at the back.
    lru_order: VecDeque<String>,
    /// URLs whose fetch/decode failed — a negative cache so a broken URL
    /// resolves to `Failed` (and so the initials placeholder) without a
    /// fresh network round trip on every frame it is visible.
    failed: HashSet<String>,
}

impl ArtworkCache {
    pub fn new() -> Self {
        let (job_tx, job_rx) = std::sync::mpsc::channel::<String>();
        let job_rx = Arc::new(Mutex::new(job_rx));
        let (result_tx, result_rx) = std::sync::mpsc::channel();

        let workers = (0..WORKER_THREADS)
            .map(|_| {
                let job_rx = Arc::clone(&job_rx);
                let result_tx = result_tx.clone();
                thread::spawn(move || worker_loop(&job_rx, &result_tx))
            })
            .collect();

        Self {
            job_tx,
            result_rx,
            _workers: workers,
            in_flight: HashSet::new(),
            textures: HashMap::new(),
            lru_order: VecDeque::new(),
            failed: HashSet::new(),
        }
    }

    /// Current state for `url` (contracts/ui-surface.md §6), kicking off a
    /// background fetch on first request. Never blocks: drains whatever
    /// results a worker finished since the last call, but never waits for
    /// one.
    ///
    /// ```
    /// use egui::Context;
    /// use modplayer_ui::artwork::{ArtworkCache, ArtworkState};
    ///
    /// let mut cache = ArtworkCache::new();
    /// let ctx = Context::default();
    /// // First request kicks off a background fetch; it cannot have
    /// // resolved yet, so this call always reports `Loading`.
    /// assert_eq!(
    ///     cache.get(&ctx, "https://i.scdn.co/image/deadbeef"),
    ///     ArtworkState::Loading
    /// );
    /// ```
    pub fn get(&mut self, ctx: &Context, url: &str) -> ArtworkState {
        self.poll_results(ctx);

        if let Some(texture) = self.textures.get(url) {
            let id = texture.id();
            self.touch(url);
            return ArtworkState::Ready(id);
        }
        if self.failed.contains(url) {
            return ArtworkState::Failed;
        }
        if !self.in_flight.contains(url) {
            self.in_flight.insert(url.to_string());
            // A full/gone job queue (worker threads exited) is not fatal
            // here — the url simply never resolves past `Loading`.
            let _ = self.job_tx.send(url.to_string());
        }
        ArtworkState::Loading
    }

    /// Number of textures currently held (tests; the LRU bound is
    /// otherwise an internal implementation detail).
    #[cfg(test)]
    fn texture_count(&self) -> usize {
        self.textures.len()
    }

    fn touch(&mut self, url: &str) {
        self.lru_order.retain(|u| u != url);
        self.lru_order.push_back(url.to_string());
    }

    fn poll_results(&mut self, ctx: &Context) {
        loop {
            match self.result_rx.try_recv() {
                Ok((url, FetchOutcome::Ready(decoded))) => {
                    self.in_flight.remove(&url);
                    self.failed.remove(&url);
                    let image = ColorImage::from_rgba_unmultiplied(
                        [decoded.width, decoded.height],
                        &decoded.rgba,
                    );
                    let texture = ctx.load_texture(&url, image, TextureOptions::LINEAR);
                    self.textures.insert(url.clone(), texture);
                    self.touch(&url);
                    self.evict_over_capacity();
                }
                Ok((url, FetchOutcome::Failed)) => {
                    self.in_flight.remove(&url);
                    self.failed.insert(url);
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
    }

    fn evict_over_capacity(&mut self) {
        while self.textures.len() > LRU_CAPACITY {
            let Some(oldest) = self.lru_order.pop_front() else {
                break;
            };
            self.textures.remove(&oldest);
        }
    }
}

impl Default for ArtworkCache {
    fn default() -> Self {
        Self::new()
    }
}

fn worker_loop(job_rx: &Arc<Mutex<Receiver<String>>>, result_tx: &Sender<(String, FetchOutcome)>) {
    loop {
        let url = {
            let rx = job_rx.lock().unwrap_or_else(|e| e.into_inner());
            match rx.recv() {
                Ok(url) => url,
                Err(_) => return,
            }
        };
        let outcome = fetch_and_decode(&url);
        if result_tx.send((url, outcome)).is_err() {
            return;
        }
    }
}

/// `MODPLAYER_ARTWORK_FORCE_FAIL` (quickstart M14, US3 T072, debug builds
/// only): every artwork fetch resolves to `Failed` — and so `rows::ListRow`
/// falls back to the initials placeholder on every row — without ever
/// calling `ureq`, so the fallback can be rehearsed without a real broken
/// URL. Read fresh on every fetch, like `catalog::force_rate_limited`;
/// compiled out of release builds (`debug_assertions`) so it can never
/// fire for a real user.
#[cfg(debug_assertions)]
fn force_artwork_fail() -> bool {
    std::env::var_os("MODPLAYER_ARTWORK_FORCE_FAIL").is_some()
}

#[cfg(not(debug_assertions))]
fn force_artwork_fail() -> bool {
    false
}

/// Fetch `url` with `ureq` and decode it with `image` (research R13) — the
/// only two operations this worker thread performs; never called from the
/// UI thread.
fn fetch_and_decode(url: &str) -> FetchOutcome {
    if force_artwork_fail() {
        return FetchOutcome::Failed;
    }
    let Ok(mut response) = ureq::get(url).call() else {
        return FetchOutcome::Failed;
    };
    let Ok(bytes) = response.body_mut().read_to_vec() else {
        return FetchOutcome::Failed;
    };
    let Ok(decoded) = image::load_from_memory(&bytes) else {
        return FetchOutcome::Failed;
    };
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    FetchOutcome::Ready(DecodedImage {
        width: width as usize,
        height: height as usize,
        rgba: rgba.into_raw(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use egui::Context;

    #[test]
    fn unknown_url_starts_loading_and_never_blocks() {
        let ctx = Context::default();
        let mut cache = ArtworkCache::new();
        // A URL that resolves to nothing real on this machine's network
        // (reserved TEST-NET-1, RFC 5737) — `get` must still return
        // `Loading` immediately, not block for the fetch to fail.
        let state = cache.get(&ctx, "http://192.0.2.1/definitely-nowhere.jpg");
        assert_eq!(state, ArtworkState::Loading);
    }

    #[test]
    fn repeated_get_for_the_same_url_does_not_queue_duplicate_jobs() {
        let ctx = Context::default();
        let mut cache = ArtworkCache::new();
        let url = "http://192.0.2.1/same.jpg";
        cache.get(&ctx, url);
        cache.get(&ctx, url);
        assert_eq!(cache.in_flight.len(), 1);
    }

    #[test]
    fn cache_starts_with_no_textures() {
        let cache = ArtworkCache::new();
        assert_eq!(cache.texture_count(), 0);
    }
}
