//! Query expansion using small generative LLMs.
//!
//! This module expands user search queries with synonyms and related concepts
//! to improve recall in semantic search. Uses a small language model (Phi-2, TinyLLama)
//! with few-shot prompting for consistent, high-quality expansions.
//!
//! Features:
//! - Config-driven (no code changes to enable)
//! - Graceful fallback on timeout/error
//! - LRU caching for repeated queries
//! - Optional feature (disabled by default)
//! - ONNX model inference with auto-download

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use ort::session::Session;
use ort::value::Tensor;
use once_cell::sync::OnceCell;
use crate::config::QueryExpansionConfig;

/// A simple LRU cache for query expansions.
struct LRUCache {
    cache: HashMap<String, String>,
    order: Vec<String>,
    max_size: usize,
}

impl LRUCache {
    fn new(max_size: usize) -> Self {
        Self {
            cache: HashMap::new(),
            order: Vec::new(),
            max_size,
        }
    }

    fn get(&mut self, key: &str) -> Option<String> {
        if let Some(value) = self.cache.get(key) {
            // Move to end (most recently used)
            self.order.retain(|k| k != key);
            self.order.push(key.to_string());
            Some(value.clone())
        } else {
            None
        }
    }

    fn insert(&mut self, key: String, value: String) {
        // If already exists, remove old entry
        if self.cache.contains_key(&key) {
            self.order.retain(|k| k != &key);
        }

        // If at capacity, remove least recently used
        if self.cache.len() >= self.max_size && !self.cache.contains_key(&key) {
            if let Some(lru_key) = self.order.first() {
                let lru_key = lru_key.clone();
                self.cache.remove(&lru_key);
                self.order.remove(0);
            }
        }

        self.cache.insert(key.clone(), value);
        self.order.push(key);
    }

    fn clear(&mut self) {
        self.cache.clear();
        self.order.clear();
    }

    fn size(&self) -> usize {
        self.cache.len()
    }
}

/// Prompt templates for query expansion.
mod prompts {
    /// Few-shot structured template (RECOMMENDED).
    /// Works well with Phi-2 and TinyLlama.
    pub const FEW_SHOT_STRUCTURED: &str = r#"Expand the search query with related terms and synonyms.
Return ONLY a comma-separated list of terms (no explanations, no period).

Query: REST API
Expansion: REST API, web service, HTTP endpoints, API design, RESTful service

Query: Python
Expansion: Python, Python programming, Django, Flask, data science

Query: Docker
Expansion: Docker, containerization, container images, Docker Compose, Kubernetes

Query: {query}
Expansion:"#;

    /// Zero-shot minimal template (FALLBACK).
    /// Simplest prompt, fastest inference.
    pub const ZERO_SHOT_MINIMAL: &str = r#"Expand search query with related terms (comma-separated):
Query: {query}
Expansion:"#;

    /// Get the appropriate prompt template for the model.
    pub fn get_template(model: &str) -> &'static str {
        match model {
            "phi-2" => FEW_SHOT_STRUCTURED,
            "tinyllama" => FEW_SHOT_STRUCTURED,
            "gpt2-small" => ZERO_SHOT_MINIMAL,
            _ => FEW_SHOT_STRUCTURED,
        }
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
    ("phi-2", "microsoft/phi-2"),
    ("tinyllama", "TinyLlama/TinyLlama-1.1B-Chat-v1.0"),
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
    
    // Download ONNX model
    let onnx_src = repo.get("onnx/model.onnx").await
        .context("Failed to download ONNX model from HuggingFace")?;
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
    cache: Arc<Mutex<LRUCache>>,
    config: QueryExpansionConfig,
}

impl QueryExpander {
    /// Create a new query expander with the given configuration.
    pub fn new(config: QueryExpansionConfig) -> Self {
        Self {
            cache: Arc::new(Mutex::new(LRUCache::new(config.cache_size))),
            config,
        }
    }

