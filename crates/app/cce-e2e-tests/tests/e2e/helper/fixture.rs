//! Test fixture access for integration tests.
//!
//! Integration tests reuse the single fixture tree under `fixtures/` through
//! the library loaders in [`cce_e2e_tests::fixture`]. This module only
//! re-exports those loaders so `crate::helper::fixture::*` paths keep working.

pub use cce_e2e_tests::fixture::{
    EmptyFixture, FixtureAccess, FixtureCategory, FixtureSpec, TestFixture,
};
