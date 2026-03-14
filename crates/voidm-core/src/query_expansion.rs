//! Query expansion using small generative LLMs.
//!
//! This module expands user search queries with synonyms and related concepts
//! to improve recall in semantic search. Uses ONNX models (GPT-2, GPT-2-Medium)
//! with few-shot prompting for consistent, high-quality expansions.
//!
//! Features:
//! - Config-driven (no code changes to enable)
//! - Real ONNX inference with no fallback
//! - Optional feature (disabled by default)
//! - ONNX model inference with auto-download from HuggingFace
//!
//! Behavior on error:
//! - If model unavailable: expansion fails with error (no fallback)
//! - If timeout: expansion fails with error (no fallback)
//! - CLI will use original query when expansion fails

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use ort::session::Session;
use ort::value::Tensor;
use once_cell::sync::OnceCell;
use crate::config::QueryExpansionConfig;

/// Prompt templates for query expansion.
mod prompts {
    /// Continuation-style template - works with base models like GPT-2
    /// Mimics the format of lists/catalogs that GPT-2 was trained on
    pub const FEW_SHOT_STRUCTURED: &str = r#"Common search terms and related queries:

web development: frontend, backend, HTML, CSS, JavaScript, React, frameworks
Python programming: Django, Flask, NumPy, machine learning, data science
Docker containers: Kubernetes, orchestration, deployment, microservices
REST API: HTTP, endpoints, JSON, web services, microservices
Database: SQL, queries, indexing, schema, transactions

{query}:"#;

    /// Get the appropriate prompt template for the model.
    pub fn get_template(_model: &str) -> &'static str {
        // Use few-shot structured prompt for all models - it works best with GPT-2
        FEW_SHOT_STRUCTURED
    }
}

// ─── Model state ──────────────────────────────────────────────────────────

struct LLMModel {
    session: std::sync::Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
}

struct SendLLM(Arc<LLMModel>);
unsafe impl Send for SendLLM {}
unsafe impl Sync for SendLLM {}

struct LLMModelCache {
    models: std::sync::Mutex<HashMap<String, SendLLM>>,
}

impl LLMModelCache {
    fn new() -> Self {
        Self {
            models: std::sync::Mutex::new(HashMap::new()),
        }
    }
    
    fn get(&self, model_name: &str) -> Option<SendLLM> {
        self.models.lock().unwrap().get(model_name).cloned()
    }
    
    fn insert(&self, model_name: String, model: SendLLM) {
        self.models.lock().unwrap().insert(model_name, model);
    }
    
    fn contains(&self, model_name: &str) -> bool {
        self.models.lock().unwrap().contains_key(model_name)
    }
}

impl Clone for SendLLM {
    fn clone(&self) -> Self {
        SendLLM(self.0.clone())
    }
}

static LLM_CACHE: OnceCell<LLMModelCache> = OnceCell::new();
static LLM_INIT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const MODEL_SPECS: &[(&str, &str)] = &[
    ("phi-2", "gpt2-medium"),
    ("tinyllama", "gpt2"),
    ("gpt2-small", "gpt2"),
];

fn get_model_spec(name: &str) -> Option<&'static str> {
    MODEL_SPECS.iter()
        .find(|(model_name, _)| model_name == &name)
        .map(|(_, hf_id)| *hf_id)
}

fn llm_cache_dir() -> PathBuf {
    let cache_root = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("~/.cache"));
    cache_root.join("voidm").join("llm-models")
}

fn get_llm_cache() -> &'static LLMModelCache {
    LLM_CACHE.get_or_init(LLMModelCache::new)
}

