//! Four-level symbol table architecture
//!
//! Provides hierarchical symbol resolution:
//! - ProjectSymbolTable: Project-level symbol management
//! - PackageSymbolTable: Package/crate-level exports
//! - ModuleSymbolTable: Module-level imports/exports
//! - LocalSymbolTable: File-local symbol index

pub mod candidate;
pub mod local;
pub mod local_store;
pub mod module;
pub mod package;
pub mod project;
pub mod type_index;
pub use candidate::{SymbolCandidate, SymbolCandidateKind};
pub use local::LocalSymbolTable;
pub use local_store::LocalSymbolStore;
pub use module::{
    ImportBinding, ModuleRecord, ModuleSymbolTable, ReexportBinding, strip_crate_prefix,
};
pub use package::PackageSymbolTable;
pub use project::{ProjectSymbolTable, ResolutionContext, SymbolCapacity};
pub use type_index::{MemberEntry, TypeEntry, TypeKey, TypeMemberIndex};

/// Namespace-qualified index key for one symbol.
///
/// The key is `"<package>::<namespace>::<simple name>"`. Both insertion and the
/// disk-backed rebuild derive it here, so a persisted index can be rebuilt
/// without a second, subtly different derivation rule.
pub(crate) fn namespace_index_key(qualified_name: &str, module_path: &str) -> Option<String> {
    use cce_types::NamespacePath;
    let ns_path = NamespacePath::parse(qualified_name);
    let ns_prefix = ns_path.namespace_prefix()?;
    let package = module_path.split("::").next()?;
    if package.is_empty() {
        return None;
    }
    let simple_name = qualified_name.rsplit("::").next()?;
    Some(format!("{}::{}::{}", package, ns_prefix, simple_name))
}
