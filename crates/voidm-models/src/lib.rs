//! Unified ONNX model management and caching for voidm ML crates.
//!
//! Provides:
//! - Centralized model registry
//! - Automatic download from HuggingFace
//! - Local disk caching (~/.cache/voidm/models)
//! - Lazy initialization with once_cell
//! - Thread-safe concurrent access

use anyhow::{anyhow, Context, Result};
use hf_hub::api::tokio::ApiBuilder;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

// ─── Model Registry ────────────────────────────────────────────────────────

/// Central registry of all ONNX models used by voidm.
/// Add new models here to make them available to all ML crates.
pub mod registry {
    /// NLI (Natural Language Inference) model metadata
    pub struct NliModel;
    impl NliModel {
        pub const ID: &'static str = "cross-encoder/nli-deberta-v3-small";
        pub const ONNX_FILE: &'static str = "onnx/model.onnx";
        pub const TOKENIZER_FILE: &'static str = "tokenizer.json";
        pub const SIZE_HINT: &'static str = "~180MB";
    }

    /// NER (Named Entity Recognition) model metadata
    pub struct NerModel;
    impl NerModel {
        pub const ID: &'static str = "Xenova/bert-base-NER";
        pub const ONNX_FILE: &'static str = "onnx/model_quantized.onnx";
        pub const TOKENIZER_FILE: &'static str = "tokenizer.json";
        pub const SIZE_HINT: &'static str = "~103MB";
    }

    /// Query Expansion model metadata
    pub struct QueryExpansionModel;
    impl QueryExpansionModel {
        pub const ID: &'static str = "TinyLlama/TinyLlama-1.1B-Chat-v1.0";
        pub const GGUF_FILE: &'static str = "model.gguf";
        pub const SIZE_HINT: &'static str = "~630MB";
    }

    /// Reranker (Cross-encoder) model metadata
    pub struct RerankerModel;
    impl RerankerModel {
        pub const ID: &'static str = "cross-encoder/ms-marco-TinyBERT-L-2";
        pub const ONNX_FILE: &'static str = "onnx/model.onnx";
        pub const TOKENIZER_FILE: &'static str = "tokenizer.json";
        pub const SIZE_HINT: &'static str = "~11MB";
    }
}

// ─── Model Cache Management ───────────────────────────────────────────────

/// Get the base cache directory for voidm models.
/// Returns: ~/.cache/voidm/models
pub fn model_cache_dir() -> PathBuf {
    let base = dirs::cache_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".cache"));
    base.join("voidm/models")
}

/// Get the cache directory for a specific model.
/// Example: ~/.cache/voidm/models/cross-encoder-nli-deberta-v3-small
pub fn model_dir(model_id: &str) -> PathBuf {
    let normalized = model_id.replace('/', "-").replace(':', "-");
    model_cache_dir().join(&normalized)
}

/// Check if a model is already cached locally.
pub fn is_model_cached(model_id: &str, required_files: &[&str]) -> bool {
    let dir = model_dir(model_id);
    required_files.iter().all(|f| dir.join(f).exists())
}

/// Ensure model directory exists, creating parent directories if needed.
pub async fn ensure_model_dir(model_id: &str) -> Result<PathBuf> {
    let dir = model_dir(model_id);
    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("Failed to create model cache directory: {}", dir.display()))?;
    Ok(dir)
}

// ─── Model Download ───────────────────────────────────────────────────────

/// Download a single file from HuggingFace to the model cache.
/// Handles retries and progress reporting.
pub async fn download_model_file(
    model_id: &str,
    remote_path: &str,
    cache_dir: &Path,
) -> Result<PathBuf> {
    debug!("Downloading {} from {}", remote_path, model_id);

    let api = ApiBuilder::new()
        .with_cache_dir(model_cache_dir())
        .build()
        .context("Failed to build HuggingFace API")?;

    let repo = api.model(model_id.to_string());

    let src = repo
        .get(remote_path)
        .await
        .with_context(|| {
            format!(
                "Failed to download {} from HuggingFace (model: {})",
                remote_path, model_id
            )
        })?;

    let filename = Path::new(remote_path)
        .file_name()
        .ok_or_else(|| anyhow!("Invalid remote path: {}", remote_path))?;

    let dst = cache_dir.join(filename);
    tokio::fs::copy(&src, &dst).await.with_context(|| {
        format!(
            "Failed to copy model file from {} to {}",
            src.display(),
            dst.display()
        )
    })?;

    debug!("Downloaded {} to {}", remote_path, dst.display());
    Ok(dst)
}

