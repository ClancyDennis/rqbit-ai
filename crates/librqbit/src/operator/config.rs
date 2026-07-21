use std::time::Duration;

/// Configuration for the in-process AI operator.
///
/// The operator is an optional supervisory loop that periodically reads session
/// state, asks a model what a vigilant human operator would do, and (optionally)
/// applies a gated set of safe, reversible actions. It never participates in the
/// data plane (piece picking, choke/unchoke, rate-limiter mechanics).
#[derive(Debug, Clone)]
pub struct OperatorOptions {
    /// Master kill-switch. When false the loop is never spawned.
    pub enabled: bool,
    /// When true (the default), the loop computes and logs decisions but never
    /// mutates any state. Must be explicitly disabled to allow actions.
    pub dry_run: bool,
    /// How often the loop wakes up. ~1-2/minute is the intended cadence.
    pub interval: Duration,
    /// Model / endpoint configuration.
    pub model: ModelConfig,
    /// Upper bound on how many AUTO-tier actions may be executed in a single tick.
    pub max_auto_actions_per_tick: usize,
}

impl Default for OperatorOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            dry_run: true,
            interval: Duration::from_secs(45),
            model: ModelConfig::default(),
            max_auto_actions_per_tick: 2,
        }
    }
}

/// Model endpoint configuration. Provider-agnostic: any OpenAI-compatible
/// `/v1/chat/completions` endpoint works. Nothing is hardcoded.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    /// Base URL of the OpenAI-compatible endpoint (e.g. `http://localhost:8080`).
    /// Empty means "no model configured" -> the operator uses `NullModel`.
    pub base_url: String,
    /// Model identifier, e.g. `gpt-5.6-luna`.
    pub model: String,
    /// Optional bearer token. Prefer supplying via environment, not flags.
    pub api_key: Option<String>,
    /// Per-request timeout. The shared reqwest client has no default timeout,
    /// so this must always be set.
    pub request_timeout: Duration,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            model: String::new(),
            api_key: None,
            request_timeout: Duration::from_secs(30),
        }
    }
}

impl ModelConfig {
    /// Whether a usable endpoint has been configured.
    pub fn is_configured(&self) -> bool {
        !self.base_url.is_empty() && !self.model.is_empty()
    }
}
