use async_trait::async_trait;
use oi_core::{Initiative, RoiOutcome, StrategicMetric};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataModelContext {
    pub model_name: String,
    pub dimensions: Vec<String>,
    pub measures: Vec<String>,
    pub sample_rows: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationalQuery {
    pub question: String,
    pub initiative_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationalInsight {
    pub answer: String,
    pub metrics: Vec<StrategicMetric>,
    pub roi: Option<RoiOutcome>,
    pub data_context: DataModelContext,
    pub evidence_summary: String,
}

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("connector error: {0}")]
    Message(String),
}

#[async_trait]
pub trait DataConnector: Send + Sync {
    fn name(&self) -> &str;
    async fn list_initiatives(&self) -> Result<Vec<Initiative>, ConnectorError>;
    async fn query(&self, query: OperationalQuery) -> Result<OperationalInsight, ConnectorError>;
    async fn data_context(&self) -> Result<DataModelContext, ConnectorError>;
}

pub struct MockAnalyticsConnector;

#[async_trait]
impl DataConnector for MockAnalyticsConnector {
    fn name(&self) -> &str {
        "mock-qlik-analytics"
    }

    async fn list_initiatives(&self) -> Result<Vec<Initiative>, ConnectorError> {
        let mut initiative = Initiative::new(
            "Revenue Acceleration Q3",
            "Map pipeline velocity to strategic revenue targets",
        );
        initiative.owner = "CFO".into();
        initiative.linked_data_models = vec!["sales_pipeline".into(), "forecast_model".into()];
        initiative.metrics = vec![
            StrategicMetric {
                name: "Pipeline Coverage".into(),
                current_value: 2.4,
                target_value: 3.0,
                unit: "x".into(),
            },
            StrategicMetric {
                name: "Win Rate".into(),
                current_value: 18.5,
                target_value: 22.0,
                unit: "%".into(),
            },
        ];
        Ok(vec![initiative])
    }

    async fn data_context(&self) -> Result<DataModelContext, ConnectorError> {
        Ok(DataModelContext {
            model_name: "sales_pipeline".into(),
            dimensions: vec![
                "region".into(),
                "product_line".into(),
                "stage".into(),
                "quarter".into(),
            ],
            measures: vec![
                "pipeline_value".into(),
                "weighted_forecast".into(),
                "days_in_stage".into(),
            ],
            sample_rows: vec![
                serde_json::json!({"region":"NA","stage":"Proposal","pipeline_value":4200000}),
                serde_json::json!({"region":"EMEA","stage":"Negotiation","pipeline_value":1800000}),
            ],
        })
    }

    async fn query(&self, query: OperationalQuery) -> Result<OperationalInsight, ConnectorError> {
        let ctx = self.data_context().await?;
        let initiatives = self.list_initiatives().await?;
        let initiative = query
            .initiative_id
            .and_then(|id| initiatives.iter().find(|i| i.id == id).cloned())
            .or_else(|| initiatives.into_iter().next());

        let roi = initiative.as_ref().map(|i| RoiOutcome {
            initiative_id: i.id,
            projected_roi_pct: 14.2,
            payback_months: 8,
            confidence: 0.78,
            assumptions: vec![
                "Win rate improves 2pp".into(),
                "Pipeline coverage reaches 3.0x".into(),
            ],
            evidence_summary: "Based on associative sales_pipeline model and Q3 forecast".into(),
        });

        Ok(OperationalInsight {
            answer: format!(
                "Operational intelligence answer for '{}': NA pipeline is $4.2M in Proposal stage. \
                 Initiative '{}' projects 14.2% ROI with 8-month payback.",
                query.question,
                initiative
                    .as_ref()
                    .map(|i| i.name.as_str())
                    .unwrap_or("unlinked")
            ),
            metrics: initiative
                .map(|i| i.metrics)
                .unwrap_or_default(),
            roi,
            data_context: ctx,
            evidence_summary: "Traced to sales_pipeline associative model".into(),
        })
    }
}
