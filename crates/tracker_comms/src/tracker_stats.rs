//! Per-tracker health telemetry.
//!
//! Distinct from [`crate::TrackerCommsStats`] (which is the announce *payload*
//! we send to trackers). This records how each tracker is *behaving*: last
//! announce status, latency, peers yielded, and error counts. Populated by the
//! announce loops and read by diagnostics / the AI operator.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use librqbit_core::hash_id::Id20;
use parking_lot::Mutex;
use serde_derive::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackerStatus {
    #[default]
    Pending,
    Working,
    Error,
}

/// Serialized per-tracker health view.
#[derive(Clone, Debug, Serialize)]
pub struct TrackerStat {
    pub url: String,
    pub status: TrackerStatus,
    pub announces: u64,
    pub errors: u64,
    pub last_peers: Option<usize>,
    pub last_latency_ms: Option<u64>,
    pub last_error: Option<String>,
    pub seconds_since_last_announce: Option<u64>,
}

#[derive(Default)]
struct Entry {
    status: TrackerStatus,
    announces: u64,
    errors: u64,
    last_peers: Option<usize>,
    last_latency_ms: Option<u64>,
    last_error: Option<String>,
    last_announce_at: Option<Instant>,
}

/// Session-wide registry of per-tracker health, keyed by `(info_hash, url)`.
/// Cheap to clone (it is meant to live behind an `Arc`).
#[derive(Default)]
pub struct TrackerStatsRegistry {
    inner: Mutex<HashMap<(Id20, String), Entry>>,
}

impl TrackerStatsRegistry {
    pub fn record_success(&self, info_hash: Id20, url: &str, latency: Duration, peers: usize) {
        let mut g = self.inner.lock();
        let e = g.entry((info_hash, url.to_string())).or_default();
        e.status = TrackerStatus::Working;
        e.announces += 1;
        e.last_peers = Some(peers);
        e.last_latency_ms = Some(u64::try_from(latency.as_millis()).unwrap_or(u64::MAX));
        e.last_error = None;
        e.last_announce_at = Some(Instant::now());
    }

    pub fn record_error(&self, info_hash: Id20, url: &str, err: &str) {
        let mut g = self.inner.lock();
        let e = g.entry((info_hash, url.to_string())).or_default();
        e.status = TrackerStatus::Error;
        e.errors += 1;
        e.last_error = Some(err.chars().take(256).collect());
    }

    /// All tracker stats for one torrent.
    pub fn snapshot_for(&self, info_hash: Id20) -> Vec<TrackerStat> {
        let g = self.inner.lock();
        g.iter()
            .filter(|((ih, _), _)| *ih == info_hash)
            .map(|((_, url), e)| TrackerStat {
                url: url.clone(),
                status: e.status,
                announces: e.announces,
                errors: e.errors,
                last_peers: e.last_peers,
                last_latency_ms: e.last_latency_ms,
                last_error: e.last_error.clone(),
                seconds_since_last_announce: e.last_announce_at.map(|t| t.elapsed().as_secs()),
            })
            .collect()
    }

    /// Drop all entries for a torrent (e.g. when it is removed).
    pub fn clear_for(&self, info_hash: Id20) {
        self.inner.lock().retain(|(ih, _), _| *ih != info_hash);
    }
}
