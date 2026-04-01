use anyhow::Result;
use clap::Subcommand;
use voidm_core::crud_trait;
use voidm_db::Database;

#[derive(Subcommand)]
pub enum ScopesCommands {
    /// List all known scopes
    List,
}

pub async fn run(cmd: ScopesCommands, db: &std::sync::Arc<dyn Database>, json: bool) -> Result<()> {
    match cmd {
        ScopesCommands::List => {
            let scopes = crud_trait::list_scopes(db).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                    "count": scopes.len(),
                    "results": scopes,
                }))?);
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
