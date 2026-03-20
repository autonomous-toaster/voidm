//! Fast vector operations for search acceleration
//! Optimized for cosine similarity computation with various vector sizes
//! Includes manual unrolling and SIMD-friendly patterns for auto-vectorization

#[inline(always)]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vector lengths must match");
    
    if a.is_empty() {
        return 0.0;
    }
    
    // Fast path for common dimensions using aggressive unrolling
    match a.len() {
        96 => cosine_similarity_96(a, b),
        192 => cosine_similarity_192(a, b),
        384 => cosine_similarity_384(a, b),
        768 => cosine_similarity_768(a, b),
        1024 => cosine_similarity_1024(a, b),
        _ => cosine_similarity_generic(a, b),
    }
}

/// Specialized for 96-dimensional vectors (SIMD-friendly: 96 = 6*16 or 3*32)
#[inline]
fn cosine_similarity_96(a: &[f32], b: &[f32]) -> f32 {
    // Process in 16-element chunks for better SIMD utilization
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    
    for chunk in 0..6 {
        let base = chunk * 16;
        for i in 0..16 {
            let av = a[base + i];
            let bv = b[base + i];
            dot += av * bv;
            norm_a += av * av;
            norm_b += bv * bv;
        }
    }
    
    let norm = (norm_a * norm_b).sqrt();
    if norm > 0.0 { dot / norm } else { 0.0 }
}

/// Specialized for 192-dimensional vectors (192 = 6*32)
#[inline]
fn cosine_similarity_192(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    
    // Process in 32-element chunks for cache efficiency and SIMD
    for chunk in 0..6 {
        let base = chunk * 32;
        for i in 0..32 {
            let av = a[base + i];
            let bv = b[base + i];
            dot += av * bv;
            norm_a += av * av;
            norm_b += bv * bv;
        }
    }
    
    let norm = (norm_a * norm_b).sqrt();
    if norm > 0.0 { dot / norm } else { 0.0 }
}

/// Specialized for 384-dimensional vectors (384 = 12*32)
/// This is the default embedding dimension, optimize heavily
#[inline]
fn cosine_similarity_384(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    
    // Process in large 32-element chunks to encourage SIMD
    // Modern compilers auto-vectorize well with this pattern
    for chunk in 0..12 {
        let base = chunk * 32;
        for i in 0..32 {
            let av = a[base + i];
            let bv = b[base + i];
            dot += av * bv;
            norm_a += av * av;
            norm_b += bv * bv;
        }
    }
    
    let norm = (norm_a * norm_b).sqrt();
    if norm > 0.0 { dot / norm } else { 0.0 }
}

/// Specialized for 768-dimensional vectors (768 = 24*32)
#[inline]
fn cosine_similarity_768(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    
    for chunk in 0..24 {
        let base = chunk * 32;
        for i in 0..32 {
            let av = a[base + i];
            let bv = b[base + i];
            dot += av * bv;
            norm_a += av * av;
            norm_b += bv * bv;
        }
    }
    
    let norm = (norm_a * norm_b).sqrt();
    if norm > 0.0 { dot / norm } else { 0.0 }
}

/// Specialized for 1024-dimensional vectors (1024 = 32*32)
#[inline]
fn cosine_similarity_1024(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    
    for chunk in 0..32 {
        let base = chunk * 32;
        for i in 0..32 {
            let av = a[base + i];
            let bv = b[base + i];
            dot += av * bv;
            norm_a += av * av;
            norm_b += bv * bv;
        }
    }
    
    let norm = (norm_a * norm_b).sqrt();
    if norm > 0.0 { dot / norm } else { 0.0 }
}

/// Generic implementation for arbitrary dimensions
/// Uses 32-element chunks for auto-vectorization friendly pattern
#[inline]
fn cosine_similarity_generic(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    
    // Process in 32-element chunks (SIMD register-friendly)
    let chunk_size = 32;
    let full_chunks = len / chunk_size;
    
    for chunk in 0..full_chunks {
        let base = chunk * chunk_size;
        for i in 0..chunk_size {
            let idx = base + i;
            let av = a[idx];
            let bv = b[idx];
            dot += av * bv;
            norm_a += av * av;
            norm_b += bv * bv;
        }
    }
    
    // Handle remainder
    for i in (full_chunks * chunk_size)..len {
        let av = a[i];
        let bv = b[i];
        dot += av * bv;
        norm_a += av * av;
        norm_b += bv * bv;
    }
    
    let norm = (norm_a * norm_b).sqrt();
    if norm > 0.0 { dot / norm } else { 0.0 }
}

/// Compute dot product only (faster if norm not needed)
#[inline(always)]
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Compute L2 norm
#[inline(always)]
pub fn norm_l2(a: &[f32]) -> f32 {
    a.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_384d() {
        let a: Vec<f32> = (0..384).map(|i| (i as f32).sin()).collect();
        let b: Vec<f32> = (0..384).map(|i| (i as f32).cos()).collect();
        let sim = cosine_similarity(&a, &b);
        assert!(sim.is_finite());
        assert!(sim >= -1.0 && sim <= 1.0);
    }
}
