//! LLM provider types
//!
//! This module provides types for LLM provider configuration.

use serde::{Deserialize, Serialize};

/// Provider type - distinguishes between remote/cloud and local services
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    /// Remote/cloud service
    #[default]
    Remote,
    /// Local service
    Local,
}

impl ProviderType {
    /// Check if this is a local service
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_type_default() {
        let provider_type = ProviderType::default();
        assert_eq!(provider_type, ProviderType::Remote);
    }

    #[test]
    fn test_provider_type_is_local() {
        assert!(!ProviderType::Remote.is_local());
        assert!(ProviderType::Local.is_local());
    }

    #[test]
    fn test_provider_type_serialization() {
        let remote = ProviderType::Remote;
        let json = serde_json::to_string(&remote).unwrap();
        assert_eq!(json, "\"remote\"");

        let local = ProviderType::Local;
        let json = serde_json::to_string(&local).unwrap();
        assert_eq!(json, "\"local\"");
    }
}
