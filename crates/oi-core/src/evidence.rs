use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SourceAuthority {
    Low,
    Medium,
    High,
    Primary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceSpan {
    pub start_line: u32,
    pub end_line: u32,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: Uuid,
    pub source_uri: String,
    pub source_title: String,
    pub authority: SourceAuthority,
    pub span: Option<EvidenceSpan>,
    pub content: String,
    pub retrieved_at: String,
}

impl Evidence {
    pub fn new(
        source_uri: impl Into<String>,
        source_title: impl Into<String>,
        authority: SourceAuthority,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            source_uri: source_uri.into(),
            source_title: source_title.into(),
            authority,
            span: None,
            content: content.into(),
            retrieved_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn with_span(mut self, start: u32, end: u32, excerpt: impl Into<String>) -> Self {
        self.span = Some(EvidenceSpan {
            start_line: start,
            end_line: end,
            excerpt: excerpt.into(),
        });
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: Uuid,
    pub statement: String,
    pub confidence: f32,
    pub evidence_ids: Vec<Uuid>,
    pub agent: String,
}

impl Claim {
    pub fn new(statement: impl Into<String>, agent: impl Into<String>, evidence_ids: Vec<Uuid>) -> Self {
        Self {
            id: Uuid::new_v4(),
            statement: statement.into(),
            confidence: if evidence_ids.is_empty() { 0.3 } else { 0.85 },
            evidence_ids,
            agent: agent.into(),
        }
    }

    pub fn is_grounded(&self) -> bool {
        !self.evidence_ids.is_empty()
    }
}
