use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    Episodic,
    Semantic,
    Procedural,
    Conceptual,
    Contextual,
}

impl fmt::Display for MemoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            MemoryType::Episodic => "episodic",
            MemoryType::Semantic => "semantic",
            MemoryType::Procedural => "procedural",
            MemoryType::Conceptual => "conceptual",
            MemoryType::Contextual => "contextual",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for MemoryType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "episodic" => Ok(MemoryType::Episodic),
            "semantic" => Ok(MemoryType::Semantic),
            "procedural" => Ok(MemoryType::Procedural),
            "conceptual" => Ok(MemoryType::Conceptual),
            "contextual" => Ok(MemoryType::Contextual),
            other => Err(anyhow::anyhow!(
                "Unknown memory type: '{}'. Valid types: episodic, semantic, procedural, conceptual, contextual",
                other
            )),
        }
    }
}

/// Valid graph edge relationship types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EdgeType {
    RelatesTo,
    Supports,
    Contradicts,
    DerivedFrom,
    Precedes,
    PartOf,
    Exemplifies,
    Invalidates,
    // Ontology edges (also valid in ontology_edges table)
    IsA,
    InstanceOf,
    HasProperty,
}

impl EdgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeType::RelatesTo => "RELATES_TO",
            EdgeType::Supports => "SUPPORTS",
            EdgeType::Contradicts => "CONTRADICTS",
            EdgeType::DerivedFrom => "DERIVED_FROM",
            EdgeType::Precedes => "PRECEDES",
            EdgeType::PartOf => "PART_OF",
            EdgeType::Exemplifies => "EXEMPLIFIES",
            EdgeType::Invalidates => "INVALIDATES",
            EdgeType::IsA => "IS_A",
            EdgeType::InstanceOf => "INSTANCE_OF",
            EdgeType::HasProperty => "HAS_PROPERTY",
        }
    }

    /// Returns the conflicting edge type if one exists (SUPPORTS↔CONTRADICTS, PRECEDES↔INVALIDATES)
    pub fn conflict(&self) -> Option<&'static str> {
        match self {
            EdgeType::Supports => Some("CONTRADICTS"),
            EdgeType::Contradicts => Some("SUPPORTS"),
            EdgeType::Precedes => Some("INVALIDATES"),
            EdgeType::Invalidates => Some("PRECEDES"),
            _ => None,
        }
    }

    /// Whether this edge requires a note (RELATES_TO)
    pub fn requires_note(&self) -> bool {
        matches!(self, EdgeType::RelatesTo)
    }
}

impl fmt::Display for EdgeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for EdgeType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().replace('-', "_").as_str() {
            "RELATES_TO" => Ok(EdgeType::RelatesTo),
            "SUPPORTS" => Ok(EdgeType::Supports),
            "CONTRADICTS" => Ok(EdgeType::Contradicts),
            "DERIVED_FROM" => Ok(EdgeType::DerivedFrom),
            "PRECEDES" => Ok(EdgeType::Precedes),
            "PART_OF" => Ok(EdgeType::PartOf),
            "EXEMPLIFIES" => Ok(EdgeType::Exemplifies),
            "INVALIDATES" => Ok(EdgeType::Invalidates),
            "IS_A" | "ISA" => Ok(EdgeType::IsA),
            "INSTANCE_OF" => Ok(EdgeType::InstanceOf),
            "HAS_PROPERTY" => Ok(EdgeType::HasProperty),
            other => Err(anyhow::anyhow!(
                "Unknown edge type: '{}'. Valid types: RELATES_TO, SUPPORTS, CONTRADICTS, DERIVED_FROM, PRECEDES, PART_OF, EXEMPLIFIES, INVALIDATES, IS_A, INSTANCE_OF, HAS_PROPERTY",
                other
            )),
        }
    }
}

/// Valid memory context types (creation-time categorization).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryContext {
    Gotcha,
    Decision,
    Procedure,
    Reference,
}

impl MemoryContext {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryContext::Gotcha => "gotcha",
            MemoryContext::Decision => "decision",
            MemoryContext::Procedure => "procedure",
            MemoryContext::Reference => "reference",
        }
    }
}

