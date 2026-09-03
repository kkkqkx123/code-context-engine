//! Relation classification types
//!
//! This module provides stable, globally-used classification systems:
//! - RelationType: Categorizes semantic relationships (35 types across 4 domains)
//! - RelationLevel: Distinguishes file-level vs entity-level relations
//! - ExternalCallType: Classifies external references

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// Relation level
///
/// Represents the level at which a relation occurs:
/// - File: File-level relations (import, export, module dependency)
/// - Entity: Entity-level relations (call, inheritance, reference, etc.)
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    SerdeSerialize,
    SerdeDeserialize,
    Archive,
    RkyvDeserialize,
    Serialize,
    Default,
)]
pub enum RelationLevel {
    /// File-level relation (import/export/module dependency)
    File,
    /// Entity-level relation (call/inheritance/reference)
    #[default]
    Entity,
}

impl std::fmt::Display for RelationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationLevel::File => write!(f, "file"),
            RelationLevel::Entity => write!(f, "entity"),
        }
    }
}

impl std::str::FromStr for RelationLevel {
    type Err = crate::types::error::ParseRelationLevelError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "file" => Ok(RelationLevel::File),
            "entity" => Ok(RelationLevel::Entity),
            _ => Err(crate::types::error::ParseRelationLevelError::unknown(s)),
        }
    }
}

/// External call type
///
/// Represents the type of external reference when `callee_id` is `None`.
/// This allows for precise filtering and analysis of external dependencies.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    SerdeSerialize,
    SerdeDeserialize,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
pub enum ExternalCallType {
    /// Standard library call (e.g., std::collections::HashMap, os.path.join)
    StandardLibrary {
        /// Library or module name
        library: String,
    },
    /// External library call (from package managers like npm, pip, cargo)
    ExternalLibrary {
        /// Package name
        package: String,
    },
    /// Development dependency call (from dev-dependencies)
    DevDependency {
        /// Package name
        package: String,
    },
    /// Local path dependency call
    LocalDependency {
        /// Package name
        package: String,
    },
    /// Unknown external reference (classification failed or not supported)
    ///
    /// Carries the raw target so the reference stays attributable even when
    /// it cannot be classified
    Unknown {
        /// The raw callee name as extracted
        raw_target: String,
    },
}

impl ExternalCallType {
    /// Create a standard library external type
    pub fn standard_library(library: impl Into<String>) -> Self {
        Self::StandardLibrary {
            library: library.into(),
        }
    }

    /// Create an external library type
    pub fn external_library(package: impl Into<String>) -> Self {
        Self::ExternalLibrary {
            package: package.into(),
        }
    }

    /// Create a dev dependency type
    pub fn dev_dependency(package: impl Into<String>) -> Self {
        Self::DevDependency {
            package: package.into(),
        }
    }

    /// Create a local dependency type
    pub fn local_dependency(package: impl Into<String>) -> Self {
        Self::LocalDependency {
            package: package.into(),
        }
    }

    /// Check if this is a standard library call
    pub fn is_standard_library(&self) -> bool {
        matches!(self, Self::StandardLibrary { .. })
    }

    /// Check if this is an external library call
    pub fn is_external_library(&self) -> bool {
        matches!(self, Self::ExternalLibrary { .. })
    }

    /// Check if this is a development dependency call
    pub fn is_dev_dependency(&self) -> bool {
        matches!(self, Self::DevDependency { .. })
    }

    /// Check if this is a local dependency call
    pub fn is_local_dependency(&self) -> bool {
        matches!(self, Self::LocalDependency { .. })
    }

    /// Get the library/package name
    pub fn library_name(&self) -> Option<&str> {
        match self {
            Self::StandardLibrary { library } => Some(library),
            Self::ExternalLibrary { package } => Some(package),
            Self::DevDependency { package } => Some(package),
            Self::LocalDependency { package } => Some(package),
            Self::Unknown { .. } => None,
        }
    }
}

/// Relation type
///
/// Categorized into four domains matching spec.md:
/// - Call: Function/method invocations
/// - Dependency: Import/include/use relations
/// - Structural: Inheritance/implementation/contains
/// - Reference: Type/field references
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    SerdeSerialize,
    SerdeDeserialize,
    Default,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
