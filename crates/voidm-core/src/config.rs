use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub embeddings: EmbeddingsConfig,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub insert: InsertConfig,
    #[serde(default)]
    pub enrichment: EnrichmentConfig,
    #[serde(default)]
    pub redaction: crate::redactor::RedactionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database backend: "sqlite" or "neo4j" (default: "sqlite")
    #[serde(default = "default_backend")]
    pub backend: String,
    
    /// SQLite configuration (used when backend = "sqlite")
    #[serde(default)]
    pub sqlite: Option<SqliteConfig>,
    
    /// Path to SQLite database file - DEPRECATED, use [database.sqlite].path instead
    /// This is kept for backward compatibility
    #[serde(default = "default_sqlite_path")]
    pub sqlite_path: String,
    
    /// Neo4j connection parameters (used when backend = "neo4j")
    #[serde(default)]
    pub neo4j: Option<Neo4jConfig>,
    
    /// Legacy field for backward compatibility
    pub path: Option<String>,
}

/// SQLite configuration section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConfig {
    /// Path to SQLite database file (optional, defaults to ~/.local/share/voidm/memories.db)
    /// Supports ~ for home directory
    #[serde(default)]
    pub path: Option<String>,
}

fn default_backend() -> String {
    "sqlite".to_string()
}

fn default_sqlite_path() -> String {
    let mut path = dirs::data_local_dir().expect("Cannot find data directory");
    path.push("voidm");
    path.push("memories.db");
    path.to_string_lossy().to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neo4jConfig {
    /// Neo4j Bolt URI (default: bolt://localhost:7687)
    #[serde(default = "default_neo4j_uri")]
    pub uri: String,
    
    /// Neo4j username (default: "neo4j")
    #[serde(default = "default_neo4j_user")]
    pub username: String,
    
    /// Neo4j password (default: "password")
    #[serde(default = "default_neo4j_password")]
    pub password: String,
}

fn default_neo4j_uri() -> String {
    "bolt://localhost:7687".to_string()
}

fn default_neo4j_user() -> String {
    "neo4j".to_string()
}

fn default_neo4j_password() -> String {
    "password".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsConfig {
    pub enabled: bool,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    pub mode: String,
    pub default_limit: usize,
    /// Minimum score threshold for hybrid mode results (0.0–1.0). Default: 0.3.
    pub min_score: f32,
    /// Score decay per hop for graph-expanded neighbors. neighbor_score = parent_score * decay^depth.
    pub neighbor_decay: f32,
    /// Minimum score for graph-expanded neighbors to be included. Default: 0.2.
    pub neighbor_min_score: f32,
    /// Default traversal depth for --include-neighbors. Hard cap: 3.
    pub default_neighbor_depth: u8,
    /// Edge types to traverse by default for neighbor expansion.
    pub default_edge_types: Vec<String>,
    /// Reranker configuration (optional).
    #[serde(default)]
    pub reranker: Option<RerankerConfig>,
    /// Query expansion configuration (optional).
    #[serde(default)]
    pub query_expansion: Option<QueryExpansionConfig>,
    /// Graph-aware retrieval configuration (optional).
    #[serde(default)]
    pub graph_retrieval: Option<crate::graph_retrieval::GraphRetrievalConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassageExtractionConfig {
    /// Enable passage extraction for reranking (default: true)
    #[serde(default = "default_passage_extraction_enabled")]
    pub enabled: bool,
    /// Number of context sentences before/after match (default: 1)
    #[serde(default = "default_context_sentences")]
    pub context_sentences: usize,
    /// Fallback length if no match found (default: 400)
    #[serde(default = "default_fallback_length")]
    pub fallback_length: usize,
    /// Minimum passage length to return (default: 50)
    #[serde(default = "default_min_passage_length")]
    pub min_passage_length: usize,
}

impl Default for PassageExtractionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            context_sentences: 1,
            fallback_length: 400,
            min_passage_length: 50,
        }
    }
}

