//! MCP (Model Context Protocol) Server for voidm
//!
//! Provides tools and resources for external applications to interact with voidm's memory system via MCP.

use anyhow::{Result, anyhow};
use rmcp::{
    Json, ServerHandler, ServiceExt, tool, tool_handler, tool_router,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        AnnotateAble, ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams,
        RawResource, RawResourceTemplate, ReadResourceRequestParams, ReadResourceResult,
        ResourceContents, ServerCapabilities, ServerInfo,
    },
    schemars,
    schemars::JsonSchema,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::SqlitePool;
use voidm_core::{
    Config, crud,
    models::{AddMemoryRequest, EdgeType, LinkSpec, MemoryType},
    ontology::{self, NodeKind, OntologyRelType},
    resolve_id_sqlite,
    search::{SearchMode, SearchOptions, search},
};
use voidm_db_trait::Database;
use voidm_sqlite::SqliteDatabase;

#[derive(Clone)]
pub struct McpServerConfig {
    pub transport: String,
}

pub async fn run_server( pool: SqlitePool, config: Config) -> Result<()> {
    let server = VoidmMcpServer::new(pool, config);
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct VoidmMcpServer {
    pool: SqlitePool,
    config: Config,
    tool_router: ToolRouter<Self>,
}

impl VoidmMcpServer {
    pub fn new(pool: SqlitePool, config: Config) -> Self {
        Self {
            pool,
            config,
            tool_router: Self::tool_router(),
        }
    }

    fn server_info() -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build()
        )
    }

    fn raw_resource(uri: String, name: String, description: &str) -> RawResource {
        RawResource {
            uri,
            name,
            title: None,
            description: Some(description.to_string()),
            mime_type: Some("application/json".to_string()),
            icons: None,
            size: None,
            meta: None,
        }
    }

    fn raw_template(uri_template: &str, name: &str, description: &str) -> RawResourceTemplate {
        RawResourceTemplate {
            uri_template: uri_template.to_string(),
            name: name.to_string(),
            title: None,
            description: Some(description.to_string()),
            mime_type: Some("application/json".to_string()),
            icons: None,
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for VoidmMcpServer {
    fn get_info(&self) -> ServerInfo {
        Self::server_info()
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<ListResourcesResult, rmcp::ErrorData> {
        let memories = crud::list_memories(&self.pool, None, None, 20)
            .await
            .map_err(mcp_err)?;
        let concepts = ontology::list_concepts(&self.pool, None, 20)
            .await
            .map_err(mcp_err)?;

        let mut resources = vec![
            Self::raw_resource(
                "voidm://memories/recent".to_string(),
                "memories/recent".to_string(),
                "Recent memory records",
            )
            .no_annotation(),
            Self::raw_resource(
                "voidm://concepts".to_string(),
                "concepts".to_string(),
                "Recent concept records",
            )
            .no_annotation(),
        ];

        resources.extend(memories.into_iter().map(|m| {
            Self::raw_resource(
                format!("voidm://memory/{}", m.id),
                format!("memory/{}", m.id),
                "Voidm memory record",
            )
            .no_annotation()
        }));
        resources.extend(concepts.into_iter().map(|c| {
            Self::raw_resource(
                format!("voidm://concept/{}", c.id),
                format!("concept/{}", c.id),
                "Voidm concept record",
            )
            .no_annotation()
        }));

        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<ListResourceTemplatesResult, rmcp::ErrorData> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![
                Self::raw_template("voidm://memory/{id}", "memory", "Fetch a memory as JSON").no_annotation(),
                Self::raw_template("voidm://concept/{id}", "concept", "Fetch a concept as JSON").no_annotation(),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<ReadResourceResult, rmcp::ErrorData> {
        let uri = request.uri;
        let value = if uri == "voidm://memories/recent" {
            json!(crud::list_memories(&self.pool, None, None, 20).await.map_err(mcp_err)?)
        } else if uri == "voidm://concepts" {
            json!(ontology::list_concepts(&self.pool, None, 20).await.map_err(mcp_err)?)
        } else if let Some(id) = uri.strip_prefix("voidm://memory/") {
            let id = resolve_id_sqlite(&self.pool, id).await.map_err(mcp_err)?;
            let memory = crud::get_memory(&self.pool, &id)
                .await
                .map_err(mcp_err)?
                .ok_or_else(|| mcp_err(anyhow!("Memory not found: {id}")))?;
            json!(memory)
        } else if let Some(id) = uri.strip_prefix("voidm://concept/") {
            let concept = ontology::get_concept_with_instances(&self.pool, id).await.map_err(mcp_err)?;
            json!(concept)
        } else {
            return Err(mcp_err(anyhow!("Unknown resource URI: {uri}")));
        };

        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            serde_json::to_string_pretty(&value)
                .map_err(|e| mcp_err(anyhow!(e.to_string())))?,
            uri,
        )]))
    }
}

#[tool_router(router = tool_router)]
impl VoidmMcpServer {
    #[tool(name = "search_memories", description = "Search memories with optional scope, type, mode and quality filters")]
    async fn search_memories(
        &self,
        Parameters(params): Parameters<SearchMemoriesParams>,
    ) -> std::result::Result<Json<Map<String, Value>>, String> {
        let mode: SearchMode = params.mode.parse().map_err(|e: anyhow::Error| e.to_string())?;
        let opts = SearchOptions {
            query: params.query,
            mode,
            limit: params.limit,
            scope_filter: params.scope,
            type_filter: params.memory_type,
            min_score: params.min_score,
            min_quality: params.min_quality,
            include_neighbors: false,
            neighbor_depth: None,
            neighbor_decay: None,
            neighbor_min_score: None,
            neighbor_limit: None,
            edge_types: None,
            intent: params.intent,
        };

        let db = SqliteDatabase::new(self.pool.clone());
        let resp = search(
            &db,
            &opts,
            &self.config.embeddings.model,
            self.config.embeddings.enabled,
            self.config.search.min_score,
            &self.config.search,
        )
        .await
        .map_err(|e| e.to_string())?;

        let mut out = Map::new();
        out.insert("results".to_string(), json!(resp.results));
        out.insert("best_score".to_string(), json!(resp.best_score));
        out.insert("threshold_applied".to_string(), json!(resp.threshold_applied));
        Ok(Json(out))
    }

    #[tool(name = "remember", description = "Store a memory and return quality score and warnings")]
    async fn remember(
        &self,
        Parameters(params): Parameters<RememberParams>,
    ) -> std::result::Result<Json<Map<String, Value>>, String> {
        let memory_type: MemoryType = params.memory_type.parse().map_err(|e: anyhow::Error| e.to_string())?;
        let importance = params.importance.unwrap_or(5);
        if !(1..=10).contains(&importance) {
            return Err("importance must be between 1 and 10".to_string());
        }

        let links = params
            .links
            .unwrap_or_default()
            .into_iter()
            .map(|l| {
                let edge_type: EdgeType = l.rel.parse().map_err(|e: anyhow::Error| e.to_string())?;
                Ok(LinkSpec {
                    target_id: l.target_id,
                    edge_type,
                    note: l.note,
                })
            })
            .collect::<std::result::Result<Vec<_>, String>>()?;

        let req = AddMemoryRequest {
            id: None,
            content: params.content,
            memory_type,
            scopes: params.scope,
            tags: params.tags.unwrap_or_default(),
            importance,
            metadata: serde_json::Value::Object(Default::default()),
            links,
        };

        let resp = crud::add_memory(&self.pool, req, &self.config)
            .await
            .map_err(|e| e.to_string())?;

        let warnings = memory_write_warnings(&resp);
        let mut out = Map::new();
        out.insert("memory".to_string(), json!(resp));
        out.insert("warnings".to_string(), json!(warnings));
        Ok(Json(out))
    }

    #[tool(name = "delete_memory", description = "Delete a memory by id or short prefix")]
    async fn delete_memory(
        &self,
        Parameters(params): Parameters<DeleteMemoryParams>,
    ) -> std::result::Result<Json<Map<String, Value>>, String> {
        let id = resolve_id_sqlite(&self.pool, &params.memory_id)
            .await
            .map_err(|e| e.to_string())?;
        let deleted = crud::delete_memory(&self.pool, &id)
            .await
            .map_err(|e| e.to_string())?;
        let mut out = Map::new();
        out.insert("deleted".to_string(), json!(deleted));
        out.insert("id".to_string(), json!(id));
        Ok(Json(out))
    }

    #[tool(name = "link_memories", description = "Create a relation between two memories")]
    async fn link_memories(
        &self,
        Parameters(params): Parameters<LinkMemoriesParams>,
    ) -> std::result::Result<Json<Map<String, Value>>, String> {
        let rel: EdgeType = params.rel.parse().map_err(|e: anyhow::Error| e.to_string())?;
        let from = resolve_id_sqlite(&self.pool, &params.from).await.map_err(|e| e.to_string())?;
        let to = resolve_id_sqlite(&self.pool, &params.to).await.map_err(|e| e.to_string())?;
        let resp = crud::link_memories(&self.pool, &from, &rel, &to, params.note.as_deref())
            .await
            .map_err(|e| e.to_string())?;
        let value = serde_json::to_value(resp).map_err(|e| e.to_string())?;
        match value {
            Value::Object(map) => Ok(Json(map)),
            _ => Err("internal error: non-object link response".to_string()),
        }
    }

    #[tool(name = "search_concepts", description = "Search concepts by name and description")]
    async fn search_concepts(
        &self,
        Parameters(params): Parameters<SearchConceptsParams>,
    ) -> std::result::Result<Json<Map<String, Value>>, String> {
        let results = ontology::search_concepts(
            &self.pool,
            &params.query,
            params.scope.as_deref(),
            params.limit,
        )
        .await
        .map_err(|e| e.to_string())?;
        let mut out = Map::new();
        out.insert("results".to_string(), json!(results));
        Ok(Json(out))
    }

    #[tool(name = "list_concepts", description = "List concepts with optional scope filter")]
    async fn list_concepts(
        &self,
        Parameters(params): Parameters<ListConceptsParams>,
    ) -> std::result::Result<Json<Map<String, Value>>, String> {
        let results = ontology::list_concepts(&self.pool, params.scope.as_deref(), params.limit)
            .await
            .map_err(|e| e.to_string())?;
        let mut out = Map::new();
        out.insert("results".to_string(), json!(results));
        Ok(Json(out))
    }

    #[tool(name = "get_concept", description = "Get a concept with instances, subclasses and superclasses")]
    async fn get_concept(
        &self,
        Parameters(params): Parameters<GetConceptParams>,
    ) -> std::result::Result<Json<Map<String, Value>>, String> {
        let concept = ontology::get_concept_with_instances(&self.pool, &params.id)
            .await
            .map_err(|e| e.to_string())?;
        let value = serde_json::to_value(concept).map_err(|e| e.to_string())?;
        match value {
            Value::Object(map) => Ok(Json(map)),
            _ => Err("internal error: non-object concept response".to_string()),
        }
    }

    #[tool(name = "create_concept", description = "Create a concept")]
    async fn create_concept(
        &self,
        Parameters(params): Parameters<CreateConceptParams>,
    ) -> std::result::Result<Json<Map<String, Value>>, String> {
        let concept = ontology::add_concept(
            &self.pool,
            &params.name,
            params.description.as_deref(),
            params.scope.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;
        let value = serde_json::to_value(concept).map_err(|e| e.to_string())?;
        match value {
            Value::Object(map) => Ok(Json(map)),
            _ => Err("internal error: non-object concept response".to_string()),
        }
    }

    #[tool(name = "link_memory_to_concept", description = "Link a memory to a concept with INSTANCE_OF")]
    async fn link_memory_to_concept(
        &self,
        Parameters(params): Parameters<LinkMemoryToConceptParams>,
    ) -> std::result::Result<Json<Map<String, Value>>, String> {
        let from_id = resolve_node_id(&self.pool, &params.memory_id, NodeKind::Memory)
            .await
            .map_err(|e| e.to_string())?;
        let to_id = resolve_node_id(&self.pool, &params.concept_id, NodeKind::Concept)
            .await
            .map_err(|e| e.to_string())?;
        let edge = ontology::add_ontology_edge(
            &self.pool,
            &from_id,
            NodeKind::Memory,
            &OntologyRelType::InstanceOf,
            &to_id,
            NodeKind::Concept,
            params.note.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;
        let value = serde_json::to_value(edge).map_err(|e| e.to_string())?;
        match value {
            Value::Object(map) => Ok(Json(map)),
            _ => Err("internal error: non-object edge response".to_string()),
        }
    }

    #[tool(name = "link_concepts", description = "Link two concepts with IS_A or another ontology relation")]
    async fn link_concepts(
        &self,
        Parameters(params): Parameters<LinkConceptsParams>,
    ) -> std::result::Result<Json<Map<String, Value>>, String> {
        let rel: OntologyRelType = params.rel.parse().map_err(|e: anyhow::Error| e.to_string())?;
        let from_id = resolve_node_id(&self.pool, &params.from, NodeKind::Concept)
            .await
            .map_err(|e| e.to_string())?;
        let to_id = resolve_node_id(&self.pool, &params.to, NodeKind::Concept)
            .await
            .map_err(|e| e.to_string())?;
        let edge = ontology::add_ontology_edge(
            &self.pool,
            &from_id,
            NodeKind::Concept,
            &rel,
            &to_id,
            NodeKind::Concept,
            params.note.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;
        let value = serde_json::to_value(edge).map_err(|e| e.to_string())?;
        match value {
            Value::Object(map) => Ok(Json(map)),
            _ => Err("internal error: non-object edge response".to_string()),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchMemoriesParams {
    query: String,
    #[serde(default = "default_search_mode")]
    mode: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default, rename = "type")]
    memory_type: Option<String>,
    #[serde(default)]
    min_score: Option<f32>,
    #[serde(default)]
    min_quality: Option<f32>,
    #[serde(default)]
    intent: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RememberParams {
    content: String,
    #[serde(rename = "type")]
    memory_type: String,
    #[serde(default)]
    importance: Option<i64>,
    #[serde(default)]
    scope: Vec<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    links: Option<Vec<LinkSpecInput>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct LinkSpecInput {
    target_id: String,
    rel: String,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DeleteMemoryParams {
    memory_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct LinkMemoriesParams {
    from: String,
    rel: String,
    to: String,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchConceptsParams {
    query: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListConceptsParams {
    #[serde(default)]
    scope: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GetConceptParams {
    id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateConceptParams {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct LinkMemoryToConceptParams {
    memory_id: String,
    concept_id: String,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct LinkConceptsParams {
    from: String,
    rel: String,
    to: String,
    #[serde(default)]
    note: Option<String>,
}

fn default_limit() -> usize { 10 }
fn default_search_mode() -> String { "hybrid".to_string() }

fn memory_write_warnings(resp: &voidm_core::models::AddMemoryResponse) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Some(score) = resp.quality_score {
        if score < 0.5 {
            warnings.push(format!("Low memory quality score ({score:.2}). Consider rewriting as a more general, timeless principle."));
        } else if score < 0.7 {
            warnings.push(format!("Moderate memory quality score ({score:.2}). Consider improving abstraction or removing task-specific language."));
        }
    }
    if let Some(dup) = &resp.duplicate_warning {
        warnings.push(format!("Possible duplicate of {} (score {:.2}): {}", dup.id, dup.score, dup.message));
    }
    warnings
}

async fn resolve_node_id(pool: &SqlitePool, id: &str, kind: NodeKind) -> Result<String> {
    match kind {
        NodeKind::Concept => ontology::resolve_concept_id(pool, id).await,
        NodeKind::Memory => resolve_id_sqlite(pool, id).await,
    }
}

fn mcp_err(err: anyhow::Error) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(err.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_output_schemas_are_objects_for_mcporter() {
        let tools = VoidmMcpServer::tool_router().list_all();
        assert!(!tools.is_empty(), "expected MCP tools to be registered");

        for tool in tools {
            let schema = tool.output_schema.as_ref().expect("tool should expose output schema");
            assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"), "tool {} should expose an object output schema", tool.name);
        }
    }

    #[test]
    fn remember_tool_input_schema_exposes_required_fields() {
        let tool = VoidmMcpServer::remember_tool_attr();
        let required = tool
            .input_schema
            .get("required")
            .and_then(|v| v.as_array())
            .expect("remember input schema should have required fields");

        let names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"content"));
        assert!(names.contains(&"type"));
    }

    #[test]
    fn resources_and_templates_use_voidm_uris() {
        let memory = VoidmMcpServer::raw_resource(
            "voidm://memory/abc".to_string(),
            "memory/abc".to_string(),
            "memory",
        );
        let template = VoidmMcpServer::raw_template("voidm://memory/{id}", "memory", "memory template");

        assert_eq!(memory.uri, "voidm://memory/abc");
        assert_eq!(template.uri_template, "voidm://memory/{id}");
        assert_eq!(memory.mime_type.as_deref(), Some("application/json"));
        assert_eq!(template.mime_type.as_deref(), Some("application/json"));
    }

    #[test]
    fn low_quality_scores_produce_warning() {
        let response = voidm_core::models::AddMemoryResponse {
            id: "id".to_string(),
            memory_type: "semantic".to_string(),
            content: "today I fixed task".to_string(),
            scopes: vec![],
            tags: vec![],
            importance: 5,
            created_at: "now".to_string(),
            quality_score: Some(0.4),
            suggested_links: vec![],
            duplicate_warning: None,
        };

        let warnings = memory_write_warnings(&response);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("Low memory quality score"));
    }
}
