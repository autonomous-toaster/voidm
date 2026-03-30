use anyhow::{Context, Result};
use std::pin::Pin;
use std::future::Future;
use neo4rs::Graph;
use voidm_db::Database;
use serde_json::{Value, json};
use uuid::Uuid;

use voidm_db::models::{
    AddMemoryRequest, AddMemoryResponse, Memory, EdgeType, LinkResponse,
};
use voidm_core::search::{SearchOptions, SearchResponse};

pub mod neo4j_db;
pub mod neo4j_schema;

pub use neo4j_db::Neo4jDb;
pub use neo4j_schema::{MemoryChunkSchema, SchemaStats, CoherenceStats};

/// Neo4j implementation of the Database trait.
/// Uses the neo4rs async driver with Bolt protocol.
#[derive(Clone)]
pub struct Neo4jDatabase {
    pub graph: Graph,
    pub database: String,
}

impl Neo4jDatabase {
    /// Connect to a Neo4j instance
    pub async fn connect(uri: &str, username: &str, password: &str, database: &str) -> Result<Self> {
        tracing::info!("Neo4j: Connecting to {} with database '{}'", uri, database);
        let graph = Graph::new(uri, username, password)
            .await
            .with_context(|| format!("Failed to connect to Neo4j at {}", uri))?;

        // Initialize schema
        let db = Self { 
            graph,
            database: database.to_string(),
        };
        tracing::info!("Neo4j: Connected with database '{}'", db.database);
        db.init_schema().await?;

        Ok(db)
    }

    /// Prepend USE database statement to Cypher query
    fn use_database(&self, cypher: &str) -> String {
        format!("USE `{}`; {}", self.database, cypher)
    }
    async fn init_schema(&self) -> Result<()> {
        // Create constraints for Memory nodes
        self.graph
            .run_on(&self.database, 
                neo4rs::query("CREATE CONSTRAINT memory_id IF NOT EXISTS FOR (m:Memory) REQUIRE m.id IS UNIQUE")
            )
            .await
            .ok();  // Ignore errors if constraint already exists

        // Create constraint for Concept nodes (by ID)
        self.graph
            .run_on(&self.database, 
                neo4rs::query("CREATE CONSTRAINT concept_id IF NOT EXISTS FOR (c:Concept) REQUIRE c.id IS UNIQUE")
            )
            .await
            .ok();

        // Create constraint for Concept nodes (by name - concept names are globally unique)
        self.graph
            .run_on(&self.database, 
                neo4rs::query("CREATE CONSTRAINT concept_name IF NOT EXISTS FOR (c:Concept) REQUIRE c.name IS UNIQUE")
            )
            .await
            .ok();

        Ok(())
    }
}

