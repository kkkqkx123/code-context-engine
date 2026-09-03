// Standard Library Detection Module
// Provides language-specific standard library detection

// Import macros first and make them available throughout the module
#[macro_use]
mod macros;

mod detector;

pub mod bash;
pub mod c;
pub mod cpp;
pub mod csharp;
pub mod dart;
pub mod go;
pub mod java;
pub mod javascript;
pub mod kotlin;
pub mod lua;
pub mod php;
pub mod python;
pub mod ruby;
pub mod rust;
pub mod scala;

// Re-export detectors
pub use csharp::CSharpStdlibDetector;
pub use go::GoStdlibDetector;
pub use java::JavaStdlibDetector;
pub use javascript::JavaScriptStdlibDetector;
pub use php::PhpStdlibDetector;
pub use python::PythonStdlibDetector;
pub use ruby::RubyStdlibDetector;
pub use rust::RustStdlibDetector;

pub use detector::StdlibDetector;