fn default_passage_extraction_enabled() -> bool {
    true
}

fn default_context_sentences() -> usize {
    1
}

fn default_fallback_length() -> usize {
    400
}

fn default_min_passage_length() -> usize {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankerConfig {
    /// Enable reranking (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Model name: "ms-marco-TinyBERT-L-2" (default)
    #[serde(default = "default_reranker_model")]
    pub model: String,
    /// Apply reranker to top-k results only (default: 15).
    #[serde(default = "default_reranker_top_k")]
    pub apply_to_top_k: usize,
    /// Passage extraction configuration for better reranking on long documents
    #[serde(default)]
    pub passage_extraction: PassageExtractionConfig,
}

impl Default for RerankerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: default_reranker_model(),
            apply_to_top_k: default_reranker_top_k(),
            passage_extraction: PassageExtractionConfig::default(),
        }
    }
}

fn default_reranker_model() -> String {
    "ms-marco-MiniLM-L-6-v2".into()
}

fn default_reranker_top_k() -> usize {
    15
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryExpansionConfig {
    /// Enable query expansion (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Model name: "tinyllama" (ONNX, default) or "tobil/qmd-query-expansion-1.7B" (GGUF, opt-in).
    /// App auto-detects backend based on model name (models with "tobil" or "qmd" use GGUF).
    #[serde(default = "default_query_expansion_model")]
    pub model: String,
    /// Maximum time to wait for expansion in milliseconds (default: 300).
    #[serde(default = "default_query_expansion_timeout_ms")]
    pub timeout_ms: u64,
    /// Intent-aware expansion configuration.
    #[serde(default)]
    pub intent: IntentConfig,
}

impl Default for QueryExpansionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: default_query_expansion_model(),
            timeout_ms: default_query_expansion_timeout_ms(),
            intent: IntentConfig::default(),
        }
    }
}

fn default_query_expansion_model() -> String {
    "tinyllama".into()
}

fn default_query_expansion_timeout_ms() -> u64 {
    300
}

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

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            use_scope_as_fallback: true,
            default_intent: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertConfig {
    pub auto_link_threshold: f32,
    pub duplicate_threshold: f32,
    pub auto_link_limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentConfig {
    #[serde(default)]
    pub semantic_dedup: Option<crate::semantic_dedup::SemanticDedupConfig>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            backend: "sqlite".to_string(),
            sqlite: None,
            sqlite_path: default_sqlite_path(),
            neo4j: None,
            path: None,
        }
    }
}

impl Default for EmbeddingsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            model: "Xenova/all-MiniLM-L6-v2".into(),
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            mode: "hybrid".into(),
            default_limit: 10,
            min_score: 0.3,
            neighbor_decay: 0.7,
            neighbor_min_score: 0.2,
            default_neighbor_depth: 1,
            default_edge_types: vec![
                "PART_OF".into(),
                "SUPPORTS".into(),
                "DERIVED_FROM".into(),
                "EXEMPLIFIES".into(),
            ],
            reranker: None,
            query_expansion: None,
            graph_retrieval: None,
        }
    }
}

impl Default for InsertConfig {
    fn default() -> Self {
        Self {
            auto_link_threshold: 0.7,
            duplicate_threshold: 0.95,
            auto_link_limit: 5,
        }
    }
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            semantic_dedup: Some(crate::semantic_dedup::SemanticDedupConfig::default()),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: Default::default(),
            embeddings: Default::default(),
            search: Default::default(),
            insert: Default::default(),
            enrichment: Default::default(),
            redaction: Default::default(),
        }
    }
}

impl Config {
    /// Load config from disk, merging with defaults. Never fails — missing file = all defaults.
    pub fn load() -> Self {
        let path = config_path();
        if let Some(p) = &path {
            if p.exists() {
                match std::fs::read_to_string(p) {
                    Ok(s) => match toml::from_str::<Config>(&s) {
                        Ok(c) => return c,
                        Err(e) => tracing::warn!("Failed to parse config {}: {}", p.display(), e),
                    },
                    Err(e) => tracing::warn!("Failed to read config {}: {}", p.display(), e),
                }
            }
        }
        Config::default()
    }