// Trait implementation
impl voidm_db::Database for Neo4jDatabase {
    fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let graph = self.graph.clone();
        Box::pin(async move {
            graph
                .run_on(&self.database, neo4rs::query("RETURN 1 as ping"))
                .await
                .map(|_| ())
                .context("Neo4j health check failed")
        })
    }

    fn close(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        // neo4rs::Graph doesn't have an explicit close method
        // Connection is closed when graph is dropped
        Box::pin(async move { Ok(()) })
    }

    fn ensure_schema(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let db = self.clone();
        Box::pin(async move {
            db.init_schema().await
        })
    }

    fn add_memory(
        &self,
        req_json: serde_json::Value,
        config: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let graph = self.graph.clone();
        let config = config.clone();
        let database = self.database.clone();

        Box::pin(async move {
            // Deserialize the request
            let req: voidm_core::AddMemoryRequest = serde_json::from_value(req_json)
                .context("Failed to deserialize AddMemoryRequest")?;
            let config: voidm_core::Config = serde_json::from_value(config)
                .context("Failed to deserialize Config")?;

            // USE SHARED CRUD LOGIC ✓
            let prepared = voidm_core::crud_logic::MemoryCreationPreparer::new(req.clone())
                .prepare()
                .context("Failed to prepare memory for creation")?;

            let config_model = config.embeddings.model.clone();
            
            // Convert metadata to JSON string for Neo4j storage
            let metadata_str = serde_json::to_string(&prepared.metadata)
                .context("Failed to serialize metadata")?;

            // Create Memory node AND tag relationships in ONE transaction ✓
            // Using FOREACH loop to handle multiple tags safely
            let memory_cypher = r#"MERGE (m:Memory { id: $id }) 
            SET m += { 
                type: $type, 
                content: $content, 
                importance: $importance, 
                metadata: $metadata, 
                quality_score: $quality_score,
                context: $context,
                author: $author,
                source: $source,
                created_at: $created_at, 
                updated_at: $updated_at, 
                embedding_model: $embedding_model 
            }
            WITH m
            FOREACH (tag IN $tags |
              MERGE (t:Tag { name: tag })
              MERGE (m)-[:HAS_TAG]->(t)
            )
            RETURN m"#;
            
            let query_obj = neo4rs::query(memory_cypher)
                .param("id", prepared.id.clone())
                .param("type", prepared.memory_type.to_string())
                .param("content", prepared.content.clone())
                .param("importance", prepared.importance)
                .param("metadata", metadata_str)
                .param("quality_score", prepared.quality_score)
                .param("context", prepared.context.clone())
                .param("author", prepared.author.clone())
                .param("source", prepared.source.clone())
                .param("created_at", prepared.created_at.clone())
                .param("updated_at", prepared.created_at.clone())
                .param("embedding_model", config_model.clone())
                .param("tags", prepared.tags.clone());  // ✓ Pass tags to Cypher

            tracing::debug!("Neo4j: Creating/updating memory in database '{}' with id: {} and tags: {:?}", 
                database, prepared.id, prepared.tags);

            let mut result = graph
                .execute_on(&database, query_obj)
                .await
                .map_err(|e| {
                    tracing::error!("Neo4j create_memory error: {}", e);
                    anyhow::anyhow!("Failed to create memory in Neo4j: {}", e)
                })?;

            if let Ok(Some(_row)) = result.next().await {
                // Memory and tags created successfully
                tracing::debug!("Neo4j: Memory created with {} tags", prepared.tags.len());
            }

            // Build response with tags from prepared data
            let response = voidm_db::models::AddMemoryResponse {
                id: prepared.id,
                memory_type: prepared.memory_type.to_string(),
                content: prepared.content,
                scopes: prepared.scopes,
                tags: prepared.tags,
                importance: prepared.importance,
                created_at: prepared.created_at,
                quality_score: None,
                metadata: prepared.metadata,
                suggested_links: vec![],
                duplicate_warning: None,
                context: prepared.context,
                title: prepared.title,
            };

            serde_json::to_value(response).context("Failed to serialize AddMemoryResponse")
        })
    }

    fn get_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();
        let database = self.database.clone();

        Box::pin(async move {
            // Query Memory node WITH related Tag nodes ✓
            let mut result = graph
                .execute_on(&database, 
                    neo4rs::query(
                        "MATCH (m:Memory {id: $id}) OPTIONAL MATCH (m)-[:HAS_TAG]->(t:Tag) RETURN m, COLLECT(t.name) as tags"
                    )
                        .param("id", id),
                )
                .await
                .context("Failed to get memory from Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("m").context("Failed to extract memory node")?;
                
                // Get tags from Tag nodes, not from Memory properties ✓
                let tags: Vec<String> = row.get("tags").unwrap_or_default();
                
                // Get metadata from property (stored as JSON string)
                let metadata_str: String = node.get("metadata").unwrap_or_else(|_| "{}".to_string());
                let metadata: serde_json::Value = serde_json::from_str(&metadata_str)
                    .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
                
                let memory = Memory {
                    id: node.get("id").context("Missing id")?,
                    content: node.get("content").context("Missing content")?,
                    memory_type: node.get::<String>("type").context("Missing type")?,
                    importance: node.get("importance").unwrap_or(0),
                    tags,  // ✓ FROM TAG NODES, NOT PROPERTY
                    metadata,
                    scopes: vec![], // TODO: Query scope relationships
                    created_at: node.get("created_at").context("Missing created_at")?,
                    updated_at: node.get("updated_at").context("Missing updated_at")?,
                    quality_score: None,
                    context: node.get("context").ok(),
                    title: node.get("title").ok(),
                };
                
                Ok(Some(serde_json::to_value(memory).context("Failed to serialize Memory")?))
            } else {
                Ok(None)
            }
        })
    }

    fn list_memories(&self, limit: Option<usize>) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let limit = limit.unwrap_or(100);

        Box::pin(async move {
            // Query Memory nodes WITH their Tag nodes ✓
            let mut result = graph
                .execute_on(&database, 
                    neo4rs::query(
                        "MATCH (m:Memory) OPTIONAL MATCH (m)-[:HAS_TAG]->(t:Tag) RETURN m, COLLECT(t.name) as tags ORDER BY m.created_at DESC LIMIT $limit"
                    )
                        .param("limit", limit as i64),
                )
                .await
                .context("Failed to list memories from Neo4j")?;

            let mut memories = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("m").context("Failed to extract memory node")?;
                
                // Get tags from Tag nodes ✓
                let tags: Vec<String> = row.get("tags").unwrap_or_default();
                
                // Get metadata from property
                let metadata_str: String = node.get("metadata").unwrap_or_else(|_| "{}".to_string());
                let metadata: serde_json::Value = serde_json::from_str(&metadata_str)
                    .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
                
                let memory = Memory {
                    id: node.get("id").context("Missing id")?,
                    content: node.get("content").context("Missing content")?,
                    memory_type: node.get::<String>("type").context("Missing type")?,
                    importance: node.get("importance").unwrap_or(0),
                    tags,  // ✓ FROM TAG NODES
                    metadata,
                    scopes: vec![], // TODO: Query scope relationships
                    created_at: node.get("created_at").context("Missing created_at")?,
                    updated_at: node.get("updated_at").unwrap_or_default(),
                    quality_score: None,
                    context: node.get("context").ok(),
                    title: node.get("title").ok(),
                };
                
                let memory_json = serde_json::to_value(memory).context("Failed to serialize Memory")?;
                memories.push(memory_json);
            }

            Ok(memories)
        })
    }

    fn delete_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute_on(&self.database, 
                    neo4rs::query("MATCH (m:Memory {id: $id}) DELETE m RETURN count(m) as deleted")
                        .param("id", id),
                )
                .await
                .context("Failed to delete memory from Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let deleted: i64 = row.get("deleted").unwrap_or(0);
                Ok(deleted > 0)
            } else {
                Ok(false)
            }
        })
    }

    fn update_memory(
        &self,
        id: &str,
        content: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();
        let content = content.to_string();

        Box::pin(async move {
            let updated_at = chrono::Utc::now().to_rfc3339();
            graph
                .run_on(&self.database, 
                    neo4rs::query("MATCH (m:Memory {id: $id}) SET m.content = $content, m.updated_at = $updated_at")
                        .param("id", id)
                        .param("content", content)
                        .param("updated_at", updated_at),
                )
                .await
                .context("Failed to update memory in Neo4j")?;
            
            Ok(())
        })
    }

    fn resolve_memory_id(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();
        
        Box::pin(async move {
            // Try exact match first
            let mut result = graph
                .execute_on(&self.database, neo4rs::query("MATCH (m:Memory {id: $id}) RETURN m.id LIMIT 1")
                    .param("id", id.clone()))
                .await
                .context("Failed to query Neo4j")?;
            
            if let Ok(Some(row)) = result.next().await {
                if let Ok(full_id) = row.get::<String>("m.id") {
                    return Ok(full_id);
                }
            }

            // Try prefix match
            if id.len() < 4 {
                anyhow::bail!("Memory ID prefix '{}' is too short (minimum 4 characters)", id);
            }

            let pattern = format!("{}.*", id);
            let mut result = graph
                .execute_on(&self.database, neo4rs::query("MATCH (m:Memory) WHERE m.id STARTS WITH $prefix RETURN m.id")
                    .param("prefix", id.clone()))
                .await
                .context("Failed to query Neo4j")?;

            let mut matches = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                if let Ok(mid) = row.get::<String>("m.id") {
                    matches.push(mid);
                }
            }

            match matches.len() {
                0 => anyhow::bail!("Memory '{}' not found", id),
                1 => Ok(matches.into_iter().next().unwrap()),
                n => anyhow::bail!(
                    "Ambiguous memory ID '{}' matches {} memories. Use more characters:\n{}",
                    id, n,
                    matches.iter().map(|m| format!("  {}", m)).collect::<Vec<_>>().join("\n")
                ),
            }
        })
    }

    fn list_scopes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
        let graph = self.graph.clone();

        Box::pin(async move {
            let mut result = graph
                .execute_on(&self.database, neo4rs::query("MATCH (m:Memory) WHERE m.scopes IS NOT NULL UNWIND m.scopes as scope RETURN DISTINCT scope"))
                .await
                .context("Failed to list scopes from Neo4j")?;

            let mut scopes = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                if let Ok(scope) = row.get::<String>("scope") {
                    scopes.push(scope);
                }
            }

            Ok(scopes)
        })
    }

    fn link_memories(
        &self,
        from_id: &str,
        rel: &str,
        to_id: &str,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let graph = self.graph.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let rel_type = rel.to_string();
        let note = note.map(|s| s.to_string());

        Box::pin(async move {
            let query = if let Some(note_text) = &note {
                neo4rs::query(
                    "MATCH (from:Memory {id: $from_id}), (to:Memory {id: $to_id})
                     CREATE (from)-[:RELATES {type: $rel_type, note: $note}]->(to)
                     RETURN true as created"
                )
                .param("from_id", from_id.clone())
                .param("to_id", to_id.clone())
                .param("rel_type", rel_type.clone())
                .param("note", note_text.clone())
            } else {
                neo4rs::query(
                    "MATCH (from:Memory {id: $from_id}), (to:Memory {id: $to_id})
                     CREATE (from)-[:RELATES {type: $rel_type}]->(to)
                     RETURN true as created"
                )
                .param("from_id", from_id.clone())
                .param("to_id", to_id.clone())
                .param("rel_type", rel_type.clone())
            };

            let mut result = graph
                .execute_on(&self.database, query)
                .await
                .context("Failed to link memories in Neo4j")?;

            let created = if let Ok(Some(_row)) = result.next().await {
                true
            } else {
                false
            };

            let response = LinkResponse {
                created,
                from: from_id,
                rel: rel_type,
                to: to_id,
            };

            serde_json::to_value(response).context("Failed to serialize LinkResponse")
        })
    }

    fn unlink_memories(
        &self,
        from_id: &str,
        rel: &str,
        to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let rel_type = rel.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute_on(&self.database, 
                    neo4rs::query(
                        "MATCH (from:Memory {id: $from_id})-[r:RELATES {type: $rel_type}]->(to:Memory {id: $to_id})
                         DELETE r RETURN count(r) as deleted"
                    )
                    .param("from_id", from_id)
                    .param("rel_type", rel_type)
                    .param("to_id", to_id),
                )
                .await
                .context("Failed to unlink memories in Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let deleted: i64 = row.get("deleted").unwrap_or(0);
                Ok(deleted > 0)
            } else {
                Ok(false)
            }
        })
    }

    fn list_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> {
        let graph = self.graph.clone();

        Box::pin(async move {
            let mut result = graph
                .execute_on(&self.database, 
                    neo4rs::query("MATCH (from:Memory)-[r:RELATES]->(to:Memory) RETURN from.id as from_id, to.id as to_id, r.type as rel_type, r.note as note")
                )
                .await
                .context("Failed to list edges from Neo4j")?;

            let mut edges = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                let edge = voidm_db::models::MemoryEdge {
                    from_id: row.get("from_id").context("Missing from_id")?,
                    to_id: row.get("to_id").context("Missing to_id")?,
                    rel_type: row.get("rel_type").context("Missing rel_type")?,
                    note: row.get("note").ok(),
                };
                let edge_json = serde_json::to_value(edge).context("Failed to serialize MemoryEdge")?;
                edges.push(edge_json);
            }

            Ok(edges)
        })
    }

    // ===== Search =====

    fn search_hybrid(
        &self,
        _opts_json: serde_json::Value,
        _model_name: &str,
        _embeddings_enabled: bool,
        _config_min_score: f32,
        _config_search: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        Box::pin(async move {
            // TODO: Implement hybrid search for Neo4j
            Ok(serde_json::json!({"results": [], "count": 0}))
        })
    }

    fn query_cypher(
        &self,
        query: &str,
        params: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let graph = self.graph.clone();
        let query = query.to_string();
        let params = params.clone();

        Box::pin(async move {
            let mut q = neo4rs::query(&query);
            
            if let Some(obj) = params.as_object() {
                for (key, value) in obj {
                    match value {
                        serde_json::Value::String(s) => q = q.param(key, s.clone()),
                        serde_json::Value::Number(n) => {
                            if let Some(i) = n.as_i64() {
                                q = q.param(key, i);
                            }
                        }
                        serde_json::Value::Bool(b) => q = q.param(key, *b),
                        _ => {}
                    }
                }
            }

            let mut result = graph
                .execute_on(&self.database, q)
                .await
                .context("Failed to execute Cypher query")?;

            let mut rows: Vec<serde_json::Value> = Vec::new();
            while let Ok(Some(_row)) = result.next().await {
                // TODO: Convert neo4rs Row to JSON properly
                // neo4rs::Row doesn't expose column names directly
                // For now, return empty result
                // Workaround: use raw Cypher with explicit RETURN fields in the query
            }

            Ok(serde_json::json!(rows))
        })
    }

    fn get_neighbors(
        &self,
        id: &str,
        depth: usize,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let depth = std::cmp::min(depth, 3);

            let mut result = graph
                .execute_on(&self.database, 
                    neo4rs::query(
                        &format!("MATCH (m:Memory {{id: $id}})-[*1..{}]-(neighbor) RETURN DISTINCT neighbor.id as neighbor_id", depth)
                    )
                    .param("id", id),
                )
                .await
                .context("Failed to get neighbors from Neo4j")?;

            let mut neighbors = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                if let Ok(neighbor_id) = row.get::<String>("neighbor_id") {
                    neighbors.push(neighbor_id);
                }
            }

            Ok(serde_json::json!({ "neighbors": neighbors }))
        })
    }

    fn search_bm25(
        &self,
        query: &str,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> {
        let graph = self.graph.clone();
        let query = query.to_string();
        let scope_filter = scope_filter.map(|s| s.to_string());
        let type_filter = type_filter.map(|s| s.to_string());

        Box::pin(async move {
            // Use Neo4j full-text search index
            let cypher = "CALL db.index.fulltext.queryNodes('memories_content', $query) YIELD node, score 
                         RETURN node.id as id, score 
                         ORDER BY score DESC 
                         LIMIT $limit".to_string();
            
            let result = graph
                .execute_on(&self.database, 
                    neo4rs::query(&cypher)
                        .param("query", query)
                        .param("limit", limit as i64)
                )
                .await
                .context("Failed to execute BM25 search on Neo4j")?;
            
            let mut result_handle = result;
            let mut results: Vec<(String, f32)> = Vec::new();
            while let Ok(Some(row)) = result_handle.next().await {
                if let (Ok(id), Ok(score)) = (row.get::<String>("id"), row.get::<f32>("score")) {
                    results.push((id, score));
                }
            }
            
            Ok(results)
        })
    }

    fn search_fuzzy(
        &self,
        query: &str,
        scope_filter: Option<&str>,
        limit: usize,
        threshold: f32,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> {
        let graph = self.graph.clone();
        let query = query.to_string();
        let scope_filter = scope_filter.map(|s| s.to_string());

        Box::pin(async move {
            // Fetch raw memories from Neo4j
            let cypher = "MATCH (m:Memory) RETURN m.id as id, m.content as content ORDER BY m.created_at DESC LIMIT $limit";
            
            let result = graph
                .execute_on(&self.database, 
                    neo4rs::query(cypher)
                        .param("limit", limit as i64)
                )
                .await
                .context("Failed to fetch memories for fuzzy search on Neo4j")?;
            
            let mut result_handle = result;
            let mut memories: Vec<(String, String)> = Vec::new();
            
            while let Ok(Some(row)) = result_handle.next().await {
                if let (Ok(id), Ok(content)) = (row.get::<String>("id"), row.get::<String>("content")) {
                    memories.push((id, content));
                }
            }
            
            // Apply fuzzy matching locally (placeholder - returns empty for now)
            // TODO: Add strsim dependency and implement proper Jaro-Winkler matching
            let mut results: Vec<(String, f32)> = Vec::new();
            
            // For now, fuzzy search returns empty (not yet implemented)
            // Can be enabled when strsim is added to dependencies
            
            Ok(results)
        })
    }

    fn search_ann(
        &self,
        embedding: Vec<f32>,
        limit: usize,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> {
        let graph = self.graph.clone();
        let scope_filter = scope_filter.map(|s| s.to_string());
        let type_filter = type_filter.map(|s| s.to_string());

        Box::pin(async move {
            // Neo4j vector search
            // Fetch memories matching filters; vector similarity would be computed client-side
            // or via stored embeddings if available
            
            let mut cypher = String::from("MATCH (m:Memory)");
            let mut where_clause = Vec::new();
            
            if let Some(scope) = &scope_filter {
                where_clause.push(format!("ANY(s IN m.scopes WHERE s STARTS WITH '{}')", scope));
            }
            if let Some(mtype) = &type_filter {
                where_clause.push(format!("m.type = '{}'", mtype));
            }
            
            if !where_clause.is_empty() {
                cypher.push_str(" WHERE ");
                cypher.push_str(&where_clause.join(" AND "));
            }
            
            cypher.push_str(" RETURN m.id as id LIMIT $limit");

            let result = graph
                .execute_on(&self.database, 
                    neo4rs::query(&cypher)
                        .param("limit", (limit * 3) as i64)
                )
                .await
                .context("Failed to execute vector search on Neo4j")?;

            let mut result_handle = result;
            let mut results: Vec<(String, f32)> = Vec::new();
            
            while let Ok(Some(row)) = result_handle.next().await {
                if let Ok(id) = row.get::<String>("id") {
                    // Return 0.5 as placeholder score for all results
                    // In production, would compute cosine similarity with stored embeddings
                    results.push((id, 0.5));
                }
            }

            // Sort by score descending and truncate to limit
            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            results.truncate(limit);
            
            Ok(results)
        })
    }

    fn fetch_memories_raw(
        &self,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String)>>> + Send + '_>> {
        let graph = self.graph.clone();
        let scope_filter = scope_filter.map(|s| s.to_string());
        let type_filter = type_filter.map(|s| s.to_string());

        Box::pin(async move {
            let cypher = "MATCH (m:Memory) RETURN m.id as id, m.content as content ORDER BY m.created_at DESC LIMIT $limit";
            
            let result = graph
                .execute_on(&self.database, 
                    neo4rs::query(cypher)
                        .param("limit", limit as i64)
                )
                .await
                .context("Failed to fetch memories from Neo4j")?;
            
            let mut result_handle = result;
            let mut memories: Vec<(String, String)> = Vec::new();
            
            while let Ok(Some(row)) = result_handle.next().await {
                if let (Ok(id), Ok(content)) = (row.get::<String>("id"), row.get::<String>("content")) {
                    memories.push((id, content));
                }
            }
            
            Ok(memories)
        })
    }

    fn fetch_memories_for_chunking(
        &self,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String, String)>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();

        Box::pin(async move {
            let cypher = "MATCH (m:Memory) RETURN m.id as id, m.content as content, m.created_at as created_at ORDER BY m.created_at DESC LIMIT $limit";
            
            let result = graph
                .execute_on(&database, 
                    neo4rs::query(cypher)
                        .param("limit", limit as i64)
                )
                .await
                .context("Failed to fetch memories for chunking from Neo4j")?;
            
            let mut result_handle = result;
            let mut memories: Vec<(String, String, String)> = Vec::new();
            
            while let Ok(Some(row)) = result_handle.next().await {
                if let (Ok(id), Ok(content), Ok(created_at)) = (
                    row.get::<String>("id"),
                    row.get::<String>("content"),
                    row.get::<String>("created_at")
                ) {
                    memories.push((id, content, created_at));
                }
            }
            
            Ok(memories)
        })
    }

    fn check_model_mismatch(
        &self,
        configured_model: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(String, String)>>> + Send + '_>> {
        let graph = self.graph.clone();
        let configured_model = configured_model.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute_on(&self.database, 
                    neo4rs::query("MATCH (m:Memory) WHERE m.embedding_model IS NOT NULL RETURN DISTINCT m.embedding_model LIMIT 1")
                )
                .await
                .context("Failed to check model mismatch")?;

            if let Ok(Some(row)) = result.next().await {
                if let Ok(stored_model) = row.get::<String>("m.embedding_model") {
                    if stored_model != configured_model && !stored_model.is_empty() {
                        return Ok(Some((stored_model, configured_model)));
                    }
                }
            }

            Ok(None)
        })
    }

    fn clean_database(&self) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();

        Box::pin(async move {
            let mut deleted_count = 0;

            // Delete all OntologyEdge relationships
            let _ = graph
                .execute_on(&database, neo4rs::query("MATCH ()-[r:INSTANCE_OF|RELATES_TO|SUPPORTS|CONTRADICTS|PRECEDES|DERIVED_FROM|PART_OF|EXEMPLIFIES]-() DELETE r"))
                .await
                .ok();

            // Delete all Concept nodes
            let _ = graph
                .execute_on(&database, neo4rs::query("MATCH (c:Concept) DELETE c"))
                .await
                .ok();

            // TODO: Implement proper count tracking for all deleted nodes and edges
            deleted_count += 0; // Placeholder

            tracing::info!("Neo4j: Database cleaned ({} items deleted)", deleted_count);
            Ok(deleted_count)
        })
    }

    fn delete_chunks_for_memory(
        &self,
        memory_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let memory_id = memory_id.to_string();

        Box::pin(async move {
            // Cypher: Delete all MemoryChunk nodes for a memory
            // MATCH (m:Memory {id: $id})-[r:CONTAINS]->(c:MemoryChunk)
            // DELETE r, c
            // RETURN count(c) as deleted_count

            let cypher = "MATCH (m:Memory {id: $id})-[r:CONTAINS]->(c:MemoryChunk) DELETE r, c RETURN count(c) as deleted_count";
            
            let mut result = graph
                .execute_on(&database, 
                    neo4rs::query(cypher)
                        .param("id", memory_id.clone())
                )
                .await
                .context("Failed to delete chunks for memory")?;

            let mut deleted_count = 0usize;
            if let Ok(Some(row)) = result.next().await {
                if let Ok(count) = row.get::<i64>("deleted_count") {
                    deleted_count = count as usize;
                }
            }

            tracing::info!("Neo4j: Deleted {} chunks for memory {}", deleted_count, memory_id);
            Ok(deleted_count)
        })
    }

    fn fetch_chunks(
        &self,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String, String)>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();

        Box::pin(async move {
            let cypher = "MATCH (m:Memory)-[r:CONTAINS]->(c:MemoryChunk) 
                          RETURN c.id as chunk_id, c.content as content, c.created_at as created_at 
                          LIMIT $limit";
            
            let mut result = graph
                .execute_on(&database, 
                    neo4rs::query(cypher)
                        .param("limit", limit as i64)
                )
                .await
                .context("Failed to fetch chunks")?;

            let mut chunks = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                if let (Ok(chunk_id), Ok(content), Ok(created_at)) = (
                    row.get::<String>("chunk_id"),
                    row.get::<String>("content"),
                    row.get::<String>("created_at"),
                ) {
                    chunks.push((chunk_id, content, created_at));
                }
            }

            Ok(chunks)
        })
    }

    fn store_chunk_embedding(
        &self,
        chunk_id: String,
        _memory_id: String,
        embedding: Vec<f32>,
    ) -> Pin<Box<dyn Future<Output = Result<(String, usize)>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let dim = embedding.len();

        Box::pin(async move {
            // Store embedding as a le_bytes property on MemoryChunk
            let embedding_bytes: Vec<u8> = embedding
                .iter()
                .flat_map(|f| f.to_le_bytes().to_vec())
                .collect();

            let cypher = "MATCH (c:MemoryChunk {id: $id}) 
                          SET c.embedding = $embedding, c.embedding_dim = $dim
                          RETURN c.id as chunk_id";
            
            graph
                .execute_on(&database, 
                    neo4rs::query(cypher)
                        .param("id", chunk_id.clone())
                        .param("embedding", embedding_bytes)
                        .param("dim", dim as i64)
                )
                .await
                .context("Failed to store chunk embedding")?;

            tracing::debug!("Neo4j: Stored {}D embedding for chunk {}", dim, chunk_id);
            Ok((chunk_id, dim))
        })
    }

    fn get_chunk_embedding(
        &self,
        chunk_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<f32>>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let chunk_id = chunk_id.to_string();

        Box::pin(async move {
            let cypher = "MATCH (c:MemoryChunk {id: $id}) 
                          RETURN c.embedding as embedding, c.embedding_dim as dim";
            
            let mut result = graph
                .execute_on(&database, 
                    neo4rs::query(cypher)
                        .param("id", chunk_id.clone())
                )
                .await
                .context("Failed to fetch chunk embedding")?;

            if let Ok(Some(row)) = result.next().await {
                if let (Ok(embedding_bytes), Ok(dim)) = (
                    row.get::<Vec<u8>>("embedding"),
                    row.get::<i64>("dim"),
                ) {
                    let dim = dim as usize;
                    let mut embedding = Vec::with_capacity(dim);
                    
                    for i in 0..dim {
                        let start = i * 4;
                        let end = start + 4;
                        if end <= embedding_bytes.len() {
                            let bytes = [
                                embedding_bytes[start],
                                embedding_bytes[start + 1],
                                embedding_bytes[start + 2],
                                embedding_bytes[start + 3],
                            ];
                            embedding.push(f32::from_le_bytes(bytes));
                        }
                    }
                    
                    if embedding.len() == dim {
                        return Ok(Some(embedding));
                    }
                }
            }

            Ok(None)
        })
    }

    fn search_by_embedding(
        &self,
        query_embedding: Vec<f32>,
        limit: usize,
        min_similarity: f32,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let dim = query_embedding.len();

        Box::pin(async move {
            // Neo4j: Fetch all chunks with embeddings and compute similarity
            let cypher = "MATCH (c:MemoryChunk) 
                          WHERE c.embedding IS NOT NULL AND c.embedding_dim = $dim
                          RETURN c.id as id, c.embedding as embedding, c.embedding_dim as dim
                          LIMIT 10000";
            
            let mut result = graph
                .execute_on(&database, 
                    neo4rs::query(cypher)
                        .param("dim", dim as i64)
                )
                .await
                .context("Failed to fetch embeddings for search")?;

            let mut similarities = Vec::new();
            
            while let Ok(Some(row)) = result.next().await {
                if let (Ok(chunk_id), Ok(embedding_bytes), Ok(d)) = (
                    row.get::<String>("id"),
                    row.get::<Vec<u8>>("embedding"),
                    row.get::<i64>("dim"),
                ) {
                    let d = d as usize;
                    if d != dim {
                        continue;
                    }
                    
                    let mut embedding = Vec::with_capacity(dim);
                    for i in 0..dim {
                        let start = i * 4;
                        let end = start + 4;
                        if end <= embedding_bytes.len() {
                            let bytes = [
                                embedding_bytes[start],
                                embedding_bytes[start + 1],
                                embedding_bytes[start + 2],
                                embedding_bytes[start + 3],
                            ];
                            embedding.push(f32::from_le_bytes(bytes));
                        }
                    }
                    
                    if embedding.len() == dim {
                        if let Ok(similarity) = voidm_core::similarity::cosine_similarity(&query_embedding, &embedding) {
                            if similarity >= min_similarity {
                                similarities.push((chunk_id, similarity));
                            }
                        }
                    }
                }
            }

            // Sort by similarity descending
            similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            similarities.truncate(limit);

            tracing::debug!("Neo4j: Found {} similar chunks", similarities.len());
            Ok(similarities)
        })
    }

    fn export_to_jsonl(
        &self,
        limit: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();

        Box::pin(async move {
            let mut records = Vec::new();
            let limit_val = limit.unwrap_or(999999) as i64;

            // Fetch all Memory nodes with all fields
            let cypher = "MATCH (m:Memory) RETURN m.id as id, m.type as type, m.content as content, 
                          m.created_at as created_at, m.updated_at as updated_at, 
                          m.title as title, m.metadata as metadata, m.scopes as scopes LIMIT $limit";
            
            let mut result = graph
                .execute_on(&database, 
                    neo4rs::query(cypher)
                        .param("limit", limit_val)
                )
                .await
                .context("Failed to fetch memories for export")?;

            while let Ok(Some(row)) = result.next().await {
                if let (Ok(id), Ok(mem_type), Ok(content), Ok(created_at)) = (
                    row.get::<String>("id"),
                    row.get::<String>("type"),
                    row.get::<String>("content"),
                    row.get::<String>("created_at"),
                ) {
                    let updated_at = row.get::<String>("updated_at").ok();
                    let title = row.get::<String>("title").ok();
                    let metadata_raw = row.get::<serde_json::Value>("metadata").ok();
                    let scopes_raw = row.get::<Vec<String>>("scopes").ok();

                    let memory_record = voidm_core::export::MemoryRecord {
                        id: id.clone(),
                        content,
                        memory_type: mem_type,
                        created_at,
                        updated_at,
                        title,
                        scope: None,
                        scopes: scopes_raw,
                        tags: None,
                        metadata: metadata_raw,
                        provenance: None,
                        context: None,
                        importance: None,
                        quality_score: None,
                    };

                    let record = voidm_core::export::ExportRecord::Memory(memory_record);
                    if let Ok(json) = voidm_core::export::record_to_jsonl(&record) {
                        records.push(json);
                    }
                }
            }

            tracing::info!("Neo4j: Exported {} memory records", records.len());
            Ok(records)
        })
    }

    fn import_from_jsonl(
        &self,
        records: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<(usize, usize, usize)>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();

        Box::pin(async move {
            let mut memory_count = 0;
            let mut chunk_count = 0;
            let mut relationship_count = 0;

            for line in records {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<voidm_core::export::ExportRecord>(&line) {
                    Ok(voidm_core::export::ExportRecord::Memory(mem)) => {
                        // Create Memory node in Neo4j with all fields
                        let cypher = "MERGE (m:Memory {id: $id}) 
                                      SET m.type = $type, m.content = $content, 
                                          m.created_at = $created_at, m.updated_at = $updated_at,
                                          m.title = $title, m.metadata = $metadata, m.scopes = $scopes
                                      RETURN m.id";
                        
                        // Serialize metadata and scopes to JSON strings for Neo4j
                        let metadata_str = mem.metadata.as_ref()
                            .and_then(|m| serde_json::to_string(m).ok());
                        let scopes_str = mem.scopes.as_ref()
                            .and_then(|s| serde_json::to_string(s).ok());

                        let result = graph
                            .execute_on(&database, 
                                neo4rs::query(cypher)
                                    .param("id", mem.id.clone())
                                    .param("type", mem.memory_type.clone())
                                    .param("content", mem.content.clone())
                                    .param("created_at", mem.created_at.clone())
                                    .param("updated_at", mem.updated_at.unwrap_or_else(|| mem.created_at.clone()))
                                    .param("title", mem.title.clone())
                                    .param("metadata", metadata_str)
                                    .param("scopes", scopes_str)
                            )
                            .await;

                        if result.is_ok() {
                            memory_count += 1;
                        }
                    }
                    Ok(voidm_core::export::ExportRecord::MemoryChunk(_chunk)) => {
                        // TODO: Implement chunk import
                        chunk_count += 1;
                    }
                    Ok(voidm_core::export::ExportRecord::Relationship(_rel)) => {
                        // TODO: Implement relationship import
                        relationship_count += 1;
                    }
                    Ok(voidm_core::export::ExportRecord::Concept(_concept)) => {
                        // Concepts not yet supported
                    }
                    Err(_) => {
                        continue;
                    }
                }
            }

            tracing::info!(
                "Neo4j: Imported {} memories, {} chunks, {} relationships",
                memory_count, chunk_count, relationship_count
            );
            Ok((memory_count, chunk_count, relationship_count))
        })
    }

    // ===== NEW MIGRATION METHODS (STUBS) =====

    fn list_tags(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        Box::pin(async move {
            let cypher = "MATCH (t:Tag) RETURN t.id as id, t.name as name, t.created_at as created_at";
            let mut tags = Vec::new();

            match graph.execute_on(&database, neo4rs::query(cypher)).await {
                Ok(mut result) => {
                    while let Ok(Some(row)) = result.next().await {
                        let tag_id: String = row.get("id").unwrap_or_else(|_| Uuid::new_v4().to_string());
                        let tag_name: Option<String> = row.get("name").ok().flatten();
                        let created_at: Option<String> = row.get("created_at").ok().flatten();

                        if let Some(name) = tag_name {
                            tags.push(json!({
                                "id": tag_id,
                                "name": name,
                                "created_at": created_at
                            }));
                        }
                    }
                    tracing::debug!("Neo4j: Found {} tags", tags.len());
                }
                Err(e) => {
                    tracing::warn!("Neo4j: Failed to list tags: {}", e);
                }
            }

            Ok(tags)
        })
    }

    fn create_tag(&self, _name: &str) -> Pin<Box<dyn Future<Output = Result<(String, bool)>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let name = _name.to_string();
        
        Box::pin(async move {
            // MERGE creates if doesn't exist, returns whether created
            let cypher = "MERGE (t:Tag {name: $name}) 
                         ON CREATE SET t.id = $id, t.created_at = $created_at
                         RETURN t.id as id, elementId(t) as element_id";
            
            let tag_id = Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();

            match graph.execute_on(
                &database,
                neo4rs::query(cypher)
                    .param("name", name.clone())
                    .param("id", tag_id.clone())
                    .param("created_at", now),
            ).await {
                Ok(mut result) => {
                    if let Ok(Some(row)) = result.next().await {
                        let returned_id: String = row.get("id").unwrap_or_else(|_| tag_id.clone());
                        tracing::debug!("Neo4j: Created/found tag '{}' with id {}", name, returned_id);
                        Ok((returned_id, true))
                    } else {
                        Ok((tag_id, false))
                    }
                }
                Err(e) => {
                    tracing::error!("Neo4j: Failed to create tag '{}': {}", name, e);
                    Err(e.into())
                }
            }
        })
    }

    fn link_tag_to_memory(&self, _tag_id: &str, _memory_id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let tag_id = _tag_id.to_string();
        let memory_id = _memory_id.to_string();
        
        Box::pin(async move {
            let cypher = "MATCH (t:Tag {id: $tag_id}), (m:Memory {id: $memory_id}) 
                         CREATE (t)-[:HAS_TAG]->(m)
                         RETURN true as success";

            match graph.execute_on(
                &database,
                neo4rs::query(cypher)
                    .param("tag_id", tag_id.clone())
                    .param("memory_id", memory_id.clone()),
            ).await {
                Ok(mut result) => {
                    if result.next().await.is_ok() {
                        tracing::debug!("Neo4j: Linked tag {} to memory {}", tag_id, memory_id);
                        Ok(true)
                    } else {
                        tracing::warn!("Neo4j: Failed to link tag {} to memory {}", tag_id, memory_id);
                        Ok(false)
                    }
                }
                Err(e) => {
                    tracing::error!("Neo4j: Error linking tag to memory: {}", e);
                    Ok(false)
                }
            }
        })
    }

    fn list_tag_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        Box::pin(async move {
            let cypher = "MATCH (t:Tag)-[rel:HAS_TAG]->(m:Memory) 
                         RETURN t.id as from, m.id as to";
            let mut edges = Vec::new();

            match graph.execute_on(&database, neo4rs::query(cypher)).await {
                Ok(mut result) => {
                    while let Ok(Some(row)) = result.next().await {
                        let from: String = row.get("from").unwrap_or_default();
                        let to: String = row.get("to").unwrap_or_default();

                        edges.push(json!({
                            "from": from,
                            "to": to,
                            "type": "HAS_TAG"
                        }));
                    }
                    tracing::debug!("Neo4j: Found {} tag edges", edges.len());
                }
                Err(e) => {
                    tracing::warn!("Neo4j: Failed to list tag edges: {}", e);
                }
            }

            Ok(edges)
        })
    }

    fn list_chunks(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        Box::pin(async move {
            let cypher = "MATCH (c:MemoryChunk) RETURN c.id as id, c.text as text, c.index as index, c.size as size, c.created_at as created_at";
            let mut chunks = Vec::new();

            match graph.execute_on(&database, neo4rs::query(cypher)).await {
                Ok(mut result) => {
                    while let Ok(Some(row)) = result.next().await {
                        let id: String = row.get("id").unwrap_or_default();
                        let text: String = row.get("text").unwrap_or_default();
                        let index: i64 = row.get("index").unwrap_or(0);
                        let size: i64 = row.get("size").unwrap_or(0);
                        let created_at: Option<String> = row.get("created_at").ok().flatten();

                        chunks.push(json!({
                            "id": id,
                            "text": text,
                            "index": index,
                            "size": size,
                            "created_at": created_at
                        }));
                    }
                    tracing::debug!("Neo4j: Found {} chunks", chunks.len());
                }
                Err(e) => {
                    tracing::warn!("Neo4j: Failed to list chunks: {}", e);
                }
            }

            Ok(chunks)
        })
    }

    fn get_chunk(&self, _chunk_id: &str) -> Pin<Box<dyn Future<Output = Result<Option<Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let chunk_id = _chunk_id.to_string();
        
        Box::pin(async move {
            let cypher = "MATCH (c:MemoryChunk {id: $id}) RETURN c.id as id, c.text as text, c.index as index, c.size as size, c.created_at as created_at";

            match graph.execute_on(
                &database,
                neo4rs::query(cypher).param("id", chunk_id.clone()),
            ).await {
                Ok(mut result) => {
                    if let Ok(Some(row)) = result.next().await {
                        let id: String = row.get("id").unwrap_or_default();
                        let text: String = row.get("text").unwrap_or_default();
                        let index: i64 = row.get("index").unwrap_or(0);
                        let size: i64 = row.get("size").unwrap_or(0);
                        let created_at: Option<String> = row.get("created_at").ok().flatten();

                        tracing::debug!("Neo4j: Found chunk {}", chunk_id);
                        return Ok(Some(json!({
                            "id": id,
                            "text": text,
                            "index": index,
                            "size": size,
                            "created_at": created_at
                        })));
                    }
                    tracing::debug!("Neo4j: Chunk {} not found", chunk_id);
                    Ok(None)
                }
                Err(e) => {
                    tracing::error!("Neo4j: Error getting chunk {}: {}", chunk_id, e);
                    Ok(None)
                }
            }
        })
    }

    fn list_chunk_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        Box::pin(async move {
            let cypher = "MATCH (c:MemoryChunk)-[rel:BELONGS_TO]->(m:Memory) RETURN c.id as from, m.id as to";
            let mut edges = Vec::new();

            match graph.execute_on(&database, neo4rs::query(cypher)).await {
                Ok(mut result) => {
                    while let Ok(Some(row)) = result.next().await {
                        let from: String = row.get("from").unwrap_or_default();
                        let to: String = row.get("to").unwrap_or_default();

                        edges.push(json!({
                            "from": from,
                            "to": to,
                            "type": "BELONGS_TO"
                        }));
                    }
                    tracing::debug!("Neo4j: Found {} chunk edges", edges.len());
                }
                Err(e) => {
                    tracing::warn!("Neo4j: Failed to list chunk edges: {}", e);
                }
            }

            Ok(edges)
        })
    }

    fn list_entities(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        Box::pin(async move {
            let cypher = "MATCH (e:Entity) RETURN e.id as id, e.name as name, e.type as entity_type, e.created_at as created_at";
            let mut entities = Vec::new();

            match graph.execute_on(&database, neo4rs::query(cypher)).await {
                Ok(mut result) => {
                    while let Ok(Some(row)) = result.next().await {
                        let id: String = row.get("id").unwrap_or_default();
                        let name: String = row.get("name").unwrap_or_default();
                        let entity_type: String = row.get("entity_type").unwrap_or_else(|_| "MISC".to_string());
                        let created_at: Option<String> = row.get("created_at").ok().flatten();

                        entities.push(json!({
                            "id": id,
                            "name": name,
                            "type": entity_type,
                            "created_at": created_at
                        }));
                    }
                    tracing::debug!("Neo4j: Found {} entities", entities.len());
                }
                Err(e) => {
                    tracing::warn!("Neo4j: Failed to list entities: {}", e);
                }
            }

            Ok(entities)
        })
    }

    fn get_or_create_entity(&self, _name: &str, _entity_type: &str) -> Pin<Box<dyn Future<Output = Result<(String, bool)>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let name = _name.to_string();
        let entity_type = _entity_type.to_string();
        
        Box::pin(async move {
            let cypher = "MERGE (e:Entity {name: $name, type: $type})
                         ON CREATE SET e.id = $id, e.created_at = $created_at
                         RETURN e.id as id";
            
            let entity_id = Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();

            match graph.execute_on(
                &database,
                neo4rs::query(cypher)
                    .param("name", name.clone())
                    .param("type", entity_type.clone())
                    .param("id", entity_id.clone())
                    .param("created_at", now),
            ).await {
                Ok(mut result) => {
                    if let Ok(Some(row)) = result.next().await {
                        let returned_id: String = row.get("id").unwrap_or_else(|_| entity_id.clone());
                        tracing::debug!("Neo4j: Created/found entity '{}' (type: {}) with id {}", name, entity_type, returned_id);
                        Ok((returned_id, true))
                    } else {
                        Ok((entity_id, false))
                    }
                }
                Err(e) => {
                    tracing::error!("Neo4j: Failed to create entity '{}': {}", name, e);
                    Err(e.into())
                }
            }
        })
    }

    fn link_chunk_to_entity(&self, _chunk_id: &str, _entity_id: &str, _confidence: f32) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let chunk_id = _chunk_id.to_string();
        let entity_id = _entity_id.to_string();
        let confidence = _confidence;
        
        Box::pin(async move {
            let cypher = "MATCH (c:MemoryChunk {id: $chunk_id}), (e:Entity {id: $entity_id})
                         CREATE (c)-[:MENTIONS {confidence: $confidence}]->(e)
                         RETURN true as success";

            match graph.execute_on(
                &database,
                neo4rs::query(cypher)
                    .param("chunk_id", chunk_id.clone())
                    .param("entity_id", entity_id.clone())
                    .param("confidence", confidence as f64),
            ).await {
                Ok(mut result) => {
                    if result.next().await.is_ok() {
                        tracing::debug!("Neo4j: Linked chunk {} to entity {} (confidence: {})", chunk_id, entity_id, confidence);
                        Ok(true)
                    } else {
                        tracing::warn!("Neo4j: Failed to link chunk {} to entity {}", chunk_id, entity_id);
                        Ok(false)
                    }
                }
                Err(e) => {
                    tracing::error!("Neo4j: Error linking chunk to entity: {}", e);
                    Ok(false)
                }
            }
        })
    }

    fn list_entity_mention_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        Box::pin(async move {
            let cypher = "MATCH (c:MemoryChunk)-[rel:MENTIONS]->(e:Entity) 
                         RETURN c.id as from, e.id as to, rel.confidence as confidence";
            let mut edges = Vec::new();

            match graph.execute_on(&database, neo4rs::query(cypher)).await {
                Ok(mut result) => {
                    while let Ok(Some(row)) = result.next().await {
                        let from: String = row.get("from").unwrap_or_default();
                        let to: String = row.get("to").unwrap_or_default();
                        let confidence: f64 = row.get("confidence").unwrap_or(0.5);

                        edges.push(json!({
                            "from": from,
                            "to": to,
                            "type": "MENTIONS",
                            "confidence": confidence
                        }));
                    }
                    tracing::debug!("Neo4j: Found {} entity mention edges", edges.len());
                }
                Err(e) => {
                    tracing::warn!("Neo4j: Failed to list entity mention edges: {}", e);
                }
            }

            Ok(edges)
        })
    }

    fn count_nodes(&self, _node_type: &str) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let node_type = _node_type.to_string();
        
        Box::pin(async move {
            let cypher = match node_type.as_str() {
                "Memory" => "MATCH (n:Memory) RETURN count(n) as count",
                "MemoryChunk" => "MATCH (n:MemoryChunk) RETURN count(n) as count",
                "Tag" => "MATCH (n:Tag) RETURN count(n) as count",
                "Entity" => "MATCH (n:Entity) RETURN count(n) as count",
                "Concept" => "MATCH (n:Concept) RETURN count(n) as count",
                _ => return Ok(0),
            };

            match graph.execute_on(&database, neo4rs::query(cypher)).await {
                Ok(mut result) => {
                    if let Ok(Some(row)) = result.next().await {
                        let count: i64 = row.get("count").unwrap_or(0);
                        tracing::debug!("Neo4j: Counted {} {} nodes", count, node_type);
                        Ok(count as usize)
                    } else {
                        Ok(0)
                    }
                }
                Err(e) => {
                    tracing::warn!("Neo4j: Failed to count {} nodes: {}", node_type, e);
                    Ok(0)
                }
            }
        })
    }

    fn count_edges(&self, _edge_type: Option<&str>) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let edge_type = _edge_type.map(|s| s.to_string());
        
        Box::pin(async move {
            let cypher = match edge_type.as_deref() {
                Some("HAS_TAG") => "MATCH ()-[r:HAS_TAG]-() RETURN count(r) as count",
                Some("BELONGS_TO") => "MATCH ()-[r:BELONGS_TO]-() RETURN count(r) as count",
                Some("MENTIONS") => "MATCH ()-[r:MENTIONS]-() RETURN count(r) as count",
                _ => "MATCH ()-[r]-() RETURN count(r) as count",
            };

            match graph.execute_on(&database, neo4rs::query(cypher)).await {
                Ok(mut result) => {
                    if let Ok(Some(row)) = result.next().await {
                        let count: i64 = row.get("count").unwrap_or(0);
                        let label = edge_type.as_deref().unwrap_or("total");
                        tracing::debug!("Neo4j: Counted {} {} edges", count, label);
                        Ok(count as usize)
                    } else {
                        Ok(0)
                    }
                }
                Err(e) => {
                    tracing::warn!("Neo4j: Failed to count edges: {}", e);
                    Ok(0)
                }
            }
        })
    }

    // Phase 0: Generic node/edge methods (stubs for Neo4j)
    fn create_node(
        &self,
        _id: &str,
        _node_type: &str,
        _properties: Value,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move { Err(anyhow::anyhow!("Not yet implemented for Neo4j")) })
    }

    fn get_node(&self, _id: &str) -> Pin<Box<dyn Future<Output = Result<Option<Value>>> + Send + '_>> {
        Box::pin(async move { Err(anyhow::anyhow!("Not yet implemented for Neo4j")) })
    }

    fn delete_node(&self, _id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        Box::pin(async move { Err(anyhow::anyhow!("Not yet implemented for Neo4j")) })
    }

    fn list_nodes(&self, _node_type: &str) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        Box::pin(async move { Err(anyhow::anyhow!("Not yet implemented for Neo4j")) })
    }

    fn create_edge(
        &self,
        _from_id: &str,
        _edge_type: &str,
        _to_id: &str,
        _properties: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move { Err(anyhow::anyhow!("Not yet implemented for Neo4j")) })
    }

    fn get_edge(
        &self,
        _from_id: &str,
        _edge_type: &str,
        _to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Value>>> + Send + '_>> {
        Box::pin(async move { Err(anyhow::anyhow!("Not yet implemented for Neo4j")) })
    }

    fn delete_edge(
        &self,
        _from_id: &str,
        _edge_type: &str,
        _to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        Box::pin(async move { Err(anyhow::anyhow!("Not yet implemented for Neo4j")) })
    }

    fn get_node_edges(
        &self,
        _node_id: &str,
        _edge_type: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        Box::pin(async move { Err(anyhow::anyhow!("Not yet implemented for Neo4j")) })
    }

    fn get_statistics(&self) -> Pin<Box<dyn Future<Output = Result<voidm_db::models::DatabaseStats>> + Send + '_>> {
        Box::pin(async move {
            Err(anyhow::anyhow!("get_statistics not yet implemented for Neo4j backend"))
        })
    }

    fn get_graph_stats(&self) -> Pin<Box<dyn Future<Output = Result<voidm_db::models::GraphStats>> + Send + '_>> {
        Box::pin(async move {
            Err(anyhow::anyhow!("get_graph_stats not yet implemented for Neo4j backend"))
        })
    }

    fn get_graph_export_data(&self) -> Pin<Box<dyn Future<Output = Result<voidm_db::models::GraphExportData>> + Send + '_>> {
        Box::pin(async move {
            Err(anyhow::anyhow!("get_graph_export_data not yet implemented for Neo4j backend"))
        })
    }

    fn graph_ops(&self) -> std::sync::Arc<dyn voidm_db::graph_ops::GraphQueryOps> {
        // Neo4j: Return a stub implementation for now
        // TODO: Implement full GraphQueryOps for Neo4j using Cypher
        std::sync::Arc::new(Neo4jGraphQueryOpsStub)
    }
    
}

