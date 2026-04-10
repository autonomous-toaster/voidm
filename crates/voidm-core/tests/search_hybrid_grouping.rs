use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use voidm_core::config::SearchConfig;
use voidm_core::search::{search, SearchMode, SearchOptions};
use voidm_db::graph_ops::GraphQueryOps;
use voidm_db::models::{DatabaseStats, EmbeddingStats, GraphExportData, GraphStats};
use voidm_db::Database;

struct FakeDb;
struct FakeGraphOps;

impl FakeDb {
    fn memory_one() -> serde_json::Value {
        json!({
            "id": "mem_1",
            "type": "semantic",
            "content": "Full memory one content that should not be returned whole.",
            "title": "Database tuning",
            "scopes": ["work/db"],
            "tags": ["db"],
            "importance": 5,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "quality_score": 0.9,
            "metadata": {},
            "context": null
        })
    }

    fn memory_two() -> serde_json::Value {
        json!({
            "id": "mem_2",
            "type": "semantic",
            "content": "Memory two content.",
            "title": "Rust ownership",
            "scopes": ["work/rust"],
            "tags": ["rust"],
            "importance": 5,
            "created_at": "2026-01-02T00:00:00Z",
            "updated_at": "2026-01-02T00:00:00Z",
            "quality_score": 0.8,
            "metadata": {},
            "context": null
        })
    }
}

impl GraphQueryOps for FakeGraphOps {
    fn upsert_node(&self, _memory_id: &str) -> Pin<Box<dyn Future<Output = Result<i64>> + Send + '_>> { Box::pin(async { Ok(1) }) }
    fn delete_node(&self, _memory_id: &str) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> { Box::pin(async { Ok(()) }) }
    fn get_node_id(&self, _memory_id: &str) -> Pin<Box<dyn Future<Output = Result<Option<i64>>> + Send + '_>> { Box::pin(async { Ok(Some(1)) }) }
    fn upsert_edge(&self, _from_memory_id: &str, _to_memory_id: &str, _rel_type: &str, _note: Option<&str>) -> Pin<Box<dyn Future<Output = Result<i64>> + Send + '_>> { Box::pin(async { Ok(1) }) }
    fn delete_edge(&self, _from_memory_id: &str, _rel_type: &str, _to_memory_id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> { Box::pin(async { Ok(true) }) }
    fn get_outgoing_edges(&self, _node_id: i64) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String, Option<String>)>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn get_incoming_edges(&self, _node_id: i64) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String, Option<String>)>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn get_all_edges(&self, _node_id: i64) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String)>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn get_all_memory_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<(i64, i64)>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn get_all_memory_nodes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<(i64, String)>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn get_all_concept_nodes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn get_all_ontology_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String)>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn get_graph_stats(&self) -> Pin<Box<dyn Future<Output = Result<(i64, i64, HashMap<String, i64>)>> + Send + '_>> { Box::pin(async { Ok((0, 0, HashMap::new())) }) }
    fn execute_cypher(&self, _sql: &str, _params: &[Value]) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
}

