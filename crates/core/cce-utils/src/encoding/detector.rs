use std::path::{Path, PathBuf};
use tracing::{debug, warn};

use crate::encoding::error::Error;
use crate::encoding::types::{DetectorConfig, EncodingResult, Result};

pub struct Detector {
    config: DetectorConfig,
}

impl Detector {
    pub fn new(config: DetectorConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self::new(DetectorConfig::default())
    }

    pub fn with_min_confidence(min_confidence: f64) -> Self {
        let config = DetectorConfig { min_confidence };
        Self::new(config)
    }

    pub fn detect_bytes(&self, data: &[u8]) -> Result<EncodingResult> {
        self.detect_bytes_with_source(data, "<bytes>")
    }

    pub fn detect_file(&self, path: &Path) -> Result<EncodingResult> {
        debug!(
            path = %path.display(),
            "Detecting file encoding"
        );
        let data = std::fs::read(path)
            .map_err(|e| Error::file_not_found(format!("{}: {}", path.display(), e)))?;
        self.detect_bytes_with_source(&data, path.display().to_string().as_str())
    }

    pub fn detect_files_parallel(
        &self,
        paths: &[PathBuf],
    ) -> Result<Vec<(PathBuf, EncodingResult)>> {
        debug!(
            files_count = paths.len(),
            "Detecting encodings for multiple files in parallel"
        );
        let results: Result<Vec<_>> = paths
            .iter()
            .map(|path| {
                let result = self.detect_file(path)?;
                Ok((path.clone(), result))
            })
            .collect();
        results
    }

    fn detect_bytes_with_source(&self, data: &[u8], source: &str) -> Result<EncodingResult> {
        let result = self.detect_internal(data)?;

        if result.confidence < self.config.min_confidence {
            warn!(
                source = %source,
                encoding = %result.encoding,
                confidence = result.confidence,
                threshold = self.config.min_confidence,
                "Low confidence encoding detection"
            );
            return Err(Error::low_confidence(
                &result.encoding,
                result.confidence,
                self.config.min_confidence,
            ));
        }

        debug!(
            source = %source,
            encoding = %result.encoding,
            confidence = result.confidence,
            "Encoding detected"
        );

        Ok(result)
    }

    fn detect_internal(&self, data: &[u8]) -> Result<EncodingResult> {
        if data.is_empty() {
            return Ok(EncodingResult::new("UTF-8", 1.0));
        }

        if let Some(bom_encoding) = self.detect_bom(data) {
            return Ok(EncodingResult::new(bom_encoding, 1.0));
        }

        // NUL-containing UTF-8 is technically valid, so a BOM-less UTF-16
        // file would otherwise be claimed as UTF-8 with mojibake. Check the
        // alternating-NUL shape before the UTF-8 fast path so the decoder
        // and the scanner pre-check agree on wide text.
        if let Some(wide) = Self::detect_utf16_without_bom(data) {
            return Ok(EncodingResult::new(wide, 0.85));
        }

        if self.is_ascii(data) {
            return Ok(EncodingResult::new("UTF-8", 1.0));
        }

        if self.is_valid_utf8(data) {
            let confidence = if self.has_high_ascii(data) {
                0.90
            } else {
                0.95
            };
            return Ok(EncodingResult::new("UTF-8", confidence));
        }

        // Delegate remaining detection to encoding-guess, which applies
        // byte-validity filtering and CJK gating instead of the legacy
        // density-penalised heuristics.
        let guess = encoding_guess::Guess::new().guess(data);
        Ok(EncodingResult::new(guess.encoding, guess.confidence))
    }

