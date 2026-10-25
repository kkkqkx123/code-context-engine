//! Remote transmission policy: wire budget enforcement.
//!
//! The byte budget is a transport strategy, not a contract rule. The
//! contract validates single-file and single-batch limits; the transport
//! decides how many chunks fit on the wire so batches stay inside the
//! admission body limit after base64 growth.

use cce_api::models::{MAX_INGEST_BATCH_FILES, MAX_INGEST_CHUNKS_PER_BATCH};

/// Wire byte budget per gateway upload batch.
///
/// Accounting uses base64 wire length so a batch stays inside the admission
/// body limit after transport growth. The gateway filters with this bound
/// before pushing so oversized batches never consume transfer bandwidth.
pub const GATEWAY_BATCH_WIRE_BYTES: u64 = 4 * 1024 * 1024;

/// Whether the accumulated batch must flush before adding more chunks.
pub fn should_flush_batch(chunk_count: usize, file_count: usize, wire_bytes: u64) -> bool {
    chunk_count >= MAX_INGEST_CHUNKS_PER_BATCH
        || file_count >= MAX_INGEST_BATCH_FILES
        || wire_bytes >= GATEWAY_BATCH_WIRE_BYTES
}

/// Number of fixed-size chunks needed for a byte size.
///
/// Single source lives in the supply kernel; this wrapper keeps transport
/// callers on the same calculation without introducing a second implementation.
pub fn total_chunks_for_size(size: u64, chunk_bytes: usize) -> u32 {
    cce_supply::total_chunks_for_size(size, chunk_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_triggers_on_any_bound() {
        assert!(should_flush_batch(MAX_INGEST_CHUNKS_PER_BATCH, 0, 0));
        assert!(should_flush_batch(0, MAX_INGEST_BATCH_FILES, 0));
        assert!(should_flush_batch(0, 0, GATEWAY_BATCH_WIRE_BYTES));
        assert!(!should_flush_batch(1, 1, 1));
    }

    #[test]
    fn chunk_count_matches_contract_shape() {
        assert_eq!(total_chunks_for_size(1, 256 * 1024), 1);
        assert_eq!(total_chunks_for_size(256 * 1024 + 1, 256 * 1024), 2);
    }
}
