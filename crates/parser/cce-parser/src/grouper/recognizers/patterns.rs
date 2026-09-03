//! Pattern detectors for design patterns and code constructs
//!
//! This module provides detectors for common design patterns and code constructs:
//! - Design patterns: Builder, Factory, Singleton, Strategy, Observer, Adapter, Decorator, Composite
//! - Architecture patterns: Service, Repository, DTO, ORM Entity, Validator
//! - Boilerplate patterns: Config, Event Handler, GUI Callback, Template Method
//!
//! Note: Getter/Setter detection has been moved to `processors::method_utils` as it is
//! a preprocessing utility rather than a design pattern detector.
//!
//! # Architecture
//!
//! The module is organized into:
//! - `detectors`: All pattern detection implementations
//! - `types`: Shared type definitions
//! - `utils`: Common utilities
//!
//! # Language-Specific Detection
//!
//! Each detector handles language-specific patterns internally:
//! - Builder detector supports Kotlin DSL and Lombok (Java) patterns
//! - Factory detector supports Kotlin companion object factories
//! - Other detectors may have language-specific heuristics

// Pattern detectors
pub mod detectors;

// Type definitions
pub mod types;

// Utilities
pub mod utils;

// Re-export commonly used types from detectors
pub use detectors::{
    AdapterDetector, AdapterPatternInfo, AdapterType, BuilderDetector, BuilderPatternInfo,
    CompositeDetector, CompositePatternInfo, ConfigDetector, DecoratorDetector,
    DecoratorPatternInfo, DtoDetector, EventHandlerDetector, FactoryDetector, FactoryMethod,
    FactoryMethodType, FactoryPatternInfo, GuiCallbackDetector, ObserverDetector,
    ObserverPatternInfo, OrmEntityDetector, RepositoryDetector, ServiceDetector, SingletonDetector,
    SingletonPatternInfo, SingletonType, StrategyDetector, StrategyPatternInfo,
    TemplateMethodDetector, TemplateMethodPatternInfo, ValidatorDetector,
};
