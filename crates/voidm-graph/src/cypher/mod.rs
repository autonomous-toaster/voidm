pub mod lexer;
pub mod ast;
pub mod parser;
pub mod translator;

use anyhow::{bail, Result};
use voidm_db::graph_ops::GraphQueryOps;
use std::collections::HashMap;

pub use ast::CypherAst;

/// Execute a read-only Cypher query. Rejects write clauses before parsing.
pub async fn execute_read(
    ops: &dyn GraphQueryOps,
    query: &str,
) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    // Step 1: Strip comments
    let stripped = lexer::strip_comments(query);

    // Step 2: Reject write clauses (token-level, not substring)
    reject_write_clauses(&stripped)?;

    // Step 3: Parse — wrap errors with usage hint
    let ast = parser::parse(&stripped).map_err(|e| {
        anyhow::anyhow!(
            "Cypher parse error: {}\n\
             Supported syntax:\n\
             \x20 MATCH (a:Memory)-[:SUPPORTS]->(b:Memory) RETURN a.memory_id, b.memory_id LIMIT 10\n\
             \x20 MATCH (a)-[:RELATES_TO]-(b) WHERE a.memory_id = '<id>' RETURN b.memory_id\n\
             \x20 MATCH (a)-[*1..3]->(b) RETURN a.memory_id, b.memory_id\n\
             Clauses: MATCH, WHERE, RETURN, ORDER BY, LIMIT, WITH\n\
             Write operations (CREATE, MERGE, SET, DELETE, REMOVE, DROP) are not allowed.",
            e
        )
    })?;

    // Step 4: Translate to SQL
    let (sql, params) = translator::translate(&ast).map_err(|e| {
        anyhow::anyhow!("Cypher translation error: {}", e)
    })?;

    // Step 5: Execute using trait
    let rows = ops.execute_cypher(&sql, &params).await?;
    Ok(rows)
}

const WRITE_KEYWORDS: &[&str] = &[
    "CREATE", "MERGE", "SET", "DELETE", "REMOVE", "DROP",
];

fn reject_write_clauses(query: &str) -> Result<()> {
    let tokens = lexer::tokenize(query);
    for token in &tokens {
        if let lexer::Token::Keyword(kw) = token {
            let upper = kw.to_uppercase();
            if WRITE_KEYWORDS.contains(&upper.as_str()) {
                bail!(
                    "'{}' is a write operation and is not allowed via 'voidm graph cypher'.\n\
                     Use 'voidm link' / 'voidm unlink' to modify the graph.\n\
                     Allowed clauses: MATCH, WHERE, RETURN, ORDER BY, LIMIT, WITH.",
                    upper
                );
            }
        }
    }
    Ok(())
}
