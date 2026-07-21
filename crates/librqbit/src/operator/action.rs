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
            | Action::SetGlobalDownloadLimit { .. } => ActionTier::Auto,
            Action::UpdateOnlyFiles { .. } => ActionTier::Notify,
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
}
