// Standard Library Detection Module (re-export from cce-stdlib)
// This module retains the original `crate::parser::stdlib` path for backward
// compatibility after the detection logic was extracted into the standalone
// `cce-stdlib` crate. New code should prefer `cce_stdlib` directly.

pub use cce_stdlib::*;
