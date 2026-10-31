//! Score normalization for the search pipeline.
//!
//! Normalizes candidate scores onto one scale so the final threshold filter
//! observes a uniform score range regardless of recall mode.

pub mod normalization;

pub use normalization::{NormalizationStrategy, normalize_scores};
