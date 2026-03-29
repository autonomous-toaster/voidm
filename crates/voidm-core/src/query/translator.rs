// Query Translator Trait - Backend-Agnostic Query Interface
//
// This trait defines the interface for translating Cypher operations
// to backend-specific SQL or other query languages.

use super::cypher::CypherOperation;
use super::QueryParams;
use std::collections::HashMap;
use serde_json::Value;

/// Trait for translating Cypher operations to backend-specific queries
pub trait QueryTranslator: Send + Sync {
    /// Get the backend name
    fn backend_name(&self) -> &'static str;

    /// Translate a Cypher operation to a backend-specific query
    ///
    /// Returns (query_string, parameters) tuple
    fn translate(&self, op: &CypherOperation) -> Result<(String, QueryParams), String>;

    /// Translate memory create operation
    fn translate_memory_create(
        &self,
        id: &str,
        memory_type: &str,
        content: &str,
        importance: i32,
        tags: &[String],
        scopes: &[String],
        created_at: &str,
        embedding: Option<&[f32]>,
        metadata: Option<&str>,
    ) -> Result<(String, QueryParams), String>;

    /// Translate memory get operation
    fn translate_memory_get(&self, id: &str) -> Result<(String, QueryParams), String>;

    /// Translate memory list operation
    fn translate_memory_list(&self, limit: Option<usize>) -> Result<(String, QueryParams), String>;

    /// Translate memory delete operation
    fn translate_memory_delete(&self, id: &str) -> Result<(String, QueryParams), String>;

    /// Translate memory update operation
    fn translate_memory_update(
        &self,
        id: &str,
        content: &str,
        updated_at: &str,
    ) -> Result<(String, QueryParams), String>;

    /// Translate memory ID resolution (prefix matching)
    fn translate_memory_resolve_id(&self, prefix: &str) -> Result<(String, QueryParams), String>;

    /// Translate list scopes operation
    fn translate_list_scopes(&self) -> Result<(String, QueryParams), String>;

    /// Translate link memories operation
    fn translate_link_memories(
        &self,
        from_id: &str,
        rel_type: &str,
        to_id: &str,
        note: Option<&str>,
        created_at: &str,
    ) -> Result<(String, QueryParams), String>;

    /// Translate unlink memories operation
    fn translate_unlink_memories(
        &self,
        from_id: &str,
        rel_type: &str,
        to_id: &str,
    ) -> Result<(String, QueryParams), String>;

    /// Translate list memory edges operation
    fn translate_list_memory_edges(&self) -> Result<(String, QueryParams), String>;

    /// Translate hybrid search operation
    fn translate_search_hybrid(
        &self,
        query: &str,
        limit: usize,
        min_score: f32,
        scopes: &[String],
        embedding: Option<&[f32]>,
    ) -> Result<(String, QueryParams), String>;

    /// Translate hybrid search with RRF (Reciprocal Rank Fusion)
    fn translate_search_hybrid_rrf(
        &self,
        query: &str,
        limit: usize,
        min_score: f32,
        scopes: &[String],
        embedding: Option<&[f32]>,
    ) -> Result<(String, QueryParams), String>;

    /// Translate arbitrary Cypher query
    fn translate_query_cypher(
        &self,
        query: &str,
        params: &HashMap<String, Value>,
    ) -> Result<(String, QueryParams), String>;

    /// Translate get neighbors operation
    fn translate_get_neighbors(&self, id: &str, depth: usize) -> Result<(String, QueryParams), String>;
}

/// Neo4j translator - Pass-through to native Cypher
pub struct Neo4jTranslator;

