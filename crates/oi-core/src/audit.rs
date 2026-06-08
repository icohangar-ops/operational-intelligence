use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TracePhase {
    Retrieve,
    Reason,
    Generate,
    Validate,
    Approve,
    Query,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub phase: TracePhase,
    pub agent: String,
    pub action: String,
    pub input_hash: String,
    pub output_hash: String,
    pub metadata: serde_json::Value,
    pub timestamp: String,
    pub signature: Option<String>,
}

pub struct AuditTrace {
    signing_key: Option<String>,
}

impl AuditTrace {
    pub fn new(signing_key: Option<String>) -> Self {
        Self { signing_key }
    }

    pub fn record(
        &self,
        workflow_id: Uuid,
        phase: TracePhase,
        agent: &str,
        action: &str,
        input: &serde_json::Value,
        output: &serde_json::Value,
        metadata: serde_json::Value,
    ) -> TraceEvent {
        let input_hash = hash_json(input);
        let output_hash = hash_json(output);
        let timestamp = chrono::Utc::now().to_rfc3339();
        let signature = self.signing_key.as_ref().map(|key| {
            sign(&format!("{input_hash}:{output_hash}:{timestamp}"), key)
        });

        TraceEvent {
            id: Uuid::new_v4(),
            workflow_id,
            phase,
            agent: agent.to_string(),
            action: action.to_string(),
            input_hash,
            output_hash,
            metadata,
            timestamp,
            signature,
        }
    }
}

pub fn hash_json(value: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}

fn sign(payload: &str, key: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("hmac key");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}
