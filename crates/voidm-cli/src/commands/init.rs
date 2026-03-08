use clap::Parser;

#[derive(Parser, Debug, Clone)]
pub struct InitArgs {}

pub async fn run(_args: InitArgs) -> anyhow::Result<()> {
    use voidm_core::{embeddings, ner, nli};

    println!("Initializing voidm models...\n");

    // List of embedding models to initialize
    let embedding_models = vec![
        "Xenova/all-MiniLM-L6-v2",  // Default
        "BAAI/bge-small-en-v1.5",
        "BAAI/bge-base-en-v1.5",
        "BAAI/bge-large-en-v1.5",
        "nomic-embed-text-v1.5",
        "mxbai-embed-large-v1",
        "multilingual-e5-base",
    ];

    let total = embedding_models.len() + 2; // embeddings + NER + NLI
    let mut initialized = 0;

    // Initialize embedding models
    for (idx, model_name) in embedding_models.iter().enumerate() {
        let progress = format!("[{}/{}]", idx + 1, total);
        print!("{} Initializing embedding model: {} ... ", progress, model_name);
        std::io::Write::flush(&mut std::io::stdout())?;

        match embeddings::get_embedder(model_name) {
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

    // Initialize NER model
    let ner_idx = embedding_models.len() + 1;
    print!("[{}/{}] Initializing NER model ... ", ner_idx, total);
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
    let nli_idx = total;
    print!("[{}/{}] Initializing NLI model ... ", nli_idx, total);
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

    println!("\n✓ Initialization complete!");
    println!("  Initialized: {}/{} models", initialized, total);

    Ok(())
}
