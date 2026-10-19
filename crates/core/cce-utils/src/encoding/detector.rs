use std::path::Path;
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
        let guess = encoding_guess::Guess::new().guess(data);
        Ok(EncodingResult::new(guess.encoding, guess.confidence))
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
        let data = "Hello, 世界.".as_bytes();
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
}
