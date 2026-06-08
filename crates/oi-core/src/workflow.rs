use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type WorkflowId = Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowKind {
    ContentCrew,
    HiringAnalysis,
    OperationalQuery,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Pending,
    Running,
    AwaitingApproval,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFormat {
    Markdown,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub title: String,
    pub format: ArtifactFormat,
    pub body: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalGate {
    pub id: Uuid,
    pub label: String,
    pub status: ApprovalStatus,
    pub artifact_preview: String,
    pub reviewer_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub approved: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    pub current_agent: Option<String>,
    pub completed_agents: Vec<String>,
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRecord {
    pub id: WorkflowId,
    pub kind: WorkflowKind,
    pub status: WorkflowStatus,
    pub topic: String,
    pub state: WorkflowState,
    pub artifacts: Vec<Artifact>,
    pub approval: Option<ApprovalGate>,
    pub created_at: String,
    pub updated_at: String,
}

impl WorkflowRecord {
    pub fn new(kind: WorkflowKind, topic: impl Into<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4(),
            kind,
            status: WorkflowStatus::Pending,
            topic: topic.into(),
            state: WorkflowState {
                current_agent: None,
                completed_agents: Vec::new(),
                context: serde_json::json!({}),
            },
            artifacts: Vec::new(),
            approval: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
