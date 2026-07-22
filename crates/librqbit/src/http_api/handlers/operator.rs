use axum::Json;
use axum::extract::{Path, State};
use axum::response::IntoResponse;

use super::ApiState;
use crate::ApiError;
use crate::api::Result;
use crate::operator::OperatorPersistedConfig;

/// GET /operator/decisions — recent operator decisions (most recent first).
/// Empty when the operator is not enabled.
pub async fn h_operator_decisions(State(state): State<ApiState>) -> impl IntoResponse {
    let decisions = state
        .api
        .session()
        .operator_handle()
        .map(|h| h.decisions())
        .unwrap_or_default();
    axum::Json(serde_json::json!({ "decisions": decisions }))
}

/// GET /operator/confirmations — destructive actions awaiting human approval.
pub async fn h_operator_confirmations(State(state): State<ApiState>) -> impl IntoResponse {
    let confirmations = state
        .api
        .session()
        .operator_handle()
        .map(|h| h.confirmations())
        .unwrap_or_default();
    axum::Json(serde_json::json!({ "confirmations": confirmations }))
}

/// POST /operator/confirmations/{id}/{approve|reject}. Approving executes the
/// queued action.
pub async fn h_operator_confirm(
    State(state): State<ApiState>,
    Path((id, decision)): Path<(u64, String)>,
) -> Result<impl IntoResponse> {
    let session = state.api.session();
    let handle = session
        .operator_handle()
        .ok_or_else(|| ApiError::from(anyhow::anyhow!("operator is not enabled")))?;
    let approve = match decision.as_str() {
        "approve" => true,
        "reject" => false,
        other => {
            return Err(ApiError::from(anyhow::anyhow!(
                "decision must be 'approve' or 'reject', got {other:?}"
            )));
        }
    };
    let status = crate::operator::confirm(session, handle, id, approve).await?;
    Ok(axum::Json(serde_json::json!({ "status": status })))
}

/// GET /operator/config — the editable operator config (never the API key),
/// plus whether a loop is currently running.
pub async fn h_operator_config(State(state): State<ApiState>) -> impl IntoResponse {
    let running = state
        .api
        .session()
        .operator_handle()
        .and_then(|h| h.effective());
    // The persisted file is the source of truth for what will apply next start;
    // fall back to the running config, then defaults.
    let config = crate::operator::load_persisted_config()
        .or_else(|| running.clone())
        .unwrap_or_default();
    axum::Json(serde_json::json!({ "config": config, "running": running.is_some() }))
}

/// POST /operator/config — persist the operator config. Takes effect on the
/// next restart (the running loop is not reconfigured live). The API key is
/// never accepted here; it stays in RQBIT_OPERATOR_API_KEY.
pub async fn h_operator_config_set(
    State(_state): State<ApiState>,
    Json(config): Json<OperatorPersistedConfig>,
) -> Result<impl IntoResponse> {
    crate::operator::save_persisted_config(&config).map_err(ApiError::from)?;
    Ok(axum::Json(serde_json::json!({
        "status": "saved",
        "note": "restart rqbit to apply"
    })))
}
