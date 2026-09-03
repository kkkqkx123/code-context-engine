//! Hash utility functions

use serde::Serialize;

use sha2::{Digest, Sha256};

/// Calculate SHA-256 hash for content
///
/// Returns a 64-character hexadecimal string representing the SHA-256 hash.
///
/// # Examples
///
/// ```
/// use cce_core::utils::hash::calculate_hash;
///
/// let content = b"hello world";
/// let hash = calculate_hash(content);
/// assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex characters
/// ```
pub fn calculate_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

/// Deterministically hash a serializable value.
///
/// Serializes the value to JSON (field order is fixed by the struct
/// definition) and hashes the bytes with SHA-256. Unserializable values
/// degrade to a stable hash of a serialization-error marker rather than
/// failing the caller.
pub fn hash_serializable<T: Serialize>(value: &T) -> String {
    let json = match serde_json::to_string(value) {
        Ok(json) => json,
        Err(error) => format!("serialization_error:{error}"),
    };
    calculate_hash(json.as_bytes())
}

/// Calculate SHA-256 hash for content with size limit
///
/// For large files, only hashes the first `limit` bytes to improve performance.
/// This is useful for file change detection where full content hashing is expensive.
///
/// # Arguments
///
/// * `content` - The content to hash
/// * `limit` - Maximum number of bytes to hash (None for full content)
///
/// # Examples
///
/// ```
/// use cce_core::utils::hash::calculate_hash_with_limit;
///
/// let content = b"hello world";
/// let hash = calculate_hash_with_limit(content, Some(5));
/// // Only hashes "hello"
/// ```
pub fn calculate_hash_with_limit(content: &[u8], limit: Option<usize>) -> String {
    let hash_len = match limit {
        Some(l) => content.len().min(l),
        None => content.len(),
    };

    let mut hasher = Sha256::new();
    hasher.update(&content[..hash_len]);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_hash_length() {
        let content = b"test content";
        let hash = calculate_hash(content);
        assert_eq!(hash.len(), 64, "SHA-256 hash should be 64 hex characters");
    }

    #[test]
    fn test_calculate_hash_consistency() {
        let content = b"consistent content";
        let hash1 = calculate_hash(content);
        let hash2 = calculate_hash(content);
        assert_eq!(hash1, hash2, "Same content should produce same hash");
    }

    #[test]
    fn test_calculate_hash_different_content() {
        let hash1 = calculate_hash(b"content1");
        let hash2 = calculate_hash(b"content2");
        assert_ne!(
            hash1, hash2,
            "Different content should produce different hashes"
        );
    }

    #[test]
    fn test_calculate_hash_empty() {
        let hash = calculate_hash(b"");
        assert_eq!(
            hash.len(),
            64,
            "Empty content should still produce 64-char hash"
        );
        // SHA-256 of empty string is known
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_calculate_hash_large_content() {
        let content = vec![0u8; 10000];
        let hash = calculate_hash(&content);
        assert_eq!(
            hash.len(),
            64,
            "Large content should still produce 64-char hash"
        );
    }

    #[test]
    fn test_calculate_hash_with_limit() {
        let content = b"hello world";
        let full_hash = calculate_hash(content);
        let limited_hash = calculate_hash_with_limit(content, Some(5));

        assert_eq!(limited_hash.len(), 64);
        assert_ne!(
            full_hash, limited_hash,
            "Limited hash should differ from full hash"
        );
    }

    #[test]
    fn test_calculate_hash_with_limit_none() {
        let content = b"hello world";
        let full_hash = calculate_hash(content);
        let unlimited_hash = calculate_hash_with_limit(content, None);

        assert_eq!(
            full_hash, unlimited_hash,
            "None limit should produce same hash as full hash"
        );
    }

    #[test]
    fn test_calculate_hash_with_limit_exceeds_content() {
        let content = b"short";
        let hash1 = calculate_hash_with_limit(content, Some(100));
        let hash2 = calculate_hash(content);

        assert_eq!(
            hash1, hash2,
            "Limit exceeding content length should hash all content"
        );
    }

    #[test]
    fn test_calculate_hash_with_limit_empty() {
        let hash = calculate_hash_with_limit(b"", Some(1024));
        assert_eq!(hash.len(), 64);
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
