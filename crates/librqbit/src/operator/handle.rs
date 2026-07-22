//! Shared operator state, readable/actionable from outside the loop (the HTTP
//! API). Holds a bounded log of recent decisions and the queue of destructive
//! actions awaiting human confirmation.

use std::collections::{HashMap, VecDeque};

use parking_lot::Mutex;
use serde::Serialize;

use crate::operator::action::{Action, ActionTier};

const MAX_DECISIONS: usize = 100;
const MAX_PENDING: usize = 128;

/// A recorded operator decision (serialized for the HTTP API / UI).
#[derive(Clone, Serialize)]
pub struct DecisionRecord {
    pub seq: u64,
    pub kind: String,
    pub tier: &'static str,
    pub torrent_idx: Option<usize>,
    pub rationale: String,
    pub confidence: Option<f32>,
    /// What actually happened: "executed", "dry-run", "queued for confirmation",
    /// "skipped: ...", "failed: ...", etc.
    pub outcome: String,
}

/// A destructive action awaiting explicit human confirmation.
pub(crate) struct PendingConfirmation {
    pub id: u64,
    pub action: Action,
    pub rationale: String,
}

/// Serialized view of a pending confirmation for the HTTP API / UI.
#[derive(Clone, Serialize)]
pub struct PendingConfirmationView {
    pub id: u64,
    pub kind: String,
    pub torrent_idx: Option<usize>,
    pub rationale: String,
}

/// The operator's latest opinion of a torrent (per-torrent "thoughts").
#[derive(Clone, Serialize)]
pub struct AssessmentRecord {
    pub torrent_idx: usize,
    pub summary: String,
    pub concern: String,
}

#[derive(Default)]
struct State {
    decisions: VecDeque<DecisionRecord>,
    pending: VecDeque<PendingConfirmation>,
    next_seq: u64,
    next_pending_id: u64,
    /// Latest per-torrent assessment, keyed by torrent id.
    assessments: HashMap<usize, AssessmentRecord>,
    /// Keyless snapshot of the config the running loop started with.
    effective: Option<crate::operator::persist::OperatorPersistedConfig>,
}

/// Cloneable-behind-Arc shared operator state.
#[derive(Default)]
pub struct OperatorHandle {
    state: Mutex<State>,
}

impl OperatorHandle {
    /// Record a decision in the bounded log.
    pub(crate) fn record_decision(
        &self,
        kind: &str,
        tier: ActionTier,
        torrent_idx: Option<usize>,
        rationale: &str,
        confidence: Option<f32>,
        outcome: String,
    ) {
        let mut s = self.state.lock();
        let seq = s.next_seq;
        s.next_seq += 1;
        if s.decisions.len() >= MAX_DECISIONS {
            s.decisions.pop_front();
        }
        s.decisions.push_back(DecisionRecord {
            seq,
            kind: kind.to_string(),
            tier: tier.as_str(),
            torrent_idx,
            rationale: rationale.to_string(),
            confidence,
            outcome,
        });
    }

    /// Enqueue a destructive action for confirmation; returns its id.
    pub(crate) fn queue_confirmation(&self, action: Action, rationale: String) -> u64 {
        let mut s = self.state.lock();
        let id = s.next_pending_id;
        s.next_pending_id += 1;
        if s.pending.len() >= MAX_PENDING {
            s.pending.pop_front();
        }
        s.pending.push_back(PendingConfirmation {
            id,
            action,
            rationale,
        });
        id
    }

    /// Remove and return a pending confirmation by id.
    pub(crate) fn take_pending(&self, id: u64) -> Option<PendingConfirmation> {
        let mut s = self.state.lock();
        let pos = s.pending.iter().position(|p| p.id == id)?;
        s.pending.remove(pos)
    }

    /// Record/replace the latest assessment for a torrent.
    pub(crate) fn record_assessment(&self, torrent_idx: usize, summary: String, concern: String) {
        self.state.lock().assessments.insert(
            torrent_idx,
            AssessmentRecord {
                torrent_idx,
                summary,
                concern,
            },
        );
    }

    /// Latest per-torrent assessments.
    pub fn assessments(&self) -> Vec<AssessmentRecord> {
        self.state.lock().assessments.values().cloned().collect()
    }

    /// Recent decisions, most recent first.
    pub fn decisions(&self) -> Vec<DecisionRecord> {
        let s = self.state.lock();
        s.decisions.iter().rev().cloned().collect()
    }

    /// Record the (keyless) config the running loop started with.
    pub(crate) fn set_effective(&self, cfg: crate::operator::persist::OperatorPersistedConfig) {
        self.state.lock().effective = Some(cfg);
    }

    /// The config the running loop is using, if any.
    pub fn effective(&self) -> Option<crate::operator::persist::OperatorPersistedConfig> {
        self.state.lock().effective.clone()
    }

    /// Pending confirmations awaiting human action.
    pub fn confirmations(&self) -> Vec<PendingConfirmationView> {
        let s = self.state.lock();
        s.pending
            .iter()
            .map(|p| PendingConfirmationView {
                id: p.id,
                kind: p.action.kind_str().to_string(),
                torrent_idx: p.action.target_idx(),
                rationale: p.rationale.clone(),
            })
            .collect()
    }
}
