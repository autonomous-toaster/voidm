use anyhow::{Context, Result};
use neo4rs::query;
use std::env;
use voidm_core::config::SearchConfig;
use voidm_core::search::{search, SearchMode, SearchOptions};
use voidm_core::vector_format::f32_to_base64;
use voidm_db::Database;
use voidm_neo4j::Neo4jDatabase;

fn neo4j_test_config() -> Option<(String, String, String, String)> {
    let password = env::var("VOIDM_NEO4J_PASSWORD").ok()?;
    let uri = env::var("VOIDM_NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".to_string());
    let username = env::var("VOIDM_NEO4J_USERNAME").unwrap_or_else(|_| "neo4j".to_string());
    let database = env::var("VOIDM_NEO4J_DATABASE").unwrap_or_else(|_| "neo4j".to_string());
    Some((uri, username, password, database))
}

async fn connect_test_db() -> Result<Option<Neo4jDatabase>> {
    let Some((uri, username, password, database)) = neo4j_test_config() else {
        eprintln!("skipping Neo4j integration test: VOIDM_NEO4J_PASSWORD not set");
        return Ok(None);
    };

    let db = Neo4jDatabase::connect(&uri, &username, &password, &database)
        .await
        .with_context(|| "failed to connect to Neo4j test database")?;
    Ok(Some(db))
}

async fn reset_search_test_data(db: &Neo4jDatabase) -> Result<()> {
    let graph = db.graph.clone();
    let database = db.database.clone();

    let mut stream = graph
        .execute_on(&database, query(
            "MATCH (n)
             WHERE (n:Memory AND n.id IN ['mem1', 'mem2'])
                OR (n:MemoryChunk AND n.id IN ['mchk_1', 'mchk_2', 'mchk_3'])
                OR (n:MemoryType AND n.name = 'semantic')
                OR (n:Scope AND n.name = 'test_search_integration')
                OR (n:Entity AND n.name IN ['Database tuning', 'Rust'])
             DETACH DELETE n
             RETURN count(n) as deleted"
        ))
        .await
        .with_context(|| "failed to clear Neo4j search test data")?;
    while let Ok(Some(_)) = stream.next().await {}

    let mut stream = graph
        .execute_on(&database, query(
            "CREATE (m1:Memory {id: 'mem1', title: 'Database tuning', content: 'Primary memory body that should not be returned whole', importance: 5, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z', tags: [], metadata: '{}'}),
                    (m2:Memory {id: 'mem2', title: 'Rust ownership', content: 'Secondary memory body', importance: 5, created_at: '2026-01-02T00:00:00Z', updated_at: '2026-01-02T00:00:00Z', tags: [], metadata: '{}'}),
                    (mt:MemoryType {name: 'semantic'}),
                    (sc:Scope {name: 'test_search_integration'}),
                    (c1:MemoryChunk {id: 'mchk_1', text: 'Most relevant chunk for database tuning', embedding: $emb1, embedding_dim: 2}),
                    (c2:MemoryChunk {id: 'mchk_2', text: 'Second supporting chunk for database tuning', embedding: $emb2, embedding_dim: 2}),
                    (c3:MemoryChunk {id: 'mchk_3', text: 'Rust chunk', embedding: $emb3, embedding_dim: 2}),
                    (m1)-[:HAS_TYPE]->(mt),
                    (m2)-[:HAS_TYPE]->(mt),
                    (m1)-[:HAS_SCOPE]->(sc),
                    (m2)-[:HAS_SCOPE]->(sc),
                    (m1)-[:HAS_CHUNK]->(c1),
                    (m1)-[:HAS_CHUNK]->(c2),
                    (m2)-[:HAS_CHUNK]->(c3)
             RETURN m1.id as id"
        )
        .param("emb1", f32_to_base64(&[1.0f32, 0.0f32]))
        .param("emb2", f32_to_base64(&[0.9f32, 0.1f32]))
        .param("emb3", f32_to_base64(&[0.0f32, 1.0f32]))
        )
        .await
        .with_context(|| "failed to seed Neo4j search test data")?;
    while let Ok(Some(_)) = stream.next().await {}

    Ok(())
}