impl Database for FakeDb {
    fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> { Box::pin(async { Ok(()) }) }
    fn close(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> { Box::pin(async { Ok(()) }) }
    fn ensure_schema(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> { Box::pin(async { Ok(()) }) }
    fn add_memory(&self, _req_json: serde_json::Value, _config: &serde_json::Value) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> { Box::pin(async { Ok(json!({})) }) }
    fn get_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>>> + Send + '_>> {
        let id = id.to_string();
        Box::pin(async move {
            Ok(match id.as_str() {
                "mem_1" => Some(Self::memory_one()),
                "mem_2" => Some(Self::memory_two()),
                _ => None,
            })
        })
    }
    fn list_memories(&self, _limit: Option<usize>) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> { Box::pin(async { Ok(vec![Self::memory_one(), Self::memory_two()]) }) }
    fn delete_memory(&self, _id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> { Box::pin(async { Ok(true) }) }
    fn update_memory(&self, _id: &str, _content: &str) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> { Box::pin(async { Ok(()) }) }
    fn resolve_memory_id(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<voidm_db::ResolveResult>> + Send + '_>> { let id = id.to_string(); Box::pin(async move { Ok(voidm_db::ResolveResult::Single(id)) }) }
    fn list_scopes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn link_memories(&self, _from_id: &str, _rel: &str, _to_id: &str, _note: Option<&str>) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> { Box::pin(async { Ok(json!({})) }) }
    fn unlink_memories(&self, _from_id: &str, _rel: &str, _to_id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> { Box::pin(async { Ok(true) }) }
    fn list_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn search_hybrid(&self, _opts_json: serde_json::Value, _model_name: &str, _embeddings_enabled: bool, _config_min_score: f32, _config_search: &serde_json::Value) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> { Box::pin(async { Ok(json!({})) }) }
    fn search_bm25(&self, _query: &str, _scope_filter: Option<&str>, _type_filter: Option<&str>, _limit: usize) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> { Box::pin(async { Ok(vec![("mem_2".to_string(), 0.4)]) }) }
    fn search_title_bm25(&self, _query: &str, _scope_filter: Option<&str>, _type_filter: Option<&str>, _limit: usize) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> { Box::pin(async { Ok(vec![("mem_1".to_string(), 1.0)]) }) }
    fn search_fuzzy(&self, _query: &str, _scope_filter: Option<&str>, _limit: usize, _threshold: f32) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn search_ann(&self, _embedding: Vec<f32>, _limit: usize, _scope_filter: Option<&str>, _type_filter: Option<&str>) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn search_chunk_ann(&self, _embedding: Vec<f32>, _limit: usize, _scope_filter: Option<&str>, _type_filter: Option<&str>) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> { Box::pin(async { Ok(vec![("mchk_1".to_string(), 0.95), ("mchk_2".to_string(), 0.85)]) }) }
    fn fetch_memories_raw(&self, _scope_filter: Option<&str>, _type_filter: Option<&str>, _limit: usize) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String)>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn fetch_memories_for_chunking(&self, _limit: usize) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String, String)>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn query_cypher(&self, _query: &str, _params: &serde_json::Value) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> { Box::pin(async { Ok(json!([])) }) }
    fn get_neighbors(&self, _id: &str, _depth: usize) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> { Box::pin(async { Ok(json!([])) }) }
    fn get_statistics(&self) -> Pin<Box<dyn Future<Output = Result<DatabaseStats>> + Send + '_>> { Box::pin(async { Ok(DatabaseStats { total_memories: 0, memories_by_type: vec![], scopes_count: 0, top_tags: vec![], embedding_coverage: EmbeddingStats { total_embeddings: 0, total_memories: 0, coverage_percentage: 0.0 }, graph: GraphStats { node_count: 0, edge_count: 0, edges_by_type: vec![] }, db_size_bytes: 0 }) }) }
    fn get_graph_stats(&self) -> Pin<Box<dyn Future<Output = Result<GraphStats>> + Send + '_>> { Box::pin(async { Ok(GraphStats { node_count: 0, edge_count: 0, edges_by_type: vec![] }) }) }
    fn get_graph_export_data(&self) -> Pin<Box<dyn Future<Output = Result<GraphExportData>> + Send + '_>> { Box::pin(async { Ok(GraphExportData { memories: vec![], concepts: vec![], nodes: vec![], edges: vec![] }) }) }
    fn check_model_mismatch(&self, _configured_model: &str) -> Pin<Box<dyn Future<Output = Result<Option<(String, String)>>> + Send + '_>> { Box::pin(async { Ok(None) }) }
    fn delete_chunks_for_memory(&self, _memory_id: &str) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>> { Box::pin(async { Ok(0) }) }
    fn fetch_chunks(&self, _limit: usize) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String, String)>>> + Send + '_>> {
        Box::pin(async {
            Ok(vec![
                ("mchk_1".to_string(), "Most relevant chunk for database tuning".to_string(), "mem_1".to_string()),
                ("mchk_2".to_string(), "Second supporting chunk for database tuning".to_string(), "mem_1".to_string()),
                ("mchk_3".to_string(), "Rust chunk".to_string(), "mem_2".to_string()),
            ])
        })
    }
    fn upsert_chunk(&self, _chunk_id: &str, _memory_id: &str, _content: &str, _index: usize, _size: usize, _created_at: &str) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> { Box::pin(async { Ok(()) }) }

