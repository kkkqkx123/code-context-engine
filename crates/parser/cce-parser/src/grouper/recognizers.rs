//! Entity recognizers for standard library and test suite detection
//!
//! This module provides recognizers for identifying:
//! - Standard library entities (Vec, HashMap, println!, etc.)
//! - Test suites and test cases (Rust, Java, Go, JS/TS, Python)
//!
//! Framework and design pattern detection is intentionally not built in;
//! plugin system provides such domain-specific recognition.

mod stdlib;
pub mod test_suite;

#[cfg(test)]
mod tests;

pub use stdlib::{StdlibEntityDetector, StdlibEntityInfo};
