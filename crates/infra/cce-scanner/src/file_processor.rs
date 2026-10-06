//! File processing module
//!
//! This module handles file content processing, including:
//! - File reading with size limits
//! - Content hash computation (SHA256)
//! - Binary file detection
//! - Language information detection
//! - FileEntry creation

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::models::FileEntry;
use cce_types::error::common;
use cce_types::language::LanguageInfo;
use cce_utils::hash::{calculate_hash, hash_file_stream_with_prefix};

/// Configuration for file processing
#[derive(Debug, Clone)]
pub struct FileProcessorConfig {
    /// Files above this size are hashed by streaming instead of a bulk read
    pub max_hash_file_size: u64,
    /// Size of content to check for binary detection
    pub binary_check_size: usize,
    /// Maximum content size to read
    pub max_content_size: u64,
}

impl Default for FileProcessorConfig {
    fn default() -> Self {
        Self {
            max_hash_file_size: 10 * 1024 * 1024, // 10MB
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
        self.process_file_with_metadata(path, root_path, &metadata)
    }

    /// Process a file reusing an already-fetched metadata record.
    ///
    /// Directory traversal already stats each path for filtering; passing the
    /// record down avoids a second metadata query per file.
    pub fn process_file_with_metadata(
        &self,
        path: &Path,
        root_path: &Path,
        metadata: &std::fs::Metadata,
    ) -> Result<FileEntry> {
        let relative_path = PathBuf::from(cce_types::path::relativize(root_path, path));

        let file_size = metadata.len();
        let modified = metadata
            .modified()
            .map_err(|e| Self::io_error("failed to get modified time", path, e))?
            .into();

        // The hash always covers the full file content: a prefix-only hash
        // would mask edits beyond the window through incremental (size+mtime
        // reuse and manifest hash) comparison. Large files stream through the
        // hasher in a single open while retaining a small prefix for binary
        // detection, instead of hashing and sampling in two separate opens.
        let (content_hash, sample) = if file_size > self.config.max_hash_file_size {
            let (hash, sample) = hash_file_stream_with_prefix(path, self.config.binary_check_size)
                .map_err(|e| Self::io_error("failed to hash file", path, e))?;
            (hash, sample)
        } else {
            let content =
                std::fs::read(path).map_err(|e| Self::io_error("failed to read file", path, e))?;
            (calculate_hash(&content), content)
        };

        Ok(Self::build_entry(
            path.to_path_buf(),
            relative_path,
            file_size,
            modified,
            content_hash,
            &sample,
            self.config.binary_check_size,
        ))
    }

    /// Shared manifest construction from fingerprint parts.
    ///
    /// Both the local path-based scan and the remote bytes-based supply end
    /// here, so language routing and the full-content hash domain stay
    /// identical.
    pub fn build_entry(
        path: PathBuf,
        relative_path: PathBuf,
        size: u64,
        modified: chrono::DateTime<chrono::Utc>,
        content_hash: String,
        sample: &[u8],
        binary_check_size: usize,
    ) -> FileEntry {
        let language_info = Self::classify_bytes(sample, binary_check_size, &relative_path);

        let mut entry = FileEntry::new(path, relative_path, size, modified).with_hash(content_hash);
        if let Some(language_info) = language_info {
            entry = entry.with_language_info(language_info);
        }
        entry
    }

    /// Pure byte classification shared by local and remote supply.
    ///
    /// Returns language info for text content and `None` for binary content.
    /// Language is still derived from the relative path; only the
    /// text/binary decision depends on the supplied bytes.
    pub fn classify_bytes(
        sample: &[u8],
        binary_check_size: usize,
        relative_path: &Path,
    ) -> Option<LanguageInfo> {
        if Self::is_text_file(sample, binary_check_size) {
            Some(LanguageInfo::detect_from_path(
                &relative_path.to_string_lossy(),
            ))
        } else {
            None
        }
    }

    /// Build a manifest entry from already-supplied bytes (remote form).
    ///
    /// No filesystem access happens here; the caller supplies the relative
    /// identity, bytes, size and modification time. The recorded path falls
    /// back to the relative form and serves display purposes only.
    pub fn process_bytes(
        &self,
        relative_path: &Path,
        bytes: &[u8],
        size: u64,
        modified: chrono::DateTime<chrono::Utc>,
    ) -> FileEntry {
        let content_hash = calculate_hash(bytes);
        let sample_len = bytes.len().min(self.config.binary_check_size);
        let language_info = Self::classify_bytes(
            &bytes[..sample_len],
            self.config.binary_check_size,
            relative_path,
        );
        let normalized = PathBuf::from(cce_types::path::normalize_project_path(
            &relative_path.to_string_lossy(),
        ));
        FileEntry {
            path: normalized.clone(),
            relative_path: normalized,
            size,
            modified,
            content_hash: Some(content_hash),
            language_info,
        }
    }

    /// Check if content is likely a text file
    ///
    /// Uses simple heuristic: if content contains null bytes, it's likely binary.
    /// BOM-less UTF-16 text is the exception: alternating NUL bytes with
    /// printable ASCII on the other parity decodes losslessly, so it counts
    /// as text to match the encoding detector.
    fn is_text_file(content: &[u8], check_size: usize) -> bool {
        if content.is_empty() {
            return true;
        }

        let check_len = content.len().min(check_size);
        if !content[..check_len].contains(&0x00) {
            return true;
        }
        Self::looks_like_utf16_without_bom(&content[..check_len])
    }

    /// Alternating-NUL shape with printable ASCII on the other parity.
    /// Thresholds mirror the encoding detector so the pre-check and the
    /// decoder agree on wide text instead of misclassifying it as binary.
    fn looks_like_utf16_without_bom(sample: &[u8]) -> bool {
        if sample.len() < 4 {
            return false;
        }
        let mut nul_even = 0usize;
        let mut nul_odd = 0usize;
        let mut printable = 0usize;
        let mut checked = 0usize;
        for pair in sample.chunks_exact(2) {
            let (a, b) = (pair[0], pair[1]);
            if a == 0 {
                nul_even += 1;
            } else if matches!(a, 0x09 | 0x0A | 0x0D | 0x20..=0x7E) {
                printable += 1;
            }
            if b == 0 {
                nul_odd += 1;
            } else if matches!(b, 0x09 | 0x0A | 0x0D | 0x20..=0x7E) {
                printable += 1;
            }
            checked += 1;
        }
        if checked == 0 {
            return false;
        }
        let nul_total = nul_even + nul_odd;
        if nul_total * 10 < checked * 4 {
            return false;
        }
        if nul_even.max(nul_odd) * 10 < nul_total * 9 {
            return false;
        }
        printable * 2 >= checked
    }

    /// Create IO error with context, preserving the original error kind
    fn io_error(context: &str, path: &Path, e: std::io::Error) -> crate::error::ScannerError {
        crate::error::ScannerError::Io(common::IoError::from(std::io::Error::new(
            e.kind(),
            format!("{}: {} - {}", context, path.display(), e),
        )))
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
    calculate_hash(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_processor() -> FileProcessor {
        FileProcessor::new()
    }

    #[test]
    fn test_process_file_hash_covers_full_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let body = b"Hello, World!";
        std::fs::write(&file_path, body).unwrap();

        let processor = create_test_processor();
        let entry = processor.process_file(&file_path, temp_dir.path()).unwrap();
        assert_eq!(entry.content_hash, Some(compute_content_hash(body)));
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
        assert_eq!(config.binary_check_size, 8192);
        assert_eq!(config.max_content_size, 1024 * 1024);
    }

    #[test]
    fn test_file_processor_config_clone() {
        let config = FileProcessorConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.max_hash_file_size, config.max_hash_file_size);
    }

    /// A file above the large-file threshold must hash its full content, so a
    /// change beyond the first window is still detected.
    #[test]
    fn test_process_file_large_streams_full_content_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("large.txt");

        // Threshold set low so the streaming path is exercised cheaply.
        let processor = FileProcessor::with_config(FileProcessorConfig {
            max_hash_file_size: 64,
            ..Default::default()
        });

        let mut body = b"a".repeat(100);
        std::fs::write(&file_path, &body).unwrap();
        let hash_before = processor
            .process_file(&file_path, temp_dir.path())
            .unwrap()
            .content_hash;

        // Change only the tail, well past the binary-check window.
        *body.last_mut().unwrap() = b'b';
        std::fs::write(&file_path, &body).unwrap();
        let hash_after = processor
            .process_file(&file_path, temp_dir.path())
            .unwrap()
            .content_hash;

        assert_ne!(hash_before, hash_after);
        assert_eq!(hash_after, Some(compute_content_hash(&body)));
    }
}
