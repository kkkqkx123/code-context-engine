//! Post-processing: impl block metadata extraction
//!
//! Extracts trait/type relationships from Rust impl blocks.

use crate::tree_sitter_query::executor::QueryMatch;
use cce_types::Entity;

/// Extract impl block metadata from captures
///
/// For trait impl `impl<'a, T> core::fmt::Debug for OnceRef<'a, T>`:
/// - entity.name set to simple trait name "Debug"
/// - metadata["impl_trait"] stores full path "core::fmt::Debug"
/// - metadata["impl_for_type"] stores simple type name "OnceRef"
///
/// For inherent impl `impl<T> MyStruct<T>`:
/// - entity.name set to "MyStruct"
/// - metadata["impl_type"] stores full type "MyStruct<T>"
/// - metadata["impl_for_type"] stores simple type name "MyStruct"
pub fn extract_impl_block_metadata(mat: &QueryMatch, entity: &mut Entity) {
    let mut trait_name = None;
    let mut for_type_name = None;

    for capture in &mat.captures {
        let name = &capture.name;
        if name.contains("impl.trait") && name.ends_with(".name") {
            trait_name = Some(capture.text.trim().to_string());
        }
        if name.contains("impl.for.type") && name.ends_with(".name") {
            for_type_name = Some(capture.text.trim().to_string());
        }
    }

    if let Some(ref name) = trait_name {
        entity.set_metadata("impl_trait", name.clone());
        entity.name = extract_simple_name(name);
    }
    if let Some(ref name) = for_type_name {
        let simple_name = extract_simple_name(name);
        entity.set_metadata("impl_for_type", simple_name.clone());
    }

    if trait_name.is_none() {
        for capture in &mat.captures {
            let name = &capture.name;
            if name.contains("impl.type") && name.ends_with(".name") {
                let full_type = capture.text.trim().to_string();
                let simple_name = extract_simple_name(&full_type);
                entity.set_metadata("impl_type", full_type.clone());
                entity.set_metadata("impl_for_type", simple_name.clone());
                entity.name = simple_name;
                break;
            }
        }
    }
}

/// Extract simple name from a qualified or generic-qualified identifier
///
/// - `core::fmt::Debug` → `Debug`
/// - `OnceRef<'a, T>` → `OnceRef`
pub fn extract_simple_name(text: &str) -> String {
    let without_generics = strip_generics(text);
    without_generics
        .split("::")
        .last()
        .unwrap_or(without_generics)
        .trim()
        .to_string()
}

/// Strip generic parameters from a type name
///
/// - `OnceRef<'a, T>` → `OnceRef`
/// - `HashMap<K, V>` → `HashMap`
fn strip_generics(text: &str) -> &str {
    if let Some(pos) = text.find('<') {
        &text[..pos]
    } else {
        text
    }
}
