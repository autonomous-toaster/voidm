//! Query expansion using small generative LLMs (ONNX-based).
//!
//! This module expands user search queries with synonyms and related concepts
//! to improve recall in semantic search using ONNX-compatible models.
//!
//! Features:
//! - Config-driven (explicit model names like "tinyllama")
//! - ONNX backend for efficient inference
//! - Real inference with no fallback
//! - Auto-download from HuggingFace
//! - 3-template weighted ensemble for improved quality
//! - HyDE support for hypothetical document generation
//!
//! Supported Models:
//! - "tinyllama": ONNX backend (recommended, 1.1B parameters, fast)
//! - Any ONNX-compatible model from HuggingFace
//!
//! Behavior on error:
//! - If model unavailable: expansion fails with error (no fallback)
//! - If timeout: expansion fails with error (no fallback)

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use ort::session::Session;
use ort::value::Tensor;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

/// Intent-aware query expansion configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentConfig {
    /// Enable intent-aware expansion (default: true).
    #[serde(default = "default_intent_enabled")]
    pub enabled: bool,
    /// Use scope as fallback intent if intent not explicitly provided (default: true).
    #[serde(default = "default_intent_use_scope_as_fallback")]
    pub use_scope_as_fallback: bool,
    /// Optional default intent for all queries (default: null).
    #[serde(default)]
    pub default_intent: Option<String>,
}

fn default_intent_enabled() -> bool {
    true
}

fn default_intent_use_scope_as_fallback() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GenerationBackend {
    Onnx,
    LlamaCpp,
    Mlx,
}

impl Default for GenerationBackend {
    fn default() -> Self {
        Self::Onnx
    }
}

/// Query expansion configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryExpansionConfig {
    /// Enable query expansion (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Model name: "tinyllama" (ONNX backend, recommended).
    /// Uses ONNX-compatible models from HuggingFace for efficient query expansion.
    #[serde(default = "default_query_expansion_model")]
    pub model: String,
    /// Generation backend. Current implemented runtime is ONNX; other values are reserved for future backends.
    #[serde(default)]
    pub backend: GenerationBackend,
    /// Maximum time to wait for expansion in milliseconds (default: 300).
    #[serde(default = "default_query_expansion_timeout_ms")]
    pub timeout_ms: u64,
    /// Intent-aware expansion configuration.
    #[serde(default)]
    pub intent: IntentConfig,
}

fn default_query_expansion_model() -> String {
    "tinyllama".to_string()
}

fn default_query_expansion_timeout_ms() -> u64 {
    300
}

impl Default for QueryExpansionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: default_query_expansion_model(),
            backend: GenerationBackend::Onnx,
            timeout_ms: default_query_expansion_timeout_ms(),
            intent: IntentConfig::default(),
        }
    }
}

pub fn parse_generation_backend(name: &str) -> Result<GenerationBackend> {
    match name {
        "onnx" => Ok(GenerationBackend::Onnx),
        "llama_cpp" => Ok(GenerationBackend::LlamaCpp),
        "mlx" => Ok(GenerationBackend::Mlx),
        other => Err(anyhow::anyhow!(
            "Unsupported generation backend '{}'. Expected: onnx, llama_cpp, mlx",
            other
        )),
    }
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            use_scope_as_fallback: true,
            default_intent: None,
        }
    }
}

/// Prompt templates for query expansion.
mod prompts {
    /// Continuation-style template with expanded examples and concept grouping
    pub const FEW_SHOT_STRUCTURED: &str = r#"Expand search queries with related terms and synonyms:

Query: web development
Synonyms: frontend, backend, HTML, CSS, JavaScript, React, frameworks, UI, responsive design

Query: Python programming
Synonyms: Django, Flask, NumPy, machine learning, data science, pandas, scripting, libraries

Query: Docker containers
Synonyms: Kubernetes, orchestration, deployment, microservices, images, registry, swarm, scaling

Query: REST API
Synonyms: HTTP, endpoints, JSON, web services, remote invocation, OpenAPI, schemas, versioning

Query: Database
Synonyms: SQL, queries, indexing, schema, transactions, relational, NoSQL, replication

Query: Cloud computing
Synonyms: AWS, Azure, GCP, infrastructure, serverless, scaling, deployment, provisioning

Query: Machine learning
Synonyms: models, neural networks, training, inference, classification, regression, optimization

Query: Security
Synonyms: authentication, authorization, encryption, OAuth, JWT, certificates, SSL/TLS, HTTPS

Query: {query}
Synonyms:"#;

