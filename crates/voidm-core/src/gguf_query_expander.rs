//! GGUF-based query expansion using llama-gguf.
//!
//! This module provides query expansion using GGUF format models,
//! specifically optimized for the tobil/qmd-query-expansion-1.7B model.
//!
//! Features:
//! - Uses llama-gguf library for GGUF model inference
//! - Structured output parsing (lex:/vec:/hyde: format)
//! - Caching via HuggingFace hub
//! - Optional feature (requires feature flag)

#[cfg(feature = "gguf")]
use anyhow::{Context, Result};

/// GGUF-based query expander for qmd model
#[cfg(feature = "gguf")]
pub struct GgufQueryExpander {
    model_name: String,
}

#[cfg(feature = "gguf")]
impl GgufQueryExpander {
    /// Create a new GGUF query expander
    pub fn new(model_name: String) -> Self {
        Self { model_name }
    }

    /// Expand a query using GGUF model
    pub async fn expand(&self, query: &str) -> Result<String> {
        tracing::info!("GGUF query expansion: Starting for query: '{}'", query);
        tracing::debug!("GGUF query expansion: model_name={}", self.model_name);

        // Use llama-gguf to perform inference
        self.expand_with_gguf(query).await
    }

    /// Internal expansion using llama-gguf
    async fn expand_with_gguf(&self, query: &str) -> Result<String> {
        // Placeholder for llama-gguf integration
        // This will be called from the main query_expansion.rs when model name is detected as GGUF
        
        tracing::debug!("GGUF: Preparing prompt for query: '{}'", query);
        
        let prompt = format!(
            r#"Expand search query with related terms and synonyms:

Query: {}
lex: "#,
            query
        );

        tracing::debug!("GGUF: Prompt prepared, length={}", prompt.len());

        // For now, return a placeholder error indicating feature needs llama-gguf runtime
        // When llama-gguf is integrated, this will perform actual inference
        Err(anyhow::anyhow!(
            "GGUF query expansion for '{}' requires llama-gguf runtime integration",
            self.model_name
        ))
    }

    /// Check if a model name should use GGUF backend
    pub fn should_use_gguf(model_name: &str) -> bool {
        model_name.contains("tobil") || model_name.contains("qmd")
    }

    /// Get the HuggingFace model ID for the given model name
    pub fn get_huggingface_id(model_name: &str) -> Option<String> {
        match model_name {
            name if name.contains("tobil/qmd-query-expansion-1.7B") 
                || name == "tobil/qmd-query-expansion-1.7B" => {
                Some("tobil/qmd-query-expansion-1.7B-gguf".to_string())
            }
            _ => None,
        }
    }
}

#[cfg(not(feature = "gguf"))]
pub struct GgufQueryExpander {
    _private: (),
}

#[cfg(not(feature = "gguf"))]
impl GgufQueryExpander {
    pub fn new(_model_name: String) -> Self {
        Self { _private: () }
    }

    pub async fn expand(&self, _query: &str) -> anyhow::Result<String> {
        Err(anyhow::anyhow!(
            "GGUF support not compiled in. Rebuild with --features gguf"
        ))
    }

    pub fn should_use_gguf(_model_name: &str) -> bool {
        false
    }

    pub fn get_huggingface_id(_model_name: &str) -> Option<String> {
        None
    }
}
