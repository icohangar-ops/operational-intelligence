use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategicMetric {
    pub name: String,
    pub current_value: f64,
    pub target_value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoiOutcome {
    pub initiative_id: Uuid,
    pub projected_roi_pct: f64,
    pub payback_months: u32,
    pub confidence: f32,
    pub assumptions: Vec<String>,
    pub evidence_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Initiative {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub owner: String,
    pub status: String,
    pub metrics: Vec<StrategicMetric>,
    pub linked_data_models: Vec<String>,
}

impl Initiative {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            owner: String::new(),
            status: "active".into(),
            metrics: Vec::new(),
            linked_data_models: Vec::new(),
        }
    }
}