#[tokio::test]
async fn neo4j_title_search_returns_title_matches() -> Result<()> {
    let Some(db) = connect_test_db().await? else {
        return Ok(());
    };
    reset_search_test_data(&db).await?;

    let results = db.search_title_bm25("database", Some("test_search_integration"), None, 10).await?;
    assert!(!results.is_empty());
    assert_eq!(results[0].0, "mem1");
    Ok(())
}

#[tokio::test]
async fn neo4j_chunk_ann_returns_best_matching_chunk() -> Result<()> {
    let Some(db) = connect_test_db().await? else {
        return Ok(());
    };
    reset_search_test_data(&db).await?;

    let results = db.search_chunk_ann(vec![1.0, 0.0], 10, Some("test_search_integration"), None).await?;
    assert!(!results.is_empty());
    assert_eq!(results[0].0, "mchk_1");
    assert!(results[0].1 >= results.last().unwrap().1);
    Ok(())
}

#[tokio::test]
async fn neo4j_hybrid_search_respects_type_filter() -> Result<()> {
    let Some(db) = connect_test_db().await? else {
        return Ok(());
    };
    reset_search_test_data(&db).await?;

    let opts = SearchOptions {
        query: "Database".to_string(),
        mode: SearchMode::Rrf,
        limit: 5,
        scope_filter: Some("test_search_integration".to_string()),
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
        intent: Some("semantic".to_string()),
    };

    let mut config = SearchConfig::default();
    config.signals.vector = false;
    config.signals.bm25 = true;
    config.signals.fuzzy = false;
    config.graph_retrieval = None;
    config.metadata_ranking = None;

    let response = search(&db, &opts, "disabled", false, 0.0, &config).await?;
    assert!(!response.results.is_empty());
    assert!(response.results.iter().all(|r| r.memory_type == "semantic"));
    assert_eq!(response.results[0].id, "mem1");
    Ok(())
}

#[tokio::test]
async fn neo4j_list_memories_and_get_memory_roundtrip_for_seeded_data() -> Result<()> {
    let Some(db) = connect_test_db().await? else {
        return Ok(());
    };
    reset_search_test_data(&db).await?;

    let list = db.list_memories(Some(10_000)).await?;
    assert!(list.iter().any(|m| m.get("id").and_then(|v| v.as_str()) == Some("mem1")));

    let mem1 = db.get_memory("mem1").await?;
    assert!(mem1.is_some());
    let mem1 = mem1.unwrap();
    assert_eq!(mem1.get("title").and_then(|v| v.as_str()), Some("Database tuning"));
    Ok(())
}

#[tokio::test]
async fn neo4j_hybrid_search_groups_chunks_back_to_memory_and_returns_bounded_context() -> Result<()> {
    let Some(db) = connect_test_db().await? else {
        return Ok(());
    };
    reset_search_test_data(&db).await?;

    let opts = SearchOptions {
        query: "Database".to_string(),
        mode: SearchMode::Rrf,
        limit: 5,
        scope_filter: Some("test_search_integration".to_string()),
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

    let mut config = SearchConfig::default();
    config.signals.vector = false;
    config.signals.bm25 = true;
    config.signals.fuzzy = false;
    config.graph_retrieval = None;
    config.metadata_ranking = None;

    let title_results = db.search_title_bm25("database", Some("test_search_integration"), None, 10).await?;
    assert!(!title_results.is_empty());

    let response = search(&db, &opts, "disabled", false, 0.0, &config).await?;
    assert!(!response.results.is_empty());
    assert_eq!(response.results[0].id, "mem1");
    assert_eq!(response.results[0].title.as_deref(), Some("Database tuning"));
    assert!(!response.results[0].context_chunks.is_empty());
    assert!(response.results[0].content.contains("database tuning"));
    assert!(response.results[0].content.len() <= voidm_core::memory_policy::RETRIEVAL_TOTAL_CHAR_BUDGET_PER_MEMORY + 2);
    Ok(())
}
