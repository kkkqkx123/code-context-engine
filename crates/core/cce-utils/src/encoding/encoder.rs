use std::path::Path;
use tracing::{debug, info};

use crate::encoding::error::Error;
use crate::encoding::types::{EncoderConfig, EncodingType, Result};

pub struct Encoder {
    config: EncoderConfig,
}

impl Encoder {
    pub fn new(config: EncoderConfig) -> Self {
        Self { config }
    }

    pub fn to_utf8(&self, data: &[u8], from_encoding: &str) -> Result<String> {
        let encoding_name = if from_encoding.is_empty() {
            "UTF-8"
        } else {
            from_encoding
        };

        debug!(
            from_encoding = %encoding_name,
            data_length = data.len(),
            "Converting to UTF-8"
        );

        let encoding_type = EncodingType::parse(encoding_name)
            .ok_or_else(|| Error::unsupported_encoding(encoding_name))?;

        if matches!(encoding_type, EncodingType::UTF8) {
            let data = self.remove_bom(data);
            return String::from_utf8(data).map_err(Error::from);
        }

        let encoding = encoding_type.to_encoding_rs();
        let (text, _, _) = encoding.decode(data);

        if text.contains('\u{FFFD}') && self.config.strict_mode {
            return Err(Error::invalid_data(
                "Invalid character sequence detected in strict mode",
            ));
        }

        Ok(text.to_string())
    }

    pub fn convert_file_to_utf8(&self, path: &Path, from_encoding: &str) -> Result<bool> {
        let data = std::fs::read(path)
            .map_err(|e| Error::file_not_found(format!("{}: {}", path.display(), e)))?;

        let encoding_type = EncodingType::parse(from_encoding)
            .ok_or_else(|| Error::unsupported_encoding(from_encoding))?;

        if matches!(encoding_type, EncodingType::UTF8) {
            let new_data = self.remove_bom(&data);
            if new_data.len() == data.len() {
                return Ok(false);
            }
            std::fs::write(path, new_data)
                .map_err(|e| Error::io(format!("Failed to write file: {}", e)))?;
            info!(
                path = %path.display(),
                "Removed BOM from UTF-8 file"
            );
            return Ok(true);
        }

        let utf8_text = self.to_utf8(&data, from_encoding)?;
        std::fs::write(path, utf8_text)
            .map_err(|e| Error::io(format!("Failed to write file: {}", e)))?;

        info!(
            path = %path.display(),
            from_encoding = %from_encoding,
            "Converted file to UTF-8"
        );

        Ok(true)
    }

