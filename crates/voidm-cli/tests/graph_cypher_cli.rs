use anyhow::Result;
use std::env;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn neo4j_env() -> Option<(String, String, String, String)> {
    let password = env::var("VOIDM_NEO4J_PASSWORD").ok()?;
    let uri = env::var("VOIDM_NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".to_string());
    let username = env::var("VOIDM_NEO4J_USERNAME").unwrap_or_else(|_| "neo4j".to_string());
    let database = env::var("VOIDM_NEO4J_DATABASE").unwrap_or_else(|_| "neo4j".to_string());
    Some((uri, username, password, database))
}

#[test]
fn graph_cypher_cli_returns_json_rows_for_read_only_query() -> Result<()> {
    let Some((uri, username, password, database)) = neo4j_env() else {
        eprintln!("skipping CLI Neo4j integration test: VOIDM_NEO4J_PASSWORD not set");
        return Ok(());
    };

    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let config_path = env::temp_dir().join(format!("voidm-cli-neo4j-{unique}.toml"));
    let config_body = format!(
        "[database]\nbackend = \"neo4j\"\n\n[database.neo4j]\nuri = \"{uri}\"\nusername = \"{username}\"\npassword = \"{password}\"\ndatabase = \"{database}\"\n"
    );
    fs::write(&config_path, config_body)?;

    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "voidm-cli",
            "--",
            "--json",
            "graph",
            "cypher",
            "MATCH (m:Memory) RETURN m.id as id LIMIT 1",
        ])
        .env("VOIDM_CONFIG", &config_path)
        .output()?;

    assert!(output.status.success(), "stdout={} stderr={}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
    let rows = parsed.as_array().expect("json array");
    assert!(!rows.is_empty(), "expected at least one row, got {stdout}");
    assert!(rows.iter().all(|row| row.get("id").is_some()), "expected projected id field, got {stdout}");

    let _ = fs::remove_file(&config_path);
    Ok(())
}

#[test]
fn graph_cypher_cli_rejects_write_queries() -> Result<()> {
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "voidm-cli",
            "--",
            "graph",
            "cypher",
            "CREATE (n:Test)",
        ])
        .output()?;

    assert!(!output.status.success(), "write query should be rejected");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Only read-only Cypher is allowed") || stderr.contains("read-only Cypher"), "stderr={stderr}");

    Ok(())
}