    /// Resolve the DB path: --db flag > $VOIDM_DB > config > XDG > ~/.local/share > ~/.voidm
    pub fn db_path(&self, override_path: Option<&str>) -> PathBuf {
        if let Some(p) = override_path {
            return PathBuf::from(p);
        }
        if let Ok(p) = std::env::var("VOIDM_DB") {
            if !p.is_empty() {
                return PathBuf::from(p);
            }
        }
        // Check new [database.sqlite].path field first
        if let Some(sqlite_config) = &self.database.sqlite {
            if let Some(p) = &sqlite_config.path {
                if !p.is_empty() {
                    return PathBuf::from(shellexpand(p));
                }
            }
        }
        // Legacy sqlite_path field for backward compatibility
        if !self.database.sqlite_path.is_empty() {
            return PathBuf::from(shellexpand(&self.database.sqlite_path));
        }
        // Legacy path field for backward compatibility
        if let Some(p) = &self.database.path {
            if !p.is_empty() {
                return PathBuf::from(shellexpand(p));
            }
        }
        // XDG_DATA_HOME
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("voidm/memories.db");
            }
        }
        // ~/.local/share/voidm/memories.db
        if let Some(home) = dirs::home_dir() {
            let p = home.join(".local/share/voidm/memories.db");
            return p;
        }
        // Fallback
        PathBuf::from(".voidm/memories.db")
    }
}

pub fn config_path_display() -> String {
    config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".into())
}

fn config_path() -> Option<PathBuf> {
    // 1. Custom config home via environment variable (cross-platform)
    if let Ok(config_home) = std::env::var("VOIDM_CONFIG_HOME") {
        if !config_home.is_empty() {
            return Some(PathBuf::from(config_home).join("config.toml"));
        }
    }
    
    // 2. Platform-specific config directory (macOS: ~/Library/Application Support, Windows: %APPDATA%, Linux: ~/.config)
    if let Some(config_dir) = dirs::config_dir() {
        return Some(config_dir.join("voidm/config.toml"));
    }
    
    // 3. Fallback: XDG_CONFIG_HOME (Linux)
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("voidm/config.toml"));
        }
    }
    
    // 4. Last resort: ~/.config/voidm/config.toml
    dirs::home_dir().map(|h| h.join(".config/voidm/config.toml"))
}

fn shellexpand(s: &str) -> String {
    if s.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &s[1..]);
        }
    }
    s.to_string()
}

pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path().context("Cannot determine config path")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let s = toml::to_string_pretty(config)?;
    std::fs::write(&path, s)?;
    Ok(())
}

/// Default configuration file content for `voidm config install`
/// All parameters are commented with examples and descriptions
pub const DEFAULT_CONFIG_CONTENT: &str = r#"# voidm Configuration File
# Location: ~/.config/voidm/config.toml (or platform-specific config directory)
# 
# This file configures all aspects of voidm: database, embeddings, search,
# and advanced features like reranking and semantic deduplication.

# ============================================================================
# DATABASE CONFIGURATION
# ============================================================================
# voidm supports two database backends: SQLite (default) and Neo4j

[database]
# Backend choice: "sqlite" (default) or "neo4j"
backend = "sqlite"

# SQLite configuration (used when backend = "sqlite")
[database.sqlite]
# Path to SQLite database file
# Supports ~ for home directory expansion
# Default: ~/.local/share/voidm/memories.db (macOS/Linux)
#          %APPDATA%\voidm\memories.db (Windows)
path = "~/.local/share/voidm/memories.db"

# Neo4j configuration (uncomment to use Neo4j instead of SQLite)
# [database.neo4j]
# uri = "bolt://localhost:7687"
# username = "neo4j"
# password = "password"

