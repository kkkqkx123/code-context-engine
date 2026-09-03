//! Class-method processor
//!
//! This processor groups small classes with their methods and keeps large classes separate.
//! It also coordinates pattern detection (Builder, Factory, Getter/Setter) to filter methods.

mod nested;
mod pattern_detection;
mod processor;
mod types;

#[cfg(test)]
mod tests;

pub use processor::ClassMethodProcessor;
