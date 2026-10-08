use cce_types::StorageError;

pub fn to_json<T: serde::Serialize>(value: &T) -> Result<String, StorageError> {
    serde_json::to_string(value).map_err(|error| StorageError::validation(error.to_string()))
}

pub fn optional_json<T: serde::Serialize>(
    value: &Option<T>,
) -> Result<Option<String>, StorageError> {
    value.as_ref().map(to_json).transpose()
}

pub fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, StorageError> {
    serde_json::from_str(value).map_err(|error| StorageError::validation(error.to_string()))
}

pub fn optional_from_json<T: serde::de::DeserializeOwned>(
    value: Option<String>,
) -> Result<Option<T>, StorageError> {
    value.map(|json| from_json(&json)).transpose()
}
