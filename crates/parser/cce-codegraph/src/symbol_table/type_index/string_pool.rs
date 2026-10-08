use std::collections::HashMap;

/// Build-time string pool for snapshot construction.
///
/// Interns strings and assigns monotonically increasing `u32` indices.
/// The dedup `HashMap` is only used during construction and is not archived.
pub struct StringPoolBuilder {
    strings: Vec<String>,
    dedup: HashMap<String, u32>,
}

impl StringPoolBuilder {
    pub fn new() -> Self {
        Self {
            strings: Vec::new(),
            dedup: HashMap::new(),
        }
    }

    /// Intern a string, returning its pool index. Deduplicates identical strings.
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&idx) = self.dedup.get(s) {
            return idx;
        }
        let idx = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.dedup.insert(s.to_string(), idx);
        idx
    }

    /// Consume the builder, returning the deduplicated string list.
    pub fn into_pool(self) -> Vec<String> {
        self.strings
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

impl Default for StringPoolBuilder {
    fn default() -> Self {
        Self::new()
    }
}
