use serde_json::json;
use uuid::Uuid;
use voidm_db_trait::Database;
use voidm_postgres::PostgresDatabase;

#[tokio::test]
#[ignore]
async fn test_postgres_integration_basic_crud() {
    let db_url = "postgresql://postgres:postgres@localhost/postgres";
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .expect("Failed to connect");

    let db = PostgresDatabase { pool };
    db.ensure_schema().await.expect("Failed to ensure schema");

    // Add memory
    let memory_req = json!({
        "id": Uuid::new_v4().to_string(),
        "content": "Integration test",
        "memory_type": "semantic",
        "tags": ["test"],
        "scopes": ["dev"]
    });

    let config = json!({});
    let result = db.add_memory(memory_req, &config).await;
    assert!(result.is_ok(), "Failed to add memory");

    let mem_response = result.unwrap();
    let memory_id = mem_response.get("id").and_then(|v| v.as_str()).expect("No id");

    // Get memory
    let fetched = db.get_memory(memory_id).await.expect("Failed to get");
    assert!(fetched.is_some(), "Memory not found");

    // Delete memory
    let delete_result = db.delete_memory(memory_id).await;
    assert!(delete_result.is_ok(), "Failed to delete");

    println!("✅ Basic CRUD test passed");
}

#[tokio::test]
#[ignore]
async fn test_postgres_integration_search() {
    let db_url = "postgresql://postgres:postgres@localhost/postgres";
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .expect("Failed to connect");

    let db = PostgresDatabase { pool };
    db.ensure_schema().await.expect("Failed to ensure schema");

    // Add test memories
    let config = json!({});
    for i in 0..3 {
        let memory_req = json!({
            "id": Uuid::new_v4().to_string(),
            "content": format!("Rust programming language example {}", i),
            "memory_type": "semantic",
            "tags": ["rust"],
            "scopes": ["search-test"]
        });
        let result = db.add_memory(memory_req, &config).await;
        println!("Added memory: {:?}", result);
    }

    // Search for memories
    let results = db
        .search_bm25("rust", None, None, 10)
        .await
        .expect("BM25 search failed");

    println!("Search results: {:?}", results);
    assert!(!results.is_empty(), "Should find memories");
    println!("✅ Search test passed - found {} results", results.len());
}

#[tokio::test]
#[ignore]
async fn test_postgres_integration_edges() {
    let db_url = "postgresql://postgres:postgres@localhost/postgres";
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .expect("Failed to connect");

    let db = PostgresDatabase { pool };
    db.ensure_schema().await.expect("Failed to ensure schema");

    // Create memories
    let config = json!({});
    let id1_resp = db
        .add_memory(
            json!({
                "id": Uuid::new_v4().to_string(),
                "content": "Memory 1",
                "memory_type": "semantic",
                "tags": [],
                "scopes": []
            }),
            &config,
        )
        .await
        .expect("Failed to add memory 1");

    let id1 = id1_resp.get("id").and_then(|v| v.as_str()).expect("No id");

    let id2_resp = db
        .add_memory(
            json!({
                "id": Uuid::new_v4().to_string(),
                "content": "Memory 2",
                "memory_type": "semantic",
                "tags": [],
                "scopes": []
            }),
            &config,
        )
        .await
        .expect("Failed to add memory 2");

    let id2 = id2_resp.get("id").and_then(|v| v.as_str()).expect("No id");

    // Link memories
    let link_result = db.link_memories(id1, "RELATES_TO", id2, Some("Test link")).await;
    assert!(link_result.is_ok(), "Failed to link");

    // List edges
    let edges = db.list_edges().await.expect("Failed to list edges");
    assert!(!edges.is_empty(), "Should have edges");

    println!("✅ Edges test passed - found {} edges", edges.len());

    let _ = db.delete_memory(id1).await;
    let _ = db.delete_memory(id2).await;
}

#[tokio::test]
#[ignore]
async fn test_postgres_integration_concepts() {
    let db_url = "postgresql://postgres:postgres@localhost/postgres";
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .expect("Failed to connect");

    let db = PostgresDatabase { pool };
    db.ensure_schema().await.expect("Failed to ensure schema");

    // Add concept
    let concept_id = Uuid::new_v4().to_string();
    let result = db
        .add_concept("TestConcept", Some("Test description"), None, Some(&concept_id))
        .await;

    assert!(result.is_ok(), "Failed to add concept");

    // Get concept
    let fetched = db.get_concept(&concept_id).await.expect("Failed to get");
    assert_ne!(fetched, json!(null), "Concept should exist");

    // Delete concept
    let delete_result = db.delete_concept(&concept_id).await;
    assert!(delete_result.is_ok(), "Failed to delete");

    println!("✅ Concepts test passed");
}

#[tokio::test]
#[ignore]
async fn test_postgres_integration_list() {
    let db_url = "postgresql://postgres:postgres@localhost/postgres";
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .expect("Failed to connect");

    let db = PostgresDatabase { pool };
    db.ensure_schema().await.expect("Failed to ensure schema");

    // Add memories
    let config = json!({});
    let mut ids = Vec::new();
    for i in 0..3 {
        let id = Uuid::new_v4().to_string();
        ids.push(id.clone());
        let _ = db
            .add_memory(
                json!({
                    "id": id,
                    "content": format!("List test {}", i),
                    "memory_type": "semantic",
                    "tags": [],
                    "scopes": ["list-test"]
                }),
                &config,
            )
            .await;
    }

    // List memories
    let list = db.list_memories(Some(100)).await.expect("Failed to list");
    assert!(list.len() >= 3, "Should have at least 3 memories");

    println!("✅ List test passed - found {} memories", list.len());

    for id in ids {
        let _ = db.delete_memory(&id).await;
    }
}

#[tokio::test]
#[ignore]
async fn test_postgres_integration_statistics() {
    let db_url = "postgresql://postgres:postgres@localhost/postgres";
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .expect("Failed to connect");

    let db = PostgresDatabase { pool: pool.clone() };
    db.ensure_schema().await.expect("Failed to ensure schema");

    // Add some memories
    let config = json!({});
    for i in 0..3 {
        let _ = db
            .add_memory(
                json!({
                    "id": Uuid::new_v4().to_string(),
                    "content": format!("Stats test {}", i),
                    "memory_type": "semantic",
                    "tags": [],
                    "scopes": ["stats-test"]
                }),
                &config,
            )
            .await;
    }

    // Get stats
    let stats = voidm_postgres::get_stats(&pool).await.expect("Failed to get stats");
    assert!(stats.get("total_memories").is_some(), "Stats should have total_memories");

    println!("✅ Statistics test passed");
    println!("   Stats: {:?}", stats);
}