async fn ensure_llm_model(model_name: &str) -> Result<()> {
    let cache = get_llm_cache();
    
    if cache.contains(model_name) {
        return Ok(());
    }
    
    let _guard = LLM_INIT.lock().await;
    
    // Double-check after acquiring lock
    if cache.contains(model_name) {
        return Ok(());
    }
    
    let hf_id = get_model_spec(model_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown model: {}", model_name))?;
    
    let model_dir = llm_cache_dir().join(model_name);
    std::fs::create_dir_all(&model_dir)
        .context("Failed to create LLM cache directory")?;
    
    let onnx_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");
    
    // Download if needed
    if !onnx_path.exists() || !tokenizer_path.exists() {
        tracing::info!("Downloading LLM model '{}' (first use) …", model_name);
        eprintln!("Downloading LLM model '{}' (first use, may take a few minutes) …", model_name);
        download_llm_files(hf_id, &model_dir).await?;
        eprintln!("LLM model ready at {}", model_dir.display());
    }
    
    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| anyhow::anyhow!("Failed to load LLM tokenizer: {}", e))?;
    
    let session = Session::builder()
        .context("Failed to create ORT session builder")?
        .commit_from_file(&onnx_path)
        .context("Failed to load LLM ONNX model")?;
    
    let model = LLMModel {
        session: std::sync::Mutex::new(session),
        tokenizer,
    };
    
    cache.insert(model_name.to_string(), SendLLM(Arc::new(model)));
    
    Ok(())
}

async fn download_llm_files(hf_id: &str, model_dir: &PathBuf) -> Result<()> {
    let cache_parent = llm_cache_dir().parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| llm_cache_dir());
    
    let api = hf_hub::api::tokio::ApiBuilder::new()
        .with_cache_dir(cache_parent)
        .build()
        .context("Failed to build hf-hub API")?;
    
    let repo = api.model(hf_id.to_string());
    
    // Download ONNX model - try multiple common paths (for different model sources)
    let onnx_paths = vec![
        "onnx/model.onnx",          // Standard HF layout
        "onnx/decoder_model.onnx",  // Some models like TinyLlama
        "onnx-model/model.onnx",    // Alternative layout
    ];
    
    let mut onnx_src = None;
    for path in &onnx_paths {
        match repo.get(path).await {
            Ok(src) => {
                tracing::info!("Found ONNX model at: {}", path);
                onnx_src = Some(src);
                break;
            }
            Err(e) => {
                tracing::debug!("ONNX not at {}: {}", path, e);
                continue;
            }
        }
    }
    
    let onnx_src = onnx_src
        .ok_or_else(|| anyhow::anyhow!("Failed to download ONNX model - tried all known paths"))?;
    
    std::fs::copy(&onnx_src, model_dir.join("model.onnx"))
        .context("Failed to copy ONNX model to cache")?;
    
    // Download tokenizer
    let tok_src = repo.get("tokenizer.json").await
        .context("Failed to download tokenizer from HuggingFace")?;
    std::fs::copy(&tok_src, model_dir.join("tokenizer.json"))
        .context("Failed to copy tokenizer to cache")?;
    
    Ok(())
}

// ─── Query expansion ──────────────────────────────────────────────────────

/// Global query expansion state (model, cache).
pub struct QueryExpander {
    config: QueryExpansionConfig,
}

impl QueryExpander {
    /// Create a new query expander with the given configuration.
    pub fn new(config: QueryExpansionConfig) -> Self {
        Self {
            config,
        }
    }

    /// Expand a query with related terms.
    ///
    /// Returns the expanded query (original + related terms separated by commas).
    /// Expand query with real ONNX inference. No fallback - either it works or returns error.
    /// Returns Err if expansion fails for any reason (model unavailable, timeout, etc.)
    pub async fn expand(&self, query: &str) -> anyhow::Result<String> {
        // If disabled, return error (no expansion)
        if !self.config.enabled {
            return Err(anyhow::anyhow!("Query expansion disabled"));
        }

        // Generate expansion - no fallback, propagate errors
        self.expand_with_timeout(query).await
    }

    /// Internal expansion with timeout.
    async fn expand_with_timeout(&self, query: &str) -> Result<String> {
        use tokio::time::{timeout, Duration};
        
        let query_str = query.to_string();
        let model = self.config.model.clone();
        
        // FIRST: Ensure model is loaded (outside timeout, can take time for download)
        ensure_llm_model(&model).await?;
        
        // NOW: Apply timeout only to inference (should be fast)
        let timeout_duration = Duration::from_millis(self.config.timeout_ms);
        let result = timeout(timeout_duration, async {
            Self::run_inference(&query_str, &model).await
        })
        .await;
        
        match result {
            Ok(Ok(expanded)) => Ok(expanded),
            Ok(Err(e)) => {
                tracing::warn!("Query expansion error: {}", e);
                Err(e)
            }
            Err(_) => {
                tracing::warn!("Query expansion inference timed out ({}ms)", self.config.timeout_ms);
                Err(anyhow::anyhow!("Query expansion inference timed out"))
            }
        }
    }
    
