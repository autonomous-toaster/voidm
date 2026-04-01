use clap::Parser;

#[derive(Parser, Debug, Clone)]
pub struct InitArgs {
    /// Force re-download of models even if already cached
    #[arg(long)]
    pub update: bool,
}

pub async fn run(args: InitArgs) -> anyhow::Result<()> {
    use voidm_core::{embeddings, Config};
    #[cfg(feature = "reranker")]
    use voidm_core::reranker;
    #[cfg(feature = "ner")]
    use voidm_core::ner;
    use voidm_nli as nli;
    #[cfg(feature = "query-expansion")]
    use voidm_core::query_expansion;

    println!("Initializing voidm models...\n");
    if args.update {
        println!("⚠️  Update mode: Will re-download all models\n");
    }

    let config = Config::load();
    let embedding_model = &config.embeddings.model;
    
    // Get default query expansion model from config
    let qe_config = &config.search.query_expansion;
    let qe_model = qe_config
        .as_ref()
        .map(|qe| qe.model.clone())
        .unwrap_or_else(|| "tinyllama".to_string());
    let qe_backend = qe_config
        .as_ref()
        .map(|qe| qe.backend.clone())
        .unwrap_or_default();
    let qe_enabled = qe_config
        .as_ref()
        .map(|qe| qe.enabled)
        .unwrap_or(true);

    // Get reranker model from config
    #[cfg(feature = "reranker")]
    let (reranker_model, reranker_enabled) = {
        let reranker_config = &config.search.reranker;
        let model = reranker_config
            .as_ref()
            .map(|r| r.model.clone())
            .unwrap_or_else(|| "ms-marco-TinyBERT".to_string());
        let enabled = reranker_config
            .as_ref()
            .map(|r| r.enabled)
            .unwrap_or(false);
        (model, enabled)
    };
    
    #[cfg(not(feature = "reranker"))]
    let reranker_enabled = false;

    let total = 5; // 1 embedding + NER + NLI + query expansion + reranker
    let mut initialized = 0;

    // Initialize configured embedding model
    print!("[1/5] Initializing embedding model: {} ... ", embedding_model);
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
    #[cfg(feature = "ner")]
    {
        print!("[2/5] Initializing NER model ... ");
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
    }
    #[cfg(not(feature = "ner"))]
    {
        println!("[2/5] Skipping NER model (feature not enabled)");
        initialized += 1;
    }

    // Initialize NLI model
    #[cfg(feature = "nli")]
    {
        print!("[3/5] Initializing NLI model ... ");
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
    }
    #[cfg(not(feature = "nli"))]
    {
        println!("[3/5] Skipping NLI model (feature not enabled)");
        initialized += 1;
    }

    // Initialize default query expansion model (only if enabled)
    #[cfg(feature = "query-expansion")]
    {
        if qe_enabled {
            print!("[4/5] Initializing query expansion model: {} ({:?}) ... ", qe_model, qe_backend);
            std::io::Write::flush(&mut std::io::stdout())?;
            match query_expansion::LocalGenerator::new(query_expansion::QueryExpansionConfig {
                enabled: true,
                model: qe_model.clone(),
                backend: qe_backend,
                timeout_ms: qe_config.as_ref().map(|qe| qe.timeout_ms).unwrap_or(10_000),
                intent: query_expansion::IntentConfig::default(),
            }).ensure_model().await {
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
            println!("[4/5] Skipping query expansion model (disabled in config)");
            initialized += 1;
        }
    }
    #[cfg(not(feature = "query-expansion"))]
    {
        println!("[4/5] Skipping query expansion model (feature not enabled)");
        initialized += 1;
    }

    // Initialize reranker model (only if enabled)
    #[cfg(feature = "reranker")]
    {
        if reranker_enabled {
            print!("[5/5] Initializing reranker model: {} ... ", reranker_model);
            std::io::Write::flush(&mut std::io::stdout())?;
            
            // Check if already cached (unless update flag is set)
            let is_cached = reranker::is_model_cached(&reranker_model);
            if is_cached && !args.update {
                println!("✓ (cached)");
                initialized += 1;
            } else {
                if args.update && is_cached {
                    println!("(updating)");
                }
                match reranker::load_reranker_cached(&reranker_model, args.update).await {
                    Ok(_) => {
                        println!("✓");
                        initialized += 1;
                    }
                    Err(e) => {
                        eprintln!("✗ FAILED: {}", e);
                        return Err(e);
                    }
                }
            }
        } else {
            println!("[5/5] Skipping reranker model (disabled in config)");
            initialized += 1;
        }
    }
    #[cfg(not(feature = "reranker"))]
    {
        println!("[5/5] Skipping reranker model (feature not enabled)");
        initialized += 1;
    }

    println!("\n✓ Initialization complete!");
    println!("  Initialized: {}/{} models", initialized, total);

    Ok(())
}
