//! An [`OperatorModel`] backed by any OpenAI-compatible `/v1/chat/completions`
//! endpoint. Provider-agnostic; base URL, model id and key are all config.

use anyhow::Context;
use serde::Deserialize;

use crate::operator::config::ModelConfig;
use crate::operator::model::{DecisionInput, DecisionOutput, OperatorModel};
use crate::operator::prompt::{SYSTEM_PROMPT, build_user_message};

pub struct OpenAiCompatModel {
    client: reqwest::Client,
    config: ModelConfig,
}

impl OpenAiCompatModel {
    pub fn new(client: reqwest::Client, config: ModelConfig) -> Self {
        Self { client, config }
    }
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

#[async_trait::async_trait]
impl OperatorModel for OpenAiCompatModel {
    async fn decide(&self, input: &DecisionInput) -> anyhow::Result<DecisionOutput> {
        let user = build_user_message(&input.snapshot)?;
        let url = format!(
            "{}/v1/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let body = serde_json::json!({
            "model": self.config.model,
            "temperature": 0,
            "response_format": { "type": "json_object" },
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": user },
            ],
        });

        let mut req = self
            .client
            .post(&url)
            // The shared client has no default timeout; always set one.
            .timeout(self.config.request_timeout)
            .json(&body);
        if let Some(key) = self.config.api_key.as_deref().filter(|k| !k.is_empty()) {
            req = req.bearer_auth(key);
        }

        let resp = req
            .send()
            .await
            .context("error sending request to model endpoint")?;
        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("model endpoint returned HTTP {status}");
        }
        let parsed: ChatCompletionResponse = resp
            .json()
            .await
            .context("error decoding model response envelope")?;
        let content = parsed
            .choices
            .first()
            .map(|c| c.message.content.as_str())
            .unwrap_or("");

        // Visible at RUST_LOG=...operator=debug so you can see exactly what the
        // model returned each tick (including empty/"no action" responses).
        tracing::debug!(raw_response = %content, "operator: model response");
        Ok(parse_model_content(content))
    }
}

/// Parse the model's message content into decisions. Fail-closed: any parse
/// error yields zero decisions (plus a warning) so we never act on garbage.
/// Tolerates a Markdown ```json fence some models wrap JSON in.
pub(crate) fn parse_model_content(content: &str) -> DecisionOutput {
    let trimmed = strip_code_fence(content.trim());
    match serde_json::from_str::<DecisionOutput>(trimmed) {
        Ok(out) => out,
        Err(e) => {
            tracing::warn!("operator: model returned unparseable decisions, ignoring: {e:#}");
            DecisionOutput::default()
        }
    }
}

fn strip_code_fence(s: &str) -> &str {
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    s.strip_suffix("```").unwrap_or(s).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_decisions() {
        let out = parse_model_content(
            r#"{"decisions":[{"torrent_idx":0,"action":{"kind":"pause"},"rationale":"stalled","confidence":0.8}]}"#,
        );
        assert_eq!(out.decisions.len(), 1);
        assert_eq!(out.decisions[0].action.kind, "pause");
    }

    #[test]
    fn tolerates_markdown_fence() {
        let out = parse_model_content("```json\n{\"decisions\":[]}\n```");
        assert!(out.decisions.is_empty());
    }

    #[test]
    fn fails_closed_on_garbage() {
        // Non-JSON / hostile content must never panic and must yield no actions.
        assert!(parse_model_content("lol not json").decisions.is_empty());
        assert!(parse_model_content("").decisions.is_empty());
        assert!(
            parse_model_content(r#"{"decisions": "not an array"}"#)
                .decisions
                .is_empty()
        );
    }
}
