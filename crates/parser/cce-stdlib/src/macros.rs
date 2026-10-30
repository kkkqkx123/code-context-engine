//! Macros for generating stdlib detection code
//!
//! This module provides macros to eliminate boilerplate code in stdlib detectors.
//! Instead of manually implementing the same function 80+ times across 17 languages,
//! we use macros to generate them consistently.

/// Generate simple containment check functions from constant arrays
///
/// # Example
/// ```ignore
/// impl_list_checker!(MyDetector, [
///     (STDLIB_TYPES, is_stdlib_type),
///     (STDLIB_TRAITS, is_stdlib_trait),
///     (STDLIB_MACROS, is_stdlib_macro),
/// ]);
/// ```
#[macro_export]
macro_rules! impl_list_checker {
    (
        $detector:ty,
        [
            $(($const_array:ident, $fn_name:ident)),* $(,)?
        ]
    ) => {
        impl $detector {
            $(
                /// Check if a name is in the stdlib
                pub fn $fn_name(name: &str) -> bool {
                    Self::$const_array.contains(&name)
                }
            )*
        }
    };
}

/// Generate the get_category function for all stdlib detectors
///
/// This macro creates a lazy-initialized HashMap that maps stdlib names to categories.
/// It consolidates the category data without duplication.
///
/// # Example
/// ```ignore
/// impl_stdlib_categorizer!(MyDetector, [
///     (Collection, ["Vec", "HashMap"]),
///     (Io, ["File", "BufReader"]),
///     (Concurrency, ["Mutex", "Arc"]),
/// ]);
/// ```
#[macro_export]
macro_rules! impl_stdlib_categorizer {
    (
        $detector:ty,
        [
            $(($category:expr, [$($item:expr),* $(,)?])),* $(,)?
        ]
    ) => {
        impl $detector {
            /// Get the standard library category for a stdlib name
            ///
            /// Returns Some(StdlibCategory) if the name is recognized, None otherwise.
            pub fn get_category(
                name: &str,
            ) -> Option<cce_types::stdlib_category::StdlibCategory> {
                use std::collections::HashMap;
                use std::sync::OnceLock;
                use cce_types::stdlib_category::StdlibCategory;

                static CATEGORY_MAP: OnceLock<HashMap<&'static str, StdlibCategory>> =
                    OnceLock::new();

                let map = CATEGORY_MAP.get_or_init(|| {
                    let mut categories = HashMap::new();

                    $(
                        $(
                            categories.insert($item, $category);
                        )*
                    )*

                    categories
                });

                // Handle macro names with trailing '!'
                let lookup_name = if let Some(stripped) = name.strip_suffix('!') {
                    stripped
                } else {
                    name
                };

                map.get(lookup_name).copied().or_else(|| {
                    // Fallback for fully-qualified names (e.g. `console.log`,
                    // `Vec::new`): match against the root object/type segment
                    // so the category is derived from the receiver instead of
                    // the method name.
                    let root = lookup_name.split(['.', ':']).next().unwrap_or(lookup_name);
                    if root.is_empty() || root == lookup_name {
                        None
                    } else {
                        map.get(root).copied()
                    }
                })
            }
        }
    };
}

/// Generate a simple is_stdlib_call implementation for detectors that follow
/// the common pattern: check builtin functions, then check module/package prefix
///
/// This reduces boilerplate for languages where the detection logic is uniform.
///
/// # Example
/// ```ignore
/// impl_stdlib_call!(PythonStdlibDetector, {
///     builtin_fn: BUILTIN_FUNCTIONS,
///     module: STDLIB_MODULES,
/// });
/// ```
#[macro_export]
macro_rules! impl_stdlib_call {
    (
        $detector:ty,
        {
            builtin_fn: $builtin_fn:ident,
            module: $module:ident,
        }
    ) => {
        impl $detector {
            /// Check if a call is to stdlib
            pub fn is_stdlib_call(call_name: &str) -> bool {
                if Self::$builtin_fn.contains(&call_name) {
                    return true;
                }

                if call_name.contains('.') {
                    let first_component = call_name.split('.').next().unwrap_or("");
                    return Self::$module.contains(&first_component);
                }

                false
            }
        }
    };
}

/// Generate `is_stdlib_by_type` for detectors that judge a call by its name
/// only, so the relation type cannot refine the answer.
///
/// Every call-domain relation delegates to `is_stdlib_call`; relations outside
/// the call domain (imports, structure, references, markup) carry no callee and
/// answer `false`. Deriving the guard from `RelationType::is_call` instead of
/// listing variants means a newly added call relation inherits the name check,
/// while a newly added non-call relation stays out by construction.
///
/// # Example
/// ```ignore
/// impl_stdlib_by_type_name_only!(GoStdlibDetector);
/// ```
#[macro_export]
macro_rules! impl_stdlib_by_type_name_only {
    ($detector:ty) => {
        impl $detector {
            /// Check if a call is to stdlib using relation type
            ///
            /// The relation type only decides whether the name is looked up at
            /// all: stdlib tables describe callables, not imports or structure.
            pub fn is_stdlib_by_type(
                call_name: &str,
                relation_type: &cce_types::relation::RelationType,
            ) -> bool {
                relation_type.is_call() && Self::is_stdlib_call(call_name)
            }
        }
    };
}

/// Generate `is_stdlib_by_type` for detectors that do refine the answer by
/// relation type.
///
/// The supplied body runs only for call-domain relations and receives the call
/// name and the relation type; non-call relations answer `false` without
/// running it. Naming both parameters at the call site keeps them visible
/// inside the body.
///
/// # Example
/// ```ignore
/// impl_stdlib_by_type_calls!(RustStdlibDetector, |name, relation_type| {
///     match relation_type {
///         RelationType::MacroCall => Self::is_stdlib_macro(name),
///         _ => Self::is_stdlib_call(name),
///     }
/// });
/// ```
#[macro_export]
macro_rules! impl_stdlib_by_type_calls {
    ($detector:ty, |$name:ident, $relation:ident| $logic:expr) => {
        impl $detector {
            /// Check if a call is to stdlib using relation type
            ///
            /// Only call-domain relations are classified; the rest carry no
            /// callee and answer `false`.
            pub fn is_stdlib_by_type(
                call_name: &str,
                $relation: &cce_types::relation::RelationType,
            ) -> bool {
                if !$relation.is_call() {
                    return false;
                }
                #[allow(unused_variables)]
                let $name = call_name;
                $logic
            }
        }
    };
}

#[cfg(test)]
mod tests {
    // The macros are tested through their usage in specific detectors
    // See crates/cce_parser/src/parser/stdlib/test_macros.rs for examples

    #[test]
    fn test_macro_compilation() {
        // This test just verifies that the macros compile correctly
        // Actual functionality is tested in individual detector files
    }
}
