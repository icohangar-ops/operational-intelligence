use oi_core::{Claim, Evidence};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub faithfulness_score: f32,
    pub hallucination_risk: f32,
    pub grounded_claims: usize,
    pub ungrounded_claims: usize,
    pub brand_voice_ok: bool,
    pub notes: Vec<String>,
    pub passed: bool,
}

pub struct Evaluator {
    pub faithfulness_threshold: f32,
    pub hallucination_threshold: f32,
}

impl Default for Evaluator {
    fn default() -> Self {
        Self {
            faithfulness_threshold: 0.7,
            hallucination_threshold: 0.3,
        }
    }
}

impl Evaluator {
    pub fn evaluate_content(
        &self,
        content: &str,
        claims: &[Claim],
        evidence: &[Evidence],
        brand_rules: &str,
    ) -> EvalReport {
        let grounded = claims.iter().filter(|c| c.is_grounded()).count();
        let ungrounded = claims.len().saturating_sub(grounded);
        let faithfulness = if claims.is_empty() {
            0.5
        } else {
            grounded as f32 / claims.len() as f32
        };

        let evidence_tokens: Vec<String> = evidence
            .iter()
            .flat_map(|e| e.content.split_whitespace().map(str::to_lowercase))
            .collect();

        let content_words: Vec<String> = content
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let supported = content_words
            .iter()
            .filter(|w| w.len() > 5 && evidence_tokens.contains(w))
            .count();
        let hallucination_risk = if content_words.is_empty() {
            1.0
        } else {
            1.0 - (supported as f32 / content_words.len() as f32).min(1.0)
        };

        let brand_voice_ok = !content.to_lowercase().contains("guaranteed")
            && !content.to_lowercase().contains("100% certain")
            && brand_rules.chars().count() > 0;

        let mut notes = Vec::new();
        if ungrounded > 0 {
            notes.push(format!("{ungrounded} claim(s) lack evidence citations"));
        }
        if hallucination_risk > self.hallucination_threshold {
            notes.push("Elevated hallucination risk — verify against knowledge base".into());
        }

        let passed = faithfulness >= self.faithfulness_threshold
            && hallucination_risk <= self.hallucination_threshold
            && brand_voice_ok;

        EvalReport {
            faithfulness_score: faithfulness,
            hallucination_risk,
            grounded_claims: grounded,
            ungrounded_claims: ungrounded,
            brand_voice_ok,
            notes,
            passed,
        }
    }

    pub fn evaluate_transcript_assessment(
        &self,
        _assessment: &str,
        cited_lines: &[u32],
        transcript_line_count: u32,
    ) -> EvalReport {
        let has_citations = !cited_lines.is_empty();
        let coverage = if transcript_line_count == 0 {
            0.0
        } else {
            cited_lines.len() as f32 / transcript_line_count as f32
        };

        let faithfulness = if has_citations {
            0.7 + (coverage * 0.3).min(0.3)
        } else {
            0.2
        };
        let hallucination_risk = if has_citations { 0.15 } else { 0.8 };

        EvalReport {
            faithfulness_score: faithfulness,
            hallucination_risk,
            grounded_claims: cited_lines.len(),
            ungrounded_claims: if has_citations { 0 } else { 1 },
            brand_voice_ok: true,
            notes: vec!["Transcript assessment requires line-level evidence".into()],
            passed: faithfulness >= self.faithfulness_threshold
                && hallucination_risk <= self.hallucination_threshold,
        }
    }
}
