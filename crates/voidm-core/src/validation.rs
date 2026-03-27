/// Memory length validation with soft and hard limits.
///
/// Constraints:
/// - Soft limit: 10,000 chars (warning to user)
/// - Hard limit: 50,000 chars (rejection with error)
/// - Target: 3,000-8,000 chars (guidance)

use anyhow::{anyhow, Result};

pub const MEMORY_SOFT_LIMIT: usize = 10_000;
pub const MEMORY_HARD_LIMIT: usize = 50_000;
pub const MEMORY_TARGET_MIN: usize = 3_000;
pub const MEMORY_TARGET_MAX: usize = 8_000;

#[derive(Debug, Clone)]
pub struct MemoryLengthValidation {
    pub content_length: usize,
    pub is_within_soft_limit: bool,
    pub is_within_hard_limit: bool,
    pub is_within_target: bool,
    pub warning_message: Option<String>,
}

/// Validate memory length and return validation info + optional warning.
///
/// # Returns
/// - Ok(validation) if within hard limit
/// - Err if exceeds hard limit (50K)
/// - validation.warning_message contains soft limit warning if 10K+ exceeded
///
/// # Example
/// ```
/// let result = validate_memory_length("short content");
/// assert!(result.is_ok());
/// assert!(result.unwrap().is_within_soft_limit);
/// ```
pub fn validate_memory_length(content: &str) -> Result<MemoryLengthValidation> {
    let length = content.len();

    // Hard limit: reject if exceeded
    if length > MEMORY_HARD_LIMIT {
        return Err(anyhow!(
            "Memory too long: {} chars (hard limit: {} chars). \
             Please split into multiple focused memories.",
            length,
            MEMORY_HARD_LIMIT
        ));
    }

    let is_within_soft_limit = length <= MEMORY_SOFT_LIMIT;
    let is_within_hard_limit = true; // We already checked this above
    let is_within_target = length >= MEMORY_TARGET_MIN && length <= MEMORY_TARGET_MAX;

    let warning_message = if !is_within_soft_limit {
        Some(format!(
            "⚠️  Memory is {} chars (soft limit: {} chars). \
             Target: {}-{} chars. Consider splitting for better coherence.",
            length, MEMORY_SOFT_LIMIT, MEMORY_TARGET_MIN, MEMORY_TARGET_MAX
        ))
    } else {
        None
    };

    Ok(MemoryLengthValidation {
        content_length: length,
        is_within_soft_limit,
        is_within_hard_limit,
        is_within_target,
        warning_message,
    })
}

/// Check if content is suitable length (soft + hard limits).
pub fn is_valid_length(content: &str) -> bool {
    content.len() <= MEMORY_HARD_LIMIT
}

/// Check if content is within target range for best coherence.
pub fn is_optimal_length(content: &str) -> bool {
    let len = content.len();
    len >= MEMORY_TARGET_MIN && len <= MEMORY_TARGET_MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_short_content() {
        let result = validate_memory_length("short").unwrap();
        assert!(result.is_within_soft_limit);
        assert!(result.is_within_hard_limit);
        assert!(!result.is_within_target); // Too short
        assert!(result.warning_message.is_none());
    }

    #[test]
    fn test_validate_optimal_content() {
        let content = "a".repeat(5000); // 5000 chars, within target
        let result = validate_memory_length(&content).unwrap();
        assert!(result.is_within_soft_limit);
        assert!(result.is_within_hard_limit);
        assert!(result.is_within_target);
        assert!(result.warning_message.is_none());
    }

    #[test]
    fn test_validate_soft_limit_warning() {
        let content = "a".repeat(15000); // 15K chars, exceeds soft limit
        let result = validate_memory_length(&content).unwrap();
        assert!(!result.is_within_soft_limit);
        assert!(result.is_within_hard_limit);
        assert!(!result.is_within_target);
        assert!(result.warning_message.is_some());
        assert!(result.warning_message.unwrap().contains("soft limit"));
    }

    #[test]
    fn test_validate_hard_limit_rejection() {
        let content = "a".repeat(60000); // 60K chars, exceeds hard limit
        let result = validate_memory_length(&content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("hard limit"));
    }

    #[test]
    fn test_validate_boundary_soft() {
        let content = "a".repeat(MEMORY_SOFT_LIMIT); // Exactly at soft limit
        let result = validate_memory_length(&content).unwrap();
        assert!(result.is_within_soft_limit);
        assert!(result.warning_message.is_none());
    }

    #[test]
    fn test_validate_boundary_hard() {
        let content = "a".repeat(MEMORY_HARD_LIMIT); // Exactly at hard limit
        let result = validate_memory_length(&content).unwrap();
        assert!(result.is_within_hard_limit);
    }

    #[test]
    fn test_validate_just_over_hard() {
        let content = "a".repeat(MEMORY_HARD_LIMIT + 1);
        let result = validate_memory_length(&content);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_valid_length_short() {
        assert!(is_valid_length("anything short"));
    }

    #[test]
    fn test_is_valid_length_at_limit() {
        let content = "a".repeat(MEMORY_HARD_LIMIT);
        assert!(is_valid_length(&content));
    }

    #[test]
    fn test_is_valid_length_over_limit() {
        let content = "a".repeat(MEMORY_HARD_LIMIT + 1);
        assert!(!is_valid_length(&content));
    }

    #[test]
    fn test_is_optimal_length_yes() {
        let content = "a".repeat(5000);
        assert!(is_optimal_length(&content));
    }

    #[test]
    fn test_is_optimal_length_no() {
        let content = "a".repeat(1000); // Too short
        assert!(!is_optimal_length(&content));
    }
}
