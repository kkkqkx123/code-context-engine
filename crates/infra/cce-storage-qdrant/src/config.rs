//! Configuration types for Qdrant vector storage
//!
//! All configuration types are defined in `cce_config` as the single source of
//! truth and re-exported here for the Qdrant backend to consume.

pub use cce_config::modules::{
    CollectionPreset, DistanceMetric, HnswConfig, ProductQuantizationConfig, QdrantConfig,
    QuantizationConfig, ScalarQuantizationConfig, VectorStorageConfig, WalConfig,
};
