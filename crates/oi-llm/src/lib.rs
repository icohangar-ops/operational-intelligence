use async_trait::async_trait;
use resilient_call::{retry, with_timeout, RetryPolicy};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

/// Per-request deadline applied to each LLM HTTP attempt.
const LLM_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub web_search_requests: u64,
}

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub system: String,
    pub messages: Vec<LlmMessage>,
    pub temperature: f32,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LlmCompletion {
    pub content: String,
    pub usage: TokenUsage,
    pub model: String,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: LlmRequest) -> Result<LlmCompletion, LlmError>;
    fn model_name(&self) -> &str {
        "mock"
    }
}

fn approx_tokens(text: &str) -> u64 {
    (text.len().max(1) / 4) as u64
}

fn estimate_usage(system: &str, messages: &[LlmMessage], output: &str, model: &str) -> LlmCompletion {
    let input_chars: usize = system.len()
        + messages
            .iter()
            .map(|m| m.content.len())
            .sum::<usize>();
    let input_tokens = approx_tokens(&"x".repeat(input_chars));
    let output_tokens = approx_tokens(output);
    LlmCompletion {
        content: output.to_string(),
        usage: TokenUsage {
            input_tokens,
            output_tokens,
            ..Default::default()
        },
        model: model.to_string(),
    }
}

pub struct MockLlm;

#[async_trait]
impl LlmProvider for MockLlm {
    async fn complete(&self, request: LlmRequest) -> Result<LlmCompletion, LlmError> {
        let user = request
            .messages
            .last()
            .map(|m| m.content.as_str())
            .unwrap_or("");
        let content = if user.contains("outline") {
            "## Outline\n1. Executive summary\n2. Key trends\n3. Strategic implications\n4. Recommendations"
                .to_string()
        } else if user.contains("article") || user.contains("write") {
            format!(
                "# Technical Analysis\n\nSynthesized insights for: {}\n\n\
                 Evidence-backed conclusions with operational intelligence mapping.",
                user.chars().take(80).collect::<String>()
            )
        } else if user.contains("transcript") || user.contains("interview") {
            "Assessment: Candidate demonstrates strong systems thinking. \
             Evidence: cited distributed systems tradeoffs at lines 12-18. \
             Risk: limited depth on production incident response."
                .to_string()
        } else {
            format!(
                "Analysis complete. Topic context: {}",
                user.chars().take(120).collect::<String>()
            )
        };
        Ok(estimate_usage(
            &request.system,
            &request.messages,
            &content,
            "mock-llm",
        ))
    }

    fn model_name(&self) -> &str {
        "mock-llm"
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
        let client = reqwest::ClientBuilder::new()
            .timeout(LLM_REQUEST_TIMEOUT)
            .build()
            .expect("reqwest client with timeout should build");
        Self {
            client,
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
    async fn complete(&self, request: LlmRequest) -> Result<LlmCompletion, LlmError> {
        #[derive(Serialize)]
        struct ChatRequest<'a> {
            model: &'a str,
            messages: Vec<LlmMessage>,
            temperature: f32,
        }
        #[derive(Deserialize)]
        struct ChatResponse {
            model: Option<String>,
            choices: Vec<Choice>,
            usage: Option<OpenAiUsage>,
        }
        #[derive(Deserialize)]
        struct Choice {
            message: LlmMessage,
        }
        #[derive(Deserialize)]
        struct OpenAiUsage {
            prompt_tokens: Option<u64>,
            completion_tokens: Option<u64>,
            #[serde(default)]
            prompt_tokens_details: Option<PromptDetails>,
        }
        #[derive(Deserialize, Default)]
        struct PromptDetails {
            cached_tokens: Option<u64>,
        }

        let mut messages = vec![LlmMessage {
            role: "system".into(),
            content: request.system,
        }];
        messages.extend(request.messages);

        let url = format!("{}/chat/completions", self.base_url);
        let temperature = request.temperature;

        // Resilient send: cap each attempt with a 30s timeout, then retry with
        // exponential backoff + full jitter, max 3 attempts. Only transient
        // failures (network/timeout, or non-success HTTP) are retried; a
        // success status short-circuits the loop.
        let policy = RetryPolicy::with_max_attempts(3);
        let resp = retry(
            || async {
                let resp = with_timeout(
                    self.client
                        .post(&url)
                        .bearer_auth(&self.api_key)
                        .json(&ChatRequest {
                            model: &self.model,
                            messages: messages.clone(),
                            temperature,
                        })
                        .send(),
                    LLM_REQUEST_TIMEOUT,
                )
                .await
                .map_err(|e| match e.into_source() {
                    Some(src) => LlmError::Http(src),
                    None => LlmError::Api(format!(
                        "request timed out after {:?}",
                        LLM_REQUEST_TIMEOUT
                    )),
                })?;

                if !resp.status().is_success() {
                    return Err(LlmError::Api(resp.text().await.unwrap_or_default()));
                }
                Ok(resp)
            },
            &policy,
            // Retry transient HTTP errors and upstream API failures; the
            // request is idempotent (no money/state mutation) so re-sending
            // is safe.
            |e: &LlmError| matches!(e, LlmError::Http(_) | LlmError::Api(_)),
        )
        .await
        .map_err(|e| match e.into_source() {
            Some(src) => src,
            None => LlmError::Api("LLM request timed out".into()),
        })?;

        let body: ChatResponse = resp.json().await?;
        let content = body
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| LlmError::Api("empty response".into()))?;

        let usage = body.usage.unwrap_or(OpenAiUsage {
            prompt_tokens: Some(0),
            completion_tokens: Some(0),
            prompt_tokens_details: None,
        });
        let cached = usage
            .prompt_tokens_details
            .unwrap_or_default()
            .cached_tokens
            .unwrap_or(0);
        let prompt = usage.prompt_tokens.unwrap_or(0);

        Ok(LlmCompletion {
            content,
            usage: TokenUsage {
                input_tokens: prompt.saturating_sub(cached),
                output_tokens: usage.completion_tokens.unwrap_or(0),
                cache_read_input_tokens: cached,
                ..Default::default()
            },
            model: body.model.unwrap_or_else(|| self.model.clone()),
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
