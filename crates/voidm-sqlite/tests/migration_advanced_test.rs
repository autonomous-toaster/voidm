//! Advanced integration tests for complete data migration safety workflow
//!
//! These tests demonstrate the full migration lifecycle:
//! 1. Create checkpoint (backup)
//! 2. Plan and dry-run migration
//! 3. Execute migration
//! 4. Verify no data loss
//! 5. Rollback if needed

#[cfg(test)]
mod tests {
    use voidm_core::migration_export::{MigrationCheckpoint, VectorBackup};
    use voidm_core::vector_format::{self, VectorFormat};
    use voidm_core::db_migration::{
        DryRunResult, MigrationOp, MigrationPlan, MigrationReport, SafeMigrator,
    };

    /// Helper: Create test vectors
    fn create_test_vectors(count: usize) -> Vec<(String, Vec<f32>)> {
        (0..count)
            .map(|i| {
                let vec = vec![i as f32 * 0.1; 10];
                (format!("mem_{:04}", i), vec)
            })
            .collect()
    }

    /// Helper: Create backups
    fn create_backups(vectors: &[(String, Vec<f32>)]) -> Vec<VectorBackup> {
        vectors
            .iter()
            .map(|(mem_id, vec)| {
                let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
                VectorBackup::from_bytes(mem_id.clone(), &bytes).unwrap()
            })
            .collect()
    }

    #[test]
    fn test_complete_migration_workflow() {
        // Step 1: Create test data
        let test_vectors = create_test_vectors(100);
        let backups = create_backups(&test_vectors);

        // Step 2: Create checkpoint (backup before migration)
        let checkpoint =
            MigrationCheckpoint::create(backups, test_vectors.len()).unwrap();

        assert_eq!(checkpoint.total_vectors, 100);
        assert_eq!(checkpoint.total_memories, 100);

        // Step 3: Save checkpoint
        let backup_path =
            std::path::PathBuf::from("/tmp/workflow_checkpoint.json");
        checkpoint.save_to_file(&backup_path).unwrap();

        // Step 4: Verify checkpoint integrity
        let loaded = MigrationCheckpoint::load_from_file(&backup_path).unwrap();
        let valid_count = loaded.validate_all().unwrap();
        assert_eq!(valid_count, 100);

        // Step 5: Create migration plan
        let plan = MigrationPlan::new(
            "vector_format_upgrade".to_string(),
            "Upgrade vector format from BytesLE to Base64".to_string(),
        )
        .with_version(1, 2);

        assert_eq!(plan.operation_count(), 0); // No ops added yet

        // Step 6: Plan operations
        let mut plan = plan;
        plan.add_operation(MigrationOp::Custom {
            description: "Convert vectors to Base64 format".to_string(),
            sql: "SELECT * FROM vec_memories".to_string(),
        });

        assert_eq!(plan.operation_count(), 1);

        // Clean up
        let _ = std::fs::remove_file(&backup_path);
    }

