//! Error handling strategies for config parsers.
//!
//! Provides configurable error handling behavior for different use cases:
//! - Initial build: fail on IO errors
//! - Hot update: skip missing files
//! - Background scan: skip all errors

use super::error::ConfigParseError;

/// Error handling strategy for config parsers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorStrategy {
    /// Fail on IO errors (for initial build).
    FailOnIoError,
    /// Skip when file not found (for hot update).
    SkipOnMissingFile,
    /// Skip on parse errors (for fault tolerance).
    SkipOnParseError,
    /// Skip all errors (for background scan).
    SkipAll,
    /// Default strategy: fail on IO errors.
    #[default]
    Default,
}

impl ErrorStrategy {
    /// Handle a parse error according to the strategy.
    ///
    /// Returns `Ok(())` if the error should be skipped, or `Err(e)` if it should propagate.
    pub fn handle_error(&self, error: ConfigParseError) -> Result<(), ConfigParseError> {
        match self {
            Self::FailOnIoError | Self::Default => Err(error),
            Self::SkipOnMissingFile => {
                if let ConfigParseError::Io { ref source, .. } = error {
                    if source.kind() == std::io::ErrorKind::NotFound {
                        return Ok(());
                    }
                }
                Err(error)
            }
            Self::SkipOnParseError => {
                if matches!(error, ConfigParseError::Parse { .. }) {
                    Ok(())
                } else {
                    Err(error)
                }
            }
            Self::SkipAll => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn io_error(kind: std::io::ErrorKind) -> ConfigParseError {
        ConfigParseError::Io {
            path: PathBuf::from("/test"),
            source: std::io::Error::new(kind, "test error"),
        }
    }

    fn parse_error() -> ConfigParseError {
        ConfigParseError::parse(PathBuf::from("/test.toml"), "Test", "invalid syntax")
    }

    #[test]
    fn test_fail_on_io_error() {
        let strategy = ErrorStrategy::FailOnIoError;
        let err = io_error(std::io::ErrorKind::NotFound);
        assert!(strategy.handle_error(err).is_err());
    }

    #[test]
    fn test_skip_on_missing_file() {
        let strategy = ErrorStrategy::SkipOnMissingFile;

        // NotFound should be skipped
        let err = io_error(std::io::ErrorKind::NotFound);
        assert!(strategy.handle_error(err).is_ok());

        // Other IO errors should propagate
        let err = io_error(std::io::ErrorKind::PermissionDenied);
        assert!(strategy.handle_error(err).is_err());

        // Parse errors should propagate
        let err = parse_error();
        assert!(strategy.handle_error(err).is_err());
    }

    #[test]
    fn test_skip_on_parse_error() {
        let strategy = ErrorStrategy::SkipOnParseError;

        // Parse errors should be skipped
        let err = parse_error();
        assert!(strategy.handle_error(err).is_ok());

        // IO errors should propagate
        let err = io_error(std::io::ErrorKind::NotFound);
        assert!(strategy.handle_error(err).is_err());
    }

    #[test]
    fn test_skip_all() {
        let strategy = ErrorStrategy::SkipAll;
        assert!(
            strategy
                .handle_error(io_error(std::io::ErrorKind::NotFound))
                .is_ok()
        );
        assert!(strategy.handle_error(parse_error()).is_ok());
    }

    #[test]
    fn test_default_strategy() {
        let strategy = ErrorStrategy::default();
        assert_eq!(strategy, ErrorStrategy::Default);
        let err = io_error(std::io::ErrorKind::NotFound);
        assert!(strategy.handle_error(err).is_err());
    }
}