    fn store_chunk_embedding(&self, _chunk_id: String, _memory_id: String, _embedding: Vec<f32>) -> Pin<Box<dyn Future<Output = Result<(String, usize)>> + Send + '_>> { Box::pin(async { Ok(("x".to_string(), 0)) }) }
    fn get_chunk_embedding(&self, _chunk_id: &str) -> Pin<Box<dyn Future<Output = Result<Option<Vec<f32>>>> + Send + '_>> { Box::pin(async { Ok(None) }) }
    fn search_by_embedding(&self, _query_embedding: Vec<f32>, _limit: usize, _min_similarity: f32) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn export_to_jsonl(&self, _limit: Option<usize>) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn import_from_jsonl(&self, _records: Vec<String>) -> Pin<Box<dyn Future<Output = Result<(usize, usize, usize)>> + Send + '_>> { Box::pin(async { Ok((0, 0, 0)) }) }
    fn list_tags(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn create_tag(&self, _name: &str) -> Pin<Box<dyn Future<Output = Result<(String, bool)>> + Send + '_>> { Box::pin(async { Ok(("tag".to_string(), true)) }) }
    fn link_tag_to_memory(&self, _tag_id: &str, _memory_id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> { Box::pin(async { Ok(true) }) }
    fn list_tag_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn list_chunks(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn get_chunk(&self, chunk_id: &str) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>>> + Send + '_>> {
        let chunk_id = chunk_id.to_string();
        Box::pin(async move {
            Ok(match chunk_id.as_str() {
                "mchk_1" => Some(json!({"id": "mchk_1", "memory_id": "mem_1", "content": "Most relevant chunk for database tuning"})),
                "mchk_2" => Some(json!({"id": "mchk_2", "memory_id": "mem_1", "content": "Second supporting chunk for database tuning"})),
                "mchk_3" => Some(json!({"id": "mchk_3", "memory_id": "mem_2", "content": "Rust chunk"})),
                _ => None,
            })
        })
    }
    fn list_chunk_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn list_entities(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn get_or_create_entity(&self, _name: &str, _entity_type: &str) -> Pin<Box<dyn Future<Output = Result<(String, bool)>> + Send + '_>> { Box::pin(async { Ok(("entity".to_string(), true)) }) }
    fn link_chunk_to_entity(&self, _chunk_id: &str, _entity_id: &str, _confidence: f32) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> { Box::pin(async { Ok(true) }) }
    fn list_entity_mention_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn count_nodes(&self, _node_type: &str) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>> { Box::pin(async { Ok(0) }) }
    fn count_edges(&self, _edge_type: Option<&str>) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>> { Box::pin(async { Ok(0) }) }
    fn create_node(&self, _id: &str, _node_type: &str, _properties: serde_json::Value) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> { Box::pin(async { Ok(()) }) }
    fn get_node(&self, _id: &str) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>>> + Send + '_>> { Box::pin(async { Ok(None) }) }
    fn delete_node(&self, _id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> { Box::pin(async { Ok(true) }) }
    fn list_nodes(&self, _node_type: &str) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn create_edge(&self, _from_id: &str, _edge_type: &str, _to_id: &str, _properties: Option<serde_json::Value>) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> { Box::pin(async { Ok(()) }) }
    fn get_edge(&self, _from_id: &str, _edge_type: &str, _to_id: &str) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>>> + Send + '_>> { Box::pin(async { Ok(None) }) }
    fn delete_edge(&self, _from_id: &str, _edge_type: &str, _to_id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> { Box::pin(async { Ok(true) }) }
    fn get_node_edges(&self, _node_id: &str, _edge_type: Option<&str>) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> { Box::pin(async { Ok(vec![]) }) }
    fn graph_ops(&self) -> Arc<dyn GraphQueryOps> { Arc::new(FakeGraphOps) }
}

#[tokio::test]
async fn hybrid_search_groups_chunks_back_to_memory_and_uses_bounded_context() {
    let db = FakeDb;
    let opts = SearchOptions {
        query: "database tuning".to_string(),
        mode: SearchMode::Rrf,
        limit: 5,
        scope_filter: None,
        type_filter: None,
        tag_filter: None,
        min_score: None,
        min_quality: None,
        include_neighbors: false,
        neighbor_depth: None,
        neighbor_decay: None,
        neighbor_min_score: None,
        neighbor_limit: None,
        edge_types: None,
        intent: None,
    };

    let response = search(&db, &opts, "disabled", false, 0.0, &SearchConfig::default())
        .await
        .expect("search response");

    assert!(!response.results.is_empty());
    assert_eq!(response.results[0].id, "mem_1");
    assert_eq!(response.results[0].title.as_deref(), Some("Database tuning"));
    assert_eq!(response.results[0].context_chunks.len(), 2);
    assert!(response.results[0].content.contains("Most relevant chunk for database tuning"));
    assert!(response.results[0].content.contains("Second supporting chunk for database tuning"));
    assert!(response.results[0].content.len() <= voidm_core::memory_policy::RETRIEVAL_TOTAL_CHAR_BUDGET_PER_MEMORY + 2);
}

#[tokio::test]
async fn hybrid_search_applies_type_filter_and_type_relevance() {
    let db = FakeDb;
    let opts = SearchOptions {
        query: "semantic database".to_string(),
        mode: SearchMode::Rrf,
        limit: 5,
        scope_filter: None,
        type_filter: Some("semantic".to_string()),
        tag_filter: None,
        min_score: None,
        min_quality: None,
        include_neighbors: false,
        neighbor_depth: None,
        neighbor_decay: None,
        neighbor_min_score: None,
        neighbor_limit: None,
        edge_types: None,
        intent: Some("semantic retrieval".to_string()),
    };

    let response = search(&db, &opts, "disabled", false, 0.0, &SearchConfig::default())
        .await
        .expect("search response");

    assert!(!response.results.is_empty());
    assert!(response.results.iter().all(|r| r.memory_type == "semantic"));
    assert_eq!(response.results[0].id, "mem_1");
    assert!(response.results[0].score > response.results.last().unwrap().score || response.results.len() == 1);
}
