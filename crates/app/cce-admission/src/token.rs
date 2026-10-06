//! Token hashing and constant-time verification.
//!
//! Raw tokens are hashed with SHA-256 at load time. Request verification
//! hashes the presented value once and compares fixed-size digests with an
//! accumulating XOR, so comparison time never depends on where two values
//! first differ.

use sha2::{Digest, Sha256};

/// Minimum accepted token length, enforced by configuration validation.
pub const MIN_TOKEN_LEN: usize = 16;

/// Number of hash bytes rendered into log fingerprints.
const FINGERPRINT_BYTES: usize = 4;

/// Hash a token value into a fixed-size digest.
pub fn hash_token(token: &str) -> [u8; 32] {
    let digest = Sha256::digest(token.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Constant-time digest comparison.
pub fn equal_hashes(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= left[i] ^ right[i];
    }
    diff == 0
}

/// Short non-sensitive fingerprint identifying a token in logs.
pub fn fingerprint(hash: &[u8; 32]) -> String {
    hex::encode(&hash[..FINGERPRINT_BYTES])
}

/// A token entry reduced to its digest, scope, and log fingerprint.
#[derive(Debug, Clone)]
pub struct StoredToken {
    /// SHA-256 digest of the token value.
    pub hash: [u8; 32],
    /// Log-safe fingerprint derived from the digest.
    pub fingerprint: String,
    /// Explicitly authorized project ids. Empty authorizes no project scope.
    pub projects: Vec<i64>,
}

/// Pre-hashed token set used on the request hot path.
#[derive(Debug, Clone, Default)]
pub struct TokenStore {
    entries: Vec<StoredToken>,
}

impl TokenStore {
    /// Pre-hash every configured token. Callers must validate first.
    pub fn from_config(config: &crate::config::AdmissionConfig) -> Self {
        let entries = config
            .tokens
            .iter()
            .map(|entry| {
                let hash = hash_token(&entry.token);
                StoredToken {
                    fingerprint: fingerprint(&hash),
                    hash,
                    projects: entry.projects.clone(),
                }
            })
            .collect();
        Self { entries }
    }

    /// Whether the store holds at least one token.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of tokens in the store.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Find the entry matching the presented token value.
    pub fn authenticate(&self, presented: &str) -> Option<&StoredToken> {
        if presented.len() < MIN_TOKEN_LEN {
            return None;
        }
        let candidate = hash_token(presented);
        self.entries
            .iter()
            .find(|entry| equal_hashes(&entry.hash, &candidate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_hashes_detects_differences() {
        let left = hash_token("alpha-token-value-1");
        let same = hash_token("alpha-token-value-1");
        let other = hash_token("alpha-token-value-2");
        assert!(equal_hashes(&left, &same));
        assert!(!equal_hashes(&left, &other));
    }

    #[test]
    fn store_authenticates_exact_value_only() {
        let config = crate::config::AdmissionConfig {
            enabled: true,
            tokens: vec![crate::config::TokenEntry {
                token: "alpha-token-value-1".to_string(),
                projects: vec![1],
            }],
            ..crate::config::AdmissionConfig::default()
        };
        let store = TokenStore::from_config(&config);
        assert!(store.authenticate("alpha-token-value-1").is_some());
        assert!(store.authenticate("alpha-token-value-2").is_none());
        assert!(store.authenticate("short").is_none());
    }
}
