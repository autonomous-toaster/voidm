use clap::Parser;

#[derive(Parser, Debug, Clone)]
pub struct InitArgs {}

pub async fn run(_args: InitArgs) -> anyhow::Result<()> {
    use voidm_core::{embeddings, Config, ner, nli};

    println!("Initializing voidm models...\n");

    let config = Config::load();
    let embedding_model = &config.embeddings.model;

    let total = 3; // 1 embedding + NER + NLI
    let mut initialized = 0;

    // Initialize configured embedding model
    print!("[1/3] Initializing embedding model: {} ... ", embedding_model);
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
    print!("[2/3] Initializing NER model ... ");
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
    print!("[3/3] Initializing NLI model ... ");
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
