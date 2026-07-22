//! Typed, tier-classified actions.
//!
//! The model proposes loosely-typed actions ([`ProposedAction`]); this module
//! maps them to a strict [`Action`] enum and assigns each a risk [`ActionTier`].
//! Crucially, the tier is decided *here in rqbit code*, from the action variant
//! — never by the model — so a hallucinating or hostile model cannot escalate a
//! destructive action into the auto-executed tier.

use std::collections::BTreeSet;

use anyhow::Context;

use crate::operator::model::ProposedAction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Pause {
        idx: usize,
    },
    Resume {
        idx: usize,
    },
    /// `None`/`0` means "unlimited".
    SetGlobalUploadLimit {
        bps: Option<u32>,
    },
    SetGlobalDownloadLimit {
        bps: Option<u32>,
    },
    UpdateOnlyFiles {
        idx: usize,
        files: BTreeSet<usize>,
    },
    /// Per-torrent upload rate limit. `None`/`0` means "unlimited".
    SetTorrentUploadLimit {
        idx: usize,
        bps: Option<u32>,
    },
    /// Per-torrent download rate limit. `None`/`0` means "unlimited".
    SetTorrentDownloadLimit {
        idx: usize,
        bps: Option<u32>,
    },
    /// Trigger an immediate tracker announce for a torrent.
    ForceReannounce {
        idx: usize,
    },
    /// Re-verify a torrent's on-disk files against the piece hashes.
    RecheckFiles {
        idx: usize,
    },
    /// Add a tracker URL to a torrent at runtime.
    AddTracker {
        idx: usize,
        url: String,
    },
    /// Ban a peer address for the rest of the session.
    BanPeer {
        addr: String,
    },
    ForgetTorrent {
        idx: usize,
    },
    DeleteTorrentWithFiles {
        idx: usize,
    },
}

/// Risk tier. Assigned by code, never by the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionTier {
    /// Reversible, low blast radius. May be executed automatically.
    Auto,
    /// User-visible but reversible. Surfaced; not auto-executed in this stage.
    Notify,
    /// Data-losing. Never auto-executed; requires explicit human confirmation.
    Confirm,
}

impl Action {
    pub fn tier(&self) -> ActionTier {
        match self {
            Action::Pause { .. }
            | Action::Resume { .. }
            | Action::SetGlobalUploadLimit { .. }
            | Action::SetGlobalDownloadLimit { .. }
            | Action::SetTorrentUploadLimit { .. }
            | Action::SetTorrentDownloadLimit { .. }
            // Reannounce is reversible and low-risk (just re-announces to
            // trackers already in use), and is now implemented, so it may run
            // automatically (capped per tick).
            | Action::ForceReannounce { .. } => ActionTier::Auto,
            Action::UpdateOnlyFiles { .. }
            | Action::RecheckFiles { .. }
            | Action::AddTracker { .. }
            | Action::BanPeer { .. } => ActionTier::Notify,
            Action::ForgetTorrent { .. } | Action::DeleteTorrentWithFiles { .. } => {
                ActionTier::Confirm
            }
        }
    }

