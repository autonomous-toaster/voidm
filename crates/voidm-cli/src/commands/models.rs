use anyhow::Result;
use clap::Subcommand;
use voidm_core::embeddings;

#[derive(Subcommand)]
pub enum ModelsCommands {
    /// List available embedding models
    List,
    /// Download a model
    Download {
        /// Model name
        model: String,
    },
}

pub fn run_list(json: bool) -> Result<()> {
    let models = embeddings::list_models();
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "count": models.len(),
            "results": models,
        }))?);
    } else {
        println!("{:<35} {:>6}  {}", "Model", "Dims", "Description");
        println!("{}", "-".repeat(70));
        for m in &models {
            println!("{:<35} {:>6}  {}", m.name, m.dims, m.description);
        }
    }
    Ok(())
}

pub async fn run(cmd: ModelsCommands, json: bool) -> Result<()> {
    match cmd {
        ModelsCommands::List => run_list(json),
        ModelsCommands::Download { model } => {
            eprintln!("Downloading model '{}'...", model);
            // fastembed downloads automatically on first use
            let _ = embeddings::embed_text(&model, "warmup")?;
            if json {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                    "result": {
                        "downloaded": true,
                        "model": model,
                    }
                }))?);
            } else {
                eprintln!("Model '{}' ready.", model);
            }
            Ok(())
        }
    }
}

