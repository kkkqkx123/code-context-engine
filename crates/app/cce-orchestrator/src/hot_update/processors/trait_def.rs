//! Update processor trait definition
//!
//! This module defines the common interface for all update processors.

use async_trait::async_trait;
use std::path::{Path, PathBuf};

use crate::hot_update::{BatchChangeResult, Result};
use crate::operation::{OperationContext, OperationProcessResult};

/// Trait for update processors
///
/// Each downstream module (relation, summary, embedding) implements this trait
/// to handle updates for changed files.
#[async_trait]
pub trait UpdateProcessor: Send + Sync {
    /// Get the name of this processor
    fn name(&self) -> &'static str;

    /// Check if this processor is enabled
    fn is_enabled(&self) -> bool;

    /// Process batch with operation context
    ///
    /// This is the unified entry point that:
    /// - Has explicit operation_id for tracking
    /// - Can report module-level failures
    /// - Integrates with ProgressManager
    /// - Tracks metrics automatically
    ///
    /// # Arguments
    ///
    /// * `ctx` - Operation context with operation_id and progress manager
    /// * `batch_result` - The batch change result containing all file changes
    ///
    /// # Returns
    ///
    /// OperationProcessResult with processed count, failures, and metrics
    ///
    /// The batch is mutable so processors can backfill derived data (e.g. a
    /// pre-generated `file_summary`) for later derived-phase processors.
    async fn process_operation(
        &self,
        ctx: &OperationContext,
        batch_result: &mut BatchChangeResult,
    ) -> Result<OperationProcessResult>;

    /// Prepare the shared candidate generation before the first write.
    async fn prepare_operation(&self, _ctx: &OperationContext) -> Result<()> {
        Ok(())
    }

    /// Commit the shared candidate generation after every processor succeeds.
    async fn commit_operation(&self, _ctx: &OperationContext) -> Result<()> {
        Ok(())
    }

    /// Retire the candidate generation after a failed operation.
    async fn abort_operation(&self, _ctx: &OperationContext, _reason: &str) -> Result<()> {
        Ok(())
    }

    /// Get files that need to be re-parsed due to dependency propagation
    ///
    /// # Returns
    ///
    /// A list of file paths that need to be re-parsed, or None if no re-parsing is needed
    async fn get_reparse_requests(&self) -> Option<Vec<PathBuf>> {
        None
    }

    // ===== Configuration Reload Support =====

    /// Check if this processor supports configuration reload
    ///
    /// Default implementation returns false.
    fn supports_config_reload(&self) -> bool {
        false
    }

    /// Reload configuration for a specific config file
    ///
    /// This method is called when a configuration file (e.g., Cargo.toml, package.json)
    /// is modified. Processors can use this to update their internal state.
    ///
    /// # Arguments
    ///
    /// * `config_path` - Path to the modified configuration file
    /// * `project_root` - Project root directory
    ///
    /// # Returns
    ///
    /// `Ok(())` if reload succeeded, `Err` otherwise
    async fn reload_config(&self, _config_path: &Path, _project_root: &Path) -> Result<()> {
        // Default: no-op
        Ok(())
    }

    /// Reload all configurations from project root
    ///
    /// This method is called to reload all build configurations.
    /// Useful when multiple config files change or during initialization.
    ///
    /// # Arguments
    ///
    /// * `project_root` - Project root directory
    async fn reload_all_configs(&self, _project_root: &Path) -> Result<()> {
        // Default: no-op
        Ok(())
    }

    /// Handle configuration change with invalidate-rebuild pattern
    ///
    /// This method is called when a configuration file (e.g., Cargo.toml, package.json)
    /// is modified. Processors should:
    /// 1. Update their internal configuration
    /// 2. Invalidate affected indexes/data
    /// 3. Trigger background rebuild if needed
    ///
    /// Unlike the old two-phase commit, this method does not guarantee atomicity.
    /// It allows for gradual rebuilding and independent error handling per processor.
    ///
    /// # Arguments
    ///
    /// * `config_path` - Path to the modified configuration file
    /// * `project_root` - Project root directory
    ///
    /// # Returns
    ///
    /// `Ok(())` if the handler accepted the change (even if rebuild is pending),
    /// `Err` if there was a critical error preventing the handler from processing
    async fn on_config_change(&self, _config_path: &Path, _project_root: &Path) -> Result<()> {
        // Default: no-op
        Ok(())
    }
}

/// Type alias for a boxed update processor
pub type BoxedUpdateProcessor = Box<dyn UpdateProcessor>;

/// Extension trait for processor collections
pub trait ProcessorCollection {
    /// Filter processors by whether they are enabled
    fn enabled_processors(&self) -> Vec<&dyn UpdateProcessor>;
}

impl ProcessorCollection for Vec<BoxedUpdateProcessor> {
    fn enabled_processors(&self) -> Vec<&dyn UpdateProcessor> {
        self.iter()
            .filter(|p| p.is_enabled())
            .map(|p| p.as_ref())
            .collect()
    }
}
