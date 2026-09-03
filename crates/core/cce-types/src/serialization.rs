//! Serialization utilities for cache storage
//!
//! This module provides optimized serialization functions using Rkyv + Zstd:
//! - Compression (30-50% smaller than uncompressed)
//!
//! Use this for short-term cache storage in pure Rust environment.
//!
//! # Envelope format
//!
//! Every payload is framed by a small durable header:
//!
//! ```text
//! [ magic "CCEI" (4B) | INDEX_FORMAT_VERSION (u32 LE)
//!   | plugin fingerprint (u16 len + bytes) | zstd(rkyv data) ]
//! ```
//!
//! Deserialization rejects any buffer whose header is missing, whose recorded
//! index-format version differs from the current one, or whose plugin-language
//! fingerprint no longer matches the registry. Payloads referencing
//! `Language::Custom` indices are thereby invalidated wholesale whenever the
//! plugin set or its registration order changes.

use rkyv::de::Pool;
use rkyv::rancor::Strategy;
use rkyv::{from_bytes, rancor::Error, to_bytes};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SerializationError {
    #[error("Zstd error: {0}")]
    Zstd(#[from] std::io::Error),
    #[error("Rkyv serialization error: {0}")]
    RkyvSerialize(String),
    #[error("Rkyv deserialization error: {0}")]
    RkyvDeserialize(String),
    #[error("Cache envelope header missing or malformed")]
    MalformedHeader,
    #[error(
        "Index format version mismatch: payload was written for version {found}, \
         current version is {expected}; rebuild the affected caches"
    )]
    VersionMismatch { expected: u32, found: u32 },
    #[error(
        "Plugin language fingerprint mismatch: the registered plugin set changed; \
         cached payloads with custom-language references must be regenerated"
    )]
    FingerprintMismatch,
}

/// Magic marker of the cache envelope.
const CACHE_MAGIC: [u8; 4] = *b"CCEI";

/// Size of the fixed envelope prefix before the variable-length
/// fingerprint field: magic + u32 version.
const FIXED_HEADER_LEN: usize = CACHE_MAGIC.len() + std::mem::size_of::<u32>();

/// Maximum encoded length of the plugin-language fingerprint field.
///
/// The fingerprint is a SHA-256 hex string (64 bytes); the bound only guards
/// against malformed headers claiming absurd lengths.
const MAX_FINGERPRINT_LEN: usize = 4096;

pub fn serialize_for_cache<T>(data: &T) -> Result<(Vec<u8>, usize, usize), SerializationError>
where
    for<'a> T: rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                Error,
            >,
        >,
{
    let bytes =
        to_bytes::<Error>(data).map_err(|e| SerializationError::RkyvSerialize(e.to_string()))?;

    let original_size = bytes.len();
    let compressed = zstd::encode_all(&*bytes, 3)?;
    let compressed_size = compressed.len();

    let fingerprint = crate::types::language::plugin_language_fingerprint();
    let fingerprint_bytes = fingerprint.as_bytes();
    let fingerprint_len =
        u16::try_from(fingerprint_bytes.len()).map_err(|_| SerializationError::MalformedHeader)?;

    let mut framed = Vec::with_capacity(
        FIXED_HEADER_LEN + std::mem::size_of::<u16>() + fingerprint_bytes.len() + compressed.len(),
    );
    framed.extend_from_slice(&CACHE_MAGIC);
    framed.extend_from_slice(&crate::types::INDEX_FORMAT_VERSION.to_le_bytes());
    framed.extend_from_slice(&fingerprint_len.to_le_bytes());
    framed.extend_from_slice(fingerprint_bytes);
    framed.extend_from_slice(&compressed);

    Ok((framed, original_size, compressed_size))
}

pub fn deserialize_from_cache<T>(data: &[u8]) -> Result<T, SerializationError>
where
    T: rkyv::Archive,
    for<'a> T::Archived: rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, Error>>
        + rkyv::Deserialize<T, Strategy<Pool, Error>>,
{
    let payload = strip_header(data)?;
    let decompressed = zstd::decode_all(payload)?;
    from_bytes::<T, Error>(&decompressed)
        .map_err(|e| SerializationError::RkyvDeserialize(e.to_string()))
}

