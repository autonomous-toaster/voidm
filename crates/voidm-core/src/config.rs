use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use voidm_embeddings::PassageExtractionConfig;

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
    
    /// Neo4j database name (default: "neo4j")
    #[serde(default = "default_neo4j_database")]
    pub database: String,
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

fn default_neo4j_database() -> String {
    "neo4j".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsConfig {
    pub enabled: bool,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalConfig {
    /// Enable vector (embedding) search signal. Default: true (if embeddings enabled).
    #[serde(default = "default_signal_vector")]
    pub vector: bool,
    /// Enable BM25 (full-text search) signal. Default: true.
    #[serde(default = "default_signal_bm25")]
    pub bm25: bool,
    /// Enable fuzzy (Jaro-Winkler) search signal. Default: true.
    #[serde(default = "default_signal_fuzzy")]
    pub fuzzy: bool,
}

fn default_signal_vector() -> bool {
    true
}

fn default_signal_bm25() -> bool {
    true
}

fn default_signal_fuzzy() -> bool {
    true
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            vector: true,
            bm25: true,
            fuzzy: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Ranking method. Currently only "rrf" is supported (RRF is always used).
    /// "hybrid" maps to "rrf" for backward compatibility.
    /// Kept for backward compatibility only.
    #[serde(default = "default_search_mode")]
    pub mode: String,
    pub default_limit: usize,
    /// Minimum score threshold for RRF results (0.0–1.0). Default: 0.3.
    pub min_score: f32,
    /// RRF signal configuration (which signals to include in fusion).
    #[serde(default)]
    pub signals: SignalConfig,
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
    #[cfg(feature = "query-expansion")]
    pub query_expansion: Option<voidm_query_expansion::QueryExpansionConfig>,
    /// Query expansion configuration (optional) - placeholder when feature disabled.
    #[serde(default)]
    #[cfg(not(feature = "query-expansion"))]
    pub query_expansion: Option<()>,
    /// Graph-aware retrieval configuration (optional).
    #[serde(default)]
    pub graph_retrieval: Option<crate::graph_retrieval::GraphRetrievalConfig>,
    /// Metadata-driven ranking signals
    #[serde(default)]
    pub metadata_ranking: Option<MetadataRankingConfig>,
}

fn default_search_mode() -> String {
    "rrf".to_string()
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankerConfig {
    /// Enable reranking (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Model name: "ms-marco-MiniLM-L-6-v2" (default, recommended)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataRankingConfig {
    #[serde(default = "default_weight_importance")]
    pub weight_importance: f32,
    #[serde(default = "default_weight_quality")]
    pub weight_quality: f32,
    #[serde(default = "default_weight_recency")]
    pub weight_recency: f32,
    #[serde(default = "default_weight_citations")]
    pub weight_citations: f32,
    #[serde(default = "default_weight_author")]
    pub weight_author: f32,
    #[serde(default = "default_weight_source")]
    pub weight_source: f32,
    #[serde(default = "default_recency_half_life")]
    pub recency_half_life_days: u32,
    #[serde(default = "default_source_boost")]
    pub source_reliability_boost: std::collections::HashMap<String, f32>,
}

impl Default for MetadataRankingConfig {
    fn default() -> Self {
        Self {
            weight_importance: default_weight_importance(),
            weight_quality: default_weight_quality(),
            weight_recency: default_weight_recency(),
            weight_citations: default_weight_citations(),
            weight_author: default_weight_author(),
            weight_source: default_weight_source(),
            recency_half_life_days: default_recency_half_life(),
            source_reliability_boost: default_source_boost(),
        }
    }
}

fn default_weight_importance() -> f32 { 0.08 }
fn default_weight_quality() -> f32 { 0.05 }
fn default_weight_recency() -> f32 { 0.025 }
fn default_weight_citations() -> f32 { 0.0 }
fn default_weight_author() -> f32 { 0.04 }
fn default_weight_source() -> f32 { 0.025 }
fn default_recency_half_life() -> u32 { 30 }

fn default_source_boost() -> std::collections::HashMap<String, f32> {
    let mut m = std::collections::HashMap::new();
    m.insert("academic".to_string(), 1.0);
    m.insert("verified".to_string(), 0.7);
    m.insert("user".to_string(), 0.4);
    m.insert("unknown".to_string(), 0.0);
    m
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
    /// Model name: "tinyllama" (ONNX backend, recommended).
    /// Uses ONNX-compatible models from HuggingFace for efficient query expansion.
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
    10000  // 10 seconds - GGUF inference + spawn_blocking overhead can take time
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

fn default_auto_extract_concepts() -> bool {
    true
}

fn default_concept_min_score() -> f32 {
    0.7
}

fn default_concept_auto_create() -> bool {
    true
}

fn default_automerge_threshold() -> f32 {
    0.98
}

fn default_episodic_temporal_window() -> u64 {
    86400 // 24 hours in seconds
}

fn default_episodic_preserve_temporal_separation() -> bool {
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
pub struct EpisodicConfig {
    /// Time window for considering episodic memories as same event (seconds, default: 86400 = 24 hours)
    #[serde(default = "default_episodic_temporal_window")]
    pub temporal_window_secs: u64,
    /// If true, link rather than merge episodic events outside temporal window (default: true)
    #[serde(default = "default_episodic_preserve_temporal_separation")]
    pub preserve_temporal_separation: bool,
}

impl Default for EpisodicConfig {
    fn default() -> Self {
        Self {
            temporal_window_secs: 86400, // 24 hours
            preserve_temporal_separation: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertConfig {
    pub auto_link_threshold: f32,
    pub duplicate_threshold: f32,
    /// Threshold for auto-merging similar memories at insertion (default: 0.98)
    #[serde(default = "default_automerge_threshold")]
    pub automerge_threshold: f32,
    pub auto_link_limit: usize,
    /// Enable automatic concept extraction and linking during memory add (default: true)
    #[serde(default = "default_auto_extract_concepts")]
    pub auto_extract_concepts: bool,
    /// NER confidence threshold for concept extraction (0.0–1.0, default: 0.7)
    #[serde(default = "default_concept_min_score")]
    pub concept_min_score: f32,
    /// Automatically create missing concepts during extraction (default: true)
    #[serde(default = "default_concept_auto_create")]
    pub concept_auto_create: bool,
    /// Episodic memory temporal awareness settings
    #[serde(default)]
    pub episodic: EpisodicConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentConfig {
    #[serde(default)]
    pub semantic_dedup: Option<voidm_embeddings::semantic_dedup::SemanticDedupConfig>,
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
            mode: "rrf".into(),
            default_limit: 10,
            min_score: 0.3,
            signals: SignalConfig::default(),
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
            graph_retrieval: Some(crate::graph_retrieval::GraphRetrievalConfig::default()),
            metadata_ranking: None,
        }
    }
}

impl Default for InsertConfig {
    fn default() -> Self {
        Self {
            auto_link_threshold: 0.7,
            duplicate_threshold: 0.95,
            automerge_threshold: 0.98,
            auto_link_limit: 5,
            auto_extract_concepts: true,
            concept_min_score: 0.7,
            concept_auto_create: true,
            episodic: EpisodicConfig::default(),
        }
    }
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            semantic_dedup: Some(voidm_embeddings::semantic_dedup::SemanticDedupConfig::default()),
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
    // VOIDM_CONFIG environment variable (highest priority)
    if let Ok(config_env) = std::env::var("VOIDM_CONFIG") {
        if !config_env.is_empty() {
            return Some(PathBuf::from(config_env));
        }
    }
    // XDG_CONFIG_HOME/voidm/config.toml
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("voidm/config.toml"));
        }
    }
    // ~/.config/voidm/config.toml
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

    #[test]
    fn test_metadata_ranking_config() {
        let toml_str = r#"
[search.metadata_ranking]
weight_importance = 0.15
weight_quality = 0.1
weight_recency = 0.05
weight_citations = 0.1
weight_author = 0.08
recency_half_life_days = 30
"#;
        let config: Config = toml::from_str(toml_str).expect("Failed to parse");
        if let Some(mr) = &config.search.metadata_ranking {
            assert_eq!(mr.weight_importance, 0.15);
            assert_eq!(mr.weight_author, 0.08);
            assert_eq!(mr.recency_half_life_days, 30);
        }
    }
}
