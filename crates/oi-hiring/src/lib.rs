use oi_core::{
    ApprovalGate, ApprovalStatus, Artifact, ArtifactFormat, AuditTrace, Evidence,
    SourceAuthority, TracePhase, WorkflowRecord, WorkflowStatus,
};
use oi_eval::Evaluator;
use oi_llm::{LlmMessage, LlmProvider, LlmRequest};
use oi_memory::SharedStore;
use oi_tools::ToolRegistry;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptLine {
    pub line: u32,
    pub speaker: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptAssessment {
    pub workflow_id: uuid::Uuid,
    pub candidate_summary: String,
    pub strengths: Vec<EvidenceBackedFinding>,
    pub risks: Vec<EvidenceBackedFinding>,
    pub patterns: Vec<String>,
    pub eval_report: oi_eval::EvalReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceBackedFinding {
    pub statement: String,
    pub evidence: Evidence,
    pub confidence: f32,
}

#[derive(Debug, Error)]
pub enum HiringError {
    #[error("llm error: {0}")]
    Llm(#[from] oi_llm::LlmError),
    #[error("memory error: {0}")]
    Memory(#[from] oi_memory::MemoryError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct HiringAnalyzer {
    llm: Arc<dyn LlmProvider>,
    store: SharedStore,
    tools: ToolRegistry,
    audit: AuditTrace,
    eval: Evaluator,
}

impl HiringAnalyzer {
    pub fn new(llm: Arc<dyn LlmProvider>, store: SharedStore, signing_key: Option<String>) -> Self {
        Self {
            llm,
            store,
            tools: ToolRegistry::default_crew(),
            audit: AuditTrace::new(signing_key),
            eval: Evaluator::default(),
        }
    }

    pub fn parse_transcript(content: &str) -> Vec<TranscriptLine> {
        content
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(idx, line)| {
                let (speaker, text) = if let Some((s, t)) = line.split_once(':') {
                    (s.trim().to_string(), t.trim().to_string())
                } else {
                    ("unknown".into(), line.trim().to_string())
                };
                TranscriptLine {
                    line: (idx + 1) as u32,
                    speaker,
                    text,
                }
            })
            .collect()
    }

    pub async fn analyze(
        &self,
        mut workflow: WorkflowRecord,
        transcript: &[TranscriptLine],
    ) -> Result<TranscriptAssessment, HiringError> {
        workflow.status = WorkflowStatus::Running;
        self.store.save(workflow.clone())?;

        let transcript_text: String = transcript
            .iter()
            .map(|l| format!("L{} {}: {}", l.line, l.speaker, l.text))
            .collect::<Vec<_>>()
            .join("\n");

        let rubric = self
            .tools
            .invoke("knowledge_base", serde_json::json!({ "query": "hiring" }))
            .await
            .unwrap_or_default();
        let rubric_text = rubric["hits"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|h| h["content"].as_str())
            .unwrap_or("Assess evidence vs impression.");

        let assessment_raw = self
            .llm
            .complete(LlmRequest {
                system: format!(
                    "You are a hiring assessor. Read transcripts like expert reviewers. \
                     Trace every conclusion to specific line evidence. Rubric: {rubric_text}"
                ),
                messages: vec![LlmMessage {
                    role: "user".into(),
                    content: format!("Analyze interview transcript:\n{transcript_text}"),
                }],
                temperature: 0.2,
            })
            .await?;

        let (strengths, risks, cited_lines) =
            self.extract_findings(transcript, &assessment_raw);

        let patterns = self.detect_patterns(transcript);
        let eval = self.eval.evaluate_transcript_assessment(
            &assessment_raw,
            &cited_lines,
            transcript.len() as u32,
        );

        let assessment = TranscriptAssessment {
            workflow_id: workflow.id,
            candidate_summary: assessment_raw.clone(),
            strengths,
            risks,
            patterns,
            eval_report: eval.clone(),
        };

        let event = self.audit.record(
            workflow.id,
            TracePhase::Reason,
            "hiring-assessor",
            "transcript_analysis",
            &serde_json::json!({ "lines": transcript.len() }),
            &serde_json::to_value(&assessment).unwrap_or_default(),
            serde_json::json!({ "eval_passed": eval.passed }),
        );
        self.store.append_trace(event)?;

        workflow.artifacts.push(Artifact {
            title: "Hiring Assessment".into(),
            format: ArtifactFormat::Json,
            body: serde_json::to_string_pretty(&assessment)?,
            metadata: serde_json::json!({ "eval": eval }),
        });
        workflow.status = WorkflowStatus::AwaitingApproval;
        workflow.approval = Some(ApprovalGate {
            id: uuid::Uuid::new_v4(),
            label: "Hiring decision review".into(),
            status: ApprovalStatus::Pending,
            artifact_preview: assessment_raw.chars().take(400).collect(),
            reviewer_notes: None,
        });
        workflow.state.completed_agents = vec![
            "evidence-tracer".into(),
            "pattern-analyzer".into(),
            "hiring-assessor".into(),
        ];
        workflow.updated_at = chrono::Utc::now().to_rfc3339();
        self.store.save(workflow)?;

        Ok(assessment)
    }

    fn extract_findings(
        &self,
        transcript: &[TranscriptLine],
        assessment: &str,
    ) -> (Vec<EvidenceBackedFinding>, Vec<EvidenceBackedFinding>, Vec<u32>) {
        let mut cited_lines = Vec::new();
        let mut strengths = Vec::new();
        let mut risks = Vec::new();

        for line in transcript {
            let lower = line.text.to_lowercase();
            let is_technical = lower.contains("system")
                || lower.contains("architecture")
                || lower.contains("production")
                || lower.contains("distributed");
            let is_risk = lower.contains("unsure")
                || lower.contains("don't know")
                || lower.contains("maybe")
                || lower.contains("i think");

            if is_technical {
                cited_lines.push(line.line);
                strengths.push(EvidenceBackedFinding {
                    statement: format!("Technical depth demonstrated at line {}", line.line),
                    evidence: Evidence::new(
                        format!("transcript://line/{}", line.line),
                        format!("Line {}", line.line),
                        SourceAuthority::Primary,
                        line.text.clone(),
                    )
                    .with_span(line.line, line.line, &line.text),
                    confidence: 0.82,
                });
            }
            if is_risk {
                cited_lines.push(line.line);
                risks.push(EvidenceBackedFinding {
                    statement: format!("Uncertainty signal at line {}", line.line),
                    evidence: Evidence::new(
                        format!("transcript://line/{}", line.line),
                        format!("Line {}", line.line),
                        SourceAuthority::Primary,
                        line.text.clone(),
                    )
                    .with_span(line.line, line.line, &line.text),
                    confidence: 0.75,
                });
            }
        }

        if strengths.is_empty() && assessment.contains("strong") {
            strengths.push(EvidenceBackedFinding {
                statement: "Assessor noted overall strength — verify line citations".into(),
                evidence: Evidence::new(
                    "assessor://summary",
                    "Assessor summary",
                    SourceAuthority::Medium,
                    assessment.chars().take(200).collect::<String>(),
                ),
                confidence: 0.6,
            });
        }

        (strengths, risks, cited_lines)
    }

    fn detect_patterns(&self, transcript: &[TranscriptLine]) -> Vec<String> {
        let mut patterns = Vec::new();
        let hedges = transcript
            .iter()
            .filter(|l| {
                let t = l.text.to_lowercase();
                t.contains("maybe") || t.contains("i think") || t.contains("probably")
            })
            .count();
        if hedges >= 2 {
            patterns.push(format!(
                "Hedging language appears {hedges} times — impression may exceed evidence"
            ));
        }
        let deep_answers = transcript
            .iter()
            .filter(|l| l.text.split_whitespace().count() > 25)
            .count();
        if deep_answers >= 2 {
            patterns.push(format!(
                "{deep_answers} extended responses — check depth vs fluency"
            ));
        }
        patterns
    }
}
