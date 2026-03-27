//! Vector format conversion and normalization
//!
//! This module handles conversion between different vector storage formats
//! to enable smooth migrations between vector backends.
//!
//! Supported formats:
//! - BytesLE: Raw bytes in little-endian f32 format (sqlite-vec native)
//! - F32Array: Direct f32 array (Rust native)
//! - Base64: Base64-encoded f32 array (portable, human-readable)

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

/// Enum of supported vector formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VectorFormat {
    /// Raw bytes in little-endian f32 format (current sqlite-vec format)
    BytesLE,
    /// Direct f32 array (Rust native, standard representation)
    F32Array,
    /// Base64-encoded f32 array (portable, human-readable)
    Base64,
}

/// Converts bytes (little-endian f32) to f32 array
pub fn bytes_le_to_f32(bytes: &[u8]) -> Result<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return Err(anyhow!(
            "Invalid byte length: {} not divisible by 4",
            bytes.len()
        ));
    }

    let mut vec = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks(4) {
        let bytes_arr: [u8; 4] = chunk
            .try_into()
            .context("Failed to convert chunk to [u8;4]")?;
        vec.push(f32::from_le_bytes(bytes_arr));
    }

    Ok(vec)
}

/// Converts f32 array to bytes (little-endian f32)
pub fn f32_to_bytes_le(vec: &[f32]) -> Vec<u8> {
    vec.iter()
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

/// Converts f32 array to base64-encoded string
pub fn f32_to_base64(vec: &[f32]) -> String {
    use base64::Engine;
    let bytes = f32_to_bytes_le(vec);
    base64::engine::general_purpose::STANDARD.encode(&bytes)
}

/// Converts base64-encoded string back to f32 array
pub fn base64_to_f32(s: &str) -> Result<Vec<f32>> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(s)
        .context("Failed to decode base64 string")?;
    bytes_le_to_f32(&bytes)
}

/// Normalizes a vector from one format to another
pub fn normalize_vector(
    data: &[u8],
    from_format: VectorFormat,
    to_format: VectorFormat,
) -> Result<Vec<u8>> {
    if from_format == to_format {
        return Ok(data.to_vec());
    }

    // Convert to f32 first
    let f32_vec = match from_format {
        VectorFormat::BytesLE => bytes_le_to_f32(data)?,
        VectorFormat::F32Array => bytes_le_to_f32(data)?, // Assume it's bytes
        VectorFormat::Base64 => {
            let s = std::str::from_utf8(data)
                .context("Base64 data should be valid UTF-8")?;
            base64_to_f32(s)?
        }
    };

    // Convert to target format
    let result = match to_format {
        VectorFormat::BytesLE => f32_to_bytes_le(&f32_vec),
        VectorFormat::F32Array => f32_to_bytes_le(&f32_vec), // Store as bytes for compatibility
        VectorFormat::Base64 => f32_to_base64(&f32_vec).into_bytes(),
    };

    Ok(result)
}

/// Verifies that a sample vector can be converted successfully
pub fn verify_format_compatibility(
    sample: &[u8],
    from_format: VectorFormat,
) -> Result<()> {
    match from_format {
        VectorFormat::BytesLE => {
            if sample.len() % 4 != 0 {
                return Err(anyhow!(
                    "BytesLE sample length {} not divisible by 4",
                    sample.len()
                ));
            }
            // Verify can convert to f32
            let _vec = bytes_le_to_f32(sample)?;
            Ok(())
        }
        VectorFormat::F32Array => {
            if sample.len() % 4 != 0 {
                return Err(anyhow!(
                    "F32Array sample length {} not divisible by 4",
                    sample.len()
                ));
            }
            // Verify can convert
            let _vec = bytes_le_to_f32(sample)?;
            Ok(())
        }
        VectorFormat::Base64 => {
            let s = std::str::from_utf8(sample)
                .context("Base64 sample should be valid UTF-8")?;
            let _vec = base64_to_f32(s)?;
            Ok(())
        }
    }
}