    pub fn detect_bom(&self, data: &[u8]) -> Option<String> {
        if data.len() >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
            return Some("UTF-8".to_string());
        }
        if data.len() >= 2 {
            if data[0] == 0xFF && data[1] == 0xFE {
                return Some("UTF-16LE".to_string());
            }
            if data[0] == 0xFE && data[1] == 0xFF {
                return Some("UTF-16BE".to_string());
            }
        }
        None
    }

    pub fn is_ascii(&self, data: &[u8]) -> bool {
        data.iter().all(|&b| b <= 127)
    }

    pub fn has_high_ascii(&self, data: &[u8]) -> bool {
        data.iter().any(|&b| b > 127)
    }

    pub fn is_valid_utf8(&self, data: &[u8]) -> bool {
        std::str::from_utf8(data).is_ok()
    }

    /// BOM-less UTF-16 shape: alternating NUL bytes with printable ASCII on
    /// the other parity. Conservative thresholds keep binary files out: NULs
    /// must be frequent and strongly aligned to one parity.
    fn detect_utf16_without_bom(data: &[u8]) -> Option<&'static str> {
        if data.len() < 4 || data.len() % 2 != 0 {
            return None;
        }
        let mut nul_even = 0usize;
        let mut nul_odd = 0usize;
        let mut printable_other = 0usize;
        let mut checked = 0usize;
        for pair in data.chunks_exact(2) {
            let (a, b) = (pair[0], pair[1]);
            if a == 0 {
                nul_even += 1;
            } else if matches!(a, 0x09 | 0x0A | 0x0D | 0x20..=0x7E) {
                printable_other += 1;
            }
            if b == 0 {
                nul_odd += 1;
            } else if matches!(b, 0x09 | 0x0A | 0x0D | 0x20..=0x7E) {
                printable_other += 1;
            }
            checked += 1;
            if checked >= 4096 {
                break;
            }
        }
        let nul_total = nul_even + nul_odd;
        if checked == 0 || nul_total * 10 < checked * 4 {
            return None;
        }
        let aligned = nul_even.max(nul_odd);
        if aligned * 10 < nul_total * 9 {
            return None;
        }
        if printable_other * 2 < checked {
            return None;
        }
        if nul_odd >= nul_even {
            Some("UTF-16LE")
        } else {
            Some("UTF-16BE")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detector_default() {
        let detector = Detector::with_default_config();
        assert_eq!(detector.config.min_confidence, 0.7);
    }

    #[test]
    fn test_detector_with_min_confidence() {
        let detector = Detector::with_min_confidence(0.8);
        assert_eq!(detector.config.min_confidence, 0.8);
    }

    #[test]
    fn test_detect_bytes_empty() {
        let detector = Detector::with_default_config();
        let result = detector.detect_bytes(&[]).expect("should detect");
        assert_eq!(result.encoding, "UTF-8");
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn test_detect_bytes_ascii() {
        let detector = Detector::with_default_config();
        let data = b"Hello World";
        let result = detector.detect_bytes(data).expect("should detect");
        assert_eq!(result.encoding, "UTF-8");
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn test_detect_bytes_utf8() {
        let detector = Detector::with_default_config();
        let data = "Hello, world.".as_bytes();
        let result = detector.detect_bytes(data).expect("should detect");
        assert_eq!(result.encoding, "UTF-8");
        assert!(result.confidence >= 0.9);
    }

    #[test]
    fn test_detect_bytes_utf8_bom() {
        let detector = Detector::with_default_config();
        let data = [0xEF, 0xBB, 0xBF, 0x48, 0x65, 0x6C, 0x6C, 0x6F];
        let result = detector.detect_bytes(&data).expect("should detect");
        assert_eq!(result.encoding, "UTF-8");
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn test_detect_bytes_utf16le_bom() {
        let detector = Detector::with_default_config();
        let data = [
            0xFF, 0xFE, 0x48, 0x00, 0x65, 0x00, 0x6C, 0x00, 0x6C, 0x00, 0x6F, 0x00,
        ];
        let result = detector.detect_bytes(&data).expect("should detect");
        assert_eq!(result.encoding, "UTF-16LE");
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn test_detect_bytes_utf16be_bom() {
        let detector = Detector::with_default_config();
        let data = [
            0xFE, 0xFF, 0x00, 0x48, 0x00, 0x65, 0x00, 0x6C, 0x00, 0x6C, 0x00, 0x6F,
        ];
        let result = detector.detect_bytes(&data).expect("should detect");
        assert_eq!(result.encoding, "UTF-16BE");
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn test_detect_bytes_low_confidence() {
        let detector = Detector::with_default_config();
        let data = [0x80, 0x81, 0x82, 0x83];
        let result = detector.detect_bytes(&data);
        assert!(result.is_err());
        assert!(matches!(result, Err(Error::LowConfidence { .. })));
    }

    #[test]
    fn test_detect_bytes_low_confidence_with_custom_threshold() {
        let detector = Detector::with_min_confidence(0.1);
        let data = [0x80, 0x81, 0x82, 0x83];
        let result = detector.detect_bytes(&data).expect("should detect");
        assert!(result.confidence >= 0.1);
    }

    #[test]
    fn test_detect_bom() {
        let detector = Detector::with_default_config();

        let utf8_bom = [0xEF, 0xBB, 0xBF, 0x48, 0x65, 0x6C, 0x6C, 0x6F];
        assert_eq!(detector.detect_bom(&utf8_bom), Some("UTF-8".to_string()));

        let utf16le_bom = [0xFF, 0xFE, 0x48, 0x00];
        assert_eq!(
            detector.detect_bom(&utf16le_bom),
            Some("UTF-16LE".to_string())
        );

        let utf16be_bom = [0xFE, 0xFF, 0x00, 0x48];
        assert_eq!(
            detector.detect_bom(&utf16be_bom),
            Some("UTF-16BE".to_string())
        );

        let no_bom = [0x48, 0x65, 0x6C, 0x6C, 0x6F];
        assert_eq!(detector.detect_bom(&no_bom), None);
    }

    #[test]
    fn test_is_ascii() {
        let detector = Detector::with_default_config();
        assert!(detector.is_ascii(b"Hello World"));
        assert!(detector.is_ascii(b""));
        assert!(!detector.is_ascii(&[0x80]));
        assert!(!detector.is_ascii(b"Hello\x80World"));
    }

    #[test]
    fn test_has_high_ascii() {
        let detector = Detector::with_default_config();
        assert!(!detector.has_high_ascii(b"Hello World"));
        assert!(!detector.has_high_ascii(b""));
        assert!(detector.has_high_ascii(&[0x80]));
        assert!(detector.has_high_ascii(b"Hello\x80World"));
    }

    #[test]
    fn test_is_valid_utf8() {
        let detector = Detector::with_default_config();
        assert!(detector.is_valid_utf8(b"Hello World"));
        assert!(detector.is_valid_utf8("Hello, world.".as_bytes()));
        assert!(!detector.is_valid_utf8(&[0x80, 0x81, 0x82, 0x83]));
    }
}
