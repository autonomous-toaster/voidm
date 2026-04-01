use anyhow::{Context, Result};
use std::env;
use voidm_core::Config;
use voidm_db::Database;
use voidm_neo4j::Neo4jDatabase;

fn neo4j_test_config() -> Option<(String, String, String, String)> {
    let password = env::var("VOIDM_NEO4J_PASSWORD").ok()?;
    let uri = env::var("VOIDM_NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".to_string());
    let username = env::var("VOIDM_NEO4J_USERNAME").unwrap_or_else(|_| "neo4j".to_string());
    let database = env::var("VOIDM_NEO4J_DATABASE").unwrap_or_else(|_| "voidmdev".to_string());
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

#[tokio::test]
async fn neo4j_add_memory_persists_first_class_scopes_and_entities() -> Result<()> {
    let Some(db) = connect_test_db().await? else {
        return Ok(());
    };

    let cleanup = db.graph.execute_on(
        &db.database,
        neo4rs::query(
            "MATCH (n)
             WHERE (n:Memory AND n.id = 'mem_scope_entity_1')
                OR (n:Scope AND n.name IN ['work/ops', 'work/ops/db'])
                OR (n:Entity AND n.name IN ['Alice', 'PostgreSQL'])
             DETACH DELETE n
             RETURN count(n) as deleted"
        ),
    ).await?;
    let mut cleanup = cleanup;
    while let Ok(Some(_)) = cleanup.next().await {}

    let req = serde_json::json!({
        "content": "Alice tuned PostgreSQL query plans for the ops team.",
        "memory_type": "semantic",
        "scopes": ["work/ops", "work/ops/db"],
        "tags": [],
        "importance": 5,
        "metadata": {},
        "links": [],
        "context": null,
        "title": "Ops database tuning"
    });

    let mut cfg = Config::default();
    cfg.embeddings.enabled = false;
    let cfg = serde_json::to_value(cfg)?;

    let resp = db.add_memory(req, &cfg).await?;
    let memory_id = resp["id"].as_str().expect("memory id").to_string();

    let mem = db.get_memory(&memory_id).await?.expect("memory exists");
    let scopes = mem["scopes"].as_array().cloned().unwrap_or_default();
    assert!(scopes.iter().any(|s| s.as_str() == Some("work/ops")));
    assert!(scopes.iter().any(|s| s.as_str() == Some("work/ops/db")));

    let mut scope_check = db.graph.execute_on(
        &db.database,
        neo4rs::query(
            "MATCH (:Memory {id: $id})-[:HAS_SCOPE]->(s:Scope)
             RETURN count(DISTINCT s) as c"
        )
        .param("id", memory_id.clone()),
    ).await?;
    let row = scope_check.next().await?.expect("scope row");
    let scope_count: i64 = row.get("c")?;
    assert_eq!(scope_count, 2);

    #[cfg(feature = "ner")]
    {
        let mut entity_check = db.graph.execute_on(
            &db.database,
            neo4rs::query(
                "MATCH (:Memory {id: $id})-[r:MENTIONS]->(e:Entity)
                 RETURN collect(DISTINCT e.name) as entity_names, count(r) as mentions"
            )
            .param("id", memory_id.clone()),
        ).await?;
        let row = entity_check.next().await?.expect("entity row");
        let entity_names: Vec<String> = row.get("entity_names")?;
        let mentions: i64 = row.get("mentions")?;
        assert!(entity_names.iter().any(|n| n == "Alice") || entity_names.iter().any(|n| n == "PostgreSQL"));
        assert!(mentions >= 1);
    }

    Ok(())
}

#[cfg(feature = "tinyllama")]
#[tokio::test]
async fn neo4j_add_memory_generates_and_persists_auto_tags() -> Result<()> {
    let Some(db) = connect_test_db().await? else {
        return Ok(());
    };

    let cleanup = db.graph.execute_on(
        &db.database,
        neo4rs::query(
            "MATCH (n)
             WHERE (n:Memory AND n.id = 'mem_auto_tag_neo4j_1')
                OR (n:Tag AND n.name IN ['docker', 'kubernetes', 'containers'])
             DETACH DELETE n
             RETURN count(n) as deleted"
        ),
    ).await?;
    let mut cleanup = cleanup;
    while let Ok(Some(_)) = cleanup.next().await {}

    let mut cfg = Config::default();
    cfg.embeddings.enabled = false;
    cfg.enrichment.auto_tagging.enabled = true;
    cfg.enrichment.auto_tagging.model = "tinyllama".to_string();
    cfg.enrichment.auto_tagging.max_tags = 5;
    let cfg_json = serde_json::to_value(&cfg)?;

    let generated = voidm_core::auto_tagging::generate_tags(
        "Docker orchestration for Kubernetes deployment pipelines and container operations.",
        &cfg,
    )
    .await?;
    assert!(!generated.is_empty(), "expected TinyLLaMA strict generator to return tags");

    let req = serde_json::json!({
        "id": "mem_auto_tag_neo4j_1",
        "content": "Docker orchestration for Kubernetes deployment pipelines and container operations.",
        "memory_type": "semantic",
        "scopes": [],
        "tags": [],
        "importance": 5,
        "metadata": {},
        "links": [],
        "context": null,
        "title": "Container platform note"
    });

    let resp = db.add_memory(req, &cfg_json).await?;
    assert_eq!(resp["id"].as_str(), Some("mem_auto_tag_neo4j_1"));
    let tags = resp["tags"].as_array().cloned().unwrap_or_default();
    assert!(!tags.is_empty(), "expected generated tags in add response");

    let mem = db.get_memory("mem_auto_tag_neo4j_1").await?.expect("memory exists");
    let persisted_tags = mem["tags"].as_array().cloned().unwrap_or_default();
    assert!(!persisted_tags.is_empty(), "expected persisted tags");

    let metadata = mem["metadata"].as_object().cloned().unwrap_or_default();
    let auto_generated = metadata
        .get("auto_generated_tags")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!auto_generated.is_empty(), "expected metadata.auto_generated_tags");

    let mut tag_check = db.graph.execute_on(
        &db.database,
        neo4rs::query(
            "MATCH (:Memory {id: 'mem_auto_tag_neo4j_1'})-[:HAS_TAG]->(t:Tag)
             RETURN count(DISTINCT t) as c"
        ),
    ).await?;
    let row = tag_check.next().await?.expect("tag row");
    let tag_count: i64 = row.get("c")?;
    assert!(tag_count >= 1, "expected canonical HAS_TAG edges in Neo4j");

    Ok(())
}
