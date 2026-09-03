//! Four-level symbol table architecture
//!
//! Provides hierarchical symbol resolution:
//! - ProjectSymbolTable: Project-level symbol management
//! - PackageSymbolTable: Package/crate-level exports
//! - ModuleSymbolTable: Module-level imports/exports
//! - LocalSymbolTable: File-local symbol index

pub mod local;
pub mod module;
pub mod package;
pub mod project;
pub mod type_index;
pub use local::LocalSymbolTable;
pub use module::{
    ImportBinding, ImportSourceType, ModuleSymbolTable, ReexportBinding, strip_crate_prefix,
};
pub use package::PackageSymbolTable;
pub use project::{ProjectSymbolTable, ResolutionContext};
pub use type_index::{MemberEntry, TypeEntry, TypeKey, TypeMemberIndex};
