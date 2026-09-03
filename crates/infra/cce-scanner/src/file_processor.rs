//! File processing module
//!
//! This module handles file content processing, including:
//! - File reading with size limits
//! - Content hash computation (SHA256)
//! - Binary file detection
//! - Language information detection
//! - FileEntry creation

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::models::FileEntry;
use cce_types::error::common;
use cce_types::language::LanguageInfo;
use cce_utils::hash::calculate_hash_with_limit;

/// Configuration for file processing
#[derive(Debug, Clone)]
pub struct FileProcessorConfig {
    /// Maximum file size for full content hash (larger files use partial hash)
    pub max_hash_file_size: u64,
    /// Size of partial hash for large files (default: 1MB)
    pub partial_hash_size: usize,
    /// Size of content to check for binary detection
    pub binary_check_size: usize,
    /// Maximum content size to read
    pub max_content_size: u64,
}

impl Default for FileProcessorConfig {
    fn default() -> Self {
        Self {
            max_hash_file_size: 10 * 1024 * 1024, // 10MB
            partial_hash_size: 1024 * 1024,       // 1MB
            binary_check_size: 8192,              // 8KB
            max_content_size: 1024 * 1024,        // 1MB
        }
    }
}

impl FileProcessorConfig {
    /// Create configuration from scanner config
    pub fn from_scanner_config(config: &cce_config::ScannerConfig) -> Self {
        Self {
            max_hash_file_size: config.max_hash_file_size,
            partial_hash_size: 1024 * 1024,
            binary_check_size: config.binary_check_size,
            max_content_size: config.default_max_content_size,
        }
    }
}

/// File processor for reading and analyzing file content
pub struct FileProcessor {
    config: FileProcessorConfig,
}

impl FileProcessor {
    /// Create a new file processor with default configuration
    pub fn new() -> Self {
        Self {
            config: FileProcessorConfig::default(),
        }
    }

    /// Create a new file processor with custom configuration
    pub fn with_config(config: FileProcessorConfig) -> Self {
        Self { config }
    }

    /// Process a single file and create FileEntry
    ///
    /// # Arguments
    ///
    /// * `path` - Absolute path to the file
    /// * `root_path` - Root directory for calculating relative path
    ///
    /// # Returns
    ///
    /// Returns `Ok(FileEntry)` on success, or an error if the file cannot be processed.
    pub fn process_file(&self, path: &Path, root_path: &Path) -> Result<FileEntry> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| Self::io_error("failed to get file metadata", path, e))?;

        let relative_path = PathBuf::from(cce_types::path::relativize(root_path, path));

        let file_size = metadata.len();

        // Read file content with size limit for hash computation
        let content = self.read_file_content_for_hash(path, file_size)?;

        let content_hash = self.compute_hash(&content, file_size);
        let is_text = Self::is_text_file(&content, self.config.binary_check_size);

        let language_info = if is_text {
            Some(LanguageInfo::detect_from_path(&path.to_string_lossy()))
        } else {
            None
        };

        Ok(FileEntry {
            path: path.to_path_buf(),
            relative_path,
            size: file_size,
            modified: metadata
                .modified()
                .map_err(|e| Self::io_error("failed to get modified time", path, e))?
                .into(),
            content_hash: Some(content_hash),
            language_info,
        })
    }

    /// Read file content for hash computation
    ///
    /// For large files, only reads the first portion to improve performance.
    fn read_file_content_for_hash(&self, path: &Path, file_size: u64) -> Result<Vec<u8>> {
        if file_size > self.config.max_hash_file_size {
            let mut file = std::fs::File::open(path)
                .map_err(|e| Self::io_error("failed to open file", path, e))?;
            let mut buffer = vec![0u8; self.config.partial_hash_size];
            let bytes_read = file
                .read(&mut buffer)
                .map_err(|e| Self::io_error("failed to read file content", path, e))?;
            buffer.truncate(bytes_read);
            Ok(buffer)
        } else {
            std::fs::read(path).map_err(|e| Self::io_error("failed to read file", path, e))
        }
    }

    /// Compute SHA256 hash of file content
    ///
    /// For large files, only hash the first portion to improve performance.
    fn compute_hash(&self, content: &[u8], file_size: u64) -> String {
        let limit = if file_size > self.config.max_hash_file_size {
            Some(self.config.partial_hash_size)
        } else {
            None
        };

        calculate_hash_with_limit(content, limit)
    }

    /// Check if content is likely a text file
    ///
    /// Uses simple heuristic: if content contains null bytes, it's likely binary.
    fn is_text_file(content: &[u8], check_size: usize) -> bool {
        if content.is_empty() {
            return true;
        }

        let check_len = content.len().min(check_size);
        !content[..check_len].contains(&0x00)
    }

    /// Create IO error with context
    fn io_error(
        context: &str,
        path: &Path,
        e: impl std::fmt::Display,
    ) -> crate::error::ScannerError {
        crate::error::ScannerError::Io(common::IoError(std::io::Error::other(format!(
            "{}: {} - {}",
            context,
            path.display(),
            e
        ))))
    }
}

