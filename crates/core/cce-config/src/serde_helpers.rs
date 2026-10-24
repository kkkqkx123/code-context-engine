//! Serde helper functions for configuration deserialization
//!
//! This module provides custom serde deserialize functions to handle
//! common configuration patterns like treating empty strings as None.

use serde::{Deserialize, Deserializer};

/// Deserialize an optional string, treating empty strings as None.
///
/// This is useful for configuration fields where an empty string
/// should be treated the same as not specifying a value.
///
/// # Example
///
/// ```rust
/// use serde::Deserialize;
/// use cce_config::serde_helpers::empty_string_as_none;
///
/// #[derive(Deserialize)]
/// struct Config {
///     #[serde(deserialize_with = "empty_string_as_none")]
///     path: Option<String>,
/// }
/// ```
pub fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) if s.is_empty() => Ok(None),
        other => Ok(other),
    }
}

/// Serialize an optional string, converting None to an empty string.
///
/// This is the counterpart to `empty_string_as_none` for round-trip serialization.
///
/// # Example
///
/// ```rust
/// use serde::Serialize;
/// use cce_config::serde_helpers::none_as_empty_string;
///
/// #[derive(Serialize)]
/// struct Config {
///     #[serde(serialize_with = "none_as_empty_string")]
///     path: Option<String>,
/// }
/// ```
pub fn none_as_empty_string<S>(value: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(s) => serializer.serialize_str(s),
        None => serializer.serialize_str(""),
    }
}

/// Deserialize an explicit JSON null as `Some(None)`.
///
/// Used for patch fields whose global counterpart is itself optional
/// (for example `max_file_size` or `document_chunking`). A missing field
/// means "inherit the global value" (`None`), while an explicit null means
/// "override the global value with nothing" (`Some(None)`). Plain
/// `Option<Option<T>>` cannot express this: serde maps null to the outer
/// `None`, collapsing the two intents.
///
/// Must be paired with `skip_serializing_if = "Option::is_none"` so an
/// absent field is omitted rather than serialized as null.
pub fn explicit_null_as_some_none<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use serde_json;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestConfig {
        #[serde(
            deserialize_with = "empty_string_as_none",
            serialize_with = "none_as_empty_string"
        )]
        value: Option<String>,
    }

    #[test]
    fn test_empty_string_as_none() {
        let json = r#"{"value": ""}"#;
        let config: TestConfig = serde_json::from_str(json).expect("Failed to deserialize");
        assert_eq!(config.value, None);
    }

    #[test]
    fn test_some_string() {
        let json = r#"{"value": "test"}"#;
        let config: TestConfig = serde_json::from_str(json).expect("Failed to deserialize");
        assert_eq!(config.value, Some("test".to_string()));
    }

    #[test]
    fn test_null_as_none() {
        let json = r#"{"value": null}"#;
        let config: TestConfig = serde_json::from_str(json).expect("Failed to deserialize");
        assert_eq!(config.value, None);
    }

    #[test]
    fn test_roundtrip() {
        let config = TestConfig { value: None };
        let json = serde_json::to_string(&config).expect("Failed to serialize");
        let parsed: TestConfig = serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(config, parsed);

        let config = TestConfig {
            value: Some("test".to_string()),
        };
        let json = serde_json::to_string(&config).expect("Failed to serialize");
        let parsed: TestConfig = serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(config, parsed);
    }
}