# ============================================================================
# EMBEDDINGS CONFIGURATION
# ============================================================================
# Controls semantic search via embeddings

[embeddings]
# Enable/disable embeddings for semantic search
enabled = true

# Embedding model to use
# Recommended: "Xenova/all-MiniLM-L6-v2" (384 dimensions, fast, good quality)
# Alternatives:
#   - "Xenova/all-mpnet-base-v2" (768 dims, slower, higher quality)
#   - "intfloat/e5-small" (384 dims, fast)
#   - "intfloat/e5-large" (1024 dims, slower, best quality)
model = "Xenova/all-MiniLM-L6-v2"

# ============================================================================
# SEARCH CONFIGURATION
# ============================================================================
# Controls how search results are ranked and returned

[search]
# Search ranking mode:
#   - "hybrid-rrf": Reciprocal Rank Fusion (vector + BM25 + fuzzy) [RECOMMENDED]
#   - "hybrid": Weighted average (vector 0.5 + BM25 0.3 + fuzzy 0.2)
#   - "semantic": Vector search only
#   - "keyword" or "bm25": Full-text BM25 search only
#   - "fuzzy": Fuzzy string matching only
mode = "hybrid-rrf"

# Default number of results to return
default_limit = 10

# Minimum relevance score (0.0-1.0) to include a result
# Lower = more results, higher = fewer but more relevant results
min_score = 0.3

# Graph retrieval: Find related memories through tags and concepts
[search.graph_retrieval]
# Enable/disable graph-aware retrieval
enabled = true

# Maximum hops for concept traversal (1-3 recommended)
# 1 hop: ~10 results per query, <100ms
# 2 hops: ~50 results per query, <500ms [RECOMMENDED]
# 3+ hops: exponential growth, >1s latency (not recommended)
max_concept_hops = 2

# Tag-based retrieval: Find memories sharing similar tags
[search.graph_retrieval.tags]
enabled = true
# Minimum overlapping tags required
min_overlap = 3
# Minimum percentage overlap (overlap_count / max(query_tags, memory_tags) * 100)
min_percentage = 50.0
# Score multiplier for tag-related results (0.0-1.0)
decay_factor = 0.7
# Max tag-related results per direct result
limit = 5

# Concept-based retrieval: Find memories through ontology relationships
[search.graph_retrieval.concepts]
enabled = true
# Optional: override global max_concept_hops for concepts only
# max_hops = 2
# Score multiplier per hop: score = decay_factor^hops
decay_factor = 0.7
# Max concept-related results per direct result
limit = 3

# Reranker: Cross-encoder for better search quality (optional, slower)
[search.reranker]
# Enable/disable reranking
enabled = false

# ONNX-compatible reranker models:
#   - "ms-marco-TinyBERT-L-2": Fast (11MB, 0.6s) [FASTEST]
#   - "ms-marco-MiniLM-L-6-v2": Balanced (100MB, ~1s) [RECOMMENDED]
#   - "mmarco-mMiniLMv2-L12-H384-v1": Multilingual (110MB, ~10s)
#   - "qnli-distilroberta-base": High quality (250MB, ~30s) [BEST QUALITY]
model = "ms-marco-MiniLM-L-6-v2"

# Rerank only top-k results (0-100, higher = more thorough but slower)
apply_to_top_k = 15

# Passage extraction: Extract relevant sentences from results
[search.reranker.passage_extraction]
enabled = true
# Include ±N sentences around matched terms for context
context_sentences = 1
# If no query terms found, use first N characters
fallback_length = 400
# Don't return passages shorter than this
min_passage_length = 50

# Query expansion: Expand queries with synonyms and related terms
[search.query_expansion]
# Enable/disable automatic query expansion
enabled = false

# Expansion model:
#   - "tinyllama": 1.1B, good quality + fast [RECOMMENDED]
#   - "phi-2": 2.7B, highest quality but slower
#   - "gpt2-small": Lightweight, acceptable quality [FASTEST]
#   - "tobil/qmd-query-expansion-1.7B": Custom GGUF model
model = "tinyllama"

