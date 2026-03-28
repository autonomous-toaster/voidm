//! Similarity metrics for vector operations
//! 
//! Supports cosine similarity and other distance metrics for embeddings.

use anyhow::{anyhow, Result};

/// Calculate cosine similarity between two vectors
/// 
/// Returns a value between -1 and 1 (typically 0 to 1 for embeddings)
/// Higher values indicate more similar vectors
/// 
/// # Arguments
/// - `a`: First vector
/// - `b`: Second vector
/// 
/// # Returns
/// Cosine similarity score (0-1 for normalized embeddings)
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.len() != b.len() {
        return Err(anyhow!(
            "Vector dimensions must match: {} vs {}",
            a.len(),
            b.len()
        ));
    }

    if a.is_empty() {
        return Err(anyhow!("Cannot compute similarity of empty vectors"));
    }

    let mut dot_product = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot_product += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    norm_a = norm_a.sqrt();
    norm_b = norm_b.sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return Err(anyhow!("Cannot compute similarity with zero-norm vector"));
    }

    Ok(dot_product / (norm_a * norm_b))
}

/// Euclidean distance between two vectors
/// 
/// Lower values indicate more similar vectors
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.len() != b.len() {
        return Err(anyhow!(
            "Vector dimensions must match: {} vs {}",
            a.len(),
            b.len()
        ));
    }

    let mut sum_sq = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        let diff = x - y;
        sum_sq += diff * diff;
    }

    Ok(sum_sq.sqrt())
}

/// Manhattan distance (L1 norm)
pub fn manhattan_distance(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.len() != b.len() {
        return Err(anyhow!(
            "Vector dimensions must match: {} vs {}",
            a.len(),
            b.len()
        ));
    }

    let sum: f32 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .sum();

    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let similarity = cosine_similarity(&a, &b).unwrap();
        assert!((similarity - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let similarity = cosine_similarity(&a, &b).unwrap();
        assert!(similarity.abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let similarity = cosine_similarity(&a, &b).unwrap();
        assert!((similarity - (-1.0)).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_partial() {
        let a = vec![1.0, 1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let similarity = cosine_similarity(&a, &b).unwrap();
        // Both have magnitude sqrt(2) and 1.0 respectively
        // dot product = 1.0, norms = sqrt(2) * 1.0
        let expected = 1.0 / 2.0_f32.sqrt();
        assert!((similarity - expected).abs() < 0.0001);
    }

    #[test]
    fn test_euclidean_distance_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let distance = euclidean_distance(&a, &b).unwrap();
        assert!(distance.abs() < 0.0001);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        let distance = euclidean_distance(&a, &b).unwrap();
        assert!((distance - 5.0).abs() < 0.0001);
    }

    #[test]
    fn test_manhattan_distance() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        let distance = manhattan_distance(&a, &b).unwrap();
        assert!((distance - 7.0).abs() < 0.0001);
    }

    #[test]
    fn test_mismatched_dimensions() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert!(cosine_similarity(&a, &b).is_err());
        assert!(euclidean_distance(&a, &b).is_err());
        assert!(manhattan_distance(&a, &b).is_err());
    }

    #[test]
    fn test_empty_vectors() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert!(cosine_similarity(&a, &b).is_err());
    }
}
