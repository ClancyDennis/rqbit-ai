//! Executes a typed [`Action`] against the session, mapping each variant to an
//! existing, coarse engine command. This module performs no gating itself
//! (tier/dry-run/kill-switch decisions are made by the caller); it only runs
//! the action once the caller has decided it should run.

use std::collections::HashSet;
use std::num::NonZeroU32;
use std::sync::Arc;

use anyhow::Context;

use crate::Session;
use crate::api::TorrentIdOrHash;
use crate::operator::action::Action;
use crate::torrent_state::ManagedTorrentHandle;

/// A destructive action awaiting explicit human confirmation. Enqueued by the
/// loop; consumed by the confirmation UI/endpoint (a later stage).
#[derive(Debug, Clone)]
pub struct PendingConfirmation {
    pub id: u64,
    pub action: Action,
    pub rationale: String,
}

pub async fn execute(session: &Arc<Session>, action: &Action) -> anyhow::Result<()> {
    match action {
        Action::Pause { idx } => {
            let h = resolve(session, *idx)?;
            session.pause(&h).await?;
        }
        Action::Resume { idx } => {
            let h = resolve(session, *idx)?;
            session.unpause(&h).await?;
        }
        Action::SetGlobalUploadLimit { bps } => {
            session.ratelimits.set_upload_bps(to_nonzero(*bps));
        }
        Action::SetGlobalDownloadLimit { bps } => {
            session.ratelimits.set_download_bps(to_nonzero(*bps));
        }
        Action::UpdateOnlyFiles { idx, files } => {
            let h = resolve(session, *idx)?;
            let set: HashSet<usize> = files.iter().copied().collect();
            session.update_only_files(&h, &set).await?;
        }
        Action::ForgetTorrent { idx } => {
            session.delete(TorrentIdOrHash::from(*idx), false).await?;
        }
        Action::DeleteTorrentWithFiles { idx } => {
            session.delete(TorrentIdOrHash::from(*idx), true).await?;
        }
    }
    Ok(())
}

fn resolve(session: &Session, idx: usize) -> anyhow::Result<ManagedTorrentHandle> {
    session
        .get(TorrentIdOrHash::from(idx))
        .with_context(|| format!("no torrent with id {idx}"))
}

/// `None` or `0` bps means "unlimited".
fn to_nonzero(bps: Option<u32>) -> Option<NonZeroU32> {
    bps.and_then(NonZeroU32::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_or_none_bps_is_unlimited() {
        assert_eq!(to_nonzero(None), None);
        assert_eq!(to_nonzero(Some(0)), None);
        assert_eq!(to_nonzero(Some(1000)), NonZeroU32::new(1000));
    }
}
