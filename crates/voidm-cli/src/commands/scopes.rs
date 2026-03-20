use anyhow::Result;
use clap::Subcommand;
use sqlx::SqlitePool;
use std::sync::Arc;
use voidm_core::{crud, crud_trait};

#[derive(Subcommand)]
pub enum ScopesCommands {
    /// List all known scopes
    List,
}

pub async fn run(cmd: ScopesCommands, db: &std::sync::Arc<dyn voidm_db_trait::Database>, pool: &sqlx::SqlitePool, json: bool) -> Result<()> {
    match cmd {
        ScopesCommands::List => {
            let scopes = crud_trait::list_scopes(db).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&scopes)?);
            } else {
                if scopes.is_empty() {
                    println!("No scopes found.");
                } else {
                    for s in &scopes {
                        println!("{}", s);
                    }
                }
            }
        }
    }
    Ok(())
}
