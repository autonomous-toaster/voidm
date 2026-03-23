use anyhow::{Context, Result};
use std::pin::Pin;
use std::future::Future;
use neo4rs::Graph;
use voidm_db_trait::Database;

use voidm_core::models::{
    AddMemoryRequest, AddMemoryResponse, Memory, EdgeType, LinkResponse,
};
use voidm_core::ontology::{Concept, ConceptWithInstances, OntologyEdge, ConceptWithSimilarityWarning, ConceptSearchResult};
use voidm_core::search::{SearchOptions, SearchResponse};

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
impl voidm_db_trait::Database for Neo4jDatabase {
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
            let config_model = config.embeddings.model.clone();

            let id = req.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let created_at = chrono::Utc::now().to_rfc3339();
            let memory_type = req.memory_type.to_string();
            
            // Compute quality score (same as SQLite path)
            let quality_mt = match req.memory_type {
                voidm_core::models::MemoryType::Episodic => voidm_scoring::MemoryType::Episodic,
                voidm_core::models::MemoryType::Semantic => voidm_scoring::MemoryType::Semantic,
                voidm_core::models::MemoryType::Procedural => voidm_scoring::MemoryType::Procedural,
                voidm_core::models::MemoryType::Conceptual => voidm_scoring::MemoryType::Conceptual,
                voidm_core::models::MemoryType::Contextual => voidm_scoring::MemoryType::Contextual,
            };
            let quality = voidm_scoring::compute_quality_score(&req.content, &quality_mt);
            
            // Extract author and source from metadata (set by MCP layer)
            let author = req.metadata.get("author")
                .and_then(|v| v.as_str())
                .unwrap_or("user")
                .to_string();
            let source = req.metadata.get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            
            // Convert metadata and tags to JSON strings for Neo4j storage
            let metadata_str = serde_json::to_string(&req.metadata)
                .context("Failed to serialize metadata")?;
            let tags_str = serde_json::to_string(&req.tags)
                .context("Failed to serialize tags")?;
            let scopes_str = serde_json::to_string(&req.scopes)
                .context("Failed to serialize scopes")?;

            // Use MERGE to handle duplicates gracefully (upsert pattern)
            let cypher = r#"MERGE (m:Memory { id: $id }) 
            SET m += { 
                type: $type, 
                content: $content, 
                importance: $importance, 
                tags: $tags, 
                metadata: $metadata, 
                scopes: $scopes, 
                quality_score: $quality_score,
                context: $context,
                author: $author,
                source: $source,
                created_at: $created_at, 
                updated_at: $updated_at, 
                embedding_model: $embedding_model 
            } 
            RETURN m"#;
            
            let query_obj = neo4rs::query(cypher)
                .param("id", id.clone())
                .param("type", memory_type.clone())
                .param("content", req.content.clone())
                .param("importance", req.importance)
                .param("tags", tags_str)
                .param("metadata", metadata_str)
                .param("scopes", scopes_str)
                .param("quality_score", quality.score)
                .param("context", req.context.clone())
                .param("author", author.clone())
                .param("source", source.clone())
                .param("created_at", created_at.clone())
                .param("updated_at", created_at.clone())
                .param("embedding_model", config_model.clone());

            tracing::debug!("Neo4j: Creating/updating memory in database '{}' with id: {}", 
                database, id);

            let mut result = graph
                .execute_on(&database, query_obj)
                .await
                .map_err(|e| {
                    tracing::error!("Neo4j create_memory error: {}", e);
                    anyhow::anyhow!("Failed to create memory in Neo4j: {}", e)
                })?;

            // Check if this was a new create or an update
            let _is_duplicate = if let Ok(Some(_row)) = result.next().await {
                // MERGE doesn't tell us if it was created or matched
                false
            } else {
                false
            };

            let response = AddMemoryResponse {
                id,
                memory_type,
                content: req.content,
                scopes: req.scopes,
                tags: req.tags,
                importance: req.importance,
                created_at,
                quality_score: None,
                metadata: req.metadata,
                suggested_links: vec![],
                duplicate_warning: None,
                context: req.context,
            };

