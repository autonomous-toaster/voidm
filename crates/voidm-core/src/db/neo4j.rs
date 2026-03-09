use anyhow::{Context, Result};
use std::pin::Pin;
use std::future::Future;
use neo4rs::Graph;

use crate::models::{
    AddMemoryRequest, AddMemoryResponse, Memory, EdgeType, LinkResponse,
};
use crate::ontology::{Concept, ConceptWithInstances, OntologyEdge, ConceptWithSimilarityWarning, ConceptSearchResult};
use crate::search::{SearchOptions, SearchResponse};

/// Neo4j implementation of the Database trait.
/// Uses the neo4rs async driver with Bolt protocol.
#[derive(Clone)]
pub struct Neo4jDatabase {
    pub graph: Graph,
}

impl Neo4jDatabase {
    /// Connect to a Neo4j instance
    pub async fn connect(uri: &str, username: &str, password: &str) -> Result<Self> {
        let graph = Graph::new(uri, username, password)
            .await
            .with_context(|| format!("Failed to connect to Neo4j at {}", uri))?;

        // Initialize schema
        let db = Self { graph };
        db.init_schema().await?;

        Ok(db)
    }

    /// Initialize Neo4j schema with constraints and indices
    async fn init_schema(&self) -> Result<()> {
        // Create constraints for Memory nodes
        self.graph
            .run(
                neo4rs::query("CREATE CONSTRAINT memory_id IF NOT EXISTS FOR (m:Memory) REQUIRE m.id IS UNIQUE")
            )
            .await
            .ok();  // Ignore errors if constraint already exists

        // Create constraint for Concept nodes
        self.graph
            .run(
                neo4rs::query("CREATE CONSTRAINT concept_id IF NOT EXISTS FOR (c:Concept) REQUIRE c.id IS UNIQUE")
            )
            .await
            .ok();

        Ok(())
    }
}