pub enum RelationType {
    // === Call Domain ===
    /// Direct function call
    #[serde(rename = "call.direct")]
    #[default]
    DirectCall,
    /// Instance method call (obj.method())
    #[serde(rename = "call.method")]
    InstanceMethodCall,
    /// Static method call (Class.method())
    #[serde(rename = "call.method.static")]
    StaticMethodCall,
    /// Chained method call (a.b().c())
    #[serde(rename = "call.method.chained")]
    ChainedMethodCall,
    /// Constructor call (new Class(), Class())
    #[serde(rename = "call.constructor")]
    ConstructorCall,
    /// Pointer call (ptr())
    #[serde(rename = "call.pointer")]
    PointerCall,
    /// Callback call (callback())
    #[serde(rename = "call.callback")]
    CallbackCall,
    /// Template/generic call (foo<T>())
    #[serde(rename = "call.generic")]
    GenericCall,
    /// Macro call (macro!())
    #[serde(rename = "call.macro")]
    MacroCall,
    /// Goroutine call (go foo())
    #[serde(rename = "call.goroutine")]
    GoroutineCall,
    /// Deferred call (defer foo())
    #[serde(rename = "call.deferred")]
    DeferredCall,
    /// Async call (await foo())
    #[serde(rename = "call.async")]
    AsyncCall,
    /// Higher-order function call (caller passes a callback to callee)
    #[serde(rename = "call.higher_order")]
    HigherOrderCall,

    // === Dependency Domain ===
    /// Include (#include <...> or #include "...")
    #[serde(rename = "dependency.include")]
    IncludeLocal,
    /// Standard import (import "pkg")
    #[serde(rename = "dependency.import.standard")]
    ImportStandard,
    /// Named import (import { foo } from "mod")
    #[serde(rename = "dependency.import.named")]
    ImportNamed,
    /// Default import (import foo from "mod")
    #[serde(rename = "dependency.import.default")]
    ImportDefault,
    /// Namespace import (import * as ns from "mod")
    #[serde(rename = "dependency.import.namespace")]
    ImportNamespace,
    /// Dynamic import (import("mod"))
    #[serde(rename = "dependency.import.dynamic")]
    ImportDynamic,
    /// Use statement (use path::to::item)
    #[serde(rename = "dependency.use")]
    Use,
    /// Using namespace (using namespace ns)
    #[serde(rename = "dependency.using")]
    Using,
    /// Macro dependency (#ifdef, #ifndef)
    #[serde(rename = "dependency.macro")]
    MacroDependency,
    /// Module dependency (module requires/exports)
    #[serde(rename = "dependency.module")]
    ModuleDependency,

    // === Structural Domain ===
    /// Type inheritance (class extends class)
    #[serde(rename = "inheritance")]
    Inheritance,
    /// Interface implementation (class implements interface)
    #[serde(rename = "implementation")]
    Implementation,
    /// Trait bound (T: Trait)
    #[serde(rename = "trait_bound")]
    TraitBound,
    /// Containment (class contains method)
    #[serde(rename = "contains")]
    Contains,
    /// Impl block association (impl block for struct/enum)
    #[serde(rename = "impl_association")]
    ImplAssociation,
    /// Struct embedding (Go embedding, Rust field composition)
    #[serde(rename = "embedding")]
    Embedding,
    /// Mixin composition (Python mixin, JavaScript mixin)
    #[serde(rename = "mixin")]
    Mixin,
    /// Trait inheritance (Rust supertrait, Swift protocol inheritance)
    #[serde(rename = "trait_inheritance")]
    TraitInheritance,
    /// Protocol/ABC implementation (Python ABC, Go implicit implementation)
    #[serde(rename = "protocol_implementation")]
    ProtocolImplementation,

    // === Reference Domain ===
    /// Type reference (variable has type T)
    #[serde(rename = "type_reference")]
    TypeReference,
    /// Field access (obj.field)
    #[serde(rename = "field_access")]
    FieldAccess,