/// Download multiple files for a model from HuggingFace.
/// Shows progress and returns paths to downloaded files.
pub async fn download_model_files(
    model_id: &str,
    files: &[&str],
    size_hint: &str,
    cache_dir: &Path,
) -> Result<Vec<PathBuf>> {
    info!(
        "Downloading model '{}' ({}) to {}",
        model_id,
        size_hint,
        cache_dir.display()
    );

    eprintln!(
        "\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    );
    eprintln!("📦 Downloading model: {}", model_id);
    eprintln!("   Size: {}", size_hint);
    eprintln!("   Cache: {}", cache_dir.display());
    eprintln!("   (First time only, then cached locally)");
    eprintln!(
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n"
    );

    let mut results = Vec::new();

    for file in files {
        let path = download_model_file(model_id, file, cache_dir).await?;
        results.push(path);
    }

    info!("Model {} downloaded successfully", model_id);
    eprintln!(
        "✅ Model ready at: {}\n",
        cache_dir.display()
    );

    Ok(results)
}

// ─── Lazy Model Loading ────────────────────────────────────────────────────

/// Example usage: Lazy load NLI model in voidm-nli crate
/// ```ignore
/// use once_cell::sync::OnceCell;
/// use voidm_models::lazy_model;
///
/// static NLI_MODEL: OnceCell<MyModel> = OnceCell::new();
///
/// pub async fn ensure_nli_model() -> Result<()> {
///     lazy_model::ensure_initialized(
///         &NLI_MODEL,
///         "cross-encoder/nli-deberta-v3-small",
///         &["onnx/model.onnx", "tokenizer.json"],
///         "~180MB",
///         |cache_dir| async {
///             // Load model from cache_dir
///             load_nli_from_cache(cache_dir).await
///         }
///     ).await
/// }
/// ```

pub mod lazy_model {
    use super::*;
    use once_cell::sync::OnceCell;

    /// Ensure a model is initialized via lazy loading.
    /// If not cached, downloads from HuggingFace first.
    pub async fn ensure_initialized<T: Send + Sync + 'static>(
        cell: &OnceCell<T>,
        model_id: &str,
        required_files: &[&str],
        size_hint: &str,
        loader: impl Fn(PathBuf) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T>>>,
        >,
    ) -> Result<()> {
        if cell.get().is_some() {
            return Ok(());
        }

        let cache_dir = ensure_model_dir(model_id).await?;

        // Check if files are cached
        if !is_model_cached(model_id, required_files) {
            download_model_files(model_id, required_files, size_hint, &cache_dir).await?;
        }

        // Load model from cache
        let model = loader(cache_dir).await?;
        let _ = cell.set(model);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_dir_normalization() {
        let dir = model_dir("cross-encoder/nli-deberta-v3-small");
        assert!(dir
            .to_string_lossy()
            .contains("cross-encoder-nli-deberta-v3-small"));
    }

    #[test]
    fn test_model_cache_dir() {
        let dir = model_cache_dir();
        assert!(dir.to_string_lossy().contains("voidm/models"));
    }

    #[test]
    fn test_nli_model_constants() {
        assert_eq!(
            registry::NliModel::ID,
            "cross-encoder/nli-deberta-v3-small"
        );
        assert!(registry::NliModel::SIZE_HINT.contains("180"));
    }

    #[test]
    fn test_reranker_model_constants() {
        assert_eq!(
            registry::RerankerModel::ID,
            "cross-encoder/ms-marco-TinyBERT-L-2"
        );
        assert!(registry::RerankerModel::SIZE_HINT.contains("11"));
    }
}