            serde_json::to_value(response).context("Failed to serialize AddMemoryResponse")
        })
    }

    fn get_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute_on(&self.database, 
                    neo4rs::query("MATCH (m:Memory {id: $id}) RETURN m")
                        .param("id", id),
                )
                .await
                .context("Failed to get memory from Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("m").context("Failed to extract memory node")?;
                
                let memory = Memory {
                    id: node.get("id").context("Missing id")?,
                    content: node.get("content").context("Missing content")?,
                    memory_type: node.get::<String>("type").context("Missing type")?,
                    importance: node.get("importance").unwrap_or(0),
                    tags: node.get("tags").unwrap_or_default(),
                    metadata: serde_json::Value::Object(Default::default()),
                    scopes: node.get("scopes").unwrap_or_default(),
                    created_at: node.get("created_at").context("Missing created_at")?,
                    updated_at: node.get("updated_at").context("Missing updated_at")?,
                    quality_score: None,
                    context: node.get("context").ok(),
                };
                
                Ok(Some(serde_json::to_value(memory).context("Failed to serialize Memory")?))
            } else {
                Ok(None)
            }
        })
    }

    fn list_memories(&self, limit: Option<usize>) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let limit = limit.unwrap_or(100);

        Box::pin(async move {
            let mut result = graph
                .execute_on(&self.database, 
                    neo4rs::query("MATCH (m:Memory) RETURN m ORDER BY m.created_at DESC LIMIT $limit")
                        .param("limit", limit as i64),
                )
                .await
                .context("Failed to list memories from Neo4j")?;

            let mut memories = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("m").context("Failed to extract memory node")?;
                
                let memory = Memory {
                    id: node.get("id").context("Missing id")?,
                    content: node.get("content").context("Missing content")?,
                    memory_type: node.get::<String>("type").context("Missing type")?,
                    importance: node.get("importance").unwrap_or(0),
                    tags: node.get("tags").unwrap_or_default(),
                    metadata: serde_json::Value::Object(Default::default()),
                    scopes: node.get("scopes").unwrap_or_default(),
                    created_at: node.get("created_at").context("Missing created_at")?,
                    updated_at: node.get("updated_at").unwrap_or_default(),
                    quality_score: None,
                    context: node.get("context").ok(),
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
                conflict_warning: None,
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
                let edge = voidm_core::models::MemoryEdge {
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

    fn list_ontology_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        
        Box::pin(async move {
            // Query all relationships with their properties
            let cypher = r#"
                MATCH (from)-[r]->(to)
                WHERE r.from_id IS NOT NULL AND r.rel_type IS NOT NULL
                RETURN r.from_id AS from_id, r.from_type AS from_type, 
                       r.to_id AS to_id, r.to_type AS to_type, 
                       r.rel_type AS rel_type, r.note AS note
            "#;
            
            let mut result = graph
                .execute_on(&self.database, neo4rs::query(cypher))
                .await
                .context("Failed to list ontology edges from Neo4j")?;
            
            let mut edges = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                if let (Ok(from_id), Ok(from_type), Ok(to_id), Ok(to_type), Ok(rel_type)) = (
                    row.get::<String>("from_id"),
                    row.get::<String>("from_type"),
                    row.get::<String>("to_id"),
                    row.get::<String>("to_type"),
                    row.get::<String>("rel_type"),
                ) {
                    let note = row.get::<Option<String>>("note").ok().flatten();
                    let edge = voidm_core::models::OntologyEdgeForMigration {
                        from_id,
                        from_type,
                        to_id,
                        to_type,
                        rel_type,
                        note,
                    };
                    let edge_json = serde_json::to_value(edge).context("Failed to serialize OntologyEdgeForMigration")?;
                    edges.push(edge_json);
                }
            }
            
            Ok(edges)
        })
    }

    /// Link a memory or concept to another memory or concept (for ontology edges)
    fn create_ontology_edge(
        &self,
        from_id: &str,
        from_type: &str,
        rel_type: &str,
        to_id: &str,
        to_type: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let from_label = if from_type == "memory" { "Memory" } else { "Concept" };
        let to_label = if to_type == "memory" { "Memory" } else { "Concept" };
        let rel_type_str = rel_type.to_string();
        let from_type_str = from_type.to_string();
        let to_type_str = to_type.to_string();

        Box::pin(async move {
            // Create relationship with properties for later querying/deletion
            let query_str = format!(
                "MATCH (from:{} {{id: $from_id}}), (to:{} {{id: $to_id}})
                 CREATE (from)-[r:{}{{from_id: $from_id, from_type: $from_type, to_id: $to_id, to_type: $to_type, rel_type: $rel_type}}]->(to)
                 RETURN true as created",
                from_label, to_label, rel_type_str
            );

            let mut result = graph
                .execute_on(&self.database, neo4rs::query(&query_str)
                    .param("from_id", from_id.clone())
                    .param("from_type", from_type_str)
                    .param("to_id", to_id.clone())
                    .param("to_type", to_type_str)
                    .param("rel_type", rel_type_str))
                .await
                .with_context(|| format!("Failed to link {} -> {} in Neo4j", from_id, to_id))?;

            if let Ok(Some(_row)) = result.next().await {
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    fn search_hybrid(
        &self,
        opts_json: serde_json::Value,
        model_name: &str,
        embeddings_enabled: bool,
        config_min_score: f32,
        config_search: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        Box::pin(async move {
            // TODO: Implement Neo4j hybrid search
            let response = serde_json::json!({
                "results": [],
                "threshold_applied": false,
                "best_score": null
            });
            Ok(response)
        })
    }

    fn add_concept(
        &self,
        name: &str,
        description: Option<&str>,
        scope: Option<&str>,
        id: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let name = name.to_string();
        let description = description.map(|s| s.to_string());
        let scope = scope.map(|s| s.to_string());
        let id_owned = id.map(|s| s.to_string());

        Box::pin(async move {
            let id = id_owned.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let created_at = chrono::Utc::now().to_rfc3339();

            let query = neo4rs::query(
                "MERGE (c:Concept {id: $id})
                 ON CREATE SET c.name = $name, c.description = $description, c.scope = $scope, c.created_at = $created_at
                 ON MATCH SET c.name = $name, c.description = $description, c.scope = $scope
                 RETURN c"
            )
            .param("id", id.clone())
            .param("name", name.clone())
            .param("description", description.clone() as Option<String>)
            .param("scope", scope.clone() as Option<String>)
            .param("created_at", created_at.clone());

            graph
                .run_on(&database, query)
                .await
                .context("Failed to create concept in Neo4j")?;

            let response = ConceptWithSimilarityWarning {
                id,
                name,
                description,
                scope,
                created_at,
                similar_concepts: vec![],
            };

            serde_json::to_value(response).context("Failed to serialize ConceptWithSimilarityWarning")
        })
    }

    fn get_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute_on(&self.database, 
                    neo4rs::query("MATCH (c:Concept {id: $id}) RETURN c")
                        .param("id", id.clone()),
                )
                .await
                .context("Failed to get concept from Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("c").context("Failed to extract concept node")?;
                
                let concept = Concept {
                    id: node.get("id").context("Missing id")?,
                    name: node.get("name").context("Missing name")?,
                    description: node.get("description").ok(),
                    scope: node.get("scope").ok(),
                    created_at: node.get("created_at").context("Missing created_at")?,
                };
                
                Ok(serde_json::to_value(concept).context("Failed to serialize Concept")?)
            } else {
                anyhow::bail!("Concept not found: {}", id)
            }
        })
    }

    fn get_concept_with_instances(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute_on(&self.database, 
                    neo4rs::query("MATCH (c:Concept {id: $id}) RETURN c")
                        .param("id", id.clone()),
                )
                .await
                .context("Failed to get concept from Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("c").context("Failed to extract concept node")?;
                
                let concept = ConceptWithInstances {
                    id: node.get("id").context("Missing id")?,
                    name: node.get("name").context("Missing name")?,
                    description: node.get("description").ok(),
                    scope: node.get("scope").ok(),
                    created_at: node.get("created_at").context("Missing created_at")?,
                    instances: vec![],
                    subclasses: vec![],
                    superclasses: vec![],
                };
                
                Ok(serde_json::to_value(concept).context("Failed to serialize ConceptWithInstances")?)
            } else {
                anyhow::bail!("Concept not found: {}", id)
            }
        })
    }

    fn list_concepts(
        &self,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let scope = scope.map(|s| s.to_string());

        Box::pin(async move {
            let query = if let Some(scope_filter) = scope {
                neo4rs::query(
                    "MATCH (c:Concept {scope: $scope}) RETURN c LIMIT $limit"
                )
                .param("scope", scope_filter)
                .param("limit", limit as i64)
            } else {
                neo4rs::query(
                    "MATCH (c:Concept) RETURN c LIMIT $limit"
                )
                .param("limit", limit as i64)
            };

            let mut result = graph
                .execute_on(&self.database, query)
                .await
                .context("Failed to list concepts from Neo4j")?;

            let mut concepts = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("c").context("Failed to extract concept node")?;
                
                let concept = Concept {
                    id: node.get("id").context("Missing id")?,
                    name: node.get("name").context("Missing name")?,
                    description: node.get("description").ok(),
                    scope: node.get("scope").ok(),
                    created_at: node.get("created_at").context("Missing created_at")?,
                };
                
                let concept_json = serde_json::to_value(concept).context("Failed to serialize Concept")?;
                concepts.push(concept_json);
            }

            Ok(concepts)
        })
    }

    fn delete_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute_on(&self.database, 
                    neo4rs::query("MATCH (c:Concept {id: $id}) DELETE c RETURN count(c) as deleted")
                        .param("id", id),
                )
                .await
                .context("Failed to delete concept from Neo4j")?;

            if let Ok(Some(row)) = result.next().await {
                let deleted: i64 = row.get("deleted").unwrap_or(0);
                Ok(deleted > 0)
            } else {
                Ok(false)
            }
        })
    }

    fn resolve_concept_id(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();
        
        Box::pin(async move {
            // Try exact match first
            let mut result = graph
                .execute_on(&self.database, neo4rs::query("MATCH (c:Concept {id: $id}) RETURN c.id LIMIT 1")
                    .param("id", id.clone()))
                .await
                .context("Failed to query Neo4j")?;
            
            if let Ok(Some(row)) = result.next().await {
                if let Ok(full_id) = row.get::<String>("c.id") {
                    return Ok(full_id);
                }
            }

            // Try prefix match
            if id.len() < 4 {
                anyhow::bail!("Concept ID prefix '{}' is too short (minimum 4 characters)", id);
            }

            let mut result = graph
                .execute_on(&self.database, neo4rs::query("MATCH (c:Concept) WHERE c.id STARTS WITH $prefix RETURN c.id")
                    .param("prefix", id.clone()))
                .await
                .context("Failed to query Neo4j")?;

            let mut matches = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                if let Ok(cid) = row.get::<String>("c.id") {
                    matches.push(cid);
                }
            }

            match matches.len() {
                0 => anyhow::bail!("Concept '{}' not found", id),
                1 => Ok(matches.into_iter().next().unwrap()),
                n => anyhow::bail!(
                    "Ambiguous concept ID '{}' matches {} concepts. Use more characters:\n{}",
                    id, n,
                    matches.iter().map(|m| format!("  {}", m)).collect::<Vec<_>>().join("\n")
                ),
            }
        })
    }

    fn search_concepts(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> {
        let graph = self.graph.clone();
        let query_str = query.to_string();
        let scope = scope.map(|s| s.to_string());

        Box::pin(async move {
            let cypher_query = if let Some(scope_filter) = scope {
                neo4rs::query(
                    "MATCH (c:Concept {scope: $scope}) WHERE c.name =~ ('(?i).*' + $query + '.*') RETURN c LIMIT $limit"
                )
                .param("scope", scope_filter)
                .param("query", query_str)
                .param("limit", limit as i64)
            } else {
                neo4rs::query(
                    "MATCH (c:Concept) WHERE c.name =~ ('(?i).*' + $query + '.*') RETURN c LIMIT $limit"
                )
                .param("query", query_str)
                .param("limit", limit as i64)
            };

            let mut result = graph
                .execute_on(&self.database, cypher_query)
                .await
                .context("Failed to search concepts in Neo4j")?;

            let mut results = Vec::new();
            while let Ok(Some(row)) = result.next().await {
                let node: neo4rs::Node = row.get("c").context("Failed to extract concept node")?;
                
                let search_result = ConceptSearchResult {
                    id: node.get("id").context("Missing id")?,
                    name: node.get("name").context("Missing name")?,
                    description: node.get("description").ok(),
                    scope: node.get("scope").ok(),
                    score: 0.5,
                };
                
                let result_json = serde_json::to_value(search_result).context("Failed to serialize ConceptSearchResult")?;
                results.push(result_json);
            }

            Ok(results)
        })
    }

    fn add_ontology_edge(
        &self,
        from_id: &str,
        from_kind: &str,
        rel: &str,
        to_id: &str,
        to_kind: &str,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let graph = self.graph.clone();
        let database = self.database.clone();
        let from_id = from_id.to_string();
        let from_kind = from_kind.to_string();
        let rel = rel.to_string();
        let to_id = to_id.to_string();
        let to_kind = to_kind.to_string();
        let note = note.map(|s| s.to_string());

        Box::pin(async move {
            let from_label = match from_kind.as_str() {
                "concept" => "Concept",
                "memory" => "Memory",
                _ => "Concept", // default
            };
            let to_label = match to_kind.as_str() {
                "concept" => "Concept", 
                "memory" => "Memory",
                _ => "Concept", // default
            };

            // Use the actual relationship type (INSTANCE_OF, SUPPORTS, etc.)
            let query_str = format!(
                "MATCH (from:{} {{id: $from_id}}), (to:{} {{id: $to_id}})
                 CREATE (from)-[r:{}]->(to)
                 SET r.note = $note
                 RETURN id(r) as edge_id",
                from_label, to_label, rel
            );

            let q = neo4rs::query(&query_str)
                .param("from_id", from_id.clone())
                .param("to_id", to_id.clone())
                .param("note", note.clone() as Option<String>);

            let mut result = graph
                .execute_on(&database, q)
                .await
                .context("Failed to create ontology edge in Neo4j")?;

            let edge_id = if let Ok(Some(row)) = result.next().await {
                row.get::<i64>("edge_id").unwrap_or(1)
            } else {
                1
            };

            let created_at = chrono::Utc::now().to_rfc3339();

            let edge = OntologyEdge {
                id: edge_id,
                from_id,
                from_type: match from_kind.as_str() {
                    "concept" => voidm_core::ontology::NodeKind::Concept,
                    "memory" => voidm_core::ontology::NodeKind::Memory,
                    _ => voidm_core::ontology::NodeKind::Concept,
                },
                rel_type: rel.to_string(),
                to_id,
                to_type: match to_kind.as_str() {
                    "concept" => voidm_core::ontology::NodeKind::Concept,
                    "memory" => voidm_core::ontology::NodeKind::Memory,
                    _ => voidm_core::ontology::NodeKind::Concept,
                },
                note: note.map(|s| s.to_string()),
                created_at,
            };

            serde_json::to_value(edge).context("Failed to serialize OntologyEdge")
        })
    }

    fn delete_ontology_edge(&self, edge_id: i64) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        
        Box::pin(async move {
            // Neo4j internal relationship IDs are not directly accessible in parameterized queries
            // For now, we'll query to find the Nth relationship (by ordinal position)
            // and delete it. This is not ideal but works for the current interface.
            
            let cypher = r#"
                MATCH (from)-[r]->(to)
                WHERE r.from_id IS NOT NULL AND r.rel_type IS NOT NULL
                WITH r, row_number() OVER () AS rn
                WHERE rn = $edge_id
                DELETE r
                RETURN true as deleted
            "#;
            
            let mut result = graph
                .execute_on(&self.database, neo4rs::query(cypher).param("edge_id", edge_id))
                .await
                .context("Failed to delete ontology edge from Neo4j")?;
            
            if let Ok(Some(_row)) = result.next().await {
                Ok(true)
            } else {
                Ok(false)
            }
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
    
}
#[cfg(test)]
mod neo4j_integration_tests {
    use super::*;
    use voidm_core::db::Database;
    use voidm_core::models::{AddMemoryRequest, MemoryType};

    /// Test Neo4j connection with local instance
    /// Requires: docker run --publish=7474:7474 --publish=7687:7687 neo4j
    /// Run with: cargo test -- --ignored --test-threads=1
    #[tokio::test]
    #[ignore]  // Run manually with local Neo4j instance
    async fn test_neo4j_health_check() {
        let db = Neo4jDatabase::connect("bolt://localhost:7687", "neo4j", "neo4jneo4j", "neo4j")
            .await
            .expect("Failed to connect to Neo4j - is it running?");

        db.health_check().await.expect("Health check failed");
        println!("✓ Neo4j health check passed");
    }

    #[tokio::test]
    #[ignore]
    async fn test_neo4j_memory_crud() {
        let db = Neo4jDatabase::connect("bolt://localhost:7687", "neo4j", "neo4jneo4j", "neo4j")
            .await
            .expect("Failed to connect to Neo4j");

        let config = voidm_core::Config::default();

        // Create memory
        let req = AddMemoryRequest {
            id: None,
            content: "Integration test memory".to_string(),
            memory_type: MemoryType::Semantic,
            scopes: vec!["integration_test".to_string()],
            tags: vec!["test".to_string()],
            importance: 5,
            metadata: serde_json::json!({}),
            links: vec![],
        };

        let response = db.add_memory(req, &config)
            .await
            .expect("Failed to add memory");
        println!("✓ Created memory: {}", response.id);

        // Get memory
        let mem = db.get_memory(&response.id)
            .await
            .expect("Failed to get memory");
        assert!(mem.is_some(), "Memory should exist");
        let mem = mem.unwrap();
        assert_eq!(mem.content, "Integration test memory");
        assert_eq!(mem.memory_type.to_lowercase(), "semantic");
        println!("✓ Retrieved memory: {}", mem.id);

        // Update memory
        db.update_memory(&response.id, "Updated content")
            .await
            .expect("Failed to update memory");
        println!("✓ Updated memory");

        // Delete memory
        let deleted = db.delete_memory(&response.id)
            .await
            .expect("Failed to delete memory");
        assert!(deleted, "Memory should be deleted");
        println!("✓ Deleted memory");
    }

    #[tokio::test]
    #[ignore]
    async fn test_neo4j_relationships() {
        let db = Neo4jDatabase::connect("bolt://localhost:7687", "neo4j", "neo4jneo4j", "neo4j")
            .await
            .expect("Failed to connect to Neo4j");

        let config = voidm_core::Config::default();

        // Create two memories
        let req1 = AddMemoryRequest {
            id: None,
            content: "First memory".to_string(),
            memory_type: MemoryType::Semantic,
            scopes: vec!["rel_test".to_string()],
            tags: vec![],
            importance: 5,
            metadata: serde_json::json!({}),
            links: vec![],
        };

        let id1 = db.add_memory(req1, &config)
            .await
            .expect("Failed to create memory 1")
            .id;

        let req2 = AddMemoryRequest {
            id: None,
            content: "Second memory".to_string(),
            memory_type: MemoryType::Semantic,
            scopes: vec!["rel_test".to_string()],
            tags: vec![],
            importance: 5,
            metadata: serde_json::json!({}),
            links: vec![],
        };

        let id2 = db.add_memory(req2, &config)
            .await
            .expect("Failed to create memory 2")
            .id;

        // Link them
        let link = db.link_memories(&id1, &voidm_core::models::EdgeType::RelatesTo, &id2, Some("test link"))
            .await
            .expect("Failed to link memories");
        assert!(link.created, "Link should be created");
        println!("✓ Created relationship: {} -> {} ({})", id1, id2, link.rel);

        // Unlink them
        let unlinked = db.unlink_memories(&id1, &voidm_core::models::EdgeType::RelatesTo, &id2)
            .await
            .expect("Failed to unlink");
        assert!(unlinked, "Relationship should be deleted");
        println!("✓ Deleted relationship");

        // Cleanup
        let _ = db.delete_memory(&id1).await;
        let _ = db.delete_memory(&id2).await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_neo4j_concepts() {
        let db = Neo4jDatabase::connect("bolt://localhost:7687", "neo4j", "neo4jneo4j", "neo4j")
            .await
            .expect("Failed to connect to Neo4j");

        // Create concept
        let concept_resp = db.add_concept("TestConcept", Some("A test concept"), Some("testing"), None)
            .await
            .expect("Failed to add concept");
        println!("✓ Created concept: {}", concept_resp.id);

        // Get concept
        let concept = db.get_concept(&concept_resp.id)
            .await
            .expect("Failed to get concept");
        assert_eq!(concept.name, "TestConcept");
        println!("✓ Retrieved concept: {}", concept.name);

        // List concepts
        let concepts = db.list_concepts(Some("testing"), 10)
            .await
            .expect("Failed to list concepts");
        assert!(!concepts.is_empty(), "Should have at least one concept");
        println!("✓ Listed {} concepts", concepts.len());

        // Search concepts
        let search_results = db.search_concepts("Test", None, 10)
            .await
            .expect("Failed to search concepts");
        assert!(!search_results.is_empty(), "Should find test concept");
        println!("✓ Found {} concepts in search", search_results.len());

        // Delete concept
        let deleted = db.delete_concept(&concept_resp.id)
            .await
            .expect("Failed to delete concept");
        assert!(deleted, "Concept should be deleted");
        println!("✓ Deleted concept");
    }

    #[tokio::test]
    async fn test_neo4j_basic_operations() {
        // Connect to local Neo4j instance (assumes it's running)
        let db_result = Neo4jDatabase::connect("bolt://localhost:7687", "neo4j", "neo4jneo4j", "neo4j").await;
        if db_result.is_err() {
            println!("Skipping Neo4j test - local instance not available: {:?}", db_result.err());
            return;
        }

        let db = Arc::new(db_result.unwrap());
        println!("Connected to Neo4j successfully");

        // Test health check
        db.health_check().await.expect("Health check should pass");

        // Test schema initialization
        db.ensure_schema().await.expect("Schema initialization should succeed");

        // Test adding a memory
        let req_json = serde_json::json!({
            "content": "Test memory for Neo4j integration",
            "memory_type": "note",
            "importance": 0.7,
            "tags": ["test", "integration"],
            "scopes": ["test"],
            "metadata": {}
        });

        let config_json = serde_json::json!({
            "embeddings": {
                "model": "text-embedding-3-small"
            }
        });

        let response = db.add_memory(req_json, &config_json).await.expect("Should add memory");
        println!("Memory added successfully: {}", response);

        // Test listing memories
        let memories = db.list_memories(Some(10)).await.expect("Should list memories");
        println!("Found {} memories in Neo4j", memories.len());
        assert!(!memories.is_empty(), "Should have at least the memory we just added");

        // Test getting a memory (assuming we can extract ID from response)
        if let Some(first_memory) = memories.first() {
            // The memory is now a JsonValue, so we need to extract the ID
            if let Some(id) = first_memory.get("id").and_then(|v| v.as_str()) {
                let retrieved = db.get_memory(id).await.expect("Should retrieve memory");
                assert!(retrieved.is_some(), "Memory should exist");
                println!("Successfully retrieved memory: {}", retrieved.unwrap());
            }
        }
    }
}
