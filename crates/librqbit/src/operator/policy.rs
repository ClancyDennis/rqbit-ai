//! Deterministic guardrails applied to operator decisions.
//!
//! These are enforced in rqbit code, never by the model: even if the model
//! proposes an action every tick, guardrails throttle how often each
//! (kind, target) can actually run — preventing pause/resume flapping, rate
//! limit thrashing, and tracker-announce spam.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::operator::action::Action;

pub struct Guardrails {
    last_run: HashMap<(&'static str, Option<usize>), Instant>,
}

impl Guardrails {
    pub fn new() -> Self {
        Self {
            last_run: HashMap::new(),
        }
    }

    /// Minimum time between two executions of the same action kind on the same
    /// target. Reannounce is the most conservative (avoid annoying trackers).
    fn cooldown_for(kind: &str) -> Duration {
        match kind {
            "force_reannounce" => Duration::from_secs(300),
            "pause" | "resume" => Duration::from_secs(60),
            "set_global_upload_limit"
            | "set_global_download_limit"
            | "set_torrent_upload_limit"
            | "set_torrent_download_limit" => Duration::from_secs(30),
            _ => Duration::from_secs(60),
        }
    }

    /// If the action is allowed now, record it as run and return `Ok`.
    /// Otherwise return `Err(reason)` describing the remaining cooldown.
    pub fn check_and_record(&mut self, action: &Action) -> Result<(), String> {
        let key = (action.kind_str(), action.target_idx());
        let cooldown = Self::cooldown_for(action.kind_str());
        if let Some(prev) = self.last_run.get(&key) {
            let elapsed = prev.elapsed();
            if elapsed < cooldown {
                return Err(format!(
                    "on cooldown ({}s of {}s remaining)",
                    (cooldown - elapsed).as_secs(),
                    cooldown.as_secs()
                ));
            }
        }
        self.last_run.insert(key, Instant::now());
        Ok(())
    }
}

impl Default for Guardrails {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_run_within_cooldown_is_blocked() {
        let mut g = Guardrails::new();
        let a = Action::Pause { idx: 0 };
        assert!(g.check_and_record(&a).is_ok(), "first run allowed");
        assert!(
            g.check_and_record(&a).is_err(),
            "immediate second run blocked by cooldown"
        );
        // A different target is independent.
        assert!(g.check_and_record(&Action::Pause { idx: 1 }).is_ok());
    }
}
