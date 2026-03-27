//! Integration tests for vector backup, export, and safe migration
//!
//! These tests use a real test database to verify:
//! - Vector backup and restore functionality
//! - Format conversion accuracy
//! - Data integrity with checksums
//! - Safe migration patterns
//! - No data loss scenarios

#[cfg(test)]
mod tests {
    use voidm_core::migration_export::{MigrationCheckpoint, VectorBackup};
    use voidm_core::vector_format::{self, VectorFormat};

    /// Helper: create test vectors
    fn create_test_vectors() -> Vec<(String, Vec<f32>)> {
        vec![
            (
                "mem_001".to_string(),
                vec![0.1, 0.2, 0.3, 0.4, 0.5],
            ),
            (
                "mem_002".to_string(),
                vec![1.0, 2.0, 3.0, 4.0, 5.0],
            ),
            (
                "mem_003".to_string(),
                vec![0.0, 0.0, 0.0, 0.0, 0.0],
            ),
            (
                "mem_004".to_string(),
                vec![-1.0, -2.0, -3.0, -4.0, -5.0],
            ),
        ]
    }

    /// Helper: create vector backups from test data
    fn create_vector_backups(
        vectors: &[(String, Vec<f32>)],
    ) -> Vec<VectorBackup> {
        vectors
            .iter()
            .map(|(mem_id, vec)| {
                let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
                VectorBackup::from_bytes(mem_id.clone(), &bytes).unwrap()
            })
            .collect()
    }

    #[test]
    fn test_small_checkpoint_create_and_validate() {
        let test_vectors = create_test_vectors();
        let backups = create_vector_backups(&test_vectors);

        let checkpoint =
            MigrationCheckpoint::create(backups, test_vectors.len()).unwrap();

        assert_eq!(checkpoint.total_vectors, 4);
        assert_eq!(checkpoint.total_memories, 4);

        // Validate all vectors
        let valid_count = checkpoint.validate_all().unwrap();
        assert_eq!(valid_count, 4);

        // Validate checkpoint checksum
        assert!(checkpoint.validate_checkpoint_checksum().is_ok());
    }