    pub fn kind_str(&self) -> &'static str {
        match self {
            Action::Pause { .. } => "pause",
            Action::Resume { .. } => "resume",
            Action::SetGlobalUploadLimit { .. } => "set_global_upload_limit",
            Action::SetGlobalDownloadLimit { .. } => "set_global_download_limit",
            Action::UpdateOnlyFiles { .. } => "update_only_files",
            Action::SetTorrentUploadLimit { .. } => "set_torrent_upload_limit",
            Action::SetTorrentDownloadLimit { .. } => "set_torrent_download_limit",
            Action::ForceReannounce { .. } => "force_reannounce",
            Action::RecheckFiles { .. } => "recheck_files",
            Action::AddTracker { .. } => "add_tracker",
            Action::BanPeer { .. } => "ban_peer",
            Action::ForgetTorrent { .. } => "forget",
            Action::DeleteTorrentWithFiles { .. } => "delete_with_files",
        }
    }

    /// Map a model-proposed action into a strict [`Action`]. Returns an error
    /// for unknown kinds or missing required parameters, so the caller can skip
    /// it (fail-closed).
    pub fn from_proposed(torrent_idx: Option<usize>, p: &ProposedAction) -> anyhow::Result<Action> {
        let idx = || torrent_idx.context("action requires a torrent_idx");
        Ok(match p.kind.as_str() {
            "pause" => Action::Pause { idx: idx()? },
            "resume" | "start" => Action::Resume { idx: idx()? },
            "set_global_upload_limit" => Action::SetGlobalUploadLimit {
                bps: parse_opt_u32(p, "bps"),
            },
            "set_global_download_limit" => Action::SetGlobalDownloadLimit {
                bps: parse_opt_u32(p, "bps"),
            },
            "update_only_files" => Action::UpdateOnlyFiles {
                idx: idx()?,
                files: parse_files(p)?,
            },
            "set_torrent_upload_limit" => Action::SetTorrentUploadLimit {
                idx: idx()?,
                bps: parse_opt_u32(p, "bps"),
            },
            "set_torrent_download_limit" => Action::SetTorrentDownloadLimit {
                idx: idx()?,
                bps: parse_opt_u32(p, "bps"),
            },
            "force_reannounce" => Action::ForceReannounce { idx: idx()? },
            "recheck_files" => Action::RecheckFiles { idx: idx()? },
            "add_tracker" => Action::AddTracker {
                idx: idx()?,
                url: parse_str(p, "url")?,
            },
            "ban_peer" => Action::BanPeer {
                addr: parse_str(p, "addr")?,
            },
            "forget" => Action::ForgetTorrent { idx: idx()? },
            "delete_with_files" => Action::DeleteTorrentWithFiles { idx: idx()? },
            other => anyhow::bail!("unknown action kind: {other}"),
        })
    }
}

fn parse_opt_u32(p: &ProposedAction, key: &str) -> Option<u32> {
    p.params
        .get(key)
        .and_then(|v| v.as_u64())
        .and_then(|n| u32::try_from(n).ok())
}

fn parse_str(p: &ProposedAction, key: &str) -> anyhow::Result<String> {
    p.params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .with_context(|| format!("action requires a string '{key}' parameter"))
}

