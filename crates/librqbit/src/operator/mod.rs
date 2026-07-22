//! In-process AI operator: an optional supervisory loop.
//!
//! The operator periodically reads session state, asks a model what a vigilant
//! human operator would do, and applies a gated set of safe, reversible actions
//! optimizing Performance, Security and Reliability.
//!
//! Hard invariants (enforced across all stages):
//! - Never in the data plane. Only coarse, existing engine commands are used.
//! - Dry-run by default; actions require explicit opt-in (`OperatorOptions::dry_run`).
//! - Untrusted text (torrent/file/peer names, errors) is passed to the model as
//!   clearly-delimited data, never as instructions.
//! - Action tiers are assigned by rqbit code (`Action::tier`), not the model.
//!
//! The loop maps model suggestions to typed actions and executes only the
//! reversible AUTO tier (when not in dry-run, capped per tick, subject to
//! cooldowns). NOTIFY actions are surfaced but not executed; destructive
//! CONFIRM actions (incl. ban-peer) are enqueued for human confirmation via the
//! HTTP API and never auto-fired. The decision log, confirmations, and (UI-
//! editable, restart-to-apply) config are exposed over HTTP.

mod action;
mod config;
mod enrich;
mod executor;
mod handle;
mod model;
mod model_openai;
mod persist;
mod policy;
mod prompt;
mod snapshot;

pub use action::{Action, ActionTier};
pub use config::{ModelConfig, OperatorOptions};
pub use handle::{DecisionRecord, OperatorHandle, PendingConfirmationView};
pub use model::{
    DecisionInput, DecisionOutput, EchoModel, NullModel, OperatorModel, SuggestedAction,
};
pub use persist::{
    OperatorPersistedConfig, load as load_persisted_config, operator_api_key,
    save as save_persisted_config,
};

use std::sync::Arc;
use tracing::{info, warn};

use crate::Session;
use model_openai::OpenAiCompatModel;

/// Run the operator loop for the lifetime of the session.
///
/// Spawned via [`Session::spawn`], so it is cancelled automatically when the
/// session's cancellation token fires; this function itself simply loops.
pub async fn run(
    session: Arc<Session>,
    opts: OperatorOptions,
    handle: Arc<OperatorHandle>,
) -> anyhow::Result<()> {
    let model: Box<dyn OperatorModel> = if opts.model.is_configured() {
        info!(model = %opts.model.model, "operator: using configured model endpoint");
        Box::new(OpenAiCompatModel::new(
            session.reqwest_client(),
            opts.model.clone(),
        ))
    } else {
        info!("operator: no model endpoint configured; running with NullModel (no suggestions)");
        Box::new(NullModel)
    };

    handle.set_effective(persist::OperatorPersistedConfig::from_options(&opts));
    run_with_model(session, opts, model, handle).await
}

/// Loop body, factored out so tests can inject a deterministic model.
async fn run_with_model(
    session: Arc<Session>,
    opts: OperatorOptions,
    model: Box<dyn OperatorModel>,
    handle: Arc<OperatorHandle>,
) -> anyhow::Result<()> {
    info!(
        interval_secs = opts.interval.as_secs(),
        dry_run = opts.dry_run,
        max_auto_actions_per_tick = opts.max_auto_actions_per_tick,
        "operator started"
    );

    // Peer ASN/org enricher (no-op unless an ASN db path is configured).
    let enricher = enrich::build_enricher(opts.asn_db_path.as_deref());
    // Deterministic anti-thrash cooldowns, enforced in code (not by the model).
    let mut guardrails = policy::Guardrails::new();

    let mut ticker = tokio::time::interval(opts.interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;

        let input = DecisionInput {
            snapshot: snapshot::build(&session, enricher.as_ref()),
        };
        let out = match model.decide(&input).await {
            Ok(o) => o,
            Err(e) => {
                warn!("operator: model decision failed: {e:#}");
                continue;
            }
        };
        // Per-tick heartbeat so it's visibly alive even when idle.
        info!(
            torrents = input.snapshot.torrents.len(),
            proposed = out.decisions.len(),
            dry_run = opts.dry_run,
            "operator tick"
        );
        if out.decisions.is_empty() {
            continue;
        }

        let mut auto_executed = 0usize;
        for d in &out.decisions {
            let action = match Action::from_proposed(d.torrent_idx, &d.action) {
                Ok(a) => a,
                Err(e) => {
                    warn!(kind = %d.action.kind, "operator: skipping action: {e:#}");
                    continue;
                }
            };
            let tier = action.tier();
            let outcome: String = match tier {
                ActionTier::Confirm => {
                    let id = handle.queue_confirmation(action.clone(), d.rationale.clone());
                    format!("queued for confirmation (id {id})")
                }
                ActionTier::Notify => "surfaced (notify tier; not executed)".to_string(),
                ActionTier::Auto => {
                    if opts.dry_run {
                        "dry-run (would execute)".to_string()
                    } else if auto_executed >= opts.max_auto_actions_per_tick {
                        "skipped: per-tick auto-action cap reached".to_string()
                    } else if let Err(reason) = guardrails.check_and_record(&action) {
                        format!("skipped: {reason}")
                    } else {
                        match executor::execute(&session, &action).await {
                            Ok(()) => {
                                auto_executed += 1;
                                "executed".to_string()
                            }
                            Err(e) => format!("failed: {e:#}"),
                        }
                    }
                }
            };
            // rationale is model output; treat as data.
            info!(
                action = action.kind_str(),
                tier = tier.as_str(),
                torrent = ?d.torrent_idx,
                confidence = ?d.confidence,
                outcome = %outcome,
                "operator decision"
            );
            handle.record_decision(
                action.kind_str(),
                tier,
                action.target_idx(),
                &d.rationale,
                d.confidence,
                outcome,
            );
        }
    }
}

/// Approve or reject a queued destructive confirmation. On approve, executes
/// the action. Called by the HTTP API.
pub async fn confirm(
    session: &Arc<Session>,
    handle: &OperatorHandle,
    id: u64,
    approve: bool,
) -> anyhow::Result<&'static str> {
    let pending = handle
        .take_pending(id)
        .ok_or_else(|| anyhow::anyhow!("no pending confirmation with id {id}"))?;
    if approve {
        executor::execute(session, &pending.action).await?;
        Ok("approved")
    } else {
        Ok("rejected")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let o = OperatorOptions::default();
        assert!(!o.enabled, "operator must be disabled by default");
        assert!(o.dry_run, "operator must be dry-run by default");
    }

    #[test]
    fn null_model_is_object_safe_and_decides_nothing() {
        let _m: Box<dyn OperatorModel> = Box::new(NullModel);
    }

    #[test]
    fn empty_model_config_is_not_configured() {
        assert!(!ModelConfig::default().is_configured());
    }
}
