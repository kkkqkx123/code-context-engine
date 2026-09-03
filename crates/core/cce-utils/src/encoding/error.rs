use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Encoding detection failed: {0}")]
    DetectionFailed(String),

    #[error(
        "Low confidence detection: encoding={encoding} confidence={confidence:.2} threshold={threshold:.2}"
    )]
    LowConfidence {
        encoding: String,
        confidence: f64,
        threshold: f64,
    },

    #[error("Encoding conversion failed: {from_encoding} -> {to_encoding}: {message}")]
    ConversionFailed {
        from_encoding: String,
        to_encoding: String,
        message: String,
    },

    #[error("Unsupported encoding: {0}")]
    UnsupportedEncoding(String),

    #[error("Invalid BOM: {0}")]
    InvalidBOM(String),

    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),
}

impl Error {
    pub fn io(message: impl Into<String>) -> Self {
        Self::Io(std::io::Error::other(message.into()))
    }

    pub fn detection_failed(message: impl Into<String>) -> Self {
        Self::DetectionFailed(message.into())
    }

    pub fn low_confidence(encoding: impl Into<String>, confidence: f64, threshold: f64) -> Self {
        Self::LowConfidence {
            encoding: encoding.into(),
            confidence,
            threshold,
        }
    }

    pub fn conversion_failed(
        from_encoding: impl Into<String>,
        to_encoding: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::ConversionFailed {
            from_encoding: from_encoding.into(),
            to_encoding: to_encoding.into(),
            message: message.into(),
        }
    }

    pub fn unsupported_encoding(encoding: impl Into<String>) -> Self {
        Self::UnsupportedEncoding(encoding.into())
    }

    pub fn invalid_bom(message: impl Into<String>) -> Self {
        Self::InvalidBOM(message.into())
    }

    pub fn invalid_data(message: impl Into<String>) -> Self {
        Self::InvalidData(message.into())
    }

    pub fn file_not_found(path: impl Into<String>) -> Self {
        Self::FileNotFound(path.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let err = Error::from(io_err);
        assert!(matches!(err, Error::Io(_)));
        assert!(err.to_string().contains("IO error"));
    }

    #[test]
    fn test_error_detection_failed() {
        let err = Error::detection_failed("test message");
        assert!(matches!(err, Error::DetectionFailed(_)));
        assert!(err.to_string().contains("test message"));
    }

    #[test]
    fn test_error_low_confidence() {
        let err = Error::low_confidence("GBK", 0.5, 0.7);
        assert!(matches!(err, Error::LowConfidence { .. }));
        assert!(err.to_string().contains("GBK"));
        assert!(err.to_string().contains("0.50"));
        assert!(err.to_string().contains("0.70"));
    }

    #[test]
    fn test_error_conversion_failed() {
        let err = Error::conversion_failed("GBK", "UTF-8", "invalid sequence");
        assert!(matches!(err, Error::ConversionFailed { .. }));
        assert!(err.to_string().contains("GBK"));
        assert!(err.to_string().contains("UTF-8"));
        assert!(err.to_string().contains("invalid sequence"));
    }

    #[test]
    fn test_error_unsupported_encoding() {
        let err = Error::unsupported_encoding("ISO-8859-1");
        assert!(matches!(err, Error::UnsupportedEncoding(_)));
        assert!(err.to_string().contains("ISO-8859-1"));
    }

    #[test]
    fn test_error_invalid_bom() {
        let err = Error::invalid_bom("invalid BOM sequence");
        assert!(matches!(err, Error::InvalidBOM(_)));
        assert!(err.to_string().contains("invalid BOM sequence"));
    }

    #[test]
    fn test_error_invalid_utf8() {
        let invalid_bytes = vec![0xFF, 0xFE];
        let result = String::from_utf8(invalid_bytes);
        assert!(result.is_err());
        let err = Error::from(result.expect_err("should be error"));
        assert!(matches!(err, Error::InvalidUtf8(_)));
    }

    #[test]
    fn test_error_file_not_found() {
        let err = Error::file_not_found("/path/to/file.txt");
        assert!(matches!(err, Error::FileNotFound(_)));
        assert!(err.to_string().contains("/path/to/file.txt"));
    }

    #[test]
    fn test_error_invalid_data() {
        let err = Error::invalid_data("invalid byte sequence");
        assert!(matches!(err, Error::InvalidData(_)));
        assert!(err.to_string().contains("invalid byte sequence"));
    }
}