fn parse_files(p: &ProposedAction) -> anyhow::Result<BTreeSet<usize>> {
    let arr = p
        .params
        .get("files")
        .and_then(|v| v.as_array())
        .context("update_only_files requires a 'files' array parameter")?;
    Ok(arr
        .iter()
        .filter_map(|v| v.as_u64())
        .filter_map(|n| usize::try_from(n).ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposed(kind: &str) -> ProposedAction {
        ProposedAction {
            kind: kind.to_string(),
            params: serde_json::Map::new(),
        }
    }

    #[test]
    fn destructive_actions_are_always_confirm_tier() {
        // The core safety invariant: delete/forget can never be Auto.
        assert_eq!(Action::ForgetTorrent { idx: 0 }.tier(), ActionTier::Confirm);
        assert_eq!(
            Action::DeleteTorrentWithFiles { idx: 0 }.tier(),
            ActionTier::Confirm
        );
    }

    #[test]
    fn reversible_actions_are_auto_tier() {
        assert_eq!(Action::Pause { idx: 0 }.tier(), ActionTier::Auto);
        assert_eq!(Action::Resume { idx: 0 }.tier(), ActionTier::Auto);
        assert_eq!(
            Action::SetGlobalUploadLimit { bps: Some(1000) }.tier(),
            ActionTier::Auto
        );
    }

    #[test]
    fn maps_known_kinds_and_rejects_unknown() {
        assert_eq!(
            Action::from_proposed(Some(3), &proposed("pause")).unwrap(),
            Action::Pause { idx: 3 }
        );
        assert_eq!(
            Action::from_proposed(Some(3), &proposed("delete_with_files")).unwrap(),
            Action::DeleteTorrentWithFiles { idx: 3 }
        );
        assert!(Action::from_proposed(Some(3), &proposed("nuke_everything")).is_err());
        // Missing required idx is rejected.
        assert!(Action::from_proposed(None, &proposed("pause")).is_err());
    }

    #[test]
    fn parses_rate_limit_param() {
        let mut p = proposed("set_global_upload_limit");
        p.params
            .insert("bps".to_string(), serde_json::json!(500_000));
        assert_eq!(
            Action::from_proposed(None, &p).unwrap(),
            Action::SetGlobalUploadLimit { bps: Some(500_000) }
        );
    }

    #[test]
    fn per_torrent_limits_are_auto_tier() {
        assert_eq!(
            Action::SetTorrentUploadLimit {
                idx: 0,
                bps: Some(1000)
            }
            .tier(),
            ActionTier::Auto
        );
        assert_eq!(
            Action::SetTorrentDownloadLimit { idx: 0, bps: None }.tier(),
            ActionTier::Auto
        );
    }

    #[test]
    fn reannounce_is_auto_tier() {
        // Implemented, reversible, low-risk -> may run automatically.
        assert_eq!(Action::ForceReannounce { idx: 0 }.tier(), ActionTier::Auto);
    }

    #[test]
    fn coarse_levers_are_notify_tier() {
        // Reversible but user-visible: never Auto, never Confirm.
        assert_eq!(Action::RecheckFiles { idx: 0 }.tier(), ActionTier::Notify);
        assert_eq!(
            Action::AddTracker {
                idx: 0,
                url: "http://t.invalid/announce".to_string()
            }
            .tier(),
            ActionTier::Notify
        );
        assert_eq!(
            Action::BanPeer {
                addr: "1.2.3.4:6881".to_string()
            }
            .tier(),
            ActionTier::Notify
        );
    }

    #[test]
    fn maps_per_torrent_limit_kinds() {
        let mut p = proposed("set_torrent_download_limit");
        p.params
            .insert("bps".to_string(), serde_json::json!(250_000));
        assert_eq!(
            Action::from_proposed(Some(7), &p).unwrap(),
            Action::SetTorrentDownloadLimit {
                idx: 7,
                bps: Some(250_000)
            }
        );
        // Per-torrent limits require a torrent_idx.
        assert!(Action::from_proposed(None, &p).is_err());
    }

    #[test]
    fn maps_reannounce_and_recheck() {
        assert_eq!(
            Action::from_proposed(Some(2), &proposed("force_reannounce")).unwrap(),
            Action::ForceReannounce { idx: 2 }
        );
        assert_eq!(
            Action::from_proposed(Some(2), &proposed("recheck_files")).unwrap(),
            Action::RecheckFiles { idx: 2 }
        );
    }

    #[test]
    fn maps_add_tracker_and_requires_url() {
        let mut p = proposed("add_tracker");
        p.params.insert(
            "url".to_string(),
            serde_json::json!("http://tracker.invalid/announce"),
        );
        assert_eq!(
            Action::from_proposed(Some(4), &p).unwrap(),
            Action::AddTracker {
                idx: 4,
                url: "http://tracker.invalid/announce".to_string()
            }
        );
        // Missing url is rejected (fail-closed).
        assert!(Action::from_proposed(Some(4), &proposed("add_tracker")).is_err());
    }

    #[test]
    fn maps_ban_peer_and_requires_addr() {
        let mut p = proposed("ban_peer");
        p.params
            .insert("addr".to_string(), serde_json::json!("1.2.3.4:6881"));
        assert_eq!(
            Action::from_proposed(None, &p).unwrap(),
            Action::BanPeer {
                addr: "1.2.3.4:6881".to_string()
            }
        );
        assert!(Action::from_proposed(None, &proposed("ban_peer")).is_err());
    }
}
