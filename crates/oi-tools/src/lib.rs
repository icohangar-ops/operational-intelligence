use async_trait::async_trait;
use oi_core::{Evidence, SourceAuthority};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool execution failed: {0}")]
    Failed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[async_trait]
pub trait OiTool: Send + Sync {
    fn descriptor(&self) -> McpToolDescriptor;
    async fn invoke(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError>;
}

pub struct WebSearchTool;

#[async_trait]
impl OiTool for WebSearchTool {
    fn descriptor(&self) -> McpToolDescriptor {
        McpToolDescriptor {
            name: "web_search".into(),
            description: "Search the web for high-authority sources on a topic".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "query": { "type": "string" }, "max_results": { "type": "integer" } },
                "required": ["query"]
            }),
        }
    }

    async fn invoke(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let query = input["query"].as_str().unwrap_or("operational intelligence");
        let results = vec![
            Evidence::new(
                "https://example.com/oi-trends-2026",
                "Operational Intelligence Trends 2026",
                SourceAuthority::High,
                format!("Latest trends on {query}: real-time BI, agentic analytics, evidence-traced AI."),
            ),
            Evidence::new(
                "https://example.com/enterprise-ai-roi",
                "Enterprise AI ROI Framework",
                SourceAuthority::Primary,
                "Organizations linking live data models to strategic initiatives see 23% faster decision cycles.",
            ),
        ];
        Ok(serde_json::json!({ "results": results, "query": query }))
    }
}

pub struct KnowledgeBaseTool {
    documents: Vec<(String, String)>,
}

impl KnowledgeBaseTool {
    pub fn with_defaults() -> Self {
        Self {
            documents: vec![
                (
                    "brand_voice.md".into(),
                    "Tone: precise, evidence-first, no hype. Always cite sources. Avoid speculative claims.".into(),
                ),
                (
                    "hiring_rubric.md".into(),
                    "Assess: evidence vs impression, depth vs fluency, pattern consistency across answers.".into(),
                ),
            ],
        }
    }

    pub fn from_dir(path: &std::path::Path) -> Result<Self, ToolError> {
        let mut documents = Vec::new();
        if path.exists() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                if entry.path().is_file() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let content = std::fs::read_to_string(entry.path())?;
                    documents.push((name, content));
                }
            }
        }
        if documents.is_empty() {
            return Ok(Self::with_defaults());
        }
        Ok(Self { documents })
    }
}

#[async_trait]
impl OiTool for KnowledgeBaseTool {
    fn descriptor(&self) -> McpToolDescriptor {
        McpToolDescriptor {
            name: "knowledge_base".into(),
            description: "RAG retrieval against brand voice and domain knowledge".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }),
        }
    }

    async fn invoke(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let query = input["query"].as_str().unwrap_or("").to_lowercase();
        let hits: Vec<_> = self
            .documents
            .iter()
            .filter(|(name, content)| {
                name.to_lowercase().contains(&query)
                    || content.to_lowercase().contains(&query)
                    || query.is_empty()
            })
            .map(|(name, content)| {
                Evidence::new(
                    format!("kb://{name}"),
                    name.clone(),
                    SourceAuthority::Primary,
                    content.clone(),
                )
            })
            .collect();
        Ok(serde_json::json!({ "hits": hits }))
    }
}

pub struct FileTool;

#[async_trait]
impl OiTool for FileTool {
    fn descriptor(&self) -> McpToolDescriptor {
        McpToolDescriptor {
            name: "read_file".into(),
            description: "Read a local file for agent context".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        }
    }

    async fn invoke(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let path = input["path"]
            .as_str()
            .ok_or_else(|| ToolError::Failed("missing path".into()))?;
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::json!({ "path": path, "content": content }))
    }
}

pub struct ToolRegistry {
    tools: Vec<Box<dyn OiTool>>,
}

impl ToolRegistry {
    pub fn default_crew() -> Self {
        Self {
            tools: vec![
                Box::new(WebSearchTool),
                Box::new(KnowledgeBaseTool::with_defaults()),
                Box::new(FileTool),
            ],
        }
    }

    pub fn descriptors(&self) -> Vec<McpToolDescriptor> {
        self.tools.iter().map(|t| t.descriptor()).collect()
    }

    pub async fn invoke(&self, name: &str, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        for tool in &self.tools {
            if tool.descriptor().name == name {
                return tool.invoke(input).await;
            }
        }
        Err(ToolError::Failed(format!("unknown tool: {name}")))
    }
}
