//! Point kind enumeration (cross-layer storage contract)

use serde::{Deserialize, Serialize};

/// Kind of a vector point stored in the shared Qdrant collection.
///
/// The variant discriminant IS the storage encoding (Qdrant payload
/// `type` field). Only append new variants; never reorder or reuse
/// existing codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum PointKind {
    /// Chunk-level point (default)
    #[default]
    Chunk = 0,
    /// File-summary point
    Summary = 1,
}

impl Serialize for PointKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.as_u8())
    }
}

impl<'de> Deserialize<'de> for PointKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = u8::deserialize(deserializer)?;
        Self::from_u8(code)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown PointKind code: {code}")))
    }
}

impl PointKind {
    /// Storage encoding of this kind.
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Decode a storage-encoded kind. Returns `None` for unknown codes.
    #[inline]
    pub const fn from_u8(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Chunk),
            1 => Some(Self::Summary),
            _ => None,
        }
    }

    /// Human-readable name for rendering layers only.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chunk => "chunk",
            Self::Summary => "summary",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_roundtrip() {
        for code in 0u8..=1 {
            let kind = PointKind::from_u8(code).expect("valid code");
            assert_eq!(kind.as_u8(), code);
        }
        assert!(PointKind::from_u8(2).is_none());
        assert_eq!(PointKind::default(), PointKind::Chunk);
    }
}
