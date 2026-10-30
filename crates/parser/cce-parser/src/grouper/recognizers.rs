//! Entity recognizers for test suite detection
//!
//! This module provides recognizers for identifying test suites and test cases
//! (Rust, Java, Go, JS/TS, Python).
//!
//! Framework, design pattern and standard library detection are intentionally
//! not built in: stdlib identity is decided once during parsing
//! (`post_processing::mark_plugin_declared_external`) and cached on the
//! entity, and the plugin
//! system provides domain-specific recognition.

pub mod test_suite;