/// Validate the envelope header and return the compressed payload slice.
fn strip_header(data: &[u8]) -> Result<&[u8], SerializationError> {
    if data.len() < FIXED_HEADER_LEN || data[..CACHE_MAGIC.len()] != CACHE_MAGIC {
        return Err(SerializationError::MalformedHeader);
    }
    let mut version_bytes = [0u8; std::mem::size_of::<u32>()];
    version_bytes
        .copy_from_slice(&data[CACHE_MAGIC.len()..CACHE_MAGIC.len() + std::mem::size_of::<u32>()]);
    let version = u32::from_le_bytes(version_bytes);
    if version != crate::types::INDEX_FORMAT_VERSION {
        return Err(SerializationError::VersionMismatch {
            expected: crate::types::INDEX_FORMAT_VERSION,
            found: version,
        });
    }

    // Variable-length plugin fingerprint field.
    let len_offset = FIXED_HEADER_LEN;
    if data.len() < len_offset + std::mem::size_of::<u16>() {
        return Err(SerializationError::MalformedHeader);
    }
    let mut len_bytes = [0u8; std::mem::size_of::<u16>()];
    len_bytes.copy_from_slice(&data[len_offset..len_offset + std::mem::size_of::<u16>()]);
    let fingerprint_len = u16::from_le_bytes(len_bytes) as usize;
    if fingerprint_len > MAX_FINGERPRINT_LEN {
        return Err(SerializationError::MalformedHeader);
    }
    let fp_offset = len_offset + std::mem::size_of::<u16>();
    let fp_end = fp_offset
        .checked_add(fingerprint_len)
        .ok_or(SerializationError::MalformedHeader)?;
    if data.len() < fp_end {
        return Err(SerializationError::MalformedHeader);
    }
    let stored = std::str::from_utf8(&data[fp_offset..fp_end])
        .map_err(|_| SerializationError::MalformedHeader)?;
    if stored != crate::types::language::plugin_language_fingerprint() {
        return Err(SerializationError::FingerprintMismatch);
    }

    Ok(&data[fp_end..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialization::{deserialize_from_cache, serialize_for_cache};
    use rkyv::{Archive, Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Archive)]
    struct TestData {
        id: u64,
        name: String,
        values: Vec<i32>,
    }

    #[test]
    fn test_cache_serialization() {
        let data = TestData {
            id: 42,
            name: "test".to_string(),
            values: vec![1, 2, 3],
        };

        let (compressed, _original_size, compressed_size) =
            serialize_for_cache(&data).expect("Serialization failed");
        assert!(
            compressed_size > 0,
            "Compressed size should be positive, got {}",
            compressed_size,
        );

        let deserialized: TestData =
            deserialize_from_cache(&compressed).expect("Deserialization failed");

        assert_eq!(data, deserialized);
    }

    /// Payloads without the current envelope header must be rejected instead
    /// of decoding into a potentially changed type layout.
    #[test]
    fn test_version_mismatch_invalidates_payload() {
        let data = TestData {
            id: 1,
            name: "stale".to_string(),
            values: vec![],
        };
        let (framed, _, _) = serialize_for_cache(&data).expect("Serialization failed");

        // Corrupt the recorded version (first byte of the u32 LE version).
        let mut stale = framed.clone();
        let version_offset = CACHE_MAGIC.len();
        stale[version_offset] = stale[version_offset].wrapping_add(1);
        let result: Result<TestData, _> = deserialize_from_cache(&stale);
        assert!(
            matches!(result, Err(SerializationError::VersionMismatch { .. })),
            "stale version must be rejected"
        );

        // Missing header entirely.
        let result: Result<TestData, _> = deserialize_from_cache(&framed[framed.len()..]);
        assert!(matches!(result, Err(SerializationError::MalformedHeader)));
    }

    /// Tampering with the recorded plugin fingerprint must invalidate the
    /// payload (models a plugin set / registration-order change).
    #[test]
    fn test_fingerprint_mismatch_invalidates_payload() {
        let data = TestData {
            id: 2,
            name: "plugin".to_string(),
            values: vec![],
        };
        let (framed, _, _) = serialize_for_cache(&data).expect("Serialization failed");

        // The fingerprint field starts after magic + version + u16 length;
        // flip one hex character of the SHA-256 string.
        let fp_offset = FIXED_HEADER_LEN + std::mem::size_of::<u16>();
        let mut tampered = framed.clone();
        tampered[fp_offset] ^= 0x01;

        let result: Result<TestData, _> = deserialize_from_cache(&tampered);
        assert!(
            matches!(result, Err(SerializationError::FingerprintMismatch)),
            "drifted plugin fingerprints must be rejected"
        );
    }
}