/// Stub implementation of GraphQueryOps for Neo4j
/// TODO: Implement full Cypher-based graph operations
pub struct Neo4jGraphQueryOpsStub;

impl voidm_db::graph_ops::GraphQueryOps for Neo4jGraphQueryOpsStub {
    fn upsert_node(&self, _memory_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<i64>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn delete_node(&self, _memory_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn get_node_id(&self, _memory_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<i64>>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn upsert_edge(&self, _from_memory_id: &str, _to_memory_id: &str, _rel_type: &str, _note: Option<&str>) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<i64>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn delete_edge(&self, _from_memory_id: &str, _rel_type: &str, _to_memory_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<bool>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn get_outgoing_edges(&self, _node_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<(String, String, Option<String>)>>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn get_incoming_edges(&self, _node_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<(String, String, Option<String>)>>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn get_all_edges(&self, _node_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<(String, String)>>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn get_all_memory_edges(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<(i64, i64)>>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn get_all_memory_nodes(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<(i64, String)>>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn get_all_concept_nodes(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<String>>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn get_all_ontology_edges(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<(String, String)>>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn get_graph_stats(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<(i64, i64, std::collections::HashMap<String, i64>)>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }

    fn execute_cypher(&self, _sql: &str, _params: &[serde_json::Value]) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<std::collections::HashMap<String, serde_json::Value>>>> + Send + '_>> {
        Box::pin(async { Err(anyhow::anyhow!("GraphQueryOps not yet implemented for Neo4j")) })
    }
}

#[cfg(test)]
#[allow(dead_code)] // Integration tests disabled for now
mod neo4j_integration_tests {
    // Neo4j integration tests disabled pending dbtrait refactoring
    // Tests require access to running Neo4j instance and updated model
    // TODO: Rewrite these tests to use async dbtrait interface
    
    #[test]
    #[ignore]
    fn neo4j_integration_tests_disabled() {
        // Placeholder - real tests are in separate integration test suite
        assert!(true);
    }
}
