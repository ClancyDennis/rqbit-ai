//! The state snapshot handed to the model each tick.
//!
//! Built purely from existing, cheap read paths on [`Session`]. All free-text
//! that originates from the network or torrent files (names, error strings) is
//! wrapped in [`UntrustedText`] and only ever serialized inside the labeled
//! `untrusted_observed_state` block (see `prompt.rs`).

use serde::Serialize;

use crate::{ManagedTorrent, Session};

/// Text that came from an untrusted source (torrent/file names, peer client
/// strings, error messages). Sanitized (control chars stripped, length-capped)
/// and marked so it is never mistaken for an instruction.
#[derive(Debug, Clone, Serialize)]
pub struct UntrustedText(String);

impl UntrustedText {
    const MAX_LEN: usize = 512;

    pub fn new(s: impl AsRef<str>) -> Self {
        let cleaned: String = s
            .as_ref()
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .take(Self::MAX_LEN)
            .collect();
        Self(cleaned)
    }
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub session: SessionSummary,
    pub torrents: Vec<TorrentSummary>,
}

#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub uptime_seconds: u64,
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub fetched_bytes: u64,
    pub uploaded_bytes: u64,
    pub blocked_incoming: u64,
    pub blocked_outgoing: u64,
    pub peers: PeerAggregate,
    pub dht: Option<DhtSummary>,
}

#[derive(Debug, Serialize)]
pub struct DhtSummary {
    pub routing_table_size: usize,
    pub routing_table_size_v6: usize,
    pub outstanding_requests: usize,
}

#[derive(Debug, Serialize)]
pub struct PeerAggregate {
    pub live: u32,
    pub connecting: u32,
    pub queued: u32,
    pub seen: u32,
    pub dead: u32,
    pub not_needed: u32,
}

#[derive(Debug, Serialize)]
pub struct TorrentSummary {
    pub idx: usize,
    pub info_hash: String,
    pub name: Option<UntrustedText>,
    pub state: String,
    pub error: Option<UntrustedText>,
    pub paused: bool,
    pub finished: bool,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub progress_percent: f64,
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub eta_seconds: Option<u64>,
    pub live_peers: u32,
    pub dead_peers: u32,
}

/// Build a snapshot from the current session state. Cheap: only reads existing
/// counters/gauges and takes a brief `db` read lock; never awaits.
pub fn build(session: &Session) -> Snapshot {
    let s = session.stats_snapshot();
    let dht = session.get_dht().map(|d| {
        let st = d.stats();
        DhtSummary {
            routing_table_size: st.routing_table_size,
            routing_table_size_v6: st.routing_table_size_v6,
            outstanding_requests: st.outstanding_requests,
        }
    });

    // stats()/is_paused()/etc. do not await, so it is safe to build summaries
    // directly inside the with_torrents closure.
    let torrents = session.with_torrents(|it| it.map(|(id, h)| torrent_summary(id, h)).collect());

    Snapshot {
        schema_version: 1,
        session: SessionSummary {
            uptime_seconds: s.uptime_seconds,
            download_mbps: s.download_speed.mbps,
            upload_mbps: s.upload_speed.mbps,
            fetched_bytes: s.counters.fetched_bytes,
            uploaded_bytes: s.counters.uploaded_bytes,
            blocked_incoming: s.counters.blocked_incoming,
            blocked_outgoing: s.counters.blocked_outgoing,
            peers: PeerAggregate {
                live: s.peers.live,
                connecting: s.peers.connecting,
                queued: s.peers.queued,
                seen: s.peers.seen,
                dead: s.peers.dead,
                not_needed: s.peers.not_needed,
            },
            dht,
        },
        torrents,
    }
}

fn torrent_summary(idx: usize, h: &ManagedTorrent) -> TorrentSummary {
    let st = h.stats();
    let (download_mbps, upload_mbps, eta_seconds, live_peers, dead_peers) = match &st.live {
        Some(l) => (
            l.download_speed.mbps,
            l.upload_speed.mbps,
            l.time_remaining.as_ref().map(|d| d.as_secs()),
            l.snapshot.peer_stats.live,
            l.snapshot.peer_stats.dead,
        ),
        None => (0.0, 0.0, None, 0, 0),
    };
    let progress_percent = if st.total_bytes > 0 {
        st.progress_bytes as f64 / st.total_bytes as f64 * 100.0
    } else {
        0.0
    };

    TorrentSummary {
        idx,
        info_hash: h.info_hash().as_string(),
        name: h.name().map(UntrustedText::new),
        state: st.state.to_string(),
        error: st.error.as_ref().map(UntrustedText::new),
        paused: h.is_paused(),
        finished: st.finished,
        progress_bytes: st.progress_bytes,
        total_bytes: st.total_bytes,
        progress_percent,
        download_mbps,
        upload_mbps,
        eta_seconds,
        live_peers,
        dead_peers,
    }
}
