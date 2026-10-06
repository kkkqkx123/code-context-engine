//! Admission configuration.
//!
//! The configuration lives in this crate on purpose: the shared application
//! configuration knows nothing about remote admission, so local builds never
//! parse remote fields. Remote deployments supply tokens through the
//! environment; each token carries the explicit project set it may access.
//! An entry without projects authenticates but authorizes no project scope.

use serde::{Deserialize, Serialize};

use crate::token::MIN_TOKEN_LEN;

/// One long-lived token and the project set it may access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEntry {
    /// Raw token value. Keep it out of logs; only hashes are compared.
    pub token: String,
    /// Explicitly authorized project ids. Empty authorizes no project scope.
    #[serde(default)]
    pub projects: Vec<i64>,
}

/// Admission settings for the remote hosting shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmissionConfig {
    /// Master switch. The server refuses non-loopback binds when this is
    /// false in an admission-enabled build.
    #[serde(default)]
    pub enabled: bool,
    /// Accepted tokens with their project bindings.
    #[serde(default)]
    pub tokens: Vec<TokenEntry>,
    /// Path prefixes that bypass authentication (health probes).
    #[serde(default = "default_public_paths")]
    pub public_path_prefixes: Vec<String>,
    /// Per-token request budget per minute. Zero disables limiting.
    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_min: u32,
    /// Largest request body the admission layer buffers for scope checks.
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,
}

fn default_public_paths() -> Vec<String> {
    vec!["/api/health".to_string()]
}

fn default_rate_limit() -> u32 {
    600
}

fn default_max_body_bytes() -> usize {
    8 * 1024 * 1024
}

impl Default for AdmissionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            tokens: Vec::new(),
            public_path_prefixes: default_public_paths(),
            rate_limit_per_min: default_rate_limit(),
            max_body_bytes: default_max_body_bytes(),
        }
    }
}

impl AdmissionConfig {
    /// Load admission settings from the process environment.
    ///
    /// `CCE_ADMISSION_TOKENS` holds semicolon separated entries shaped as
    /// `token@1,2` or a bare `token` with no project scope. The optional
    /// `CCE_ADMISSION_PUBLIC_PATHS` overrides the default bypass list with
    /// comma separated prefixes, `CCE_ADMISSION_RATE_LIMIT_PER_MIN` tunes the
    /// per-token budget, and `CCE_ADMISSION_MAX_BODY_BYTES` bounds buffered
    /// bodies. A missing token variable yields a disabled configuration.
    pub fn from_env() -> Result<Self, crate::middleware::AdmissionError> {
        let mut config = Self::default();
        let Some(raw) = std::env::var("CCE_ADMISSION_TOKENS")
            .ok()
            .filter(|v| !v.trim().is_empty())
        else {
            return Ok(config);
        };
        config.enabled = true;
        config.tokens = parse_token_env(&raw)?;
        if let Ok(raw_paths) = std::env::var("CCE_ADMISSION_PUBLIC_PATHS") {
            let paths: Vec<String> = raw_paths
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            if !paths.is_empty() {
                config.public_path_prefixes = paths;
            }
        }
        if let Ok(raw_limit) = std::env::var("CCE_ADMISSION_RATE_LIMIT_PER_MIN") {
            config.rate_limit_per_min = raw_limit.trim().parse().map_err(|_| {
                crate::middleware::AdmissionError::config(
                    "CCE_ADMISSION_RATE_LIMIT_PER_MIN must be a non-negative integer",
                )
            })?;
        }
        if let Ok(raw_max) = std::env::var("CCE_ADMISSION_MAX_BODY_BYTES") {
            config.max_body_bytes = raw_max.trim().parse().map_err(|_| {
                crate::middleware::AdmissionError::config(
                    "CCE_ADMISSION_MAX_BODY_BYTES must be a non-negative integer",
                )
            })?;
            if config.max_body_bytes == 0 {
                config.max_body_bytes = default_max_body_bytes();
            }
        }
        config.validate()?;
        Ok(config)
    }

    /// Whether this configuration can actually admit traffic.
    pub fn is_configured(&self) -> bool {
        self.enabled && !self.tokens.is_empty()
    }

    /// Reject weak or ambiguous token bindings before serving traffic.
    pub fn validate(&self) -> Result<(), crate::middleware::AdmissionError> {
        use crate::middleware::AdmissionError;
        if !self.enabled {
            return Ok(());
        }
        if self.tokens.is_empty() {
            return Err(AdmissionError::config(
                "admission is enabled but no tokens are configured",
            ));
        }
        let mut seen: Vec<[u8; 32]> = Vec::with_capacity(self.tokens.len());
        for entry in &self.tokens {
            if entry.token.len() < MIN_TOKEN_LEN {
                return Err(AdmissionError::config(
                    "every admission token must be at least 16 characters",
                ));
            }
            if entry.projects.iter().any(|id| *id <= 0) {
                return Err(AdmissionError::config(
                    "admission token projects must hold positive project ids",
                ));
            }
            let hash = crate::token::hash_token(&entry.token);
            if seen
                .iter()
                .any(|known| crate::token::equal_hashes(known, &hash))
            {
                return Err(AdmissionError::config("duplicate admission token value"));
            }
            seen.push(hash);
        }
        if self.public_path_prefixes.iter().any(|p| p.is_empty()) {
            return Err(AdmissionError::config(
                "admission public path prefixes must not be empty",
            ));
        }
        Ok(())
    }
}

/// Parse the `CCE_ADMISSION_TOKENS` entries described in `from_env`.
fn parse_token_env(raw: &str) -> Result<Vec<TokenEntry>, crate::middleware::AdmissionError> {
    use crate::middleware::AdmissionError;
    let mut entries = Vec::new();
    for item in raw.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (token, scope) = match item.split_once('@') {
            Some((token, scope)) => (token.trim(), Some(scope)),
            None => (item, None),
        };
        if token.is_empty() {
            return Err(AdmissionError::config(
                "admission token value must not be empty",
            ));
        }
        let mut projects = Vec::new();
        if let Some(scope) = scope {
            for part in scope.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                let id: i64 = part.parse().map_err(|_| {
                    AdmissionError::config("admission token projects must be integer ids")
                })?;
                projects.push(id);
            }
        }
        projects.sort_unstable();
        projects.dedup();
        entries.push(TokenEntry {
            token: token.to_string(),
            projects,
        });
    }
    if entries.is_empty() {
        return Err(AdmissionError::config(
            "CCE_ADMISSION_TOKENS holds no usable entries",
        ));
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_env_parses_scoped_and_bare_entries() {
        let entries =
            parse_token_env("alpha-token-value-1@1,2;beta-token-value-22").expect("parse");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].projects, vec![1, 2]);
        assert!(entries[1].projects.is_empty());
    }

    #[test]
    fn token_env_rejects_non_integer_scope() {
        let err = parse_token_env("alpha-token-value-1@nope").expect_err("must fail");
        assert!(err.to_string().contains("integer"));
    }

    #[test]
    fn disabled_config_skips_validation() {
        let config = AdmissionConfig::default();
        assert!(!config.is_configured());
        config.validate().expect("disabled config is valid");
    }

    #[test]
    fn enabled_config_rejects_short_tokens() {
        let config = AdmissionConfig {
            enabled: true,
            tokens: vec![TokenEntry {
                token: "short".to_string(),
                projects: vec![1],
            }],
            ..AdmissionConfig::default()
        };
        config.validate().expect_err("short token must fail");
    }
}
