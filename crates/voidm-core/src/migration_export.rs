//! Vector backup and export layer for safe data migrations
//!
//! This module provides functionality to:
//! - Create snapshots of all vectors in the database
//! - Export vectors to JSON for backup/portability
//! - Verify data integrity with checksums
//! - Restore vectors from backups
//! - Support safe migrations between vector backends
//!
//! The backup format is backend-agnostic and portable:
//! ```json
//! {
//!   "timestamp": "2026-03-26T12:00:00Z",
//!   "total_vectors": 1000,
//!   "total_memories": 1000,
//!   "checksum": "sha256:...",
//!   "vectors": [
//!     {
//!       "memory_id": "abc123",
//!       "embedding": [0.1, 0.2, ...],
//!       "dimension": 384,
//!       "backed_up_at": "2026-03-26T12:00:00Z",
//!       "checksum": "sha256:..."
//!     }
//!   ]
//! }
//! ```

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

/// A single vector backup record with integrity checking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorBackup {
    /// Memory ID this vector belongs to
    pub memory_id: String,
    /// Vector as f32 array (backend-agnostic representation)
    pub embedding: Vec<f32>,
    /// Dimension of the vector (usually 384-1024)
    pub dimension: usize,
    /// When this vector was backed up
    pub backed_up_at: String,
    /// SHA256 checksum for integrity verification
    pub checksum: String,
}

/// A checkpoint snapshot of all vectors at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationCheckpoint {
    /// ISO-8601 timestamp when checkpoint was created
    pub timestamp: String,
    /// Total number of vectors in checkpoint
    pub total_vectors: usize,
    /// Total number of memories in checkpoint
    pub total_memories: usize,
    /// All vectors in this checkpoint
    pub vectors: Vec<VectorBackup>,
    /// SHA256 checksum of entire checkpoint
    pub checksum: String,
}

impl VectorBackup {
    /// Create a backup record from raw bytes (little-endian f32 format)
    pub fn from_bytes(memory_id: String, bytes: &[u8]) -> Result<Self> {
        // Validate bytes are divisible by 4 (f32 size)
        if bytes.len() % 4 != 0 {
            return Err(anyhow!(
                "Invalid vector bytes: {} not divisible by 4 (f32 size)",
                bytes.len()
            ));
        }

        // Convert bytes to f32 array
        let mut embedding = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks(4) {
            let bytes_arr: [u8; 4] = chunk
                .try_into()
                .context("Failed to convert chunk to [u8;4]")?;
            embedding.push(f32::from_le_bytes(bytes_arr));
        }

        let dimension = embedding.len();
        let backed_up_at = chrono::Utc::now().to_rfc3339();

        // Calculate checksum before adding to struct
        let checksum = Self::compute_checksum_for_vector(&embedding);

        Ok(VectorBackup {
            memory_id,
            embedding,
            dimension,
            backed_up_at,
            checksum,
        })
    }

    /// Convert this backup back to bytes (little-endian f32 format)
    pub fn to_bytes(&self) -> Vec<u8> {
        self.embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect()
    }

    /// Compute SHA256 checksum for a vector
    fn compute_checksum_for_vector(embedding: &[f32]) -> String {
        let mut hasher = Sha256::new();
        for f in embedding {
            hasher.update(f.to_le_bytes());
        }
        format!("sha256:{}", hex::encode(hasher.finalize()))
    }

    /// Compute checksum for this backup record
    pub fn compute_checksum(&self) -> String {
        Self::compute_checksum_for_vector(&self.embedding)
    }

    /// Validate that stored checksum matches computed checksum
    pub fn validate_checksum(&self) -> Result<()> {
        let expected = self.compute_checksum();
        if self.checksum == expected {
            Ok(())
        } else {
            Err(anyhow!(
                "Checksum mismatch for memory {}: expected {}, got {}",
                self.memory_id,
                expected,
                self.checksum
            ))
        }
    }

    /// Size of this backup in bytes
    pub fn size_bytes(&self) -> usize {
        self.embedding.len() * std::mem::size_of::<f32>()
    }
}