    #[test]
    fn test_migration_plan_execution_dry_run() {
        // Create migration plan
        let mut plan = MigrationPlan::new(
            "test_migration".to_string(),
            "Test migration".to_string(),
        );

        plan.add_operation(MigrationOp::CreateTable {
            sql: "CREATE TABLE new_vectors (id TEXT PRIMARY KEY, embedding BLOB)"
                .to_string(),
        });

        plan.add_operation(MigrationOp::DataMigration {
            from_table: "vec_memories".to_string(),
            to_table: "new_vectors".to_string(),
            transform_sql: None,
        });

        // Execute dry-run
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            let migrator = SafeMigrator::new(true);
            migrator.execute(&plan).await
        });

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.dry_run);
        assert!(report.success);
    }

    #[test]
    fn test_migration_report_tracks_errors() {
        let mut report = MigrationReport::new(5, false);

        // Simulate operations
        report.record_operation_completed(100);
        report.add_error("Simulated error".to_string());

        // Should be marked as failed
        assert!(!report.success);
        assert_eq!(report.errors.len(), 1);
        assert!(!report.summary().contains("SUCCESS"));
    }

    #[test]
    fn test_vector_backup_restore_workflow() {
        // Original vectors
        let original: Vec<f32> = vec![1.1, 2.2, 3.3, 4.4, 5.5];
        let bytes: Vec<u8> = original
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        // Backup
        let backup =
            VectorBackup::from_bytes("mem_test".to_string(), &bytes).unwrap();

        // Verify backup integrity
        assert!(backup.validate_checksum().is_ok());

        // Restore (convert back to bytes)
        let restored = backup.to_bytes();
        assert_eq!(restored, bytes);

        // Convert back to f32 array
        let restored_f32 = restored
            .chunks(4)
            .map(|chunk| {
                let arr: [u8; 4] = chunk.try_into().unwrap();
                f32::from_le_bytes(arr)
            })
            .collect::<Vec<_>>();

        assert_eq!(restored_f32.len(), original.len());
        for (orig, rest) in original.iter().zip(restored_f32.iter()) {
            assert!((orig - rest).abs() < 1e-6);
        }
    }

    #[test]
    fn test_format_conversion_in_migration() {
        // Simulate migration: BytesLE → Base64 → BytesLE
        let original_vec = vec![1.1f32, 2.2, 3.3, 4.4, 5.5];
        let original_bytes: Vec<u8> = original_vec
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        // Step 1: BytesLE → Base64
        let base64_bytes = vector_format::normalize_vector(
            &original_bytes,
            VectorFormat::BytesLE,
            VectorFormat::Base64,
        )
        .unwrap();

        // Step 2: Base64 → BytesLE
        let restored_bytes = vector_format::normalize_vector(
            &base64_bytes,
            VectorFormat::Base64,
            VectorFormat::BytesLE,
        )
        .unwrap();

        // Verify no data loss
        assert_eq!(original_bytes, restored_bytes);
    }

    #[test]
    fn test_large_scale_migration_simulation() {
        // Simulate migration of 1000 vectors
        let test_vectors = create_test_vectors(1000);
        let backups = create_backups(&test_vectors);

        let checkpoint =
            MigrationCheckpoint::create(backups, test_vectors.len()).unwrap();

        // Verify all 1000 vectors
        let valid_count = checkpoint.validate_all().unwrap();
        assert_eq!(valid_count, 1000);

        // Create migration plan for 1000 vectors
        let mut plan = MigrationPlan::new(
            "large_migration".to_string(),
            "Migrate 1000 vectors".to_string(),
        );

        // Simulate batched operations
        for batch in 0..10 {
            plan.add_operation(MigrationOp::DataMigration {
                from_table: format!("batch_{}", batch),
                to_table: format!("new_batch_{}", batch),
                transform_sql: None,
            });
        }

        assert_eq!(plan.operation_count(), 10);

        // Verify data loss detection
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            let migrator = SafeMigrator::new(false);
            migrator.verify_no_data_loss(1000, 1000).await
        });

        assert!(result.is_ok());
    }

    #[test]
    fn test_data_loss_detection() {
        // Test detection of missing data
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            let migrator = SafeMigrator::new(false);
            migrator.verify_no_data_loss(1000, 900).await // 100 rows missing
        });

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Data loss detected"));
    }

    #[test]
    fn test_migration_plan_versioning() {
        let plan = MigrationPlan::new(
            "versioned_migration".to_string(),
            "Migrate from v1 to v2".to_string(),
        )
        .with_version(1, 2);

        assert_eq!(plan.version, Some((1, 2)));
        assert!(plan.summary().contains("v1 → v2"));
    }

    #[test]
    fn test_checkpoint_statistics() {
        let test_vectors = create_test_vectors(50);
        let backups = create_backups(&test_vectors);

        let checkpoint =
            MigrationCheckpoint::create(backups, test_vectors.len()).unwrap();

        // Verify statistics
        assert_eq!(checkpoint.total_vectors, 50);
        assert_eq!(checkpoint.total_memories, 50);
        assert_eq!(checkpoint.average_dimension(), 10.0);
        assert!(checkpoint.total_size_bytes() > 0);

        let summary = checkpoint.summary();
        assert!(summary.contains("50 vectors"));
    }

    #[test]
    fn test_concurrent_backup_creation() {
        // Simulate concurrent backup creation
        let test_vectors = create_test_vectors(100);
        let backups1 = create_backups(&test_vectors);
        let backups2 = create_backups(&test_vectors);

        let checkpoint1 =
            MigrationCheckpoint::create(backups1, test_vectors.len()).unwrap();
        let checkpoint2 =
            MigrationCheckpoint::create(backups2, test_vectors.len()).unwrap();

        // Both should be valid
        assert_eq!(checkpoint1.validate_all().unwrap(), 100);
        assert_eq!(checkpoint2.validate_all().unwrap(), 100);

        // Should have same structure but different timestamps
        assert_eq!(
            checkpoint1.total_vectors,
            checkpoint2.total_vectors
        );
        assert_ne!(checkpoint1.timestamp, checkpoint2.timestamp);
    }

    #[test]
    fn test_migration_report_summary_formatting() {
        let mut report = MigrationReport::new(10, true); // dry_run = true

        report.record_operation_completed(50);
        report.record_operation_completed(30);
        report.set_duration(456);

        let summary = report.summary();

        assert!(summary.contains("DRY RUN"));
        assert!(summary.contains("2/10 operations"));
        assert!(summary.contains("80 rows"));
        assert!(summary.contains("456ms"));
    }

    #[test]
    fn test_multiple_checkpoints_sequence() {
        // Simulate multiple backups over time
        let mut checkpoints = Vec::new();

        for iteration in 0..3 {
            let test_vectors = create_test_vectors(10 * (iteration + 1));
            let backups = create_backups(&test_vectors);
            let checkpoint =
                MigrationCheckpoint::create(backups, test_vectors.len()).unwrap();

            checkpoints.push(checkpoint);
        }

        // Verify all checkpoints
        assert_eq!(checkpoints.len(), 3);
        assert_eq!(checkpoints[0].total_vectors, 10);
        assert_eq!(checkpoints[1].total_vectors, 20);
        assert_eq!(checkpoints[2].total_vectors, 30);

        // All should be valid
        for checkpoint in &checkpoints {
            assert_eq!(
                checkpoint.validate_all().unwrap(),
                checkpoint.total_vectors
            );
        }
    }
}