    /// Run actual model inference.
    async fn run_inference(query: &str, model_name: &str) -> Result<String> {
        // Model should already be loaded by expand_with_timeout
        // This just runs the inference
        
        // Get the appropriate prompt template
        let template = prompts::get_template(model_name);
        let prompt = template.replace("{query}", query);
        
        tracing::debug!("Query expansion prompt (first 100 chars): {}", 
                       prompt.chars().take(100).collect::<String>());
        
        // Get model from cache and run inference
        let cache = get_llm_cache();
        if let Some(SendLLM(model_arc)) = cache.get(model_name) {
            let expanded_terms = Self::infer_expansion(&model_arc, &prompt)?;
            
            // Prepend original query to the expansion (enhancement, not replacement)
            // Format: "original_query, expanded_term1, expanded_term2, ..."
            let result = if expanded_terms.is_empty() {
                query.to_string()
            } else {
                format!("{}, {}", query, expanded_terms)
            };
            
            Ok(result)
        } else {
            Err(anyhow::anyhow!("Model not loaded: {}", model_name))
        }
    }
    
    /// Perform ONNX inference to expand query with greedy text generation.
    fn infer_expansion(model: &Arc<LLMModel>, prompt: &str) -> Result<String> {
        // Tokenize the prompt
        let encoding = model.tokenizer
            .encode(prompt, true)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;
        
        let mut input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        
        if input_ids.is_empty() {
            return Err(anyhow::anyhow!("Empty input after tokenization"));
        }
        
        // Constants for generation
        const MAX_NEW_TOKENS: usize = 30;  // Max tokens to generate
        const MAX_SEQ_LEN: usize = 512;    // Sequence length limit
        const EOS_TOKEN: i64 = 2;          // End-of-sequence token ID
        
        let mut generated_tokens = Vec::new();
        
        // Autoregressive text generation (greedy decoding)
        for _ in 0..MAX_NEW_TOKENS {
            if input_ids.len() >= MAX_SEQ_LEN {
                break;
            }
            
            // Create attention mask
            let attention_mask: Vec<i64> = (0..input_ids.len()).map(|_| 1i64).collect();
            let seq_len = input_ids.len();
            
            // Create input tensors
            let input_ids_tensor = Tensor::<i64>::from_array(
                ([1usize, seq_len], input_ids.clone().into_boxed_slice())
            ).context("Failed to create input_ids tensor")?;
            
            let attention_mask_tensor = Tensor::<i64>::from_array(
                ([1usize, seq_len], attention_mask.into_boxed_slice())
            ).context("Failed to create attention_mask tensor")?;
            
            // Run inference to get logits for next token
            let mut session = model.session.lock().unwrap();
            
            let outputs = session.run(
                ort::inputs![
                    "input_ids" => input_ids_tensor,
                    "attention_mask" => attention_mask_tensor
                ]
            ).context("LLM inference failed")?;
            
            // Extract logits from last position
            let logits_value = outputs.get("logits")
                .or_else(|| outputs.get("last_hidden_state"))
                .context("No logits output from LLM model")?;
            
            let logits = logits_value
                .try_extract_tensor::<f32>()
                .context("Failed to extract logits as f32")?;
            
            let (_shape, logits_data) = logits;
            
            if logits_data.len() < 32000 {
                // Not enough logits for vocab (usually ~32k or more for LLMs)
                // This might be hidden states instead of logits
                break;
            }
            
            // Get logits for last token position
            // Shape is [batch_size=1, seq_len, vocab_size]
            // We want the last token's logits
            let vocab_size = logits_data.len() / seq_len;
            let last_token_logits_start = (seq_len - 1) * vocab_size;
            let last_token_logits = &logits_data[last_token_logits_start..];
            
            // Find token with highest logit (greedy decoding)
            let mut next_token: i64 = 0;
            let mut max_logit = f32::NEG_INFINITY;
            
            for (idx, &logit) in last_token_logits.iter().enumerate() {
                if logit > max_logit {
                    max_logit = logit;
                    next_token = idx as i64;
                }
            }
            
            // Stop if we generated end-of-sequence token
            if next_token == EOS_TOKEN {
                break;
            }
            
            // Add to generated tokens
            generated_tokens.push(next_token);
            input_ids.push(next_token);
        }
        
        drop(model.session.lock());
        
        // Decode generated tokens to text
        let generated_ids: Vec<u32> = generated_tokens.iter().map(|&id| id as u32).collect();
        
        let decoded = model.tokenizer
            .decode(&generated_ids, true)
            .map_err(|e| anyhow::anyhow!("Decoding failed: {}", e))?;
        
        // Clean up the decoded text - extract meaningful terms
        let expanded = decoded.trim();
        
        // The prompt ends with "query:" so we expect output after that
        // Extract text after the last colon if there is one
        let terms = if expanded.contains(':') {
            // Get everything after the last colon
            expanded.rsplit(':').next().unwrap_or(expanded).trim().to_string()
        } else {
            expanded.to_string()
        };
        
        if terms.is_empty() {
            tracing::warn!("Generated empty expansion from: {}", expanded);
            return Err(anyhow::anyhow!("Generated empty expansion"));
        }
        
        // Truncate at sentence boundaries (period, newline) to avoid rambling
        let truncated = if let Some(period_idx) = terms.find('.') {
            &terms[..period_idx]
        } else if let Some(newline_idx) = terms.find('\n') {
            &terms[..newline_idx]
        } else if terms.len() > 80 {
            // Truncate long outputs early to avoid repetition
            &terms[..80]
        } else {
            &terms
        };
        
        // Remove excessive repetition - if we see the same word repeated 3+ times, keep only first
        let deduped = if let Some(first_comma_idx) = truncated.find(',') {
            let first_term = &truncated[..first_comma_idx].trim();
            let rest = &truncated[first_comma_idx..];
            
            // Count occurrences of the first term in the rest
            let count = rest.matches(first_term).count();
            if count >= 2 {
                // Too much repetition, truncate at first occurrence of repetition
                if let Some(rep_pos) = rest[1..].find(&format!("{},", first_term)) {
                    &truncated[..first_comma_idx + rep_pos + 1]
                } else {
                    truncated
                }
            } else {
                truncated
            }
        } else {
            truncated
        };
        // Remove excessive repetition
        let final_expansion = {
            let parts: Vec<&str> = deduped.split(',').map(|s| s.trim()).collect();
            let mut seen = std::collections::HashSet::new();
            let mut unique_parts = Vec::new();
            
            for part in parts {
                if !part.is_empty() && !seen.contains(part) {
                    unique_parts.push(part);
                    seen.insert(part);
                }
            }
            
            // Limit to reasonable number of terms (10 max)
            unique_parts.truncate(10);
            unique_parts.join(", ")
        };
        
        tracing::debug!("LLM generated expansion: {}", final_expansion);
        Ok(final_expansion)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_templates() {
        assert!(prompts::FEW_SHOT_STRUCTURED.contains("{query}"));

        // Test template selection - always uses FEW_SHOT_STRUCTURED
        assert_eq!(prompts::get_template("phi-2"), prompts::FEW_SHOT_STRUCTURED);
        assert_eq!(prompts::get_template("tinyllama"), prompts::FEW_SHOT_STRUCTURED);
        assert_eq!(prompts::get_template("gpt2-small"), prompts::FEW_SHOT_STRUCTURED);
    }

    #[tokio::test]
    async fn test_query_expander_disabled() {
        let config = QueryExpansionConfig {
            enabled: false,
            ..Default::default()
        };
        let expander = QueryExpander::new(config);

        let result = expander.expand("Docker").await;
        // When disabled, should return error
        assert!(result.is_err(), "Expansion should fail when disabled");
    }
}