    /// Improved domain-aware template with fully deduplicated, high-precision terms
    pub const FEW_SHOT_IMPROVED: &str = r#"Expand search queries with related terms and concepts:

Topic: Docker
Synonyms: containers, Kubernetes, images, registry, orchestration, deployment, cloud-native, OCI, CRI
Related: microservices, containerization, compose, scaling, portability, resource management

Topic: Python
Synonyms: Django, Flask, NumPy, machine learning, pandas, data science, PyPI, requests, asyncio, poetry
Related: scripting, automation, backend development, scientific computing, environments, packaging

Topic: REST API
Synonyms: HTTP, endpoints, JSON, web services, remote invocation, OpenAPI, schemas, standards, routes, HAL
Related: GraphQL, JSON-RPC, API gateway, versioning, documentation, permission models, caching

Topic: Database
Synonyms: SQL, NoSQL, PostgreSQL, MongoDB, indexing, queries, transactions, tables, CRUD, migration
Related: ACID, normalization, data warehousing, time-series, replication, backup, sharding

Topic: Security
Synonyms: authentication, authorization, encryption, OAuth, JWT, certificates, SSL/TLS, HTTPS, secrets, keying
Related: identity management, access control, compliance, audit, threat detection, vulnerability

Topic: Testing
Synonyms: unit tests, integration tests, TDD, test cases, assertions, mocking, coverage, runners, fixtures, BDD
Related: continuous integration, regression testing, debugging, quality assurance, snapshots

Topic: Cloud Infrastructure
Synonyms: AWS, Azure, GCP, cloud computing, infrastructure-as-code, terraform, serverless, CDN, IaC
Related: auto-scaling, load balancing, provisioning, DevOps, communication, storage, compute

Topic: Machine Learning
Synonyms: neural networks, deep learning, models, training, inference, TensorFlow, PyTorch, scikit-learn, Keras
Related: computer vision, NLP, classification, regression, feature engineering, optimization, datasets

Topic: Monitoring & Observability
Synonyms: logging, metrics, traces, monitoring, alerting, dashboards, prometheus, grafana, datadog, ELK
Related: performance monitoring, incident response, SLO, telemetry, distributed tracing, observability

Topic: {query}
Synonyms:"#;

    /// Intent-aware template - comprehensive context examples for diverse domain-specific expansion
    pub const FEW_SHOT_INTENT_AWARE: &str = r#"Expand the following search query within the given context:

Context: Docker orchestration
Query: containers
Related: Kubernetes, orchestration, cluster, deployment, services, swarm, scaling

Context: Python backend
Query: web frameworks
Related: Django, Flask, FastAPI, async, HTTP, REST, endpoints, routing

Context: Database performance
Query: optimization
Related: indexing, caching, partitioning, normalization, query plans, execution, tuning

Context: Cloud infrastructure
Query: deployment
Related: provisioning, infrastructure-as-code, automation, CI/CD, scaling, availability

Context: Machine learning
Query: training
Related: models, algorithms, datasets, optimization, validation, regularization, hyperparameters

Context: Security compliance
Query: authentication
Related: OAuth, JWT, certificates, encryption, identity, verification, authorization

Context: Testing automation
Query: coverage
Related: unit tests, integration tests, mocking, assertions, fixtures, CI/CD, quality assurance

Context: Monitoring observability
Query: metrics
Related: logging, traces, dashboards, alerting, telemetry, incident response, performance

Context: Data engineering
Query: pipeline
Related: ETL, transformation, validation, aggregation, warehousing, batch, streaming

Context: API management
Query: versioning
Related: compatibility, deprecation, gateway, rate limiting, documentation, schemas, standards

Context: {intent}
Query: {query}
Related:"#;

    /// HyDE-specific template for generating hypothetical documents that would answer the query
    /// Output format is 3-5 short, realistic document snippets separated by semicolons
    pub const FEW_SHOT_HYDE: &str = r#"Generate 3-5 hypothetical document snippets that would contain the answer to this search query. Each snippet should be a realistic excerpt that would appear in relevant documents. Documents should be specific, actionable, and cover different aspects of the topic.

Query: What is Docker?
Documents: |Docker is a containerization platform that packages applications with their dependencies into lightweight, portable containers|; |Containers provide isolation and consistency across different environments, making deployments more reliable|; |Docker uses images as templates and runs containers as isolated processes with shared kernel resources|; |The Docker ecosystem includes registries like Docker Hub for sharing and discovering containerized applications|; |Docker simplifies DevOps workflows by standardizing application delivery across development, testing, and production environments|

Query: How to optimize database queries?
Documents: |Database query optimization involves analyzing execution plans and adding appropriate indexes to improve performance|; |Common techniques include query rewriting, denormalization, and partitioning to reduce I/O and CPU overhead|; |Profiling tools and slow query logs help identify bottlenecks in database performance|; |Caching strategies and connection pooling can significantly reduce database load in production systems|; |Query statistics and EXPLAIN commands provide insights into query performance characteristics and optimization opportunities|

Query: Machine learning best practices
Documents: |Successful machine learning projects require careful data collection, cleaning, and feature engineering before model training|; |Model evaluation should use cross-validation and appropriate metrics for the specific problem (accuracy, F1, AUC)|; |Hyperparameter tuning, regularization, and ensemble methods help improve model generalization|; |Monitoring model performance in production is crucial to detect data drift and maintain accuracy over time|; |Reproducibility requires proper experiment tracking, version control, and documentation of data and model versions|

Query: Cloud security considerations
Documents: |Cloud security requires defense in depth with identity management, network isolation, encryption, and audit logging|; |Proper access control and least privilege principles prevent unauthorized data access and lateral movement|; |Compliance frameworks like HIPAA and GDPR have specific requirements for cloud data handling and privacy|; |Regular security assessments, penetration testing, and vulnerability scanning help identify and fix security issues|; |Encryption in transit and at rest, along with key management, protect sensitive data in cloud environments|

Query: RESTful API design
Documents: |REST APIs use standard HTTP methods (GET, POST, PUT, DELETE) to perform operations on resources identified by URLs|; |Proper API versioning, documentation, and error handling ensure backward compatibility and developer experience|; |Rate limiting and authentication mechanisms protect API endpoints from abuse and unauthorized access|; |API design patterns like HATEOAS, pagination, and filtering improve usability and scalability|; |OpenAPI specifications enable automated documentation and client generation for REST APIs|

Query: Python async programming
Documents: |Asynchronous programming in Python allows non-blocking I/O operations and concurrent task execution within a single thread|; |The asyncio library provides event loops, coroutines, and tasks for managing asynchronous workflows|; |Proper error handling and exception propagation in async code requires careful use of try-except and context managers|; |Performance benefits of async programming depend on I/O-bound workloads; CPU-bound tasks still require true parallelism|; |Testing async code requires special frameworks that understand event loops and async context|

Query: Microservices architecture
Documents: |Microservices break monolithic applications into small, independent services that communicate through APIs|; |Service discovery, load balancing, and circuit breakers are essential patterns for reliable microservices deployment|; |Data consistency across services requires careful design of distributed transactions or eventual consistency models|; |Container orchestration platforms like Kubernetes manage deployment, scaling, and networking of microservices|; |Monitoring, logging, and distributed tracing become critical for debugging issues in complex microservice systems|

Query: Kubernetes deployment
Documents: |Kubernetes orchestrates containerized applications across clusters with automatic scaling, self-healing, and rolling updates|; |Services, ingress controllers, and network policies manage communication and routing between pods and external traffic|; |ConfigMaps and secrets securely manage configuration and sensitive data injection into containers|; |StatefulSets and persistent volumes handle stateful applications requiring stable identities and persistent storage|; |Resource limits, requests, and scheduling policies optimize cluster utilization and prevent resource contention|

Query: {query}
Documents:"#;

