//! Entity processors
//!
//! This module contains processors for entity transformation and optimization:
//! - Call merging (with stdlib specialization)
//! - Class-method processing
//! - Impl block member processing
//! - Test suite processing
//! - Method utilities (getter/setter detection)

mod call_merger;
mod class_method;
mod function_member;
#[cfg(test)]
mod integration_test;
mod method_utils;
mod small_fragment_merger;
mod test_suite;
pub use call_merger::{
    CallMerger, EntityCallMergeExt, MergedCallInfo, ParameterPattern, SemanticRole, ValuePattern,
};
pub use cce_config::modules::pattern_detection::GetterSetterDetectionConfig;
pub use class_method::ClassMethodProcessor;
pub use function_member::FunctionMemberProcessor;
pub use method_utils::{GetterSetterDetector, MethodType};
pub use small_fragment_merger::SmallFragmentMerger;
pub use test_suite::TestSuiteProcessor;
