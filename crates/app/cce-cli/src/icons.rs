//! Centralised CLI icon glyphs.
//!
//! Keeping the glyphs in one place makes it easy to keep output consistent
//! and to swap in ASCII fallbacks later without touching every call site.

/// Success / completed.
pub const SUCCESS: &str = "✓";
/// Failure / error.
pub const ERROR: &str = "✗";
/// Warning.
pub const WARNING: &str = "⚠";
/// Section separator.
pub const SEPARATOR: &str = "═";