    fn remove_bom(&self, data: &[u8]) -> Vec<u8> {
        if data.len() >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
            return data[3..].to_vec();
        }
        if data.len() >= 4 {
            if data[0] == 0xFF && data[1] == 0xFE && data[2] == 0x00 && data[3] == 0x00 {
                return data[4..].to_vec();
            }
            if data[0] == 0x00 && data[1] == 0x00 && data[2] == 0xFE && data[3] == 0xFF {
                return data[4..].to_vec();
            }
        }
        if data.len() >= 2 {
            if data[0] == 0xFF && data[1] == 0xFE {
                return data[2..].to_vec();
            }
            if data[0] == 0xFE && data[1] == 0xFF {
                return data[2..].to_vec();
            }
        }
        data.to_vec()
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new(EncoderConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoder_default() {
        let encoder = Encoder::default();
        assert!(encoder.config.remove_bom);
        assert!(!encoder.config.strict_mode);
    }

    #[test]
    fn test_to_utf8_utf8() {
        let encoder = Encoder::default();
        let data = "Hello, world.".as_bytes();
        let result = encoder.to_utf8(data, "UTF-8").expect("should convert");
        assert_eq!(result, "Hello, world.");
    }

    #[test]
    fn test_to_utf8_utf8_bom() {
        let encoder = Encoder::default();
        let data = [0xEF, 0xBB, 0xBF, 0x48, 0x65, 0x6C, 0x6C, 0x6F];
        let result = encoder.to_utf8(&data, "UTF-8").expect("should convert");
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_to_utf8_empty() {
        let encoder = Encoder::default();
        let data = [];
        let result = encoder.to_utf8(&data, "UTF-8").expect("should convert");
        assert_eq!(result, "");
    }

    #[test]
    fn test_to_utf8_empty_encoding() {
        let encoder = Encoder::default();
        let data = "Hello, world.".as_bytes();
        let result = encoder.to_utf8(data, "").expect("should convert");
        assert_eq!(result, "Hello, world.");
    }

    #[test]
    fn test_to_utf8_gbk() {
        let encoder = Encoder::default();
        let data = [0xC4, 0xE3, 0xBA, 0xC3];
        let result = encoder.to_utf8(&data, "GBK").expect("should convert");
        assert_eq!(result, "你好");
    }

    #[test]
    fn test_to_utf8_gb18030() {
        let encoder = Encoder::default();
        let data = [0xC4, 0xE3, 0xBA, 0xC3];
        let result = encoder.to_utf8(&data, "GB18030").expect("should convert");
        assert_eq!(result, "你好");
    }

    #[test]
    fn test_to_utf8_big5() {
        let encoder = Encoder::default();
        let data = [0xA7, 0x41, 0xA6, 0x6E];
        let result = encoder.to_utf8(&data, "Big5").expect("should convert");
        assert_eq!(result, "你好");
    }

    #[test]
    fn test_to_utf8_shift_jis() {
        let encoder = Encoder::default();
        let data = [0x82, 0xC9, 0x82, 0xC4];
        let result = encoder.to_utf8(&data, "Shift_JIS").expect("should convert");
        assert_eq!(result, "にて");
    }

    #[test]
    fn test_to_utf8_shift_jis_alt_name() {
        let encoder = Encoder::default();
        let data = [0x82, 0xC9, 0x82, 0xC4];
        let result = encoder.to_utf8(&data, "ShiftJIS").expect("should convert");
        assert_eq!(result, "にて");
    }

    #[test]
    fn test_to_utf8_utf16le() {
        let encoder = Encoder::default();
        let data = [0x48, 0x00, 0x65, 0x00, 0x6C, 0x00, 0x6C, 0x00, 0x6F, 0x00];
        let result = encoder.to_utf8(&data, "UTF-16LE").expect("should convert");
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_to_utf8_utf16be() {
        let encoder = Encoder::default();
        let data = [0x00, 0x48, 0x00, 0x65, 0x00, 0x6C, 0x00, 0x6C, 0x00, 0x6F];
        let result = encoder.to_utf8(&data, "UTF-16BE").expect("should convert");
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_to_utf8_unsupported_encoding() {
        let encoder = Encoder::default();
        let data = b"Hello";
        let result = encoder.to_utf8(data, "ISO-8859-1");
        assert!(result.is_err());
        assert!(matches!(result, Err(Error::UnsupportedEncoding(_))));
    }

    #[test]
    fn test_remove_bom() {
        let encoder = Encoder::default();

        let utf8_bom = [0xEF, 0xBB, 0xBF, 0x48, 0x65, 0x6C, 0x6C, 0x6F];
        assert_eq!(
            encoder.remove_bom(&utf8_bom),
            [0x48, 0x65, 0x6C, 0x6C, 0x6F]
        );

        let utf16le_bom = [0xFF, 0xFE, 0x48, 0x00];
        assert_eq!(encoder.remove_bom(&utf16le_bom), [0x48, 0x00]);

        let utf16be_bom = [0xFE, 0xFF, 0x00, 0x48];
        assert_eq!(encoder.remove_bom(&utf16be_bom), [0x00, 0x48]);

        let no_bom = [0x48, 0x65, 0x6C, 0x6C, 0x6F];
        assert_eq!(encoder.remove_bom(&no_bom), no_bom);

        let empty: [u8; 0] = [];
        assert_eq!(encoder.remove_bom(&empty), empty);

        let too_short = [0x48];
        assert_eq!(encoder.remove_bom(&too_short), too_short);
    }

    #[test]
    fn test_convert_file_to_utf8() {
        let encoder = Encoder::default();
        let temp_file = tempfile::NamedTempFile::new().expect("should create temp file");

        let content = "Hello, world.";
        std::fs::write(temp_file.path(), content).expect("should write file");

        let converted = encoder
            .convert_file_to_utf8(temp_file.path(), "UTF-8")
            .expect("should convert");

        assert!(!converted);

        let data = std::fs::read(temp_file.path()).expect("should read file");
        assert_eq!(String::from_utf8(data).expect("should be utf8"), content);
    }

    #[test]
    fn test_convert_file_to_utf8_with_bom() {
        let encoder = Encoder::default();
        let temp_file = tempfile::NamedTempFile::new().expect("should create temp file");

        let content = [0xEF, 0xBB, 0xBF, 0x48, 0x65, 0x6C, 0x6C, 0x6F];
        std::fs::write(temp_file.path(), content).expect("should write file");

        let converted = encoder
            .convert_file_to_utf8(temp_file.path(), "UTF-8")
            .expect("should convert");

        assert!(converted);

        let data = std::fs::read(temp_file.path()).expect("should read file");
        assert_eq!(String::from_utf8(data).expect("should be utf8"), "Hello");
    }

    #[test]
    fn test_convert_file_to_utf8_gbk() {
        let encoder = Encoder::default();
        let temp_file = tempfile::NamedTempFile::new().expect("should create temp file");

        let content = [0xC4, 0xE3, 0xBA, 0xC3];
        std::fs::write(temp_file.path(), content).expect("should write file");

        let converted = encoder
            .convert_file_to_utf8(temp_file.path(), "GBK")
            .expect("should convert");

        assert!(converted);

        let data = std::fs::read(temp_file.path()).expect("should read file");
        assert_eq!(String::from_utf8(data).expect("should be utf8"), "你好");
    }

    #[test]
    fn test_convert_file_to_utf8_not_exists() {
        let encoder = Encoder::default();
        let result = encoder.convert_file_to_utf8(Path::new("nonexistent_file.txt"), "UTF-8");
        assert!(result.is_err());
        assert!(matches!(result, Err(Error::FileNotFound(_))));
    }
}