impl MigrationCheckpoint {
    /// Create a new checkpoint from vector data
    pub fn create(
        vectors: Vec<VectorBackup>,
        total_memories: usize,
    ) -> Result<Self> {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let total_vectors = vectors.len();

        // Validate all vectors before creating checkpoint
        for (idx, vec) in vectors.iter().enumerate() {
            vec.validate_checksum().with_context(|| {
                format!("Vector {} failed validation", idx)
            })?;
        }

        // Compute checkpoint-level checksum
        let mut hasher = Sha256::new();
        hasher.update(timestamp.as_bytes());
        hasher.update(total_vectors.to_le_bytes());
        hasher.update(total_memories.to_le_bytes());
        for vec in &vectors {
            hasher.update(vec.checksum.as_bytes());
        }
        let checksum = format!("sha256:{}", hex::encode(hasher.finalize()));

        Ok(MigrationCheckpoint {
            timestamp,
            total_vectors,
            total_memories,
            vectors,
            checksum,
        })
    }

    /// Save checkpoint to JSON file
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
            .with_context(|| format!("Failed to write checkpoint to {:?}", path))?;
        Ok(())
    }

    /// Load checkpoint from JSON file
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read checkpoint from {:?}", path))?;
        let checkpoint: MigrationCheckpoint = serde_json::from_str(&json)
            .with_context(|| format!("Failed to parse checkpoint JSON from {:?}", path))?;
        Ok(checkpoint)
    }

    /// Validate all vectors in this checkpoint
    pub fn validate_all(&self) -> Result<usize> {
        let mut valid_count = 0;
        for (idx, vec) in self.vectors.iter().enumerate() {
            match vec.validate_checksum() {
                Ok(_) => valid_count += 1,
                Err(e) => {
                    eprintln!("Vector {} failed validation: {}", idx, e);
                }
            }
        }

        if valid_count != self.total_vectors {
            return Err(anyhow!(
                "Checkpoint validation failed: {} of {} vectors valid",
                valid_count,
                self.total_vectors
            ));
        }

        Ok(valid_count)
    }

    /// Validate checkpoint-level checksum
    pub fn validate_checkpoint_checksum(&self) -> Result<()> {
        let mut hasher = Sha256::new();
        hasher.update(self.timestamp.as_bytes());
        hasher.update(self.total_vectors.to_le_bytes());
        hasher.update(self.total_memories.to_le_bytes());
        for vec in &self.vectors {
            hasher.update(vec.checksum.as_bytes());
        }
        let expected = format!("sha256:{}", hex::encode(hasher.finalize()));

        if self.checksum == expected {
            Ok(())
        } else {
            Err(anyhow!(
                "Checkpoint checksum mismatch: expected {}, got {}",
                expected,
                self.checksum
            ))
        }
    }

    /// Total size of all vectors in bytes
    pub fn total_size_bytes(&self) -> usize {
        self.vectors.iter().map(|v| v.size_bytes()).sum()
    }

    /// Average vector dimension
    pub fn average_dimension(&self) -> f32 {
        if self.vectors.is_empty() {
            return 0.0;
        }
        let total: usize = self.vectors.iter().map(|v| v.dimension).sum();
        total as f32 / self.vectors.len() as f32
    }

    /// Summary statistics
    pub fn summary(&self) -> String {
        format!(
            "MigrationCheckpoint: {} vectors, {} memories, {} bytes, avg_dim={}",
            self.total_vectors,
            self.total_memories,
            self.total_size_bytes(),
            self.average_dimension()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_backup_from_bytes() {
        // Create a simple f32 vector: [1.0, 2.0, 3.0]
        let vec = vec![1.0f32, 2.0, 3.0];
        let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();

        let backup = VectorBackup::from_bytes("mem123".to_string(), &bytes).unwrap();

        assert_eq!(backup.memory_id, "mem123");
        assert_eq!(backup.embedding, vec![1.0, 2.0, 3.0]);
        assert_eq!(backup.dimension, 3);
        assert!(backup.checksum.starts_with("sha256:"));
    }

    #[test]
    fn test_vector_backup_to_bytes_roundtrip() {
        let original = vec![1.5f32, 2.5, 3.5];
        let bytes: Vec<u8> = original.iter().flat_map(|f| f.to_le_bytes()).collect();

        let backup = VectorBackup::from_bytes("mem123".to_string(), &bytes).unwrap();
        let restored_bytes = backup.to_bytes();

        assert_eq!(bytes, restored_bytes);
        // Verify float values are preserved
        let restored: Vec<f32> = restored_bytes
            .chunks(4)
            .map(|chunk| {
                let arr: [u8; 4] = chunk.try_into().unwrap();
                f32::from_le_bytes(arr)
            })
            .collect();
        assert_eq!(restored, original);
    }

    #[test]
    fn test_vector_backup_checksum() {
        let bytes: Vec<u8> = vec![1.0f32, 2.0, 3.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        let backup = VectorBackup::from_bytes("mem123".to_string(), &bytes).unwrap();

        // Checksum should be valid
        assert!(backup.validate_checksum().is_ok());

        // Modifying the backup should invalidate checksum
        let mut corrupted = backup.clone();
        corrupted.embedding[0] = 99.0;

        assert!(corrupted.validate_checksum().is_err());
    }

    #[test]
    fn test_vector_backup_invalid_bytes() {
        // 3 bytes is not divisible by 4
        let bytes = vec![1u8, 2, 3];

        let result = VectorBackup::from_bytes("mem123".to_string(), &bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_migration_checkpoint_create() {
        let vec1 = VectorBackup::from_bytes(
            "mem1".to_string(),
            &vec![1.0f32, 2.0].iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<_>>(),
        )
        .unwrap();

        let vec2 = VectorBackup::from_bytes(
            "mem2".to_string(),
            &vec![3.0f32, 4.0].iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<_>>(),
        )
        .unwrap();

        let checkpoint = MigrationCheckpoint::create(vec![vec1, vec2], 2).unwrap();

        assert_eq!(checkpoint.total_vectors, 2);
        assert_eq!(checkpoint.total_memories, 2);
        assert!(checkpoint.validate_checkpoint_checksum().is_ok());
    }

    #[test]
    fn test_migration_checkpoint_validate_all() {
        let vec1 = VectorBackup::from_bytes(
            "mem1".to_string(),
            &vec![1.0f32, 2.0].iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<_>>(),
        )
        .unwrap();

        let checkpoint = MigrationCheckpoint::create(vec![vec1], 1).unwrap();

        let valid_count = checkpoint.validate_all().unwrap();
        assert_eq!(valid_count, 1);
    }

    #[test]
    fn test_migration_checkpoint_file_roundtrip() {
        let vec1 = VectorBackup::from_bytes(
            "mem1".to_string(),
            &vec![1.0f32, 2.0].iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<_>>(),
        )
        .unwrap();

        let checkpoint = MigrationCheckpoint::create(vec![vec1], 1).unwrap();

        // Save to temp file
        let temp_path = std::path::PathBuf::from("/tmp/test_checkpoint.json");
        checkpoint.save_to_file(&temp_path).unwrap();

        // Load from temp file
        let loaded = MigrationCheckpoint::load_from_file(&temp_path).unwrap();

        assert_eq!(loaded.total_vectors, checkpoint.total_vectors);
        assert_eq!(loaded.total_memories, checkpoint.total_memories);
        assert_eq!(loaded.checksum, checkpoint.checksum);

        // Clean up
        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn test_large_vector_backup() {
        // Test with a large 1024-D vector
        let large_vec: Vec<f32> = (0..1024).map(|i| i as f32).collect();
        let bytes: Vec<u8> = large_vec.iter().flat_map(|f| f.to_le_bytes()).collect();

        let backup = VectorBackup::from_bytes("mem_large".to_string(), &bytes).unwrap();

        assert_eq!(backup.dimension, 1024);
        assert_eq!(backup.size_bytes(), 1024 * 4);
        assert!(backup.validate_checksum().is_ok());
    }
}
