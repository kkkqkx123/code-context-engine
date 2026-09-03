//! Call capture types

use serde::{Deserialize, Serialize};

crate::capture_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum CallCategory {
        #[serde(rename = "function")]
        Function,
        #[serde(rename = "method")]
        Method,
        #[serde(rename = "constructor")]
        Constructor,
        #[serde(rename = "pointer")]
        Pointer,
        #[serde(rename = "callback")]
        Callback,
        #[serde(rename = "template")]
        Template,
        #[serde(rename = "generic")]
        Generic,
        #[serde(rename = "macro")]
        Macro,
        #[serde(rename = "closure")]
        Closure,
        #[serde(rename = "closure_variable")]
        ClosureVariable,
        #[serde(rename = "closure_inline")]
        ClosureInline,
        #[serde(rename = "hof")]
        HigherOrder,
        #[serde(rename = "async")]
        Async,
        #[serde(rename = "promise")]
        Promise,
        #[serde(rename = "special")]
        Special,
        #[serde(rename = "delegate")]
        Delegate,
        #[serde(rename = "goroutine")]
        Goroutine,
        #[serde(rename = "deferred")]
        Deferred,
        #[serde(rename = "associated")]
        Associated,
        #[serde(rename = "reference")]
        Reference,
        #[serde(rename = "super")]
        Super,
        #[serde(rename = "yield")]
        Yield,
        #[serde(rename = "return")]
        Return,
        // ===== Missing categories (added for scheme compatibility) =====
        #[serde(rename = "apply")]
        Apply,
        #[serde(rename = "binary")]
        Binary,
        #[serde(rename = "getter")]
        Getter,
        #[serde(rename = "infix")]
        Infix,
        #[serde(rename = "parent")]
        Parent,
        #[serde(rename = "scope")]
        Scope,
        #[serde(rename = "self")]
        Self_,
        #[serde(rename = "static")]
        Static,
        #[serde(rename = "field")]
        Field,
        #[serde(rename = "event")]
        Event,
        #[serde(rename = "component")]
        Component,
    }
}

crate::capture_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum CallSubcategory {
        #[serde(rename = "static")]
        Static,
        #[serde(rename = "chained")]
        Chained,
        #[serde(rename = "qualified")]
        Qualified,
        #[serde(rename = "constructor.qualified")]
        ConstructorQualified,
        #[serde(rename = "member")]
        Member,
        #[serde(rename = "super")]
        Super,
        #[serde(rename = "promise.then")]
        PromiseThen,
        #[serde(rename = "promise.catch")]
        PromiseCatch,
        #[serde(rename = "call")]
        Call,
        #[serde(rename = "apply")]
        Apply,
        #[serde(rename = "bind")]
        Bind,
        #[serde(rename = "method")]
        Method,
        #[serde(rename = "type")]
        Type,
        #[serde(rename = "function")]
        Function,
        #[serde(rename = "nested")]
        Nested,
        #[serde(rename = "component")]
        Component,
        #[serde(rename = "component.self_closing")]
        ComponentSelfClosing,
        #[serde(rename = "chained.to.name")]
        ChainedToName,
        #[serde(rename = "callback.event")]
        CallbackEvent,
        #[serde(rename = "callback.event.modifier")]
        CallbackEventModifier,
        #[serde(rename = "scoped")]
        Scoped,
    }
}
