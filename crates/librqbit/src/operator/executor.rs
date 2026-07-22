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
        Action::SetTorrentUploadLimit { idx, bps } => {
            let h = resolve(session, *idx)?;
            session.set_torrent_upload_limit(&h, to_nonzero(*bps))?;
        }
        Action::SetTorrentDownloadLimit { idx, bps } => {
            let h = resolve(session, *idx)?;
            session.set_torrent_download_limit(&h, to_nonzero(*bps))?;
        }
        Action::ForceReannounce { idx } => {
            let h = resolve(session, *idx)?;
            session.force_reannounce(h.info_hash())?;
        }
        // Stubbed levers: the Action variant, tier, and mapping exist so the
        // operator can propose them and they are correctly gated, but the
        // engine command they map to does not exist yet. Each requires
        // non-trivial core plumbing (see the task report) that would touch
        // restricted crates or the connection/data plane, so we fail-closed
        // rather than force an unsafe implementation.
        Action::RecheckFiles { .. } => {
            anyhow::bail!(
                "recheck_files not yet implemented: needs an in-place re-verify hook \
                 in the torrent state machine (currently only possible via remove + re-add)"
            );
        }
        Action::AddTracker { .. } => {
            anyhow::bail!(
                "add_tracker not yet implemented: needs runtime tracker registration \
                 on a live TrackerComms handle (tracker_comms crate; no handle is stored today)"
            );
        }
        Action::BanPeer { .. } => {
            anyhow::bail!(
                "ban_peer not yet implemented: needs a mutable session ban-set consulted \
                 in the connection-accept path (connection plane)"
            );
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
