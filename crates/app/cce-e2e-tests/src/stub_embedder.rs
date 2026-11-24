//! Deterministic vector generator for drift-sweep tests.
//!
//! Vectors are derived from an FNV-1a hash of `(model_name, text)`, so the
//! same input always yields byte-identical output within and across runs,
//! while changing the model name changes every vector. This makes
//! "re-embedded vectors differ / stay equal" exactly assertable without any
//! tolerance threshold, network, or real LLM service.

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Deterministic vector generator: vectors are pure functions of model name +
/// input text. Records every embedded text and every batch call for assertions.
pub struct HashEmbedder {
    model_name: String,
    dimension: usize,
    calls: AtomicUsize,
    seen_texts: Mutex<HashSet<String>>,
}

impl HashEmbedder {
    pub fn new(model_name: impl Into<String>, dimension: usize) -> Self {
        Self {
            model_name: model_name.into(),
            dimension: dimension.max(1),
            calls: AtomicUsize::new(0),
            seen_texts: Mutex::new(HashSet::new()),
        }
    }

    /// Number of batch `embed` calls.
    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    /// Every distinct text this embedder has embedded so far.
    pub fn seen_texts(&self) -> HashSet<String> {
        self.seen_texts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Deterministic vector for `text` under the current model name.
    pub fn vector_for(&self, text: &str) -> Vec<f32> {
        (0..self.dimension)
            .map(|index| {
                let hash = fnv1a(format!("{}:{text}:{index}", self.model_name).as_bytes());
                let scaled = (hash % 20_001) as f32 - 10_000.0;
                scaled / 10_000.0
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_inputs_produce_identical_vectors() {
        let embedder = HashEmbedder::new("model-a", 8);
        let first = embedder.vector_for("some chunk text");
        let second = embedder.vector_for("some chunk text");
        assert_eq!(first, second);
    }

    #[test]
    fn different_model_names_change_every_vector() {
        let a = HashEmbedder::new("model-a", 8);
        let b = HashEmbedder::new("model-b", 8);
        assert_ne!(a.vector_for("same text"), b.vector_for("same text"));
    }

    #[test]
    fn different_texts_change_the_vector() {
        let embedder = HashEmbedder::new("model-a", 8);
        assert_ne!(
            embedder.vector_for("text one"),
            embedder.vector_for("text two")
        );
    }
}
