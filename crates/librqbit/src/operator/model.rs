use serde::Deserialize;

use crate::operator::snapshot::Snapshot;

/// Input handed to the model on each tick: the injection-safe state snapshot.
pub struct DecisionInput {
    pub snapshot: Snapshot,
}

/// The model's response for a tick.
#[derive(Debug, Default, Deserialize)]
pub struct DecisionOutput {
    #[serde(default)]
    pub decisions: Vec<SuggestedAction>,
    /// Per-torrent assessment, including "no action" notes.
    #[serde(default)]
    pub assessments: Vec<Assessment>,
}

/// The model's brief opinion of one torrent, whether or not it warrants action.
#[derive(Debug, Clone, Deserialize)]
pub struct Assessment {
    pub torrent_idx: usize,
    #[serde(default)]
    pub summary: String,
    /// "none" | "watch" | "problem".
    #[serde(default)]
    pub concern: String,
}

/// A single action the model proposes. The action is intentionally loosely
/// typed here (a `kind` + free-form `params`); Stage C maps it to a strictly
/// typed, tier-classified action before anything is ever executed.
#[derive(Debug, Clone, Deserialize)]
pub struct SuggestedAction {
    #[serde(default)]
    pub torrent_idx: Option<usize>,
    pub action: ProposedAction,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProposedAction {
    pub kind: String,
    #[serde(default)]
    pub params: serde_json::Map<String, serde_json::Value>,
}

/// Abstraction over the decision model, so the concrete provider/endpoint is a
/// swappable implementation detail and tests can run with zero network.
#[async_trait::async_trait]
pub trait OperatorModel: Send + Sync {
    async fn decide(&self, input: &DecisionInput) -> anyhow::Result<DecisionOutput>;
}

/// A model that decides nothing. Used when no endpoint is configured, so the
/// loop degrades to a harmless heartbeat rather than failing.
pub struct NullModel;

#[async_trait::async_trait]
impl OperatorModel for NullModel {
    async fn decide(&self, _input: &DecisionInput) -> anyhow::Result<DecisionOutput> {
        Ok(DecisionOutput::default())
    }
}

/// A deterministic model that always returns a fixed set of decisions. Used to
/// exercise the loop and action layer in tests with zero network.
pub struct EchoModel {
    pub decisions: Vec<SuggestedAction>,
}

#[async_trait::async_trait]
impl OperatorModel for EchoModel {
    async fn decide(&self, _input: &DecisionInput) -> anyhow::Result<DecisionOutput> {
        Ok(DecisionOutput {
            decisions: self.decisions.clone(),
            ..Default::default()
        })
    }
}