# Maximum time to wait for query expansion (milliseconds)
timeout_ms = 300

# Intent-aware expansion: guide expansion toward specific context
[search.query_expansion.intent]
enabled = true
# Use --scope parameter as fallback intent
use_scope_as_fallback = true

# ============================================================================
# INSERT CONFIGURATION
# ============================================================================
# Controls behavior when adding new memories

[insert]
# Auto-link threshold: automatically link memories with similarity >= this value
# (0.0-1.0, higher = stricter)
auto_link_threshold = 0.7

# Duplicate threshold: mark memories as duplicates if similarity >= this value
# (0.0-1.0, typically 0.95+)
duplicate_threshold = 0.95

# Maximum number of auto-links to create per memory
auto_link_limit = 5

# ============================================================================
# ENRICHMENT CONFIGURATION
# ============================================================================
# Advanced features: semantic dedup, NLI reasoning

# Semantic deduplication: use embeddings to improve concept merging
[enrichment.semantic_dedup]
enabled = true
# Model identifier (for caching)
model = "minilm-l6-v2"
# Semantic similarity threshold (0.0-1.0)
# 0.50-0.65: Liberal merging (more merges, some false positives)
# 0.70-0.80: Balanced [RECOMMENDED default 0.75]
# 0.85-0.95: Conservative (fewer merges, higher confidence)
threshold = 0.75
# Use ONNX runtime optimization (future)
use_onnx = false

# NLI (Natural Language Inference): auto-suggest relation types between concepts
[enrichment.nli]
# Enable/disable NLI reasoning (slow, ~300MB model)
enabled = false
# Number of relation candidates to suggest
top_k = 5

# ============================================================================
# REDACTION CONFIGURATION
# ============================================================================
# Automatically redact sensitive secrets from memory content
# Prevents API keys, passwords, tokens from leaking into search results

[redaction]
# Master switch: enable/disable all redaction
enabled = true

# API keys redaction (OpenAI sk-..., AWS AKIA..., etc.)
[redaction.api_keys]
enabled = true
strategy = "mask"       # "mask" (preserve edges) or "remove" (delete)
prefix_length = 3       # Keep first N characters
suffix_length = 2       # Keep last N characters

# JWT tokens redaction (eyJ... format)
[redaction.jwt_tokens]
enabled = true
strategy = "mask"
prefix_length = 3
suffix_length = 3

# Database connection strings redaction
[redaction.db_connections]
enabled = true
strategy = "mask"       # Special: hides credentials, shows host/db

# Auth tokens redaction (Bearer tokens, session_id=...)
[redaction.auth_tokens]
enabled = true
strategy = "mask"
prefix_length = 2
suffix_length = 2

# Email addresses redaction
[redaction.emails]
enabled = true
strategy = "mask"
prefix_length = 1
suffix_length = 3