    // === Template/Markup Domain ===
    /// Element containment (parent/child DOM structure)
    #[serde(rename = "contains.element")]
    ElementContains,
    /// Template reference access
    #[serde(rename = "reference.template")]
    TemplateReference,
    /// Parameter binding (props, attributes)
    #[serde(rename = "parameter.binding")]
    ParameterBinding,
    /// Event handler binding
    #[serde(rename = "callback.event")]
    EventCallback,
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Call domain
            RelationType::DirectCall => write!(f, "call.direct"),
            RelationType::InstanceMethodCall => write!(f, "call.method"),
            RelationType::StaticMethodCall => write!(f, "call.method.static"),
            RelationType::ChainedMethodCall => write!(f, "call.method.chained"),
            RelationType::ConstructorCall => write!(f, "call.constructor"),
            RelationType::PointerCall => write!(f, "call.pointer"),
            RelationType::CallbackCall => write!(f, "call.callback"),
            RelationType::GenericCall => write!(f, "call.generic"),
            RelationType::MacroCall => write!(f, "call.macro"),
            RelationType::GoroutineCall => write!(f, "call.goroutine"),
            RelationType::DeferredCall => write!(f, "call.deferred"),
            RelationType::AsyncCall => write!(f, "call.async"),
            RelationType::HigherOrderCall => write!(f, "call.higher_order"),
            // Dependency domain
            RelationType::IncludeLocal => write!(f, "dependency.include"),
            RelationType::ImportStandard => write!(f, "dependency.import.standard"),
            RelationType::ImportNamed => write!(f, "dependency.import.named"),
            RelationType::ImportDefault => write!(f, "dependency.import.default"),
            RelationType::ImportNamespace => write!(f, "dependency.import.namespace"),
            RelationType::ImportDynamic => write!(f, "dependency.import.dynamic"),
            RelationType::Use => write!(f, "dependency.use"),
            RelationType::Using => write!(f, "dependency.using"),
            RelationType::MacroDependency => write!(f, "dependency.macro"),
            RelationType::ModuleDependency => write!(f, "dependency.module"),
            // Structural domain
            RelationType::Inheritance => write!(f, "inheritance"),
            RelationType::Implementation => write!(f, "implementation"),
            RelationType::TraitBound => write!(f, "trait_bound"),
            RelationType::Contains => write!(f, "contains"),
            RelationType::ImplAssociation => write!(f, "impl_association"),
            RelationType::Embedding => write!(f, "embedding"),
            RelationType::Mixin => write!(f, "mixin"),
            RelationType::TraitInheritance => write!(f, "trait_inheritance"),
            RelationType::ProtocolImplementation => write!(f, "protocol_implementation"),
            // Reference domain
            RelationType::TypeReference => write!(f, "type_reference"),
            RelationType::FieldAccess => write!(f, "field_access"),
            // Template/Markup domain
            RelationType::ElementContains => write!(f, "contains.element"),
            RelationType::TemplateReference => write!(f, "reference.template"),
            RelationType::ParameterBinding => write!(f, "parameter.binding"),
            RelationType::EventCallback => write!(f, "callback.event"),
        }
    }
}