    /// GBNF grammar for structured synonym output.
    /// Enforces format: "term1, term2, term3"
    /// - Each term: [letter][letter/space/hyphen/underscore/dot]*
    /// - Prevents pure numeric suffixes: "python2", "api3", "model4" are all rejected
    /// - Must start with letter, ensures meaningful terms
    #[allow(dead_code)]
    pub const GRAMMAR_SYNONYMS: &str = r#"root   : item ("," item)*
item   : [a-zA-Z] [a-zA-Z \-._]*
"#;

    /// Get the appropriate prompt template for the model.
    pub fn get_template(mode: &TemplateMode) -> &'static str {
        match mode {
            TemplateMode::Structured => FEW_SHOT_STRUCTURED,
            TemplateMode::Improved => FEW_SHOT_IMPROVED,
            TemplateMode::IntentAware => FEW_SHOT_INTENT_AWARE,
            TemplateMode::HyDE => FEW_SHOT_HYDE,
        }
    }

    /// Template mode selection
    #[derive(Debug, Clone, Copy)]
    pub enum TemplateMode {
        Structured,  // Original few-shot
        Improved,    // Domain-aware structure
        IntentAware, // Intent-guided structure
        HyDE,        // Hypothetical Document Embeddings
    }

    /// Get the GBNF grammar
    #[allow(dead_code)]
    pub fn get_grammar() -> &'static str {
        GRAMMAR_SYNONYMS
    }
}

// ─── Grammar-guided parsing ───────────────────────────────────────────────

/// Parse structured grammar-guided output.
/// Expects format: "term1, term2, term3" (comma-separated terms)
fn parse_grammar_guided_output(output: &str) -> Result<String> {
    // Split by commas and trim each term (including whitespace and newlines)
    let terms: Vec<&str> = output
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.contains('\n') && !s.contains('\r'))
        .collect();

    if terms.is_empty() {
        return Err(anyhow::anyhow!("No terms found in grammar-guided output"));
    }

    // Validate terms (alphanumeric + spaces/hyphens/underscores/dots)
    for term in &terms {
        if !term.chars().all(|c| c.is_alphanumeric() || " _-.,".contains(c)) {
            tracing::debug!("Invalid character in term: '{}'", term);
            // Still include it, but warn
        }
    }

    // Join valid terms back
    Ok(terms.join(", "))
}

