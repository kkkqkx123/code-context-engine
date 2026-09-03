//! Encoding detection and conversion module
//!
//! This module provides functionality for detecting text encodings and converting
//! between different character encodings, with a focus on UTF-8 as the target encoding.
//!
//! # Features
//!
//! - **Encoding Detection**: Automatic detection of file encodings with confidence scoring
//! - **Encoding Conversion**: Convert between various encodings (UTF-8, GBK, Big5, Shift_JIS, etc.)
//! - **BOM Handling**: Automatic detection and removal of Byte Order Marks
//! - **Configurable**: Customizable detection settings and conversion options
//!
//! # Supported Encodings
//!
//! - UTF-8 (with and without BOM)
//! - UTF-16LE / UTF-16BE
//! - GBK / GB18030
//! - Big5
//! - Shift_JIS
//!
//! # Example
//!
//! ```no_run
//! use cce_core::utils::encoding::{Detector, Encoder};
//! use std::path::Path;
//!
//! // Detect encoding
//! let detector = Detector::with_default_config();
//! let result = detector.detect_file(Path::new("test.txt")).expect("detection failed");
//! println!("Detected: {} (confidence: {:.2})", result.encoding, result.confidence);
//!
//! // Convert to UTF-8
//! let encoder = Encoder::default();
//! encoder.convert_file_to_utf8(Path::new("test.txt"), &result.encoding).expect("conversion failed");
//! ```

pub mod detector;
pub mod encoder;
pub mod error;
pub mod types;

pub use detector::Detector;
pub use encoder::Encoder;
pub use error::Error;
pub use types::{DetectorConfig, EncoderConfig, EncodingResult, EncodingType, Result};