# ============================================================================
# USAGE EXAMPLES
# ============================================================================
#
# Show current configuration:
#   voidm config show
#
# Install default config:
#   voidm config install
#   voidm config install --force      # Override existing
#
# Modify single setting:
#   voidm config set search.mode hybrid-rrf
#   voidm config set embeddings.enabled true
#   voidm config set search.default_limit 20
#
# Search with options:
#   voidm search "query"
#   voidm search "query" --graph-retrieval      # Force enable
#   voidm search "query" --no-graph-retrieval   # Force disable
#   voidm search "query" --query-expand true    # Override config
#   voidm search "query" --reranker             # Use reranking
#
# Add memory with auto-linking:
#   voidm add -t episodic "content" --importance 5
#
# ============================================================================
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_neo4j_config() {
        let toml_str = r#"
[database.neo4j]
uri = "bolt://localhost:7687"
username = "neo4j"
password = "neo4jneo4j"
"#;
        
        let config: Config = toml::from_str(toml_str).expect("Failed to parse");
        assert!(config.database.neo4j.is_some(), "neo4j config should be parsed");
        if let Some(nc) = &config.database.neo4j {
            assert_eq!(nc.uri, "bolt://localhost:7687");
            assert_eq!(nc.username, "neo4j");
            assert_eq!(nc.password, "neo4jneo4j");
        }
    }

    #[test]
    fn test_config_with_both_backends() {
        let toml_str = r#"
[database]
backend = "sqlite"

[database.sqlite]
path = "~/.local/share/voidm/memories.db"

[database.neo4j]
uri = "bolt://localhost:7687"
username = "neo4j"
password = "neo4jneo4j"
"#;
        
        let config: Config = toml::from_str(toml_str).expect("Failed to parse");
        
        // Verify backend selection
        assert_eq!(config.database.backend, "sqlite");
        
        // Verify SQLite config is present
        assert!(config.database.sqlite.is_some());
        if let Some(sqlite) = &config.database.sqlite {
            assert_eq!(sqlite.path, Some("~/.local/share/voidm/memories.db".to_string()));
        }
        
        // Verify Neo4j config is present
        assert!(config.database.neo4j.is_some());
        if let Some(neo4j) = &config.database.neo4j {
            assert_eq!(neo4j.uri, "bolt://localhost:7687");
            assert_eq!(neo4j.username, "neo4j");
        }
    }

    #[test]
    fn test_switch_to_neo4j_backend() {
        let toml_str = r#"
[database]
backend = "neo4j"

[database.sqlite]
path = "~/.local/share/voidm/memories.db"

[database.neo4j]
uri = "bolt://localhost:7687"
username = "neo4j"
password = "neo4jneo4j"
"#;
        
        let config: Config = toml::from_str(toml_str).expect("Failed to parse");
        
        // Verify backend is switched to neo4j
        assert_eq!(config.database.backend, "neo4j");
        
        // Both are still configured
        assert!(config.database.sqlite.is_some());
        assert!(config.database.neo4j.is_some());
    }

    #[test]
    fn test_reranker_config_defaults() {
        let toml_str = r#"
[search]
mode = "hybrid"
default_limit = 10
min_score = 0.3
neighbor_decay = 0.7
neighbor_min_score = 0.2
default_neighbor_depth = 1
default_edge_types = ["PART_OF", "SUPPORTS"]
"#;
        let config: Config = toml::from_str(toml_str).expect("Failed to parse");
        assert!(config.search.reranker.is_none(), "reranker should be absent by default");
    }

    #[test]
    fn test_reranker_config_enabled() {
        let toml_str = r#"
[search]
mode = "hybrid"
default_limit = 10
min_score = 0.3
neighbor_decay = 0.7
neighbor_min_score = 0.2
default_neighbor_depth = 1
default_edge_types = ["PART_OF"]

[search.reranker]
enabled = true
model = "bge-reranker-base"
apply_to_top_k = 15
"#;
        let config: Config = toml::from_str(toml_str).expect("Failed to parse");
        assert!(config.search.reranker.is_some(), "reranker config should be parsed");
        if let Some(r) = &config.search.reranker {
            assert_eq!(r.enabled, true);
            assert_eq!(r.model, "bge-reranker-base");
            assert_eq!(r.apply_to_top_k, 15);
        }
    }

    #[test]
    fn test_reranker_config_partial() {
        let toml_str = r#"
[search]
mode = "hybrid"
default_limit = 10
min_score = 0.3
neighbor_decay = 0.7
neighbor_min_score = 0.2
default_neighbor_depth = 1
default_edge_types = ["PART_OF"]

[search.reranker]
enabled = true
"#;
        let config: Config = toml::from_str(toml_str).expect("Failed to parse");
        if let Some(r) = &config.search.reranker {
            assert_eq!(r.enabled, true);
            assert_eq!(r.model, "ms-marco-MiniLM-L-6-v2", "should use default model");
            assert_eq!(r.apply_to_top_k, 15, "should use default top_k");
        }
    }
}