/// Batch convert multiple vectors
pub fn batch_normalize_vectors(
    data: &[Vec<u8>],
    from_format: VectorFormat,
    to_format: VectorFormat,
) -> Result<Vec<Vec<u8>>> {
    let mut results = Vec::with_capacity(data.len());
    for (idx, item) in data.iter().enumerate() {
        let converted = normalize_vector(item, from_format, to_format)
            .with_context(|| format!("Failed to convert vector {}", idx))?;
        results.push(converted);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_le_to_f32() {
        let vec = vec![1.0f32, 2.0, 3.0];
        let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();

        let result = bytes_le_to_f32(&bytes).unwrap();

        assert_eq!(result, vec);
    }

    #[test]
    fn test_f32_to_bytes_le_roundtrip() {
        let original = vec![1.5f32, 2.5, 3.5, 4.5];

        let bytes = f32_to_bytes_le(&original);
        let restored = bytes_le_to_f32(&bytes).unwrap();

        assert_eq!(restored, original);
    }

    #[test]
    fn test_f32_to_base64() {
        let vec = vec![1.0f32, 2.0, 3.0];

        let base64_str = f32_to_base64(&vec);

        // Should be a valid base64 string
        assert!(!base64_str.is_empty());
        assert!(base64_str.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='));
    }

    #[test]
    fn test_base64_to_f32_roundtrip() {
        let original = vec![1.0f32, 2.0, 3.0];

        let base64_str = f32_to_base64(&original);
        let restored = base64_to_f32(&base64_str).unwrap();

        assert_eq!(restored, original);
    }

    #[test]
    fn test_base64_roundtrip_precision() {
        // Test with more complex values to ensure precision
        let original = vec![
            std::f32::consts::PI,
            std::f32::consts::E,
            -1.23456789,
            0.0,
            1e-6,
            1e6,
        ];

        let base64_str = f32_to_base64(&original);
        let restored = base64_to_f32(&base64_str).unwrap();

        // Should be exactly equal for f32 round-trip
        assert_eq!(restored.len(), original.len());
        for (orig, rest) in original.iter().zip(restored.iter()) {
            assert!((orig - rest).abs() < 1e-6);
        }
    }

    #[test]
    fn test_verify_format_compatibility_bytes_le() {
        let vec = vec![1.0f32, 2.0];
        let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();

        assert!(verify_format_compatibility(&bytes, VectorFormat::BytesLE).is_ok());
    }

    #[test]
    fn test_verify_format_compatibility_invalid_bytes() {
        // 3 bytes is not divisible by 4
        let bytes = vec![1u8, 2, 3];

        assert!(verify_format_compatibility(&bytes, VectorFormat::BytesLE).is_err());
    }

    #[test]
    fn test_verify_format_compatibility_base64() {
        let vec = vec![1.0f32, 2.0, 3.0];
        let base64_str = f32_to_base64(&vec);
        let base64_bytes = base64_str.as_bytes();

        assert!(verify_format_compatibility(base64_bytes, VectorFormat::Base64).is_ok());
    }

    #[test]
    fn test_batch_normalize_vectors() {
        let vec1 = vec![1.0f32, 2.0];
        let vec2 = vec![3.0f32, 4.0];
        let vec3 = vec![5.0f32, 6.0];

        let bytes_data: Vec<Vec<u8>> = vec![
            vec1.iter().flat_map(|f| f.to_le_bytes()).collect(),
            vec2.iter().flat_map(|f| f.to_le_bytes()).collect(),
            vec3.iter().flat_map(|f| f.to_le_bytes()).collect(),
        ];

        let result =
            batch_normalize_vectors(&bytes_data, VectorFormat::BytesLE, VectorFormat::BytesLE)
                .unwrap();

        assert_eq!(result.len(), 3);
        for (original, converted) in bytes_data.iter().zip(result.iter()) {
            assert_eq!(original, converted);
        }
    }

    #[test]
    fn test_large_vector_normalization() {
        // Test with 1024-D vector
        let large_vec: Vec<f32> = (0..1024).map(|i| i as f32).collect();

        let bytes = f32_to_bytes_le(&large_vec);
        assert_eq!(bytes.len(), 1024 * 4);

        let restored = bytes_le_to_f32(&bytes).unwrap();
        assert_eq!(restored, large_vec);
    }
}