impl std::str::FromStr for RelationType {
    type Err = crate::types::error::ParseRelationTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            // Call domain
            "call.direct" => Ok(RelationType::DirectCall),
            "call.method" => Ok(RelationType::InstanceMethodCall),
            "call.method.static" => Ok(RelationType::StaticMethodCall),
            "call.method.chained" => Ok(RelationType::ChainedMethodCall),
            "call.constructor" => Ok(RelationType::ConstructorCall),
            "call.pointer" => Ok(RelationType::PointerCall),
            "call.callback" => Ok(RelationType::CallbackCall),
            "call.generic" => Ok(RelationType::GenericCall),
            "call.macro" => Ok(RelationType::MacroCall),
            "call.goroutine" => Ok(RelationType::GoroutineCall),
            "call.deferred" => Ok(RelationType::DeferredCall),
            "call.async" => Ok(RelationType::AsyncCall),
            "call.higher_order" => Ok(RelationType::HigherOrderCall),
            // Dependency domain
            "dependency.include" => Ok(RelationType::IncludeLocal),
            "dependency.import.standard" => Ok(RelationType::ImportStandard),
            "dependency.import.named" => Ok(RelationType::ImportNamed),
            "dependency.import.default" => Ok(RelationType::ImportDefault),
            "dependency.import.namespace" => Ok(RelationType::ImportNamespace),
            "dependency.import.dynamic" => Ok(RelationType::ImportDynamic),
            "dependency.use" => Ok(RelationType::Use),
            "dependency.using" => Ok(RelationType::Using),
            "dependency.macro" => Ok(RelationType::MacroDependency),
            "dependency.module" => Ok(RelationType::ModuleDependency),
            // Structural domain
            "inheritance" => Ok(RelationType::Inheritance),
            "implementation" => Ok(RelationType::Implementation),
            "trait_bound" => Ok(RelationType::TraitBound),
            "contains" => Ok(RelationType::Contains),
            "impl_association" => Ok(RelationType::ImplAssociation),
            "embedding" => Ok(RelationType::Embedding),
            "mixin" => Ok(RelationType::Mixin),
            "trait_inheritance" => Ok(RelationType::TraitInheritance),
            "protocol_implementation" => Ok(RelationType::ProtocolImplementation),
            // Reference domain
            "type_reference" => Ok(RelationType::TypeReference),
            "field_access" => Ok(RelationType::FieldAccess),
            // Template/Markup domain
            "contains.element" => Ok(RelationType::ElementContains),
            "reference.template" => Ok(RelationType::TemplateReference),
            "parameter.binding" => Ok(RelationType::ParameterBinding),
            "callback.event" => Ok(RelationType::EventCallback),
            _ => Err(crate::types::error::ParseRelationTypeError::unknown(s)),
        }
    }
}

impl RelationType {
    /// Check if this is a call relation (any call type)
    pub fn is_call(&self) -> bool {
        matches!(
            self,
            RelationType::DirectCall
                | RelationType::InstanceMethodCall
                | RelationType::StaticMethodCall
                | RelationType::ChainedMethodCall
                | RelationType::ConstructorCall
                | RelationType::PointerCall
                | RelationType::CallbackCall
                | RelationType::GenericCall
                | RelationType::MacroCall
                | RelationType::GoroutineCall
                | RelationType::DeferredCall
                | RelationType::AsyncCall
                | RelationType::HigherOrderCall
        )
    }

    /// Check if this is a dependency relation (any import/include/use type)
    pub fn is_dependency(&self) -> bool {
        matches!(
            self,
            RelationType::IncludeLocal
                | RelationType::ImportStandard
                | RelationType::ImportNamed
                | RelationType::ImportDefault
                | RelationType::ImportNamespace
                | RelationType::ImportDynamic
                | RelationType::Use
                | RelationType::Using
                | RelationType::MacroDependency
                | RelationType::ModuleDependency
        )
    }

    /// Check if this is a structural relation (inheritance, implementation, trait bound, contains, impl association, embedding, mixin, trait inheritance, protocol implementation)
    pub fn is_structural(&self) -> bool {
        matches!(
            self,
            RelationType::Inheritance
                | RelationType::Implementation
                | RelationType::TraitBound
                | RelationType::Contains
                | RelationType::ImplAssociation
                | RelationType::Embedding
                | RelationType::Mixin
                | RelationType::TraitInheritance
                | RelationType::ProtocolImplementation
        )
    }

    /// Check if this is a reference relation (type reference, field access)
    pub fn is_reference(&self) -> bool {
        matches!(
            self,
            RelationType::TypeReference | RelationType::FieldAccess
        )
    }

    /// Check if this is a method call (instance, static, or chained)
    pub fn is_method_call(&self) -> bool {
        matches!(
            self,
            RelationType::InstanceMethodCall
                | RelationType::StaticMethodCall
                | RelationType::ChainedMethodCall
        )
    }

    /// Check if this is an import relation (any import type)
    pub fn is_import(&self) -> bool {
        matches!(
            self,
            RelationType::ImportStandard
                | RelationType::ImportNamed
                | RelationType::ImportDefault
                | RelationType::ImportNamespace
                | RelationType::ImportDynamic
        )
    }

    /// Check if this is an include relation (C/C++ include)
    pub fn is_include(&self) -> bool {
        matches!(self, RelationType::IncludeLocal)
    }

