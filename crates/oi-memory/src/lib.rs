use oi_core::{TraceEvent, WorkflowId, WorkflowRecord};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("workflow not found: {0}")]
    NotFound(Uuid),
    #[error("lock poisoned")]
    LockPoisoned,
}

pub struct WorkflowStore {
    workflows: RwLock<HashMap<WorkflowId, WorkflowRecord>>,
    traces: RwLock<HashMap<WorkflowId, Vec<TraceEvent>>>,
    persist_path: Option<PathBuf>,
}

impl WorkflowStore {
    pub fn in_memory() -> Self {
        Self {
            workflows: RwLock::new(HashMap::new()),
            traces: RwLock::new(HashMap::new()),
            persist_path: None,
        }
    }

    pub fn with_persistence(path: impl Into<PathBuf>) -> Result<Self, MemoryError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let store = Self {
            workflows: RwLock::new(HashMap::new()),
            traces: RwLock::new(HashMap::new()),
            persist_path: Some(path.clone()),
        };
        if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            if !data.trim().is_empty() {
                let snapshot: StoreSnapshot = serde_json::from_str(&data)?;
                *store.workflows.write().map_err(|_| MemoryError::LockPoisoned)? =
                    snapshot.workflows;
                *store.traces.write().map_err(|_| MemoryError::LockPoisoned)? = snapshot.traces;
            }
        }
        Ok(store)
    }

    pub fn save(&self, workflow: WorkflowRecord) -> Result<(), MemoryError> {
        self.workflows
            .write()
            .map_err(|_| MemoryError::LockPoisoned)?
            .insert(workflow.id, workflow);
        self.flush()
    }

    pub fn get(&self, id: WorkflowId) -> Result<WorkflowRecord, MemoryError> {
        self.workflows
            .read()
            .map_err(|_| MemoryError::LockPoisoned)?
            .get(&id)
            .cloned()
            .ok_or(MemoryError::NotFound(id))
    }

    pub fn list(&self) -> Result<Vec<WorkflowRecord>, MemoryError> {
        Ok(self
            .workflows
            .read()
            .map_err(|_| MemoryError::LockPoisoned)?
            .values()
            .cloned()
            .collect())
    }

    pub fn append_trace(&self, event: TraceEvent) -> Result<(), MemoryError> {
        self.traces
            .write()
            .map_err(|_| MemoryError::LockPoisoned)?
            .entry(event.workflow_id)
            .or_default()
            .push(event);
        self.flush()
    }

    pub fn traces(&self, id: WorkflowId) -> Result<Vec<TraceEvent>, MemoryError> {
        Ok(self
            .traces
            .read()
            .map_err(|_| MemoryError::LockPoisoned)?
            .get(&id)
            .cloned()
            .unwrap_or_default())
    }

    fn flush(&self) -> Result<(), MemoryError> {
        let Some(path) = &self.persist_path else {
            return Ok(());
        };
        let snapshot = StoreSnapshot {
            workflows: self
                .workflows
                .read()
                .map_err(|_| MemoryError::LockPoisoned)?
                .clone(),
            traces: self
                .traces
                .read()
                .map_err(|_| MemoryError::LockPoisoned)?
                .clone(),
        };
        std::fs::write(path, serde_json::to_string_pretty(&snapshot)?)?;
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoreSnapshot {
    workflows: HashMap<WorkflowId, WorkflowRecord>,
    traces: HashMap<WorkflowId, Vec<TraceEvent>>,
}

pub type SharedStore = Arc<WorkflowStore>;

pub fn default_store_dir(root: &Path) -> PathBuf {
    root.join(".oi").join("workflows.json")
}