    /// Expand a query with related terms.
    ///
    /// Returns the expanded query (original + related terms separated by commas).
    /// On timeout or error, returns the original query as fallback.
    pub async fn expand(&self, query: &str) -> String {
        // If disabled, return original query
        if !self.config.enabled {
            return query.to_string();
        }

        // Check cache
        let mut cache = self.cache.lock().await;
        if let Some(expanded) = cache.get(query) {
            tracing::debug!("Query expansion cache hit for: {}", query);
            return expanded;
        }
        drop(cache);

        // Generate expansion with timeout and fallback
        let expanded = self
            .expand_with_timeout(query)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("Query expansion failed ({}), using original query", e);
                query.to_string()
            });

        // Cache the result
        let mut cache = self.cache.lock().await;
        cache.insert(query.to_string(), expanded.clone());

        expanded
    }

    /// Internal expansion with timeout.
    async fn expand_with_timeout(&self, query: &str) -> Result<String> {
        use tokio::time::{timeout, Duration};
        
        let timeout_duration = Duration::from_millis(self.config.timeout_ms);
        let query_str = query.to_string();
        let model = self.config.model.clone();
        
        // Inference with timeout
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
                tracing::warn!("Query expansion timed out ({}ms)", self.config.timeout_ms);
                Err(anyhow::anyhow!("Query expansion timed out"))
            }
        }
    }
    
    /// Run actual model inference.
    async fn run_inference(query: &str, model_name: &str) -> Result<String> {
        // Try to ensure model is loaded
        // If it fails (e.g., no network), fall back to mock expansion
        match ensure_llm_model(model_name).await {
            Ok(()) => {
                // Get the appropriate prompt template
                let template = prompts::get_template(model_name);
                let prompt = template.replace("{query}", query);
                
                tracing::debug!("Query expansion prompt: {}", prompt);
                
                // Get model from cache and run inference
                let cache = get_llm_cache();
                if let Some(SendLLM(model_arc)) = cache.get(model_name) {
                    Self::infer_expansion(&model_arc, &prompt)
                } else {
                    Err(anyhow::anyhow!("Model not loaded: {}", model_name))
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load LLM model, using mock expansion: {}", e);
                // Fallback to mock expansion if model can't be loaded
                Ok(Self::mock_expand(query))
            }
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
        
        // Clean up the decoded text
        let expanded = format!("{}", decoded.trim());
        
        if expanded.is_empty() {
            tracing::warn!("Generated empty expansion");
            return Err(anyhow::anyhow!("Generated empty expansion"));
        }
        
        tracing::debug!("LLM generated expansion: {}", expanded);
        Ok(expanded)
    }
    
    /// Mock expansion for fallback when model unavailable (Phase 2b demo).
    fn mock_expand(query: &str) -> String {
        let lower = query.to_lowercase();
        
        // Common expansion patterns for demo
        let expansions = if lower.contains("docker") {
            format!("{}, containerization, container images, Docker Compose, Kubernetes", query)
        } else if lower.contains("kubernetes") || lower.contains("k8s") {
            format!("{}, container orchestration, pods, services, deployments", query)
        } else if lower.contains("python") {
            format!("{}, Python programming, Django, Flask, data science", query)
        } else if lower.contains("api") {
            format!("{}, REST, web services, HTTP endpoints, API design", query)
        } else if lower.contains("rust") {
            format!("{}, Rust programming, systems programming, performance", query)
        } else if lower.contains("database") || lower.contains("sql") {
            format!("{}, relational database, queries, schema design", query)
        } else if lower.contains("aws") {
            format!("{}, Amazon Web Services, cloud computing, S3, EC2", query)
        } else if lower.contains("git") || lower.contains("github") {
            format!("{}, version control, branches, commits, repositories", query)
        } else {
            // Default: return query as-is for unknown terms
            query.to_string()
        };
        
        expansions
    }

    /// Clear the expansion cache.
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.lock().await;
        cache.clear();
        tracing::info!("Query expansion cache cleared");
    }

    /// Get cache statistics.
    pub async fn cache_stats(&self) -> CacheStats {
        let cache = self.cache.lock().await;
        CacheStats {
            size: cache.size(),
            max_size: self.config.cache_size,
        }
    }
}

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub size: usize,
    pub max_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_cache_basic() {
        let mut cache = LRUCache::new(3);

        // Insert items
        cache.insert("a".to_string(), "expansion_a".to_string());
        cache.insert("b".to_string(), "expansion_b".to_string());
        cache.insert("c".to_string(), "expansion_c".to_string());

        assert_eq!(cache.size(), 3);
        assert_eq!(cache.get("a"), Some("expansion_a".to_string()));
    }

    #[test]
    fn test_lru_cache_eviction() {
        let mut cache = LRUCache::new(2);

        cache.insert("a".to_string(), "expansion_a".to_string());
        cache.insert("b".to_string(), "expansion_b".to_string());
        cache.insert("c".to_string(), "expansion_c".to_string()); // 'a' should be evicted

        assert_eq!(cache.size(), 2);
        assert_eq!(cache.get("a"), None); // 'a' was evicted
        assert_eq!(cache.get("b"), Some("expansion_b".to_string()));
        assert_eq!(cache.get("c"), Some("expansion_c".to_string()));
    }

    #[test]
    fn test_prompt_templates() {
        assert!(prompts::FEW_SHOT_STRUCTURED.contains("{query}"));
        assert!(prompts::ZERO_SHOT_MINIMAL.contains("{query}"));

        // Test template selection
        assert_eq!(prompts::get_template("phi-2"), prompts::FEW_SHOT_STRUCTURED);
        assert_eq!(prompts::get_template("tinyllama"), prompts::FEW_SHOT_STRUCTURED);
        assert_eq!(prompts::get_template("gpt2-small"), prompts::ZERO_SHOT_MINIMAL);
    }

    #[tokio::test]
    async fn test_query_expander_disabled() {
        let config = QueryExpansionConfig {
            enabled: false,
            ..Default::default()
        };
        let expander = QueryExpander::new(config);

        let result = expander.expand("Docker").await;
        assert_eq!(result, "Docker");
    }

    #[tokio::test]
    async fn test_query_expander_cache() {
        let config = QueryExpansionConfig {
            enabled: true,
            cache_size: 10,
            ..Default::default()
        };
        let expander = QueryExpander::new(config);

        let result1 = expander.expand("API").await;
        let result2 = expander.expand("API").await;

        assert_eq!(result1, result2);

        let stats = expander.cache_stats().await;
        assert_eq!(stats.size, 1);
    }
}