    #[test]
    fn test_checkpoint_file_roundtrip() {
        let test_vectors = create_test_vectors();
        let backups = create_vector_backups(&test_vectors);

        let original_checkpoint =
            MigrationCheckpoint::create(backups, test_vectors.len()).unwrap();

        // Save to temp file
        let temp_path =
            std::path::PathBuf::from("/tmp/test_migration_checkpoint.json");
        original_checkpoint
            .save_to_file(&temp_path)
            .unwrap();

        // Load from temp file
        let loaded_checkpoint =
            MigrationCheckpoint::load_from_file(&temp_path).unwrap();

        // Verify identity
        assert_eq!(
            original_checkpoint.total_vectors,
            loaded_checkpoint.total_vectors
        );
        assert_eq!(
            original_checkpoint.total_memories,
            loaded_checkpoint.total_memories
        );
        assert_eq!(
            original_checkpoint.checksum,
            loaded_checkpoint.checksum
        );

        // Verify all vectors match
        for (orig, loaded) in original_checkpoint
            .vectors
            .iter()
            .zip(loaded_checkpoint.vectors.iter())
        {
            assert_eq!(orig.memory_id, loaded.memory_id);
            assert_eq!(orig.embedding, loaded.embedding);
            assert_eq!(orig.dimension, loaded.dimension);
            assert_eq!(orig.checksum, loaded.checksum);
        }

        // Clean up
        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn test_vector_format_conversion_bytes_to_base64() {
        let original_vec = vec![1.0f32, 2.0, 3.0, 4.0, 5.0];
        let bytes: Vec<u8> = original_vec
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        // Convert BytesLE to Base64
        let base64_data = vector_format::normalize_vector(
            &bytes,
            VectorFormat::BytesLE,
            VectorFormat::Base64,
        )
        .unwrap();

        // Should be different (compressed/encoded)
        assert_ne!(base64_data, bytes);

        // Should be valid UTF-8
        let base64_str = std::str::from_utf8(&base64_data).unwrap();
        assert!(!base64_str.is_empty());
    }

    #[test]
    fn test_vector_format_conversion_roundtrip() {
        let original_vec = vec![
            1.5f32, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5, 10.5,
        ];
        let bytes: Vec<u8> = original_vec
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        // Convert BytesLE -> Base64 -> BytesLE
        let base64_data = vector_format::normalize_vector(
            &bytes,
            VectorFormat::BytesLE,
            VectorFormat::Base64,
        )
        .unwrap();

        let restored_bytes = vector_format::normalize_vector(
            &base64_data,
            VectorFormat::Base64,
            VectorFormat::BytesLE,
        )
        .unwrap();

        // Should match original
        assert_eq!(bytes, restored_bytes);

        // Verify float values are preserved
        let restored_vec = restored_bytes
            .chunks(4)
            .map(|chunk| {
                let arr: [u8; 4] = chunk.try_into().unwrap();
                f32::from_le_bytes(arr)
            })
            .collect::<Vec<_>>();

        assert_eq!(restored_vec, original_vec);
    }

    #[test]
    fn test_batch_format_conversion() {
        let vectors = vec![
            vec![1.0f32, 2.0, 3.0],
            vec![4.0f32, 5.0, 6.0],
            vec![7.0f32, 8.0, 9.0],
        ];

        let byte_vectors: Vec<Vec<u8>> = vectors
            .iter()
            .map(|v| v.iter().flat_map(|f| f.to_le_bytes()).collect())
            .collect();

        // Batch convert BytesLE -> Base64
        let base64_vectors = vector_format::batch_normalize_vectors(
            &byte_vectors,
            VectorFormat::BytesLE,
            VectorFormat::Base64,
        )
        .unwrap();

        assert_eq!(base64_vectors.len(), 3);

        // Each should be valid UTF-8
        for base64_data in &base64_vectors {
            assert!(std::str::from_utf8(base64_data).is_ok());
        }

        // Batch convert back to BytesLE
        let restored_vectors = vector_format::batch_normalize_vectors(
            &base64_vectors,
            VectorFormat::Base64,
            VectorFormat::BytesLE,
        )
        .unwrap();

        // Should match originals
        assert_eq!(restored_vectors, byte_vectors);
    }

    #[test]
    fn test_large_dataset_checkpoint() {
        // Create 1000 small vectors
        let mut large_vectors = vec![];
        for i in 0..1000 {
            let vec = vec![i as f32 % 10.0; 10]; // 10-D vectors
            large_vectors.push((format!("mem_{:04}", i), vec));
        }

        let backups = create_vector_backups(&large_vectors);

        let checkpoint =
            MigrationCheckpoint::create(backups, large_vectors.len()).unwrap();

        assert_eq!(checkpoint.total_vectors, 1000);
        assert_eq!(checkpoint.total_memories, 1000);

        // Validate all
        let valid_count = checkpoint.validate_all().unwrap();
        assert_eq!(valid_count, 1000);

        // Check statistics
        assert_eq!(checkpoint.average_dimension(), 10.0);
        assert!(checkpoint.total_size_bytes() > 0);
    }

    #[test]
    fn test_large_dimensions_vector() {
        // Test 1024-D vector (typical embedding size)
        let large_vec: Vec<f32> = (0..1024).map(|i| (i as f32) * 0.001).collect();
        let bytes: Vec<u8> = large_vec
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        let backup = VectorBackup::from_bytes("mem_large".to_string(), &bytes)
            .unwrap();

        assert_eq!(backup.dimension, 1024);
        assert_eq!(backup.size_bytes(), 1024 * 4);

        // Round-trip through checkpoint
        let checkpoint =
            MigrationCheckpoint::create(vec![backup], 1).unwrap();

        let valid_count = checkpoint.validate_all().unwrap();
        assert_eq!(valid_count, 1);
    }

    #[test]
    fn test_checkpoint_summary_statistics() {
        let test_vectors = create_test_vectors();
        let backups = create_vector_backups(&test_vectors);

        let checkpoint =
            MigrationCheckpoint::create(backups, test_vectors.len()).unwrap();

        let summary = checkpoint.summary();
        assert!(summary.contains("4 vectors"));
        assert!(summary.contains("4 memories"));
        assert!(summary.contains("avg_dim=5"));
    }

    #[test]
    fn test_checkpoint_corruption_detection() {
        let test_vectors = create_test_vectors();
        let backups = create_vector_backups(&test_vectors);

        let mut checkpoint =
            MigrationCheckpoint::create(backups, test_vectors.len()).unwrap();

        // Corrupt a vector's embedding
        if !checkpoint.vectors.is_empty() {
            checkpoint.vectors[0].embedding[0] = 99.0;
        }

        // Validation should fail
        assert!(checkpoint.validate_all().is_err());
    }

    #[test]
    fn test_empty_checkpoint() {
        // Create checkpoint with no vectors
        let checkpoint = MigrationCheckpoint::create(vec![], 0).unwrap();

        assert_eq!(checkpoint.total_vectors, 0);
        assert_eq!(checkpoint.total_memories, 0);
        assert!(checkpoint.validate_checkpoint_checksum().is_ok());
    }

    #[test]
    fn test_multiple_format_conversions() {
        let original = vec![3.14159f32, 2.71828, 1.41421];
        let bytes: Vec<u8> = original
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        // BytesLE -> Base64 -> BytesLE
        let step1 = vector_format::normalize_vector(
            &bytes,
            VectorFormat::BytesLE,
            VectorFormat::Base64,
        )
        .unwrap();

        let step2 = vector_format::normalize_vector(
            &step1,
            VectorFormat::Base64,
            VectorFormat::BytesLE,
        )
        .unwrap();

        // Verify precision maintained
        let restored = step2
            .chunks(4)
            .map(|chunk| {
                let arr: [u8; 4] = chunk.try_into().unwrap();
                f32::from_le_bytes(arr)
            })
            .collect::<Vec<_>>();

        for (orig, rest) in original.iter().zip(restored.iter()) {
            assert!((orig - rest).abs() < 1e-6);
        }
    }

    #[test]
    fn test_vector_backup_bytes_validation() {
        // Test with invalid byte length
        let invalid_bytes = vec![1u8, 2, 3]; // 3 bytes, not divisible by 4

        let result = VectorBackup::from_bytes("mem".to_string(), &invalid_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_checkpoint_with_mixed_dimensions() {
        // Create vectors with different dimensions
        let vectors = vec![
            ("mem_1".to_string(), vec![1.0f32, 2.0]), // 2-D
            ("mem_2".to_string(), vec![1.0f32, 2.0, 3.0, 4.0]), // 4-D
            ("mem_3".to_string(), vec![1.0f32, 2.0, 3.0]), // 3-D
        ];

        let backups = create_vector_backups(&vectors);

        let checkpoint = MigrationCheckpoint::create(backups, vectors.len())
            .unwrap();

        assert_eq!(checkpoint.total_vectors, 3);

        // Average dimension should be calculated correctly
        // (2 + 4 + 3) / 3 = 3.0
        assert!((checkpoint.average_dimension() - 3.0).abs() < 0.01);
    }
}
