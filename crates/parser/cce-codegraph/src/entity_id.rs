//! Shared EntityId space conventions.
//!
//! Real entity IDs are allocated sequentially from 0 and never set the high
//! bit. Synthetic IDs produced by the project symbol table set it, keeping
//! the two spaces disjoint so they can be distinguished with
//! [`is_synthetic_id`].

/// Marker bit (63) distinguishing synthetic symbol-table IDs from real IDs.
pub const SYNTHETIC_MARK: u64 = 1u64 << 63;

/// Whether the given raw ID carries the synthetic marker bit.
pub fn is_synthetic_id(raw: u64) -> bool {
    raw & SYNTHETIC_MARK != 0
}

/// Extract the low counter from a synthetic raw ID.
pub fn synthetic_counter(raw: u64) -> u64 {
    raw & !SYNTHETIC_MARK
}
