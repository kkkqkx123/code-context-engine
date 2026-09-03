//! Custom macros for capture enums

/// Macro to generate enum with `from_capture_name()` method
///
/// This macro reduces boilerplate by automatically generating the `from_capture_name()`
/// implementation from the serde rename attributes.
///
/// # Example
///
/// ```
/// use cce_parser::capture_enum;
/// use serde::{Serialize, Deserialize};
///
/// capture_enum! {
///     #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
///     pub enum TestCategory {
///         #[serde(rename = "class")]
///         Class,
///         #[serde(rename = "struct")]
///         Struct,
///     }
/// }
///
/// assert_eq!(TestCategory::from_capture_name("class"), Some(TestCategory::Class));
/// assert_eq!(TestCategory::from_capture_name("unknown"), None);
/// ```
///
/// This expands to:
///
/// ```ignore
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
/// pub enum TestCategory {
///     #[serde(rename = "class")]
///     Class,
///     #[serde(rename = "struct")]
///     Struct,
/// }
///
/// impl TestCategory {
///     pub fn from_capture_name(s: &str) -> Option<Self> {
///         match s {
///             "class" => Some(TestCategory::Class),
///             "struct" => Some(TestCategory::Struct),
///             _ => None,
///         }
///     }
/// }
/// ```
#[macro_export]
macro_rules! capture_enum {
    (
        $(#[$enum_meta:meta])*
        $vis:vis enum $enum_name:ident {
            $(
                #[serde(rename = $value:literal)]
                $variant_name:ident
            ),* $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        $vis enum $enum_name {
            $(
                #[serde(rename = $value)]
                $variant_name
            ),*
        }

        impl $enum_name {
            pub fn from_capture_name(s: &str) -> Option<Self> {
                match s {
                    $(
                        $value => Some($enum_name::$variant_name),
                    )*
                    _ => None,
                }
            }
        }
    };
}
