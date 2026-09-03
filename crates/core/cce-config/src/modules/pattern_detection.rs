//! Getter/Setter pattern detection configuration
//!
//! The grouper keeps only spec-based detection (getter/setter, test suites);
//! framework and design pattern detection has been removed.

use serde::{Deserialize, Serialize};

/// Getter/Setter detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetterSetterDetectionConfig {
    pub strict_mode: bool,
    pub max_simple_lines: usize,
    pub kotlin_accessor_support: bool,
}

impl Default for GetterSetterDetectionConfig {
    fn default() -> Self {
        Self {
            strict_mode: false,
            max_simple_lines: 3,
            kotlin_accessor_support: true,
        }
    }
}
