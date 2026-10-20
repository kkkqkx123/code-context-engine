use serde::{Deserialize, Serialize};

use crate::encoding::error::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingResult {
    pub encoding: String,
    pub confidence: f64,
}

impl EncodingResult {
    pub fn new(encoding: impl Into<String>, confidence: f64) -> Self {
        Self {
            encoding: encoding.into(),
            confidence,
        }
    }

    pub fn is_confident(&self, threshold: f64) -> bool {
        self.confidence >= threshold
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorConfig {
    pub min_confidence: f64,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderConfig {
    pub remove_bom: bool,
    pub strict_mode: bool,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            remove_bom: true,
            strict_mode: false,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoding_result() {
        let result = EncodingResult::new("UTF-8", 0.95);
        assert_eq!(result.encoding, "UTF-8");
        assert_eq!(result.confidence, 0.95);
        assert!(result.is_confident(0.7));
        assert!(!result.is_confident(0.99));
    }

    #[test]
    fn test_detector_config_default() {
        let config = DetectorConfig::default();
        assert_eq!(config.min_confidence, 0.7);
    }

    #[test]
    fn test_encoder_config_default() {
        let config = EncoderConfig::default();
        assert!(config.remove_bom);
        assert!(!config.strict_mode);
    }
}
