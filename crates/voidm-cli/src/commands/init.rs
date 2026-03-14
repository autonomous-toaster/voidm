use clap::Parser;

#[derive(Parser, Debug, Clone)]
pub struct InitArgs {}

pub async fn run(_args: InitArgs) -> anyhow::Result<()> {
    use voidm_core::{embeddings, Config, ner, nli, query_expansion};

    println!("Initializing voidm models...\n");

    let config = Config::load();
    let embedding_model = &config.embeddings.model;
    
    // Get default query expansion model from config
    let qe_config = &config.search.query_expansion;
    let qe_model = qe_config
        .as_ref()
        .map(|qe| qe.model.clone())
        .unwrap_or_else(|| "tinyllama".to_string());
    let qe_enabled = qe_config
        .as_ref()
        .map(|qe| qe.enabled)
        .unwrap_or(true);

    let total = 4; // 1 embedding + NER + NLI + query expansion
    let mut initialized = 0;

    // Initialize configured embedding model
    print!("[1/4] Initializing embedding model: {} ... ", embedding_model);
    std::io::Write::flush(&mut std::io::stdout())?;
    match embeddings::get_embedder(embedding_model) {
        Ok(_) => {
            println!("✓");
            initialized += 1;
        }
        Err(e) => {
            eprintln!("✗ FAILED: {}", e);
            return Err(e);
        }
    }

    // Initialize NER model
    print!("[2/4] Initializing NER model ... ");
    std::io::Write::flush(&mut std::io::stdout())?;
    match ner::ensure_ner_model().await {
        Ok(_) => {
            println!("✓");
            initialized += 1;
        }
        Err(e) => {
            eprintln!("✗ FAILED: {}", e);
            return Err(e);
        }
    }

    // Initialize NLI model
    print!("[3/4] Initializing NLI model ... ");
    std::io::Write::flush(&mut std::io::stdout())?;
    match nli::ensure_nli_model().await {
        Ok(_) => {
            println!("✓");
            initialized += 1;
        }
        Err(e) => {
            eprintln!("✗ FAILED: {}", e);
            return Err(e);
        }
    }

    // Initialize default query expansion model (only if enabled)
    if qe_enabled {
        print!("[4/4] Initializing query expansion model: {} ... ", qe_model);
        std::io::Write::flush(&mut std::io::stdout())?;
        match query_expansion::ensure_llm_model(&qe_model).await {
            Ok(_) => {
                println!("✓");
                initialized += 1;
            }
            Err(e) => {
                eprintln!("✗ FAILED: {}", e);
                return Err(e);
            }
        }
    } else {
        println!("[4/4] Skipping query expansion model (disabled in config)");
        initialized += 1;
    }

    println!("\n✓ Initialization complete!");
    println!("  Initialized: {}/{} models", initialized, total);

    Ok(())
}