impl Default for FileProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility function to compute content hash
///
/// This is a standalone version that doesn't require creating a FileProcessor.
pub fn compute_content_hash(content: &[u8]) -> String {
    calculate_hash_with_limit(content, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_processor() -> FileProcessor {
        FileProcessor::new()
    }

    #[test]
    fn test_compute_hash_empty_content() {
        let processor = create_test_processor();
        let content = b"";
        let hash = processor.compute_hash(content, 0);
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_compute_hash_text_content() {
        let processor = create_test_processor();
        let content = b"Hello, World!";
        let hash = processor.compute_hash(content, content.len() as u64);
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64);

        let hash2 = processor.compute_hash(content, content.len() as u64);
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_compute_hash_different_content() {
        let processor = create_test_processor();
        let content1 = b"Hello, World!";
        let content2 = b"Hello, World?";
        let hash1 = processor.compute_hash(content1, content1.len() as u64);
        let hash2 = processor.compute_hash(content2, content2.len() as u64);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_hash_large_file_truncation() {
        let processor = FileProcessor::with_config(FileProcessorConfig {
            max_hash_file_size: 1024,
            partial_hash_size: 100,
            ..Default::default()
        });
        let large_content = vec![b'a'; 1024 + 100];
        let hash = processor.compute_hash(&large_content, large_content.len() as u64);
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_is_text_file_empty() {
        assert!(FileProcessor::is_text_file(b"", 8192));
    }

    #[test]
    fn test_is_text_file_plain_text() {
        let content = b"Hello, World!\nThis is a text file.\n";
        assert!(FileProcessor::is_text_file(content, 8192));
    }

    #[test]
    fn test_is_text_file_with_unicode() {
        let content = "Hello, world! 🌍\n".as_bytes();
        assert!(FileProcessor::is_text_file(content, 8192));
    }

    #[test]
    fn test_is_text_file_binary_with_null() {
        let content = b"Hello\x00World";
        assert!(!FileProcessor::is_text_file(content, 8192));
    }

    #[test]
    fn test_is_text_file_binary_multiple_nulls() {
        let content = vec![0u8; 100];
        assert!(!FileProcessor::is_text_file(&content, 8192));
    }

    #[test]
    fn test_process_file_text() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, World!").unwrap();

        let processor = create_test_processor();
        let entry = processor.process_file(&file_path, temp_dir.path()).unwrap();

        assert_eq!(entry.size, 13);
        assert!(entry.content_hash.is_some());
        assert!(entry.is_text());
    }

    #[test]
    fn test_process_file_binary() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.bin");
        std::fs::write(&file_path, vec![0u8, 1, 2, 0, 3]).unwrap();

        let processor = create_test_processor();
        let entry = processor.process_file(&file_path, temp_dir.path()).unwrap();

        assert!(entry.content_hash.is_some());
        assert!(!entry.is_text());
    }

    #[test]
    fn test_compute_content_hash_utility() {
        let hash1 = compute_content_hash(b"Hello, World!");
        let hash2 = compute_content_hash(b"Hello, World!");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn test_file_processor_config_default() {
        let config = FileProcessorConfig::default();
        assert_eq!(config.max_hash_file_size, 10 * 1024 * 1024);
        assert_eq!(config.partial_hash_size, 1024 * 1024);
        assert_eq!(config.binary_check_size, 8192);
        assert_eq!(config.max_content_size, 1024 * 1024);
    }

    #[test]
    fn test_file_processor_config_clone() {
        let config = FileProcessorConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.max_hash_file_size, config.max_hash_file_size);
        assert_eq!(cloned.partial_hash_size, config.partial_hash_size);
    }
}
