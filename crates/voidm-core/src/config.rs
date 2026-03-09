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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database backend: "sqlite" or "neo4j" (default: "sqlite")
    #[serde(default = "default_backend")]
    pub backend: String,
    
    /// Path to SQLite database file (only for sqlite backend)
    #[serde(default = "default_sqlite_path")]
    pub sqlite_path: String,
    
    /// Neo4j connection parameters (only for neo4j backend)
    #[serde(default)]
    pub neo4j: Option<Neo4jConfig>,
    
    /// Legacy field for backward compatibility
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertConfig {
    pub auto_link_threshold: f32,
    pub duplicate_threshold: f32,
    pub auto_link_limit: usize,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            backend: "sqlite".to_string(),
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

impl Default for Config {
    fn default() -> Self {
        Self {
            database: Default::default(),
            embeddings: Default::default(),
            search: Default::default(),
            insert: Default::default(),
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
        // New sqlite_path field takes precedence
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
