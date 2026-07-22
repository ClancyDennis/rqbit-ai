//! On-disk, UI-editable operator configuration.
//!
//! This is the durable config the web UI writes; it takes effect on the next
//! start (the running loop is not reconfigured live). It NEVER contains the API
//! key — that stays in the `RQBIT_OPERATOR_API_KEY` environment variable, off
//! the HTTP surface and off disk.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::operator::config::{ModelConfig, OperatorOptions};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorPersistedConfig {
    pub enabled: bool,
    pub dry_run: bool,
    pub poll_interval_secs: u64,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub asn_db_path: Option<String>,
}

impl Default for OperatorPersistedConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            dry_run: true,
            poll_interval_secs: 45,
            base_url: String::new(),
            model: String::new(),
            asn_db_path: None,
        }
    }
}

impl OperatorPersistedConfig {
    /// Build runtime options, filling the API key from the environment.
    pub fn to_options(&self) -> OperatorOptions {
        OperatorOptions {
            enabled: self.enabled,
            dry_run: self.dry_run,
            interval: std::time::Duration::from_secs(self.poll_interval_secs.max(1)),
            model: ModelConfig {
                base_url: self.base_url.clone(),
                model: self.model.clone(),
                api_key: std::env::var("RQBIT_OPERATOR_API_KEY").ok(),
                request_timeout: std::time::Duration::from_secs(30),
            },
            max_auto_actions_per_tick: 2,
            asn_db_path: self.asn_db_path.as_ref().map(PathBuf::from),
        }
    }

    /// Keyless view of the currently-effective options (for the GET endpoint).
    pub fn from_options(o: &OperatorOptions) -> Self {
        Self {
            enabled: o.enabled,
            dry_run: o.dry_run,
            poll_interval_secs: o.interval.as_secs(),
            base_url: o.model.base_url.clone(),
            model: o.model.model.clone(),
            asn_db_path: o.asn_db_path.as_ref().map(|p| p.display().to_string()),
        }
    }
}

fn config_path() -> anyhow::Result<PathBuf> {
    let dirs = librqbit_core::directories::get_configuration_directory("operator")?;
    Ok(dirs.config_dir().join("config.json"))
}

/// Load the persisted config, if any. Returns `None` on missing/unreadable file.
pub fn load() -> Option<OperatorPersistedConfig> {
    let path = config_path().ok()?;
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Persist the config (creating the config directory if needed).
pub fn save(cfg: &OperatorPersistedConfig) -> anyhow::Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(cfg)?)?;
    Ok(())
}
