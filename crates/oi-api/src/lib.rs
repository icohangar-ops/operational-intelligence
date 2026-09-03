use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use oi_connector::{DataConnector, MockAnalyticsConnector, OperationalQuery};
use oi_core::{
    ApprovalDecision, ApprovalStatus, WorkflowKind, WorkflowRecord, WorkflowStatus,
};
use oi_crew::{ContentCrewConfig, ContentCrewOrchestrator};
use oi_hiring::HiringAnalyzer;
use oi_memory::SharedStore;
use oi_usage::build_metered_llm;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub store: SharedStore,
    pub signing_key: Option<String>,
    pub require_approval: bool,
    pub connector: Arc<dyn DataConnector>,
}

#[derive(Debug, Deserialize)]
pub struct StartCrewRequest {
    pub topic: String,
}

#[derive(Debug, Deserialize)]
pub struct StartHiringRequest {
    pub transcript: String,
}

#[derive(Debug, Deserialize)]
pub struct OiQueryRequest {
    pub question: String,
    pub initiative_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UiPathIntakeRequest {
    pub kind: Option<String>,
    pub topic: Option<String>,
    pub transcript: Option<String>,
    pub question: Option<String>,
    pub initiative_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowResponse {
    pub workflow: WorkflowRecord,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/workflows", get(list_workflows))
        .route("/workflows/crew", post(start_crew))
        .route("/workflows/hiring", post(start_hiring))
        .route("/workflows/{id}", get(get_workflow))
        .route("/workflows/{id}/approve", post(approve_workflow))
        .route("/workflows/{id}/traces", get(get_traces))
        .route("/oi/query", post(oi_query))
        .route("/oi/initiatives", get(list_initiatives))
        .route("/uipath/intake", post(uipath_intake))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "operational-intelligence"
    }))
}

async fn list_workflows(State(state): State<AppState>) -> Result<Json<Vec<WorkflowRecord>>, StatusCode> {
    state
        .store
        .list()
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_workflow(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkflowResponse>, StatusCode> {
    let workflow = state.store.get(id).map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(WorkflowResponse { workflow }))
}

async fn get_traces(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let traces = state.store.traces(id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "workflow_id": id, "traces": traces })))
}

async fn start_crew(
    State(state): State<AppState>,
    Json(req): Json<StartCrewRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workflow = WorkflowRecord::new(WorkflowKind::ContentCrew, req.topic);
    let llm = build_metered_llm(workflow.id.to_string());
    let orchestrator = ContentCrewOrchestrator::new(
        llm,
        state.store.clone(),
        ContentCrewConfig {
            require_approval: state.require_approval,
            signing_key: state.signing_key.clone(),
        },
    );
    let output = orchestrator
        .run(workflow)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(output).unwrap_or_default()))
}

async fn start_hiring(
    State(state): State<AppState>,
    Json(req): Json<StartHiringRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workflow = WorkflowRecord::new(WorkflowKind::HiringAnalysis, "interview-transcript");
    let lines = HiringAnalyzer::parse_transcript(&req.transcript);
    let llm = build_metered_llm(workflow.id.to_string());
    let analyzer = HiringAnalyzer::new(llm, state.store.clone(), state.signing_key.clone());
    let assessment = analyzer
        .analyze(workflow, &lines)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(assessment).unwrap_or_default()))
}

async fn approve_workflow(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(decision): Json<ApprovalDecision>,
) -> Result<Json<WorkflowResponse>, StatusCode> {
    let mut workflow = state.store.get(id).map_err(|_| StatusCode::NOT_FOUND)?;
    if let Some(ref mut gate) = workflow.approval {
        gate.status = if decision.approved {
            ApprovalStatus::Approved
        } else {
            ApprovalStatus::Rejected
        };
        gate.reviewer_notes = decision.notes;
    }
    workflow.status = if decision.approved {
        WorkflowStatus::Completed
    } else {
        WorkflowStatus::Failed
    };
    workflow.updated_at = chrono::Utc::now().to_rfc3339();
    state
        .store
        .save(workflow.clone())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(WorkflowResponse { workflow }))
}

async fn oi_query(
    State(state): State<AppState>,
    Json(req): Json<OiQueryRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let insight = state
        .connector
        .query(OperationalQuery {
            question: req.question,
            initiative_id: req.initiative_id,
    })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(insight).unwrap_or_default()))
}

async fn uipath_intake(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<UiPathIntakeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(secret) = std::env::var("OI_UIPATH_WEBHOOK_SECRET").ok().filter(|s| !s.is_empty()) {
        let signature = headers
            .get("x-uipath-signature")
            .and_then(|v| v.to_str().ok())
            .or_else(|| headers.get("x-webhook-signature").and_then(|v| v.to_str().ok()));
        if signature != Some(secret.as_str()) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    let kind = req.kind.as_deref().unwrap_or("crew");

    match kind {
        "hiring" => {
            let transcript = req.transcript.ok_or(StatusCode::BAD_REQUEST)?;
            let workflow = WorkflowRecord::new(WorkflowKind::HiringAnalysis, "uipath-handoff");
            let lines = HiringAnalyzer::parse_transcript(&transcript);
            let llm = build_metered_llm(workflow.id.to_string());
            let analyzer = HiringAnalyzer::new(llm, state.store.clone(), state.signing_key.clone());
            let assessment = analyzer
                .analyze(workflow, &lines)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Json(serde_json::to_value(assessment).unwrap_or_default()))
        }
        "query" => {
            let question = req.question.ok_or(StatusCode::BAD_REQUEST)?;
            let insight = state
                .connector
                .query(OperationalQuery {
                    question,
                    initiative_id: req.initiative_id,
                })
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Json(serde_json::to_value(insight).unwrap_or_default()))
        }
        _ => {
            let topic = req.topic.unwrap_or_else(|| "uipath-handoff".to_string());
            let workflow = WorkflowRecord::new(WorkflowKind::ContentCrew, topic);
            let llm = build_metered_llm(workflow.id.to_string());
            let orchestrator = ContentCrewOrchestrator::new(
                llm,
                state.store.clone(),
                ContentCrewConfig {
                    require_approval: state.require_approval,
                    signing_key: state.signing_key.clone(),
                },
            );
            let output = orchestrator
                .run(workflow)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Json(serde_json::to_value(output).unwrap_or_default()))
        }
    }
}

async fn list_initiatives(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let initiatives = state
        .connector
        .list_initiatives()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "initiatives": initiatives })))
}

pub async fn serve(bind: &str, state: AppState) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "Operational Intelligence API listening");
    axum::serve(listener, router(state)).await
}

pub fn default_state(store: SharedStore) -> AppState {
    AppState {
        store,
        signing_key: std::env::var("OI_LEDGER_SIGNING_KEY").ok(),
        require_approval: std::env::var("OI_REQUIRE_APPROVAL")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(true),
        connector: Arc::new(MockAnalyticsConnector),
    }
}