// Trait implementation
impl crate::db::Database for Neo4jDatabase {
    fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let graph = self.graph.clone();
        Box::pin(async move {
            graph
                .run(neo4rs::query("RETURN 1 as ping"))
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
        req: AddMemoryRequest,
        config: &crate::Config,
    ) -> Pin<Box<dyn Future<Output = Result<AddMemoryResponse>> + Send + '_>> {
        let graph = self.graph.clone();
        let config_model = config.embeddings.model.clone();

        Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let created_at = chrono::Utc::now().to_rfc3339();
            let memory_type = req.memory_type.to_string();

            let query = neo4rs::query(
                "CREATE (m:Memory {
                    id: $id,
                    type: $type,
                    content: $content,
                    importance: $importance,
                    tags: $tags,
                    metadata: $metadata,
                    scopes: $scopes,
                    created_at: $created_at,
                    updated_at: $created_at,
                    embedding_model: $model
                }) RETURN m"
            )
            .param("id", id.clone())
            .param("type", memory_type.clone())
            .param("content", req.content.clone())
            .param("importance", req.importance)
            .param("tags", req.tags.clone())
            .param("metadata", req.metadata.to_string())
            .param("scopes", req.scopes.clone())
            .param("created_at", created_at.clone())
            .param("model", config_model);

            graph
                .run(query)
                .await
                .context("Failed to create memory in Neo4j")?;

            Ok(AddMemoryResponse {
                id,
                memory_type,
                content: req.content,
                scopes: req.scopes,
                tags: req.tags,
                importance: req.importance,
                created_at,
                quality_score: None,
                suggested_links: vec![],
                duplicate_warning: None,
            })
        })
    }

    fn get_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Option<Memory>>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute(
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
                };
                
                Ok(Some(memory))
            } else {
                Ok(None)
            }
        })
    }

    fn list_memories(&self, limit: Option<usize>) -> Pin<Box<dyn Future<Output = Result<Vec<Memory>>> + Send + '_>> {
        let graph = self.graph.clone();
        let limit = limit.unwrap_or(100);

        Box::pin(async move {
            let mut result = graph
                .execute(
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
                };
                
                memories.push(memory);
            }

            Ok(memories)
        })
    }

    fn delete_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute(
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
                .run(
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
        let id = id.to_string();
        Box::pin(async move { Ok(id) })
    }

    fn list_scopes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
        let graph = self.graph.clone();

        Box::pin(async move {
            let mut result = graph
                .execute(neo4rs::query("MATCH (m:Memory) WHERE m.scopes IS NOT NULL UNWIND m.scopes as scope RETURN DISTINCT scope"))
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
        rel: &EdgeType,
        to_id: &str,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<LinkResponse>> + Send + '_>> {
        let graph = self.graph.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let rel_type = format!("{:?}", rel);
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
                .execute(query)
                .await
                .context("Failed to link memories in Neo4j")?;

            let created = if let Ok(Some(_row)) = result.next().await {
                true
            } else {
                false
            };

            Ok(LinkResponse {
                created,
                from: from_id,
                rel: rel_type,
                to: to_id,
                conflict_warning: None,
            })
        })
    }

    fn unlink_memories(
        &self,
        from_id: &str,
        rel: &EdgeType,
        to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let rel_type = format!("{:?}", rel);

        Box::pin(async move {
            let mut result = graph
                .execute(
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

    fn search_hybrid(
        &self,
        _opts: &SearchOptions,
        _model_name: &str,
        _embeddings_enabled: bool,
        _config_min_score: f32,
        _config_search: &crate::config::SearchConfig,
    ) -> Pin<Box<dyn Future<Output = Result<SearchResponse>> + Send + '_>> {
        Box::pin(async move {
            // Phase 2.5: Implement vector/hybrid search
            anyhow::bail!("Neo4j hybrid search not yet implemented")
        })
    }

    fn add_concept(
        &self,
        name: &str,
        description: Option<&str>,
        scope: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<ConceptWithSimilarityWarning>> + Send + '_>> {
        let graph = self.graph.clone();
        let name = name.to_string();
        let description = description.map(|s| s.to_string());
        let scope = scope.map(|s| s.to_string());

        Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let created_at = chrono::Utc::now().to_rfc3339();

            let query = neo4rs::query(
                "CREATE (c:Concept {
                    id: $id,
                    name: $name,
                    description: $description,
                    scope: $scope,
                    created_at: $created_at
                }) RETURN c"
            )
            .param("id", id.clone())
            .param("name", name.clone())
            .param("description", description.clone() as Option<String>)
            .param("scope", scope.clone() as Option<String>)
            .param("created_at", created_at.clone());

            graph
                .run(query)
                .await
                .context("Failed to create concept in Neo4j")?;

            Ok(ConceptWithSimilarityWarning {
                id,
                name,
                description,
                scope,
                created_at,
                similar_concepts: vec![],
            })
        })
    }

    fn get_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Concept>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute(
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
                
                Ok(concept)
            } else {
                anyhow::bail!("Concept not found: {}", id)
            }
        })
    }

    fn get_concept_with_instances(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<ConceptWithInstances>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute(
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
                
                Ok(concept)
            } else {
                anyhow::bail!("Concept not found: {}", id)
            }
        })
    }

    fn list_concepts(
        &self,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Concept>>> + Send + '_>> {
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
                .execute(query)
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
                
                concepts.push(concept);
            }

            Ok(concepts)
        })
    }

    fn delete_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let graph = self.graph.clone();
        let id = id.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute(
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
        let id = id.to_string();
        Box::pin(async move { Ok(id) })
    }

    fn search_concepts(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ConceptSearchResult>>> + Send + '_>> {
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
                .execute(cypher_query)
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
                
                results.push(search_result);
            }

            Ok(results)
        })
    }

    fn add_ontology_edge(
        &self,
        from_id: &str,
        from_kind: crate::ontology::NodeKind,
        rel: &crate::ontology::OntologyRelType,
        to_id: &str,
        to_kind: crate::ontology::NodeKind,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<OntologyEdge>> + Send + '_>> {
        let graph = self.graph.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let rel_type = format!("{:?}", rel);
        let note = note.map(|s| s.to_string());

        Box::pin(async move {
            let from_label = match from_kind {
                crate::ontology::NodeKind::Concept => "Concept",
                crate::ontology::NodeKind::Memory => "Memory",
            };
            let to_label = match to_kind {
                crate::ontology::NodeKind::Concept => "Concept",
                crate::ontology::NodeKind::Memory => "Memory",
            };

            let query = format!(
                "MATCH (from:{} {{id: $from_id}}), (to:{} {{id: $to_id}})
                 CREATE (from)-[r:ONTOLOGY {{type: $rel_type, note: $note}}]->(to)
                 RETURN id(r) as edge_id",
                from_label, to_label
            );

            let q = neo4rs::query(&query)
                .param("from_id", from_id.clone())
                .param("rel_type", rel_type.clone())
                .param("to_id", to_id.clone())
                .param("note", note.clone() as Option<String>);

            let mut result = graph
                .execute(q)
                .await
                .context("Failed to create ontology edge in Neo4j")?;

            let edge_id = if let Ok(Some(row)) = result.next().await {
                row.get::<i64>("edge_id").unwrap_or(1)
            } else {
                1
            };

            let created_at = chrono::Utc::now().to_rfc3339();

            Ok(OntologyEdge {
                id: edge_id,
                from_id,
                from_type: from_kind,
                rel_type,
                to_id,
                to_type: to_kind,
                note,
                created_at,
            })
        })
    }

    fn delete_ontology_edge(&self, _edge_id: i64) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        Box::pin(async move {
            // Neo4j relationship IDs are not easily accessible in patterns
            // This would need a different approach
            anyhow::bail!("Neo4j delete_ontology_edge needs refactoring for relationship management")
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
                .execute(q)
                .await
                .context("Failed to execute Cypher query")?;

            let mut rows = Vec::new();
            while let Ok(Some(_row)) = result.next().await {
                // TODO: Convert row to JSON
                rows.push(serde_json::json!({}));
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
                .execute(
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

    fn check_model_mismatch(
        &self,
        configured_model: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(String, String)>>> + Send + '_>> {
        let graph = self.graph.clone();
        let configured_model = configured_model.to_string();

        Box::pin(async move {
            let mut result = graph
                .execute(
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