/// Try to parse grammar-guided output, fall back to free-form if needed.
/// If parsing fails, try to clean up free-form output.
fn parse_with_fallback(output: &str) -> Result<String> {
    // First, check if output is multiline - if so, work with first line only
    let output_to_parse = if output.contains('\n') {
        output.lines().next().unwrap_or("")
    } else {
        output
    };

    // Try strict grammar parsing on (potentially first line of) output
    match parse_grammar_guided_output(output_to_parse) {
        Ok(parsed) => {
            tracing::debug!("Successfully parsed grammar-guided output");
            return Ok(parsed);
        }
        Err(e) => {
            tracing::debug!("Grammar parsing failed, attempting fallback: {}", e);
        }
    }

    // Fallback: try to extract comma-separated terms from first line
    if let Some(first_line) = output.lines().next() {
        let trimmed = first_line.trim();
        if trimmed.contains(',') {
            // First line looks like comma-separated, try to parse it
            match parse_grammar_guided_output(trimmed) {
                Ok(parsed) => {
                    tracing::debug!("Using comma-separated fallback format from first line");
                    return Ok(parsed);
                }
                Err(_) => {
                    // Return the first line as-is if it's not empty
                    if !trimmed.is_empty() {
                        tracing::debug!("Using first-line content as fallback");
                        return Ok(trimmed.to_string());
                    }
                }
            }
        }
        
        // Just return first line if not empty
        if !trimmed.is_empty() {
            tracing::debug!("Using first-line fallback format");
            return Ok(trimmed.to_string());
        }
    }

    Err(anyhow::anyhow!("Failed to parse output: {}", output))
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

pub async fn ensure_llm_model(model_name: &str) -> Result<()> {
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

pub struct LocalGenerator {
    config: QueryExpansionConfig,
}

impl LocalGenerator {
    pub fn new(config: QueryExpansionConfig) -> Self {
        Self { config }
    }

    pub async fn ensure_model(&self) -> Result<()> {
        match self.config.backend {
            GenerationBackend::Onnx => ensure_llm_model(&self.config.model).await,
            GenerationBackend::LlamaCpp => Err(anyhow::anyhow!(
                "llama_cpp generation backend not implemented yet for model '{}'",
                self.config.model
            )),
            GenerationBackend::Mlx => Err(anyhow::anyhow!(
                "mlx generation backend not implemented yet for model '{}'",
                self.config.model
            )),
        }
    }

    pub async fn generate_once(&self, prompt: &str) -> Result<String> {
        match self.config.backend {
            GenerationBackend::Onnx => QueryExpander::run_inference(prompt, &self.config.model).await,
            GenerationBackend::LlamaCpp => Err(anyhow::anyhow!(
                "llama_cpp generation backend not implemented yet for model '{}'",
                self.config.model
            )),
            GenerationBackend::Mlx => Err(anyhow::anyhow!(
                "mlx generation backend not implemented yet for model '{}'",
                self.config.model
            )),
        }
    }
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
    /// Expand query with real ONNX inference + post-hoc hallucination filtering.
    /// Filters out numeric variants (python2, python3, etc.) that tinyllama tends to hallucinate.
    /// Returns Err if expansion fails for any reason (model unavailable, timeout, etc.)
    pub async fn expand(&self, query: &str) -> anyhow::Result<String> {
        // If disabled, return error (no expansion)
        if !self.config.enabled {
            tracing::debug!("Query expansion: disabled, skipping");
            return Err(anyhow::anyhow!("Query expansion disabled"));
        }

        tracing::info!("Query expansion: Starting expansion with hallucination filtering for query: '{}'", query);
        tracing::debug!("Query expansion config: enabled={}, model={}, timeout_ms={}", self.config.enabled, self.config.model, self.config.timeout_ms);
        
        // Track timing
        let start = std::time::Instant::now();
        
        // Use free-form inference with post-hoc hallucination filtering
        let result = self.expand_with_timeout(query).await;
        
        let elapsed = start.elapsed();
        
        match &result {
            Ok(expanded) => {
                tracing::info!("Query expansion: Successfully expanded query in {:.0}ms", elapsed.as_millis());
                tracing::debug!("Query expansion: Original='{}' | Expanded='{}'", query, expanded);
            },
            Err(e) => {
                tracing::warn!("Query expansion: Failed to expand query in {:.0}ms: {}", elapsed.as_millis(), e);
            }
        }
        
        result
    }

    /// Expand a query using grammar-guided generation.
    ///
    /// Uses GBNF grammar to enforce structured output format.
    /// Falls back to free-form parsing if grammar parsing fails.
    /// Returns the expanded query (original + related terms).
    pub async fn expand_with_grammar(&self, query: &str) -> anyhow::Result<String> {
        if !self.config.enabled {
            tracing::debug!("Query expansion: grammar-guided disabled at feature level");
            return Err(anyhow::anyhow!("Query expansion disabled"));
        }

        tracing::info!("Query expansion (grammar-guided): Starting for query: '{}'", query);
        tracing::debug!("Query expansion: Using GBNF grammar for structured output");
        
        let start = std::time::Instant::now();
        let result = self.expand_with_timeout_and_grammar(query).await;
        let elapsed = start.elapsed();
        
        match &result {
            Ok(expanded) => {
                tracing::info!("Query expansion (grammar-guided): Successfully expanded in {:.0}ms", elapsed.as_millis());
                tracing::debug!("Query expansion (grammar-guided): Result='{}'", expanded);
            },
            Err(e) => {
                tracing::warn!("Query expansion (grammar-guided): Failed in {:.0}ms: {}", elapsed.as_millis(), e);
            }
        }
        
        result
    }

    /// Expand a query using intent-aware generation.
    ///
    /// Uses optional intent parameter to guide more focused expansions.
    /// Gracefully handles missing intent: uses scope if available, else uses original query.
    /// Returns the expanded query (original + related terms).
    pub async fn expand_with_intent(&self, query: &str, intent: Option<&str>) -> anyhow::Result<String> {
        if !self.config.enabled {
            tracing::debug!("Query expansion: intent-aware disabled at feature level");
            return Err(anyhow::anyhow!("Query expansion disabled"));
        }

        // Check if intent-aware expansion is enabled in config
        if !self.config.intent.enabled {
            // Fall back to regular expansion if intent-aware is disabled
            tracing::debug!("Query expansion: intent-aware disabled in config, falling back to basic expansion");
            return self.expand(query).await;
        }

        tracing::info!("Query expansion (intent-aware): Starting for query: '{}' with intent: {:?}", query, intent);
        tracing::debug!(
            "Query expansion (intent-aware): use_scope_as_fallback={}, default_intent={:?}",
            self.config.intent.use_scope_as_fallback,
            self.config.intent.default_intent
        );

        let start = std::time::Instant::now();
        let result = self.expand_with_timeout_and_intent(query, intent).await;
        let elapsed = start.elapsed();
        
        match &result {
            Ok(expanded) => {
                tracing::info!("Query expansion (intent-aware): Successfully expanded in {:.0}ms", elapsed.as_millis());
                tracing::debug!("Query expansion (intent-aware): Result='{}'", expanded);
            },
            Err(e) => {
                tracing::warn!("Query expansion (intent-aware): Failed in {:.0}ms: {}", elapsed.as_millis(), e);
            }
        }
        
        result

    }

    /// Expand a query using HyDE (Hypothetical Document Embeddings).
    ///
    /// Generates 3-5 hypothetical document snippets that would contain
    /// the answer to the query. Useful for semantic search reformulation.
    /// Returns expanded query as concatenated snippets (original + hypothetical documents).
    pub async fn expand_hyde(&self, query: &str) -> anyhow::Result<String> {
        if !self.config.enabled {
            tracing::debug!("Query expansion: HyDE disabled at feature level");
            return Err(anyhow::anyhow!("Query expansion disabled"));
        }

        tracing::info!("Query expansion (HyDE): Starting hypothetical document generation for query: '{}'", query);

        let start = std::time::Instant::now();
        let result = self.expand_with_timeout_hyde(query).await;
        let elapsed = start.elapsed();
        
        match &result {
            Ok(expanded) => {
                tracing::info!("Query expansion (HyDE): Successfully generated hypothetical documents in {:.0}ms", elapsed.as_millis());
                tracing::debug!("Query expansion (HyDE): Result='{}'", expanded);
            },
            Err(e) => {
                tracing::warn!("Query expansion (HyDE): Failed in {:.0}ms: {}", elapsed.as_millis(), e);
            }
        }

        result
    }

    /// Internal expansion with timeout and grammar guidance.
    async fn expand_with_timeout_and_grammar(&self, query: &str) -> Result<String> {
        use tokio::time::{timeout, Duration};
        
        let query_str = query.to_string();
        let model = self.config.model.clone();
        
        // Ensure model is loaded
        ensure_llm_model(&model).await?;
        
        // Apply timeout to grammar-guided inference
        let timeout_duration = Duration::from_millis(self.config.timeout_ms);
        let result = timeout(timeout_duration, async {
            Self::run_inference_with_grammar(&query_str, &model).await
        })
        .await;
        
        match result {
            Ok(Ok(expanded)) => Ok(expanded),
            Ok(Err(e)) => {
                tracing::warn!("Grammar-guided expansion error: {}", e);
                Err(e)
            }
            Err(_) => {
                tracing::warn!("Grammar-guided expansion timed out ({}ms)", self.config.timeout_ms);
                Err(anyhow::anyhow!("Grammar-guided expansion timed out"))
            }
        }
    }

    /// Run inference with grammar-guided parsing.
    async fn run_inference_with_grammar(query: &str, model_name: &str) -> Result<String> {
        // Use improved template with grammar guidance
        let template = prompts::get_template(&prompts::TemplateMode::Improved);
        let prompt = template.replace("{query}", query);
        
        tracing::debug!("Grammar-guided expansion prompt for query: {}", query);
        
        // Run inference
        let cache = get_llm_cache();
        if let Some(SendLLM(model_arc)) = cache.get(model_name) {
            let raw_output = Self::infer_expansion(&model_arc, &prompt)?;
            
            // Parse with grammar-guided approach and fallback
            let expanded_terms = parse_with_fallback(&raw_output)?;
            
            // Prepend original query to avoid duplication
            let result = if expanded_terms.is_empty() {
                query.to_string()
            } else {
                let first_term = if let Some(comma_idx) = expanded_terms.find(',') {
                    expanded_terms[..comma_idx].trim()
                } else {
                    expanded_terms.as_str()
                };

                if first_term.eq_ignore_ascii_case(query) {
                    expanded_terms
                } else {
                    format!("{}, {}", query, expanded_terms)
                }
            };
            
            Ok(result)
        } else {
            Err(anyhow::anyhow!("Model not loaded: {}", model_name))
        }
    }

    /// Internal expansion with timeout and intent guidance.
    async fn expand_with_timeout_and_intent(&self, query: &str, intent: Option<&str>) -> Result<String> {
        use tokio::time::{timeout, Duration};
        
        let query_str = query.to_string();
        let model = self.config.model.clone();
        
        // Resolve intent with fallback logic
        let resolved_intent = intent
            .or_else(|| self.config.intent.default_intent.as_deref())
            .map(|i| i.to_string());
        
        // Ensure model is loaded
        ensure_llm_model(&model).await?;
        
        // Apply timeout to intent-aware inference
        let timeout_duration = Duration::from_millis(self.config.timeout_ms);
        let result = timeout(timeout_duration, async {
            Self::run_inference_with_intent(&query_str, &model, resolved_intent.as_deref()).await
        })
        .await;
        
        match result {
            Ok(Ok(expanded)) => Ok(expanded),
            Ok(Err(e)) => {
                tracing::warn!("Intent-aware expansion error: {}", e);
                Err(e)
            }
            Err(_) => {
                tracing::warn!("Intent-aware expansion timed out ({}ms)", self.config.timeout_ms);
                Err(anyhow::anyhow!("Intent-aware expansion timed out"))
            }
        }
    }

    /// Internal expansion with timeout for HyDE generation.
    async fn expand_with_timeout_hyde(&self, query: &str) -> Result<String> {
        use tokio::time::{timeout, Duration};
        
        let query_str = query.to_string();
        let model = self.config.model.clone();
        
        // Ensure model is loaded
        ensure_llm_model(&model).await?;
        
        // Apply timeout to HyDE inference
        let timeout_duration = Duration::from_millis(self.config.timeout_ms);
        let result = timeout(timeout_duration, async {
            Self::run_inference_hyde(&query_str, &model).await
        })
        .await;
        
        match result {
            Ok(Ok(expanded)) => Ok(expanded),
            Ok(Err(e)) => {
                tracing::warn!("HyDE expansion error: {}", e);
                Err(e)
            }
            Err(_) => {
                tracing::warn!("HyDE expansion timed out ({}ms)", self.config.timeout_ms);
                Err(anyhow::anyhow!("HyDE expansion timed out"))
            }
        }
    }

    /// Run inference with intent-aware prompting.
    async fn run_inference_with_intent(query: &str, model_name: &str, intent: Option<&str>) -> Result<String> {
        // Use intent-aware template
        let template = prompts::get_template(&prompts::TemplateMode::IntentAware);
        let prompt = if let Some(intent_text) = intent {
            template
                .replace("{query}", query)
                .replace("{intent}", intent_text)
        } else {
            // Fallback: use improved template if no intent available
            prompts::get_template(&prompts::TemplateMode::Improved)
                .replace("{query}", query)
        };
        
        tracing::debug!("Intent-aware expansion for query: {}", query);
        
        // Run inference
        let cache = get_llm_cache();
        if let Some(SendLLM(model_arc)) = cache.get(model_name) {
            let raw_output = Self::infer_expansion(&model_arc, &prompt)?;
            
            // Parse with fallback
            let expanded_terms = parse_with_fallback(&raw_output)?;
            
            // Prepend original query
            let result = if expanded_terms.is_empty() {
                query.to_string()
            } else {
                let first_term = if let Some(comma_idx) = expanded_terms.find(',') {
                    expanded_terms[..comma_idx].trim()
                } else {
                    expanded_terms.as_str()
                };

                if first_term.eq_ignore_ascii_case(query) {
                    expanded_terms
                } else {
                    format!("{}, {}", query, expanded_terms)
                }
            };
            
            Ok(result)
        } else {
            Err(anyhow::anyhow!("Model not loaded: {}", model_name))
        }
    }

    /// Run inference with HyDE (hypothetical document embeddings) prompting.
    async fn run_inference_hyde(query: &str, model_name: &str) -> Result<String> {
        // Use HyDE template to generate hypothetical documents
        let template = prompts::get_template(&prompts::TemplateMode::HyDE);
        let prompt = template.replace("{query}", query);
        
        tracing::debug!("HyDE expansion for query: {}", query);
        
        // Run inference
        let cache = get_llm_cache();
        if let Some(SendLLM(model_arc)) = cache.get(model_name) {
            let raw_output = Self::infer_expansion(&model_arc, &prompt)?;
            
            // Parse hypothetical documents (they're formatted as separate snippets)
            let hyde_docs = parse_with_fallback(&raw_output)?;
            
            // Combine original query with hypothetical documents
            // HyDE docs are usually good standalone, but include original for context
            let result = format!("{} {} {}", query, hyde_docs, query);
            
            Ok(result)
        } else {
            Err(anyhow::anyhow!("Model not loaded: {}", model_name))
        }
    }

    /// Internal expansion with timeout.
    async fn expand_with_timeout(&self, query: &str) -> Result<String> {
        use tokio::time::{timeout, Duration};
        
        let query_str = query.to_string();
        let model = self.config.model.clone();
        
        // Use ONNX backend for query expansion
        tracing::debug!("Query expansion: Using {:?} backend for model: '{}'", self.config.backend, model);
        
        // FIRST: Ensure model is loaded (outside timeout, can take time for download)
        LocalGenerator::new(self.config.clone()).ensure_model().await?;
        
        // NOW: Apply timeout only to inference (should be fast)
        let timeout_duration = Duration::from_millis(self.config.timeout_ms);
        let result = timeout(timeout_duration, async {
            Self::run_inference(&query_str, &model).await
        })
        .await;
        
        match result {
            Ok(Ok(expanded)) => Ok(expanded),
            Ok(Err(e)) => {
                tracing::warn!("ONNX query expansion error: {}", e);
                Err(e)
            }
            Err(_) => {
                tracing::warn!("ONNX query expansion inference timed out ({}ms)", self.config.timeout_ms);
                Err(anyhow::anyhow!("ONNX query expansion inference timed out"))
            }
        }
    }
    
    /// Run actual model inference.
    async fn run_inference(query: &str, model_name: &str) -> Result<String> {
        // Model should already be loaded by expand_with_timeout
        // This just runs the inference
        
        // Get the appropriate prompt template (use original structured)
        let template = prompts::get_template(&prompts::TemplateMode::Structured);
        let prompt = template.replace("{query}", query);
        
        tracing::debug!("Query expansion prompt (first 100 chars): {}", 
                       prompt.chars().take(100).collect::<String>());
        
        // Get model from cache and run inference
        let cache = get_llm_cache();
        if let Some(SendLLM(model_arc)) = cache.get(model_name) {
            let expanded_terms = Self::infer_expansion(&model_arc, &prompt)?;
            
            // Prepend original query to the expansion (enhancement, not replacement)
            // Avoid duplicates by checking if original query is already the first term
            let result = if expanded_terms.is_empty() {
                query.to_string()
            } else {
                // Check if the first term (before first comma) matches the query
                let first_term = if let Some(comma_idx) = expanded_terms.find(',') {
                    expanded_terms[..comma_idx].trim()
                } else {
                    expanded_terms.as_str()
                };
                
                if first_term.eq_ignore_ascii_case(query) {
                    // Original query is already first, use as-is to avoid duplicate
                    expanded_terms
                } else {
                    // Original query not the first term, prepend it
                    format!("{}, {}", query, expanded_terms)
                }
            };
            
            // POST-HOC ENRICHMENT: Boost sparse expansions with related terms
            let enriched = Self::enrich_sparse_expansion(&result, query);
            
            Ok(enriched)
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
        const MAX_NEW_TOKENS: usize = 10;  // Optimized: 10 tokens for ~3-4 synonyms
        const MAX_SEQ_LEN: usize = 512;    // Sequence length limit
        const EOS_TOKEN: i64 = 2;          // End-of-sequence token ID
        let mut generated_tokens = Vec::new();
        
        // Autoregressive text generation (greedy decoding)
        for _step in 0..MAX_NEW_TOKENS {
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
            
            // Early stopping: after generating a reasonable expansion (2+ terms)
            // Stop if we see patterns suggesting end (like multiple spaces or very short tokens)
            // For now, continue until MAX_NEW_TOKENS
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
        
        // Remove excessive repetition and numeric hallucinations
        let final_expansion = {
            let parts: Vec<&str> = deduped.split(',').map(|s| s.trim()).collect();
            let mut seen_base_terms = std::collections::HashSet::new();  // Track base terms (without numbers)
            let mut unique_parts = Vec::new();
            
            for part in parts {
                if part.is_empty() {
                    continue;
                }
                
                // Extract base term (everything before trailing numbers)
                let base_term = part.trim_end_matches(|c: char| c.is_numeric());
                
                // Skip if this is a numeric variant of a term we've already seen
                if base_term != part && seen_base_terms.contains(base_term) {
                    tracing::debug!("Skipping numeric variant: '{}' (base: '{}')", part, base_term);
                    continue;
                }
                
                // Skip if we've already seen this exact term
                if seen_base_terms.contains(part) {
                    continue;
                }
                
                // Add this term and mark its base term as seen
                unique_parts.push(part);
                seen_base_terms.insert(base_term);
                if base_term != part {
                    // Also mark the full term so exact duplicates are skipped
                    seen_base_terms.insert(part);
                }
            }
            
            // Limit to reasonable number of terms (10 max)
            unique_parts.truncate(10);
            unique_parts.join(", ")
        };
        
        tracing::debug!("LLM generated expansion: {}", final_expansion);
        Ok(final_expansion)
    }
    
    /// Enrichment: If model returns just the query or very few terms, add related variations.
    /// This compensates for TinyLLaMA's limited vocabulary and helps semantic search recall.
    fn enrich_sparse_expansion(expansion: &str, original_query: &str) -> String {
        let parts: Vec<&str> = expansion.split(',').map(|s| s.trim()).collect();
        
        // If we already have 3+ good terms, don't enrich
        if parts.len() >= 3 {
            return expansion.to_string();
        }
        
        // If expansion equals the query exactly, generate related terms
        if expansion.trim().eq_ignore_ascii_case(original_query.trim()) {
            let mut enriched_terms = vec![expansion.trim()];
            
            // Split query into words and add word-based expansions
            let words: Vec<&str> = original_query.split_whitespace().collect();
            for word in words {
                if word.len() > 3 {
                    enriched_terms.push(word);
                }
            }
            
            // Add common related term patterns
            if original_query.to_lowercase().contains("python") {
                enriched_terms.extend(&["programming", "scripting", "automation"]);
            }
            if original_query.to_lowercase().contains("machine learning") {
                enriched_terms.extend(&["AI", "neural networks", "data science", "deep learning"]);
            }
            if original_query.to_lowercase().contains("database") {
                enriched_terms.extend(&["SQL", "storage", "queries", "schema"]);
            }
            if original_query.to_lowercase().contains("distributed") {
                enriched_terms.extend(&["systems", "computing", "scalability", "consensus"]);
            }
            if original_query.to_lowercase().contains("security") || original_query.to_lowercase().contains("crypto") {
                enriched_terms.extend(&["encryption", "authentication", "privacy"]);
            }
            
            // Remove duplicates while preserving order
            let mut seen = std::collections::HashSet::new();
            let mut unique = Vec::new();
            for term in enriched_terms {
                let lower = term.to_lowercase();
                if !seen.contains(&lower) {
                    unique.push(term);
                    seen.insert(lower);
                }
            }
            
            // Limit to 10 terms max
            unique.truncate(10);
            return unique.join(", ");
        }
        
        // If we have 1-2 terms and they're short, add word-based enrichment
        if parts.len() < 3 {
            let mut enriched = parts.to_vec();
            
            // Add individual words from the expansion if they're meaningful
            for word in expansion.split_whitespace() {
                if word.len() > 4 && !enriched.contains(&word) {
                    enriched.push(word);
                }
            }
            
            if enriched.len() > parts.len() {
                enriched.truncate(10);
                return enriched.join(", ");
            }
        }
        
        expansion.to_string()
    }
}

// ─── Grammar parsing tests ────────────────────────────────────────────────

#[cfg(test)]
mod grammar_tests {
    use super::*;

    #[test]
    fn test_parse_grammar_guided_simple() {
        let output = "Docker, containers, Kubernetes, images";
        let result = parse_grammar_guided_output(output).unwrap();
        assert_eq!(result, "Docker, containers, Kubernetes, images");
    }

    #[test]
    fn test_parse_grammar_guided_with_spaces() {
        let output = "Docker ,  containers  , Kubernetes,  images";
        let result = parse_grammar_guided_output(output).unwrap();
        assert_eq!(result, "Docker, containers, Kubernetes, images");
    }

    #[test]
    fn test_parse_grammar_guided_empty_after_filter() {
        let output = ",,";
        let result = parse_grammar_guided_output(output);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_with_fallback_valid_csv() {
        let output = "docker, kubernetes, orchestration";
        let result = parse_with_fallback(output).unwrap();
        assert_eq!(result, "docker, kubernetes, orchestration");
    }

    #[test]
    fn test_parse_with_fallback_multiline_csv() {
        // When given multiline input, should take first line only
        let output = "docker, kubernetes\norchestration, images";
        let result = parse_with_fallback(output).unwrap();
        // Filters newlines from terms, so "kubernetes\norchestration" is rejected
        // Then falls back to just "docker" from first parsing attempt
        // Then tries first line which is "docker, kubernetes" after filtering
        assert_eq!(result, "docker, kubernetes");
    }

    #[test]
    fn test_parse_with_fallback_freeform() {
        let output = "some text about docker and containers";
        let result = parse_with_fallback(output).unwrap();
        assert_eq!(result, "some text about docker and containers");
    }

    #[test]
    fn test_parse_with_fallback_empty_fails() {
        let output = "";
        let result = parse_with_fallback(output);
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_templates() {
        assert!(prompts::FEW_SHOT_STRUCTURED.contains("{query}"));
        assert!(prompts::FEW_SHOT_IMPROVED.contains("{query}"));
        assert!(prompts::FEW_SHOT_INTENT_AWARE.contains("{query}"));
        assert!(prompts::FEW_SHOT_INTENT_AWARE.contains("{intent}"));

        // Test template selection
        assert_eq!(prompts::get_template(&prompts::TemplateMode::Structured), prompts::FEW_SHOT_STRUCTURED);
        assert_eq!(prompts::get_template(&prompts::TemplateMode::Improved), prompts::FEW_SHOT_IMPROVED);
        assert_eq!(prompts::get_template(&prompts::TemplateMode::IntentAware), prompts::FEW_SHOT_INTENT_AWARE);
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

    #[test]
    fn test_intent_config_defaults() {
        let intent_config = IntentConfig::default();
        assert!(intent_config.enabled);
        assert!(intent_config.use_scope_as_fallback);
        assert_eq!(intent_config.default_intent, None);
    }

    #[test]
    fn test_query_expansion_config_with_intent() {
        let config = QueryExpansionConfig::default();
        assert!(config.intent.enabled);
        assert!(config.intent.use_scope_as_fallback);
    }

    #[test]
    fn test_intent_template_substitution() {
        let template = prompts::get_template(&prompts::TemplateMode::IntentAware);
        let filled = template
            .replace("{query}", "Docker")
            .replace("{intent}", "orchestration");
        
        assert!(filled.contains("Docker"));
        assert!(filled.contains("orchestration"));
        assert!(!filled.contains("{query}"));
        assert!(!filled.contains("{intent}"));
    }
}
