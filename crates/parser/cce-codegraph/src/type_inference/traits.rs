//! Language-specific type inference traits.

use cce_types::ControlFlowStore;
use cce_types::entity::Entity;

use super::types::ScopedTypeContext;
use crate::symbol_table::TypeMemberIndex;

/// Context for type inference, providing access to shared resources.
///
/// This builder-pattern struct encapsulates optional resources that language
/// inferers may need during control flow narrowing. Not all languages require
/// all resources, so fields are optional.
///
/// # Example
///
/// ```ignore
/// let ctx = InferenceContext::new()
///     .with_type_index(&type_index);
/// inferer.infer_control_flow(entities, &control_flow, &mut ctx, &inference_ctx);
/// ```
pub struct InferenceContext<'a> {
    type_index: Option<&'a TypeMemberIndex>,
}

impl<'a> InferenceContext<'a> {
    /// Create a new empty inference context.
    pub fn new() -> Self {
        Self { type_index: None }
    }

    /// Set the type member index for discriminated union narrowing.
    ///
    /// This is used by languages that support discriminated unions (e.g.,
    /// TypeScript, Python, Dart, C#) to narrow types based on field equality
    /// checks like `x.kind == "circle"`.
    pub fn with_type_index(mut self, type_index: &'a TypeMemberIndex) -> Self {
        self.type_index = Some(type_index);
        self
    }

    /// Get the type member index, if available.
    pub fn type_index(&self) -> Option<&TypeMemberIndex> {
        self.type_index
    }
}

impl Default for InferenceContext<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// Language-specific type inference trait.
///
/// Each supported language implements this trait to provide custom type inference
/// logic. The trait separates inference into declaration extraction (from
/// function signatures, variable annotations, etc.) and optional control-flow
/// narrowing.
pub trait LanguageTypeInferer {
    /// Infer type bindings from entity declarations.
    ///
    /// This is the main inference method, called for each parsed file.
    /// Implementations should extract type information from:
    /// - Function signatures (parameter types, return types)
    /// - Variable annotations and assignments
    /// - Class/struct field types
    /// - Language-specific patterns (e.g., Rust impl blocks, Go receivers)
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext);

    /// Infer type narrowing from control flow structures (optional).
    ///
    /// Default implementation does nothing. Languages can override this to
    /// implement narrowing for patterns like:
    /// - Python: `isinstance(x, Type)`
    /// - Rust: `if let Some(val) = option`
    /// - TypeScript: `typeof x === "string"`
    /// - Go: `err != nil`
    ///
    /// `inference_ctx` provides access to shared resources like the type-member
    /// index for discriminated union narrowing. Languages that don't need these
    /// resources can ignore the context.
    fn infer_control_flow(
        &self,
        _entities: &[Entity],
        _control_flow: &ControlFlowStore,
        _ctx: &mut ScopedTypeContext,
        _inference_ctx: &InferenceContext<'_>,
    ) {
        // Default: no control flow narrowing
    }

    /// Collect declarations in the first pass (two-pass inference).
    ///
    /// Default implementation delegates to `infer_declarations`.
    fn collect_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        self.infer_declarations(entities, ctx);
    }

    /// Resolve references in the second pass (two-pass inference).
    ///
    /// Default implementation does nothing. Languages that need forward
    /// reference resolution can override.
    fn resolve_references(&self, _entities: &[Entity], _ctx: &mut ScopedTypeContext) {}
}
