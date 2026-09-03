//! Entity ID types

use std::str::FromStr;

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize, Deserializer, Serialize as SerdeSerialize, Serializer};

/// Entity ID (file-local incremental, simple and efficient)
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
#[rkyv(derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash))]
pub struct EntityId(pub u64);

/// Sentinel entity ID for file-level content (e.g., file-header comment
/// fragments). Shared by the comment processor dispatch and the grouper's
/// FileDocumentation group header so both refer to the same pseudo-entity.
pub const FILE_DOC_SENTINEL_ID: EntityId = EntityId(u64::MAX - 1);

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for EntityId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(EntityId(s.parse()?))
    }
}

impl EntityId {
    /// Parse entity ID from string, supporting both "123" and "entity:123" formats
    pub fn from_str_with_prefix(s: &str) -> Result<Self, String> {
        // Try to parse as plain number
        if let Ok(num) = u64::from_str(s) {
            return Ok(EntityId(num));
        }

        // Try to parse "entity:123" format
        if let Some(num_str) = s.strip_prefix("entity:") {
            if let Ok(num) = u64::from_str(num_str) {
                return Ok(EntityId(num));
            }
        }

        Err(format!("Invalid entity ID format: {}", s))
    }
}

impl SerdeSerialize for EntityId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for EntityId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(EntityId(u64::deserialize(deserializer)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_id() {
        let id = EntityId(42);
        assert_eq!(format!("{}", id), "42");
    }
}