    /// Check if this is a template/markup relation
    pub fn is_template_relation(&self) -> bool {
        matches!(
            self,
            RelationType::ElementContains
                | RelationType::TemplateReference
                | RelationType::ParameterBinding
                | RelationType::EventCallback
        )
    }

    /// Check if this is a constructor call (including component instantiation)
    pub fn is_constructor_call(&self) -> bool {
        matches!(self, RelationType::ConstructorCall)
    }

    /// Get the domain name for this relation type
    pub fn domain(&self) -> &'static str {
        if self.is_call() {
            "call"
        } else if self.is_dependency() {
            "dependency"
        } else if self.is_structural() {
            "structural"
        } else if self.is_template_relation() {
            "template"
        } else {
            "reference"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_relation_type_checks() {
        // Call types
        assert!(RelationType::DirectCall.is_call());
        assert!(RelationType::InstanceMethodCall.is_call());
        assert!(RelationType::StaticMethodCall.is_call());
        assert!(RelationType::ConstructorCall.is_call());
        assert!(RelationType::GenericCall.is_call());
        assert!(RelationType::MacroCall.is_call());
        assert!(!RelationType::DirectCall.is_structural());

        // Method call check
        assert!(RelationType::InstanceMethodCall.is_method_call());
        assert!(RelationType::StaticMethodCall.is_method_call());
        assert!(RelationType::ChainedMethodCall.is_method_call());
        assert!(!RelationType::DirectCall.is_method_call());

        // Structural types
        assert!(RelationType::Inheritance.is_structural());
        assert!(RelationType::Implementation.is_structural());
        assert!(RelationType::TraitBound.is_structural());
        assert!(RelationType::ImplAssociation.is_structural());
        assert!(!RelationType::ImplAssociation.is_call());
        assert!(!RelationType::Inheritance.is_call());

        // Dependency types
        assert!(RelationType::ImportStandard.is_dependency());
        assert!(RelationType::Use.is_dependency());
        assert!(RelationType::IncludeLocal.is_dependency());
        assert!(!RelationType::ImportStandard.is_call());

        // Import check
        assert!(RelationType::ImportNamed.is_import());
        assert!(RelationType::ImportDefault.is_import());
        assert!(!RelationType::Use.is_import());

        // Include check
        assert!(RelationType::IncludeLocal.is_include());
        assert!(!RelationType::ImportStandard.is_include());

        // Reference types
        assert!(RelationType::TypeReference.is_reference());
        assert!(RelationType::FieldAccess.is_reference());
        assert!(!RelationType::DirectCall.is_reference());
    }

    #[test]
    fn test_relation_type_domain() {
        assert_eq!(RelationType::DirectCall.domain(), "call");
        assert_eq!(RelationType::InstanceMethodCall.domain(), "call");
        assert_eq!(RelationType::ImportStandard.domain(), "dependency");
        assert_eq!(RelationType::Use.domain(), "dependency");
        assert_eq!(RelationType::Inheritance.domain(), "structural");
        assert_eq!(RelationType::TypeReference.domain(), "reference");
        assert_eq!(RelationType::ElementContains.domain(), "template");
        assert_eq!(RelationType::EventCallback.domain(), "template");
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", RelationType::DirectCall), "call.direct");
        assert_eq!(
            format!("{}", RelationType::InstanceMethodCall),
            "call.method"
        );
        assert_eq!(
            format!("{}", RelationType::ImportStandard),
            "dependency.import.standard"
        );
        assert_eq!(format!("{}", RelationType::Use), "dependency.use");
        assert_eq!(format!("{}", RelationType::Inheritance), "inheritance");
        assert_eq!(format!("{}", RelationType::TypeReference), "type_reference");
    }

    #[test]
    fn test_from_str() {
        assert_eq!(
            RelationType::from_str("call.direct").unwrap(),
            RelationType::DirectCall
        );
        assert_eq!(
            RelationType::from_str("call.method").unwrap(),
            RelationType::InstanceMethodCall
        );
        assert_eq!(
            RelationType::from_str("dependency.import.standard").unwrap(),
            RelationType::ImportStandard
        );
        assert!(RelationType::from_str("unknown").is_err());
    }
}
