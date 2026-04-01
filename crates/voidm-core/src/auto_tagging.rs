use anyhow::Result;

#[cfg(any(feature = "tinyllama", test))]
use std::collections::HashSet;

#[cfg(feature = "tinyllama")]
use voidm_query_expansion::{LocalGenerator, QueryExpander, parse_generation_backend};

use crate::Config;

#[cfg(any(feature = "tinyllama", test))]
fn normalize_tag(s: &str) -> Option<String> {
    let cleaned = s
        .trim()
        .trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
        .trim_matches(|c: char| matches!(c, '-' | '_' | '/' | ' '))
        .to_lowercase()
        .replace(['\n', '\r', '\t'], " ");

    let cleaned = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" / ", "/")
        .replace(" - ", "-")
        .trim_matches(|c: char| matches!(c, '-' | '_' | '/' | ' '))
        .to_string();

    if cleaned.len() < 2 || cleaned.len() > 32 {
        return None;
    }
    if cleaned.chars().all(|c| c.is_numeric()) {
        return None;
    }
    if cleaned.contains(':') || cleaned.contains('.') || cleaned.contains("  ") {
        return None;
    }
    if !cleaned.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '/')) {
        return None;
    }
    Some(cleaned)
}

#[cfg(any(feature = "tinyllama", test))]
fn strict_filter_tags(raw: &str, max_tags: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut tags = Vec::new();

    for part in raw.split([',', '\n', ';']) {
        if let Some(tag) = normalize_tag(part) {
            if seen.insert(tag.clone()) {
                tags.push(tag);
            }
        }
        if tags.len() >= max_tags {
            break;
        }
    }

    tags
}

#[cfg(feature = "tinyllama")]
pub async fn generate_tags(content: &str, config: &Config) -> Result<Vec<String>> {
    if !config.enrichment.auto_tagging.enabled {
        return Ok(Vec::new());
    }

    let model = config.enrichment.auto_tagging.model.clone();

    let prompt = format!(
        "Generate up to {max_tags} concise tags for this memory. Return only a comma-separated list of lowercase tags. No explanation.\n\nMemory:\n{content}",
        max_tags = config.enrichment.auto_tagging.max_tags,
        content = content
    );

    let backend = parse_generation_backend(&config.enrichment.auto_tagging.backend)?;

    let qe_config = voidm_query_expansion::QueryExpansionConfig {
        enabled: true,
        model,
        backend,
        timeout_ms: 10_000,
        intent: voidm_query_expansion::IntentConfig::default(),
    };
    LocalGenerator::new(qe_config.clone()).ensure_model().await?;
    let qe = QueryExpander::new(qe_config);

    let raw = qe.expand(&prompt).await.unwrap_or_default();
    Ok(strict_filter_tags(&raw, config.enrichment.auto_tagging.max_tags))
}

#[cfg(not(feature = "tinyllama"))]
pub async fn generate_tags(_content: &str, _config: &Config) -> Result<Vec<String>> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_filter_dedupes_and_normalizes() {
        let tags = strict_filter_tags("Docker, docker, api-design, Rust systems, 123, -arm64, /cuda/", 8);
        assert!(tags.contains(&"docker".to_string()));
        assert!(tags.contains(&"api-design".to_string()));
        assert!(tags.contains(&"rust systems".to_string()));
        assert!(tags.contains(&"arm64".to_string()));
        assert!(tags.contains(&"cuda".to_string()));
        assert!(!tags.contains(&"123".to_string()));
        assert!(!tags.iter().any(|t| t.starts_with('-')));
    }
}
