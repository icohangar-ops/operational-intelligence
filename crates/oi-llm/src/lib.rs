use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("api error: {0}")]
    Api(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub system: String,
    pub messages: Vec<LlmMessage>,
    pub temperature: f32,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: LlmRequest) -> Result<String, LlmError>;
}

pub struct MockLlm;

#[async_trait]
impl LlmProvider for MockLlm {
    async fn complete(&self, request: LlmRequest) -> Result<String, LlmError> {
        let user = request
            .messages
            .last()
            .map(|m| m.content.as_str())
            .unwrap_or("");
        if user.contains("outline") {
            return Ok(
                "## Outline\n1. Executive summary\n2. Key trends\n3. Strategic implications\n4. Recommendations"
                    .into(),
            );
        }
        if user.contains("article") || user.contains("write") {
            return Ok(format!(
                "# Technical Analysis\n\nSynthesized insights for: {}\n\n\
                 Evidence-backed conclusions with operational intelligence mapping.",
                user.chars().take(80).collect::<String>()
            ));
        }
        if user.contains("transcript") || user.contains("interview") {
            return Ok(
                "Assessment: Candidate demonstrates strong systems thinking. \
                 Evidence: cited distributed systems tradeoffs at lines 12-18. \
                 Risk: limited depth on production incident response."
                    .into(),
            );
        }
        Ok(format!(
            "Analysis complete. Topic context: {}",
            user.chars().take(120).collect::<String>()
        ))
    }
}

pub struct HttpLlm {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl HttpLlm {
    pub fn openai_compatible(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: "https://api.openai.com/v1".into(),
            model: model.into(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

#[async_trait]
impl LlmProvider for HttpLlm {
    async fn complete(&self, request: LlmRequest) -> Result<String, LlmError> {
        #[derive(Serialize)]
        struct ChatRequest<'a> {
            model: &'a str,
            messages: Vec<LlmMessage>,
            temperature: f32,
        }
        #[derive(Deserialize)]
        struct ChatResponse {
            choices: Vec<Choice>,
        }
        #[derive(Deserialize)]
        struct Choice {
            message: LlmMessage,
        }

        let mut messages = vec![LlmMessage {
            role: "system".into(),
            content: request.system,
        }];
        messages.extend(request.messages);

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&ChatRequest {
                model: &self.model,
                messages,
                temperature: request.temperature,
            })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(LlmError::Api(resp.text().await.unwrap_or_default()));
        }

        let body: ChatResponse = resp.json().await?;
        body.choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| LlmError::Api("empty response".into()))
    }
}