impl fmt::Display for MemoryContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for MemoryContext {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gotcha" => Ok(MemoryContext::Gotcha),
            "decision" => Ok(MemoryContext::Decision),
            "procedure" => Ok(MemoryContext::Procedure),
            "reference" => Ok(MemoryContext::Reference),
            other => Err(anyhow::anyhow!(
                "Unknown memory context: '{}'. Valid contexts: gotcha, decision, procedure, reference",
                other
            )),
        }
    }
}

/// Valid search intent types (search-time categorization).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchIntent {
    Debug,
    Optimize,
    Implement,
    Understand,
    Architecture,
    Troubleshoot,
}

impl SearchIntent {
    pub fn as_str(&self) -> &'static str {
        match self {
            SearchIntent::Debug => "debug",
            SearchIntent::Optimize => "optimize",
            SearchIntent::Implement => "implement",
            SearchIntent::Understand => "understand",
            SearchIntent::Architecture => "architecture",
            SearchIntent::Troubleshoot => "troubleshoot",
        }
    }
}

impl fmt::Display for SearchIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for SearchIntent {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "debug" => Ok(SearchIntent::Debug),
            "optimize" => Ok(SearchIntent::Optimize),
            "implement" => Ok(SearchIntent::Implement),
            "understand" => Ok(SearchIntent::Understand),
            "architecture" => Ok(SearchIntent::Architecture),
            "troubleshoot" => Ok(SearchIntent::Troubleshoot),
            other => Err(anyhow::anyhow!(
                "Unknown search intent: '{}'. Valid intents: debug, optimize, implement, understand, architecture, troubleshoot",
                other
            )),
        }
    }
}

/// A memory record as stored in the DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub content: String,
    pub importance: i64,
    pub tags: Vec<String>,
    pub metadata: serde_json::Value,
    pub scopes: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Request to add a memory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddMemoryRequest {
    pub id: Option<String>,
    pub content: String,
    pub memory_type: MemoryType,
    pub scopes: Vec<String>,
    pub tags: Vec<String>,
    pub importance: i64,
    pub metadata: serde_json::Value,
    pub links: Vec<LinkSpec>,
    pub context: Option<String>,
    pub title: Option<String>,
}

/// A link spec from --link id:TYPE or --link id:RELATES_TO:"note"
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LinkSpec {
    pub target_id: String,
    pub edge_type: EdgeType,
    pub note: Option<String>,
}

/// Response from voidm add.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMemoryResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub content: String,
    pub scopes: Vec<String>,
    pub tags: Vec<String>,
    pub importance: i64,
    pub created_at: String,
    pub quality_score: Option<f32>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub suggested_links: Vec<SuggestedLink>,
    pub duplicate_warning: Option<DuplicateWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedLink {
    pub id: String,
    pub score: f32,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub content: String, // truncated at 120 chars
    pub hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateWarning {
    pub id: String,
    pub score: f32,
    pub content: String,
    pub message: String,
}

/// A graph edge record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from_id: String,
    pub rel_type: String,
    pub to_id: String,
    pub note: Option<String>,
    pub created_at: String,
}

/// Return type for link command.
/// Representation of a link/edge between two memories for migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEdge {
    pub from_id: String,
    pub to_id: String,
    pub rel_type: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkResponse {
    pub created: bool,
    pub from: String,
    pub rel: String,
    pub to: String,
}

/// Hint logic for suggested links based on type pairs.
pub fn edge_hint(new_type: &str, existing_type: &str) -> &'static str {
    match (new_type, existing_type) {
        ("episodic", "episodic") => "PRECEDES, RELATES_TO",
        ("episodic", "semantic") => "SUPPORTS, CONTRADICTS",
        ("episodic", "procedural") => "DERIVED_FROM, RELATES_TO",
        ("semantic", "semantic") => "SUPPORTS, CONTRADICTS, DERIVED_FROM",
        ("semantic", "conceptual") => "SUPPORTS, EXEMPLIFIES",
        ("conceptual", "conceptual") => "SUPPORTS, CONTRADICTS, DERIVED_FROM",
        ("conceptual", "semantic") => "DERIVED_FROM, SUPPORTS",
        ("procedural", "procedural") => "INVALIDATES, PART_OF",
        ("procedural", "episodic") => "DERIVED_FROM",
        ("contextual", "contextual") => "RELATES_TO (with note), PART_OF",
        ("contextual", "semantic") => "EXEMPLIFIES, RELATES_TO",
        _ => "RELATES_TO (with note required)",
    }
}

