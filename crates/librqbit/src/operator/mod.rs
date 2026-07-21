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
//! Stage C (current): the loop maps model suggestions to typed actions and
//! executes only the reversible AUTO tier (and only when not in dry-run,
//! capped per tick). NOTIFY actions are surfaced but not executed; destructive
//! CONFIRM actions are enqueued for human confirmation and never auto-fired.

mod action;
mod config;
mod executor;
mod model;
mod model_openai;
mod prompt;
mod snapshot;

pub use action::{Action, ActionTier};
pub use config::{ModelConfig, OperatorOptions};
pub use executor::PendingConfirmation;
pub use model::{
    DecisionInput, DecisionOutput, EchoModel, NullModel, OperatorModel, SuggestedAction,
};

use std::collections::VecDeque;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::Session;
use model_openai::OpenAiCompatModel;

/// Bound on the in-memory pending-confirmation queue (until a UI consumes it).
const MAX_PENDING_CONFIRMATIONS: usize = 128;

/// Run the operator loop for the lifetime of the session.
///
/// Spawned via [`Session::spawn`], so it is cancelled automatically when the
/// session's cancellation token fires; this function itself simply loops.
pub async fn run(session: Arc<Session>, opts: OperatorOptions) -> anyhow::Result<()> {
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

    run_with_model(session, opts, model).await
}

/// Loop body, factored out so tests can inject a deterministic model.
async fn run_with_model(
    session: Arc<Session>,
    opts: OperatorOptions,
    model: Box<dyn OperatorModel>,
) -> anyhow::Result<()> {
    info!(
        interval_secs = opts.interval.as_secs(),
        dry_run = opts.dry_run,
        max_auto_actions_per_tick = opts.max_auto_actions_per_tick,
        "operator started"
    );

    let mut pending: VecDeque<PendingConfirmation> = VecDeque::new();
    let mut next_confirmation_id: u64 = 0;

    let mut ticker = tokio::time::interval(opts.interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;

        let input = DecisionInput {
            snapshot: snapshot::build(&session),
        };
        let out = match model.decide(&input).await {
            Ok(o) => o,
            Err(e) => {
                warn!("operator: model decision failed: {e:#}");
                continue;
            }
        };
        if out.decisions.is_empty() {
            debug!("operator: no suggestions this tick");
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
            // rationale is model output; treat as data in logs.
            info!(
                action = action.kind_str(),
                ?tier,
                torrent = ?d.torrent_idx,
                confidence = ?d.confidence,
                rationale = %d.rationale,
                "operator decision"
            );

            match tier {
                ActionTier::Confirm => {
                    info!(
                        action = action.kind_str(),
                        "operator: destructive action requires confirmation; NOT executed"
                    );
                    if pending.len() >= MAX_PENDING_CONFIRMATIONS {
                        pending.pop_front();
                    }
                    pending.push_back(PendingConfirmation {
                        id: next_confirmation_id,
                        action,
                        rationale: d.rationale.clone(),
                    });
                    next_confirmation_id += 1;
                }
                ActionTier::Notify => {
                    info!(
                        action = action.kind_str(),
                        "operator: notify-tier action surfaced; not executed in this stage"
                    );
                }
                ActionTier::Auto => {
                    if opts.dry_run {
                        info!(
                            action = action.kind_str(),
                            "operator: would execute (dry-run)"
                        );
                    } else if auto_executed >= opts.max_auto_actions_per_tick {
                        info!(
                            action = action.kind_str(),
                            "operator: per-tick auto-action cap reached; deferring"
                        );
                    } else {
                        match executor::execute(&session, &action).await {
                            Ok(()) => {
                                auto_executed += 1;
                                info!(action = action.kind_str(), "operator: executed");
                            }
                            Err(e) => {
                                warn!(
                                    action = action.kind_str(),
                                    "operator: execution failed: {e:#}"
                                )
                            }
                        }
                    }
                }
            }
        }
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
