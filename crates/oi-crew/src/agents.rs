use oi_core::{Claim, Evidence};
use oi_llm::{LlmMessage, LlmProvider, LlmRequest};
use oi_tools::ToolRegistry;
use std::sync::Arc;

pub async fn run_research(
    topic: &str,
    tools: &ToolRegistry,
    llm: &dyn LlmProvider,
) -> Result<(Vec<Evidence>, Vec<Claim>), oi_llm::LlmError> {
    let search = tools
        .invoke(
            "web_search",
            serde_json::json!({ "query": topic, "max_results": 5 }),
        )
        .await
        .unwrap_or_default();

    let evidence: Vec<Evidence> = serde_json::from_value(search["results"].clone()).unwrap_or_default();
    let high_auth: Vec<_> = evidence
        .iter()
        .filter(|e| e.authority >= oi_core::SourceAuthority::High)
        .collect();

    let summary = llm
        .complete(LlmRequest {
            system: "You are a research agent. Summarize findings with source attribution.".into(),
            messages: vec![LlmMessage {
                role: "user".into(),
                content: format!(
                    "Research topic: {topic}. Sources: {}",
                    high_auth
                        .iter()
                        .map(|e| e.source_title.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }],
            temperature: 0.2,
        })
        .await?;

    let claim = Claim::new(
        summary,
        "research-agent",
        evidence.iter().map(|e| e.id).collect(),
    );
    Ok((evidence, vec![claim]))
}

pub async fn run_analyst(
    topic: &str,
    claims: &[Claim],
    llm: &dyn LlmProvider,
) -> Result<String, oi_llm::LlmError> {
    let insights: String = claims
        .iter()
        .map(|c| format!("- {} (confidence: {:.0}%)", c.statement, c.confidence * 100.0))
        .collect::<Vec<_>>()
        .join("\n");

    llm.complete(LlmRequest {
        system: "You are an analyst agent. Create a structured outline from research insights.".into(),
        messages: vec![LlmMessage {
            role: "user".into(),
            content: format!("Topic: {topic}\nInsights:\n{insights}\n\nProduce a structured outline."),
        }],
        temperature: 0.3,
    })
    .await
}

pub async fn run_writer(outline: &str, llm: &dyn LlmProvider) -> Result<String, oi_llm::LlmError> {
    llm.complete(LlmRequest {
        system: "You are a writer agent. Produce SEO-optimized technical content.".into(),
        messages: vec![LlmMessage {
            role: "user".into(),
            content: format!("Write article from outline:\n{outline}"),
        }],
        temperature: 0.5,
    })
    .await
}

pub async fn run_editor(
    draft: &str,
    tools: &ToolRegistry,
    llm: &dyn LlmProvider,
) -> Result<(String, Vec<Claim>), oi_llm::LlmError> {
    let kb = tools
        .invoke("knowledge_base", serde_json::json!({ "query": "brand" }))
        .await
        .unwrap_or_default();
    let brand: String = kb["hits"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|h| h["content"].as_str())
        .unwrap_or("Evidence-first tone.")
        .into();

    let edited = llm
        .complete(LlmRequest {
            system: format!(
                "You are an editor agent. Verify factual accuracy and brand voice.\nBrand rules: {brand}"
            ),
            messages: vec![LlmMessage {
                role: "user".into(),
                content: format!("Edit and finalize:\n{draft}"),
            }],
            temperature: 0.2,
        })
        .await?;

    let claim = Claim::new(
        "Editor-verified final article".to_string(),
        "editor-agent",
        vec![],
    );
    Ok((edited, vec![claim]))
}

pub type SharedLlm = Arc<dyn LlmProvider>;