impl QueryTranslator for Neo4jTranslator {
    fn backend_name(&self) -> &'static str {
        "neo4j"
    }

    fn translate(&self, op: &CypherOperation) -> Result<(String, QueryParams), String> {
        // Neo4j uses Cypher directly - extract parameters from operation
        match op {
            CypherOperation::MemoryCreate {
                id,
                memory_type,
                content,
                importance,
                tags,
                scopes,
                created_at,
                embedding,
                metadata,
            } => self.translate_memory_create(
                id, memory_type, content, *importance, tags, scopes, created_at, embedding.as_deref(), metadata.as_deref()
            ),
            CypherOperation::MemoryGet { id } => self.translate_memory_get(id),
            CypherOperation::MemoryList { limit } => self.translate_memory_list(*limit),
            CypherOperation::MemoryDelete { id } => self.translate_memory_delete(id),
            CypherOperation::MemoryUpdate { id, content, updated_at } => {
                self.translate_memory_update(id, content, updated_at)
            }
            CypherOperation::MemoryResolveId { prefix } => self.translate_memory_resolve_id(prefix),
            CypherOperation::MemoryListScopes => self.translate_list_scopes(),
            CypherOperation::LinkMemories {
                from_id,
                rel_type,
                to_id,
                note,
                created_at,
            } => self.translate_link_memories(from_id, rel_type, to_id, note.as_deref(), created_at),
            CypherOperation::UnlinkMemories {
                from_id,
                rel_type,
                to_id,
            } => self.translate_unlink_memories(from_id, rel_type, to_id),
            CypherOperation::ListMemoryEdges => self.translate_list_memory_edges(),
            CypherOperation::SearchHybrid {
                query,
                limit,
                min_score,
                scopes,
                embedding,
            } => self.translate_search_hybrid(query, *limit, *min_score, scopes, embedding.as_deref()),
            CypherOperation::SearchHybridRRF {
                query,
                limit,
                min_score,
                scopes,
                embedding,
            } => self.translate_search_hybrid_rrf(query, *limit, *min_score, scopes, embedding.as_deref()),
            CypherOperation::QueryCypher { query, params } => self.translate_query_cypher(query, params),
            CypherOperation::GetNeighbors { id, depth } => self.translate_get_neighbors(id, *depth),
        }
    }

    // Implementation stubs for Neo4j translator
    // These will pass through the Cypher patterns directly

    fn translate_memory_create(
        &self,
        id: &str,
        memory_type: &str,
        content: &str,
        importance: i32,
        tags: &[String],
        scopes: &[String],
        created_at: &str,
        embedding: Option<&[f32]>,
        metadata: Option<&str>,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("id", id)
            .with_param("type", memory_type)
            .with_param("content", content)
            .with_param("importance", importance)
            .with_param("tags", tags)
            .with_param("scopes", scopes)
            .with_param("created_at", created_at)
            .with_param("embedding", embedding.map(|e| e.to_vec()))
            .with_param("metadata", metadata);

        let cypher = r#"
            CREATE (m:Memory {
              id: $id,
              type: $type,
              content: $content,
              importance: $importance,
              tags: $tags,
              scopes: $scopes,
              created_at: $created_at,
              embedding: $embedding,
              metadata: $metadata
            })
            RETURN m
        "#.to_string();

        Ok((cypher, params))
    }

    fn translate_memory_get(&self, id: &str) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new().with_param("id", id);
        let cypher = r#"
            MATCH (m:Memory {id: $id})
            RETURN m
        "#.to_string();
        Ok((cypher, params))
    }

    fn translate_memory_list(&self, limit: Option<usize>) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new().with_param("limit", limit.unwrap_or(1000));
        let cypher = r#"
            MATCH (m:Memory)
            RETURN m
            LIMIT $limit
        "#.to_string();
        Ok((cypher, params))
    }

    fn translate_memory_delete(&self, id: &str) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new().with_param("id", id);
        let cypher = r#"
            MATCH (m:Memory {id: $id})
            DELETE m
            RETURN true as deleted
        "#.to_string();
        Ok((cypher, params))
    }

    fn translate_memory_update(
        &self,
        id: &str,
        content: &str,
        updated_at: &str,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("id", id)
            .with_param("content", content)
            .with_param("updated_at", updated_at);
        let cypher = r#"
            MATCH (m:Memory {id: $id})
            SET m.content = $content, m.updated_at = $updated_at
            RETURN m
        "#.to_string();
        Ok((cypher, params))
    }

    fn translate_memory_resolve_id(&self, prefix: &str) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new().with_param("prefix", prefix);
        let cypher = r#"
            MATCH (m:Memory)
            WHERE m.id STARTS WITH $prefix
            RETURN m.id
            LIMIT 1
        "#.to_string();
        Ok((cypher, params))
    }

    fn translate_list_scopes(&self) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new();
        let cypher = r#"
            MATCH (m:Memory)
            UNWIND m.scopes as scope
            RETURN DISTINCT scope
            ORDER BY scope
        "#.to_string();
        Ok((cypher, params))
    }

    fn translate_link_memories(
        &self,
        from_id: &str,
        rel_type: &str,
        to_id: &str,
        note: Option<&str>,
        created_at: &str,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("from_id", from_id)
            .with_param("rel_type", rel_type)
            .with_param("to_id", to_id)
            .with_param("note", note)
            .with_param("created_at", created_at);
        let cypher = r#"
            MATCH (from:Memory {id: $from_id}), (to:Memory {id: $to_id})
            CREATE (from)-[r {rel_type: $rel_type, note: $note, created_at: $created_at}]->(to)
            RETURN r, from.id, to.id
        "#.to_string();
        Ok((cypher, params))
    }

    fn translate_unlink_memories(
        &self,
        from_id: &str,
        rel_type: &str,
        to_id: &str,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("from_id", from_id)
            .with_param("rel_type", rel_type)
            .with_param("to_id", to_id);
        let cypher = r#"
            MATCH (from:Memory {id: $from_id})-[r {rel_type: $rel_type}]->(to:Memory {id: $to_id})
            DELETE r
            RETURN true as deleted
        "#.to_string();
        Ok((cypher, params))
    }

    fn translate_list_memory_edges(&self) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new();
        let cypher = r#"
            MATCH (from:Memory)-[r]->(to:Memory)
            RETURN from.id, r.rel_type, to.id, r.note, r.created_at
        "#.to_string();
        Ok((cypher, params))
    }

    fn translate_search_hybrid(
        &self,
        query: &str,
        limit: usize,
        min_score: f32,
        scopes: &[String],
        embedding: Option<&[f32]>,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("query", query)
            .with_param("limit", limit)
            .with_param("min_score", min_score)
            .with_param("scopes", scopes)
            .with_param("embedding", embedding.map(|e| e.to_vec()));

        // Note: The actual hybrid search implementation is more complex
        // This is a simplified stub for the translator interface
        let cypher = r#"
            MATCH (m:Memory)
            WHERE m.content CONTAINS $query
            RETURN m
            LIMIT $limit
        "#.to_string();

        Ok((cypher, params))
    }

    fn translate_search_hybrid_rrf(
        &self,
        query: &str,
        limit: usize,
        min_score: f32,
        scopes: &[String],
        embedding: Option<&[f32]>,
    ) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("query", query)
            .with_param("limit", limit)
            .with_param("min_score", min_score)
            .with_param("scopes", scopes)
            .with_param("embedding", embedding.map(|e| e.to_vec()));

        // RRF hybrid search - uses Reciprocal Rank Fusion to combine signals
        let cypher = r#"
            // Implementation handled by search_with_rrf() in search.rs
            // This translator stub is for interface compatibility
            MATCH (m:Memory)
            WHERE m.content CONTAINS $query
            RETURN m
            LIMIT $limit
        "#.to_string();

        Ok((cypher, params))
    }

    fn translate_query_cypher(
        &self,
        query: &str,
        params: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<(String, QueryParams), String> {
        let mut query_params = QueryParams::new();
        for (key, value) in params {
            query_params.params.insert(key.clone(), value.clone());
        }
        Ok((query.to_string(), query_params))
    }

    fn translate_get_neighbors(&self, id: &str, depth: usize) -> Result<(String, QueryParams), String> {
        let params = QueryParams::new()
            .with_param("id", id)
            .with_param("depth", depth as i32);
        let cypher = r#"
            MATCH path = (n {id: $id})-[*0..$depth]-(neighbor)
            RETURN collect(neighbor) as neighbors
        "#.to_string();
        Ok((cypher, params))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neo4j_translator_memory_get() {
        let translator = Neo4jTranslator;
        let (query, params) = translator.translate_memory_get("test-id").unwrap();
        assert!(query.contains("MATCH"));
        assert!(query.contains("Memory"));
        assert!(params.get("id").is_some());
    }

    #[test]
    fn test_neo4j_translator_concept_list() {
        let translator = Neo4jTranslator;
        let (query, params) = translator.translate_concept_list(Some("test"), 10).unwrap();
        assert!(query.contains("Concept"));
        assert!(params.get("scope").is_some());
        assert!(params.get("limit").is_some());
    }
}