/// Validate a title field for memory.
/// 
/// Rules:
/// - If Some, must be non-empty after trimming
/// - Max length: 200 characters
/// - Whitespace is trimmed
/// 
/// # Arguments
/// * `title` - Optional title string to validate
/// 
/// # Returns
/// - Ok(Some(trimmed)) if valid and non-empty after trim
/// - Ok(None) if input is None or empty after trim
/// - Err with message if validation fails
pub fn validate_title(title: Option<String>) -> anyhow::Result<Option<String>> {
    match title {
        None => Ok(None),
        Some(t) => {
            let trimmed = t.trim().to_string();
            
            // Empty after trimming: treat as None
            if trimmed.is_empty() {
                return Ok(None);
            }
            
            // Check max length
            if trimmed.len() > 200 {
                return Err(anyhow::anyhow!(
                    "Title must be ≤ 200 characters, got {} characters",
                    trimmed.len()
                ));
            }
            
            Ok(Some(trimmed))
        }
    }
}

// ── Batch merge operations ─────────────────────────────────────────────────

/// Machine-readable merge plan: list of (source_id, target_id) pairs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePlan {
    pub merges: Vec<MergePair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePair {
    pub source: String,
    pub target: String,
}

/// Merge log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeLogEntry {
    pub id: String,
    pub batch_id: String,
    pub source_id: String,
    pub target_id: String,
    pub edges_retargeted: i32,
    pub conflicts_kept: i32,
    pub status: String,
    pub reason: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

// ── Database Statistics ────────────────────────────────────────────────────

/// Overall database statistics collected from multiple queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    // Memory statistics
    pub total_memories: i64,
    pub memories_by_type: Vec<(String, i64)>,
    pub scopes_count: i64,
    pub top_tags: Vec<(String, usize)>,
    pub embedding_coverage: EmbeddingStats,
    
    // Graph statistics
    pub graph: GraphStats,
    
    // Database metadata
    pub db_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingStats {
    pub total_embeddings: i64,
    pub total_memories: i64,
    pub coverage_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub node_count: i64,
    pub edge_count: i64,
    pub edges_by_type: Vec<(String, i64)>,
}

/// Graph export data structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphExportData {
    pub memories: Vec<GraphMemory>,
    pub concepts: Vec<GraphConcept>,
    pub nodes: Vec<GenericGraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMemory {
    pub id: String,
    pub mem_type: String,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConcept {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericGraphNode {
    pub id: String,
    pub node_type: String,
    pub properties: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from_id: String,
    pub to_id: String,
    pub rel_type: String,
    #[serde(default)]
    pub properties: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_title_none() {
        let result = validate_title(None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_validate_title_valid() {
        let result = validate_title(Some("Quick summary".to_string()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("Quick summary".to_string()));
    }

    #[test]
    fn test_validate_title_with_whitespace() {
        let result = validate_title(Some("  spaces  ".to_string()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("spaces".to_string()));
    }

    #[test]
    fn test_validate_title_empty_string() {
        let result = validate_title(Some("".to_string()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_validate_title_whitespace_only() {
        let result = validate_title(Some("   ".to_string()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_validate_title_max_length() {
        let title = "a".repeat(200);
        let result = validate_title(Some(title.clone()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(title));
    }

    #[test]
    fn test_validate_title_exceeds_max_length() {
        let title = "a".repeat(201);
        let result = validate_title(Some(title));
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("≤ 200 characters"));
        assert!(err_msg.contains("201"));
    }

    #[test]
    fn test_validate_title_boundary_201() {
        let title = "a".repeat(201);
        let result = validate_title(Some(title));
        assert!(result.is_err());
    }
}
