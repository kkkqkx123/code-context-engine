use std::path::{Path, PathBuf};
use tracing::{debug, warn};

use crate::encoding::error::Error;
use crate::encoding::types::{DetectorConfig, EncodingResult, EncodingType, Result};

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

    pub fn with_encodings(encodings: Vec<String>) -> Self {
        let config = DetectorConfig {
            detect_encodings: encodings,
            ..Default::default()
        };
        Self::new(config)
    }

    pub fn with_min_confidence(min_confidence: f64) -> Self {
        let config = DetectorConfig {
            min_confidence,
            ..Default::default()
        };
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

        if self.is_ascii(data) {
            return Ok(EncodingResult::new("UTF-8", 1.0));
        }

        // NUL-containing UTF-8 is technically valid, so a BOM-less UTF-16
        // file would otherwise be claimed as UTF-8 with mojibake. Check the
        // alternating-NUL shape before the UTF-8 fast path so the decoder
        // and the scanner pre-check agree on wide text.
        if let Some(wide) = Self::detect_utf16_without_bom(data) {
            return Ok(EncodingResult::new(wide, 0.85));
        }

        if self.is_valid_utf8(data) {
            let confidence = if self.has_high_ascii(data) {
                0.90
            } else {
                0.95
            };
            return Ok(EncodingResult::new("UTF-8", confidence));
        }

        self.heuristic_detect(data)
    }

    fn detect_bom(&self, data: &[u8]) -> Option<String> {
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

    fn is_ascii(&self, data: &[u8]) -> bool {
        data.iter().all(|&b| b <= 127)
    }

    fn has_high_ascii(&self, data: &[u8]) -> bool {
        data.iter().any(|&b| b > 127)
    }

    fn is_valid_utf8(&self, data: &[u8]) -> bool {
        std::str::from_utf8(data).is_ok()
    }

    fn heuristic_detect(&self, data: &[u8]) -> Result<EncodingResult> {
        let mut candidates = Vec::new();

        for encoding_name in &self.config.detect_encodings {
            if let Some(encoding_type) = EncodingType::parse(encoding_name) {
                if let Some(confidence) = self.detect_encoding(data, encoding_type) {
                    candidates.push((encoding_type, confidence));
                }
            }
        }

        if candidates.is_empty() {
            // No multi-byte encoding matched: remaining high-byte text is
            // most likely a single-byte Western encoding. WINDOWS-1252
            // decodes every byte, so label it honestly at low confidence
            // instead of mislabeling the bytes as UTF-8.
            return Ok(EncodingResult::new("WINDOWS-1252", 0.3));
        }

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let best = &candidates[0];
        Ok(EncodingResult::new(best.0.as_str(), best.1))
    }

    fn detect_encoding(&self, data: &[u8], encoding_type: EncodingType) -> Option<f64> {
        match encoding_type {
            EncodingType::GBK | EncodingType::GB18030 => self.detect_gbk(data),
            EncodingType::Big5 => self.detect_big5(data),
            EncodingType::ShiftJIS => self.detect_shift_jis(data),
            EncodingType::Windows1252 => Self::detect_windows1252(data),
            _ => None,
        }
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

    /// Single-byte Western text: high bytes outside CJK lead ranges with a
    /// dense printable share. Capped below structured-encoding confidence
    /// so CJK-looking data keeps its prior claim.
    fn detect_windows1252(data: &[u8]) -> Option<f64> {
        if data.is_empty() {
            return None;
        }
        let mut printable = 0usize;
        let mut high = 0usize;
        for &b in data.iter().take(8192) {
            if matches!(b, 0x09 | 0x0A | 0x0D | 0x20..=0x7E | 0xA0..=0xFF) {
                printable += 1;
            }
            if b >= 0x80 {
                high += 1;
            }
        }
        let len = data.len().min(8192) as f64;
        if high == 0 {
            return None;
        }
        let ratio = printable as f64 / len;
        if ratio < 0.7 {
            return None;
        }
        Some((0.55 + 0.25 * ratio).min(0.8))
    }

    fn detect_gbk(&self, data: &[u8]) -> Option<f64> {
        let mut valid_count = 0;
        let mut total_multi_byte = 0;

        let mut i = 0;
        while i < data.len() {
            if data[i] >= 0x81 && data[i] <= 0xFE {
                total_multi_byte += 1;
                if i + 1 < data.len() {
                    let next = data[i + 1];
                    if (0x40..=0x7E).contains(&next) || (0x80..=0xFE).contains(&next) {
                        valid_count += 1;
                        i += 2;
                        continue;
                    }
                }
            }
            i += 1;
        }

        if total_multi_byte == 0 {
            return None;
        }

        let ratio = valid_count as f64 / total_multi_byte as f64;
        if ratio < 0.5 {
            return None;
        }

        let density = (total_multi_byte * 2) as f64 / data.len() as f64;
        let confidence = ratio * (0.5 + 0.5 * density);
        Some(confidence.min(1.0))
    }

    fn detect_big5(&self, data: &[u8]) -> Option<f64> {
        let mut valid_count = 0;
        let mut total_multi_byte = 0;

        let mut i = 0;
        while i < data.len() {
            if data[i] >= 0x81 && data[i] <= 0xFE {
                total_multi_byte += 1;
                if i + 1 < data.len() {
                    let next = data[i + 1];
                    if (0x40..=0x7E).contains(&next) || (0xA1..=0xFE).contains(&next) {
                        valid_count += 1;
                        i += 2;
                        continue;
                    }
                }
            }
            i += 1;
        }

        if total_multi_byte == 0 {
            return None;
        }

        let ratio = valid_count as f64 / total_multi_byte as f64;
        if ratio < 0.5 {
            return None;
        }

        let density = (total_multi_byte * 2) as f64 / data.len() as f64;
        let confidence = ratio * (0.5 + 0.5 * density);
        Some(confidence.min(1.0))
    }

    fn detect_shift_jis(&self, data: &[u8]) -> Option<f64> {
        let mut valid_count = 0;
        let mut total_multi_byte = 0;

        let mut i = 0;
        while i < data.len() {
            if (data[i] >= 0x81 && data[i] <= 0x9F) || (data[i] >= 0xE0 && data[i] <= 0xEF) {
                total_multi_byte += 1;
                if i + 1 < data.len() {
                    let next = data[i + 1];
                    if (0x40..=0x7E).contains(&next) || (0x80..=0xFC).contains(&next) {
                        valid_count += 1;
                        i += 2;
                        continue;
                    }
                }
            }
            i += 1;
        }

        if total_multi_byte == 0 {
            return None;
        }

        let ratio = valid_count as f64 / total_multi_byte as f64;
        if ratio < 0.5 {
            return None;
        }

        let density = (total_multi_byte * 2) as f64 / data.len() as f64;
        let confidence = ratio * (0.5 + 0.5 * density);
        Some(confidence.min(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detector_default() {
        let detector = Detector::with_default_config();
        assert_eq!(detector.config.min_confidence, 0.7);
        assert_eq!(detector.config.detect_encodings.len(), 5);
    }

    #[test]
    fn test_detector_with_encodings() {
        let detector = Detector::with_encodings(vec!["UTF-8".to_string(), "GBK".to_string()]);
        assert_eq!(detector.config.detect_encodings.len(), 2);
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

    #[test]
    fn test_detect_gbk() {
        let detector = Detector::with_default_config();
        let gbk_data = [0xC4, 0xE3, 0xBA, 0xC3];
        let confidence = detector.detect_gbk(&gbk_data);
        assert!(confidence.is_some());
        assert!(confidence.expect("confidence should be Some") >= 0.5);
    }

    #[test]
    fn test_detect_big5() {
        let detector = Detector::with_default_config();
        let big5_data = [0xA4, 0x40, 0xA4, 0x55];
        let confidence = detector.detect_big5(&big5_data);
        assert!(confidence.is_some());
        assert!(confidence.expect("confidence should be Some") >= 0.5);
    }

    #[test]
    fn test_detect_shift_jis() {
        let detector = Detector::with_default_config();
        let shift_jis_data = [0x82, 0xA0, 0x82, 0xA2];
        let confidence = detector.detect_shift_jis(&shift_jis_data);
        assert!(confidence.is_some());
        assert!(confidence.expect("confidence should be Some") >= 0.5);
    }
}
