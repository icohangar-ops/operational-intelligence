use crate::agents::{run_analyst, run_editor, run_research, run_writer, SharedLlm};
use oi_core::{
    ApprovalGate, ApprovalStatus, Artifact, ArtifactFormat, AuditTrace, TracePhase, WorkflowRecord,
    WorkflowStatus,
};
use oi_eval::Evaluator;
use oi_memory::SharedStore;
use oi_tools::ToolRegistry;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ContentCrewConfig {
    pub require_approval: bool,
    pub signing_key: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrewOutput {
    pub workflow_id: uuid::Uuid,
    pub outline: String,
    pub article: String,
    pub eval_report: oi_eval::EvalReport,
    pub evidence_count: usize,
}

#[derive(Debug, Error)]
pub enum CrewError {
    #[error("llm error: {0}")]
    Llm(#[from] oi_llm::LlmError),
    #[error("memory error: {0}")]
    Memory(#[from] oi_memory::MemoryError),
    #[error("eval failed: {0}")]
    EvalFailed(String),
}

pub struct ContentCrewOrchestrator {
    llm: SharedLlm,
    tools: ToolRegistry,
    store: SharedStore,
    audit: AuditTrace,
    eval: Evaluator,
    config: ContentCrewConfig,
}

impl ContentCrewOrchestrator {
    pub fn new(
        llm: SharedLlm,
        store: SharedStore,
        config: ContentCrewConfig,
    ) -> Self {
        Self {
            llm,
            tools: ToolRegistry::default_crew(),
            store,
            audit: AuditTrace::new(config.signing_key.clone()),
            eval: Evaluator::default(),
            config,
        }
    }

    pub async fn run(&self, mut workflow: WorkflowRecord) -> Result<CrewOutput, CrewError> {
        workflow.status = WorkflowStatus::Running;
        self.store.save(workflow.clone())?;

        // Research
        workflow.state.current_agent = Some("research-agent".into());
        let (evidence, mut claims) =
            run_research(&workflow.topic, &self.tools, self.llm.as_ref()).await?;
        self.trace(&workflow, TracePhase::Retrieve, "research-agent", "web_search", &evidence)?;
        workflow.state.completed_agents.push("research-agent".into());

        // Analyst
        workflow.state.current_agent = Some("analyst-agent".into());
        let outline = run_analyst(&workflow.topic, &claims, self.llm.as_ref()).await?;
        self.trace(
            &workflow,
            TracePhase::Reason,
            "analyst-agent",
            "synthesize_outline",
            &outline,
        )?;
        workflow.state.completed_agents.push("analyst-agent".into());

        // Writer
        workflow.state.current_agent = Some("writer-agent".into());
        let draft = run_writer(&outline, self.llm.as_ref()).await?;
        self.trace(
            &workflow,
            TracePhase::Generate,
            "writer-agent",
            "draft_article",
            &draft,
        )?;
        workflow.state.completed_agents.push("writer-agent".into());

        // Editor
        workflow.state.current_agent = Some("editor-agent".into());
        let (article, editor_claims) =
            run_editor(&draft, &self.tools, self.llm.as_ref()).await?;
        claims.extend(editor_claims);
        self.trace(
            &workflow,
            TracePhase::Validate,
            "editor-agent",
            "edit_and_verify",
            &article,
        )?;
        workflow.state.completed_agents.push("editor-agent".into());

        let kb = self
            .tools
            .invoke("knowledge_base", serde_json::json!({ "query": "brand" }))
            .await
            .unwrap_or_default();
        let brand = kb["hits"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|h| h["content"].as_str())
            .unwrap_or("");

        let eval = self
            .eval
            .evaluate_content(&article, &claims, &evidence, brand);

        workflow.artifacts = vec![
            Artifact {
                title: "Outline".into(),
                format: ArtifactFormat::Markdown,
                body: outline.clone(),
                metadata: serde_json::json!({ "agent": "analyst-agent" }),
            },
            Artifact {
                title: "Article".into(),
                format: ArtifactFormat::Markdown,
                body: article.clone(),
                metadata: serde_json::json!({
                    "agent": "editor-agent",
                    "eval": eval,
                }),
            },
        ];

        if self.config.require_approval {
            workflow.status = WorkflowStatus::AwaitingApproval;
            workflow.approval = Some(ApprovalGate {
                id: uuid::Uuid::new_v4(),
                label: "Human review before publication".into(),
                status: ApprovalStatus::Pending,
                artifact_preview: article.chars().take(500).collect(),
                reviewer_notes: None,
            });
        } else {
            workflow.status = WorkflowStatus::Completed;
        }

        workflow.state.current_agent = None;
        workflow.updated_at = chrono::Utc::now().to_rfc3339();
        self.store.save(workflow.clone())?;

        Ok(CrewOutput {
            workflow_id: workflow.id,
            outline,
            article,
            eval_report: eval,
            evidence_count: evidence.len(),
        })
    }

    fn trace(
        &self,
        workflow: &WorkflowRecord,
        phase: TracePhase,
        agent: &str,
        action: &str,
        output: &impl serde::Serialize,
    ) -> Result<(), CrewError> {
        let event = self.audit.record(
            workflow.id,
            phase,
            agent,
            action,
            &serde_json::json!({ "topic": workflow.topic }),
            &serde_json::to_value(output).unwrap_or_default(),
            serde_json::json!({}),
        );
        self.store.append_trace(event)?;
        Ok(())
    }
}
