//! Regression tests for tree-sitter query false negatives.
//!
//! Each test parses a small snippet with the production pipeline
//! (`ParseCoordinator`) and asserts the entities and raw relations that
//! previously went missing: grouped Go imports, plain C# usings,
//! C# this/base calls, short-path Rust uses, Ruby require forms, PHP
//! single uses, C system includes, bare Python relative imports and
//! Scala renamed selectors.

use cce_parser::parser::ParseCoordinator;
use cce_types::entity::EntityKind;
use cce_types::relation::RelationType;
use cce_types::{Entity, ParsedFile};

fn parse_file(path: &str, content: &str) -> ParsedFile {
    let mut coordinator = ParseCoordinator::new();
    coordinator
        .parse(path, content)
        .expect("snippet should parse")
}

fn find<'a>(entities: &'a [Entity], kind: EntityKind, name: &str) -> &'a Entity {
    entities
        .iter()
        .find(|e| e.kind == kind && e.name == name)
        .unwrap_or_else(|| {
            panic!(
                "expected {kind:?} '{name}', got {:?}",
                entities
                    .iter()
                    .map(|e| (&e.kind, e.name.as_str()))
                    .collect::<Vec<_>>()
            )
        })
}

fn relations_snapshot(parsed: &ParsedFile) -> Vec<(&RelationType, &str)> {
    parsed
        .raw_relations
        .iter()
        .map(|r| (&r.relation_type, r.dst_name.as_str()))
        .collect::<Vec<_>>()
}

fn has_relation(parsed: &ParsedFile, relation: RelationType, needle: &str) -> bool {
    parsed.raw_relations.iter().any(|r| {
        r.relation_type == relation
            && (r.dst_name.contains(needle)
                || parsed
                    .import_table
                    .as_ref()
                    .is_some_and(|t| format!("{t:?}").contains(needle)))
    })
}

#[test]
fn go_grouped_imports_all_forms() {
    let parsed = parse_file(
        "app.go",
        "package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n\talias \"example.com/foo\"\n\t. \"example.com/dot\"\n\t_ \"example.com/blank\"\n)\n",
    );
    for needle in [
        "fmt",
        "os",
        "example.com/foo",
        "example.com/dot",
        "example.com/blank",
    ] {
        assert!(
            parsed
                .raw_relations
                .iter()
                .any(|r| r.dst_name.contains(needle)),
            "grouped import '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn csharp_usings_all_forms() {
    let parsed = parse_file(
        "App.cs",
        "using System;\nusing System.IO;\nusing Foo = Bar.Baz;\nusing static System.Math;\nglobal using System.Collections;\nclass C { }\n",
    );
    for needle in [
        "System",
        "System.IO",
        "Bar.Baz",
        "System.Math",
        "System.Collections",
    ] {
        assert!(
            has_relation(&parsed, RelationType::Using, needle),
            "using '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        !parsed.raw_relations.iter().any(|r| r.dst_name == "Foo"),
        "using alias must not become a dependency target, got {:?}",
        relations_snapshot(&parsed)
    );
    find(&parsed.entities, EntityKind::Import, "System.IO");
}

#[test]
fn csharp_this_and_base_calls_resolve_to_bare_name() {
    let parsed = parse_file(
        "C.cs",
        "class B { public void M() { } }\nclass C : B {\n void H() { }\n void G() { this.H(); }\n void F() { base.M(); }\n}\n",
    );
    for needle in ["H", "M"] {
        assert!(
            parsed.raw_relations.iter().any(|r| r.relation_type
                == RelationType::InstanceMethodCall
                && r.dst_name == needle),
            "this/base call '{needle}' should resolve to the bare method name, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn csharp_plain_object_call_keeps_static_winner() {
    let parsed = parse_file("C.cs", "class C { void F() { obj.M(); } }\n");
    assert!(
        parsed
            .raw_relations
            .iter()
            .any(|r| r.relation_type == RelationType::StaticMethodCall && r.dst_name == "obj.M"),
        "obj.M() must keep its existing edge, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn rust_use_short_paths_and_list_items() {
    let parsed = parse_file(
        "lib.rs",
        "use foo as bar;\nuse plain;\nuse a::{b, c as d};\nuse x::y::z;\n",
    );
    for needle in ["foo", "plain", "a::b", "a::c", "x::y::z"] {
        assert!(
            has_relation(&parsed, RelationType::Use, needle),
            "use target '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        !parsed
            .raw_relations
            .iter()
            .any(|r| r.dst_name == "a::{b, c as d}" || r.dst_name == "a"),
        "whole-list and bare-prefix targets must not leak, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn ruby_require_forms_recorded() {
    let parsed = parse_file(
        "app.rb",
        "require \"foo\"\nrequire_relative \"./bar\"\nload \"baz.rb\"\n",
    );
    for needle in ["foo", "./bar", "baz.rb"] {
        assert!(
            has_relation(&parsed, RelationType::ImportStandard, needle),
            "ruby require '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn lua_and_js_require_entities_exist() {
    let parsed = parse_file("app.lua", "local x = require(\"foo\")\n");
    find(&parsed.entities, EntityKind::Require, "\"foo\"");
    let parsed = parse_file("app.js", "const x = require(\"foo\");\n");
    assert!(
        !parsed.entities.is_empty(),
        "require call should still yield entities"
    );
}

#[test]
fn c_system_include_recorded_once() {
    let parsed = parse_file("app.c", "#include <stdio.h>\n#include \"helper.h\"\n");
    for needle in ["stdio.h", "helper.h"] {
        assert!(
            has_relation(&parsed, RelationType::IncludeLocal, needle),
            "include '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    let local_edges = parsed
        .raw_relations
        .iter()
        .filter(|r| {
            r.relation_type == RelationType::IncludeLocal && r.dst_name.contains("helper.h")
        })
        .count();
    assert_eq!(
        local_edges,
        1,
        "quoted include must yield exactly one edge, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn php_single_and_group_uses() {
    let parsed = parse_file(
        "app.php",
        "<?php\nuse Foo\\Bar;\nuse Qux\\{Quux, Corge as C};\n",
    );
    for needle in ["Foo\\Bar", "Quux", "Corge"] {
        assert!(
            has_relation(&parsed, RelationType::Use, needle),
            "use target '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        !parsed.raw_relations.iter().any(|r| r.dst_name == "C"),
        "group alias must not become a dependency target, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn python_bare_relative_import() {
    let parsed = parse_file("app.py", "from . import sibling\n");
    assert!(
        has_relation(&parsed, RelationType::ImportStandard, "sibling"),
        "bare relative import should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn scala_renamed_selector_targets_original() {
    let parsed = parse_file("App.scala", "import a.{b, c => d}\n");
    for needle in ["a", "b", "c"] {
        assert!(
            parsed.raw_relations.iter().any(|r| r.dst_name == needle),
            "import target '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn csharp_class_signature_excludes_body() {
    let parsed = parse_file("A.cs", "class A { void M() { } }\n");
    let class = find(&parsed.entities, EntityKind::Class, "A");
    assert!(
        !class.signature.contains("void M"),
        "class signature must not swallow the body, got {:?}",
        class.signature
    );
}

#[test]
fn rust_use_glob_self_super_crate_paths() {
    let parsed = parse_file(
        "lib.rs",
        "use g::*;\nuse self::shape;\nuse super::sibling;\nuse crate::root_mod;\n",
    );
    for needle in ["g", "self::shape", "super::sibling", "crate::root_mod"] {
        assert!(
            has_relation(&parsed, RelationType::Use, needle),
            "use target '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        !parsed
            .raw_relations
            .iter()
            .any(|r| r.relation_type == RelationType::Use && r.dst_name.contains('*')),
        "glob star must not leak into the target, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn rust_use_nested_list_two_levels() {
    let parsed = parse_file("lib.rs", "use outer::{inner::{deep, deeper as d2}};\n");
    for needle in ["outer::inner::deep", "outer::inner::deeper"] {
        assert!(
            has_relation(&parsed, RelationType::Use, needle),
            "nested use target '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        !parsed
            .raw_relations
            .iter()
            .any(|r| r.dst_name.contains('{') || r.dst_name == "outer::inner"),
        "list scaffolding must not leak, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn rust_use_pub_and_self_item() {
    let parsed = parse_file(
        "lib.rs",
        "pub use shared::Reexport;\nuse with_self::{self};\n",
    );
    for needle in ["shared::Reexport", "with_self"] {
        assert!(
            has_relation(&parsed, RelationType::Use, needle),
            "use target '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn rust_calls_assoc_method_generic_macro() {
    let parsed = parse_file(
        "lib.rs",
        "fn demo() {\n let _ = Type::new();\n let _ = a::b::c();\n obj.method();\n generic_fn::<i32>();\n println!(\"v={}\", 1);\n}\n",
    );
    for (relation, needle) in [
        (RelationType::DirectCall, "Type::new"),
        (RelationType::DirectCall, "a::b::c"),
        (RelationType::InstanceMethodCall, "obj.method"),
        (RelationType::GenericCall, "generic_fn"),
        (RelationType::MacroCall, "println"),
    ] {
        assert!(
            has_relation(&parsed, relation, needle),
            "{relation:?} '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn csharp_global_static_and_nested_usings() {
    let parsed = parse_file(
        "A.cs",
        "global using static System.Text;\nusing L = System.Collections.Generic.List<int>;\nnamespace N { using Inner.Space; }\n",
    );
    for needle in [
        "System.Text",
        "System.Collections.Generic.List",
        "Inner.Space",
    ] {
        assert!(
            has_relation(&parsed, RelationType::Using, needle),
            "using '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    for junk in ["Text", "List", "Collections", "Generic", "Space"] {
        assert!(
            !parsed.raw_relations.iter().any(|r| r.dst_name == junk),
            "inner fragment '{junk}' must not become a target, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn csharp_calls_new_chain_nullcond_generic() {
    let parsed = parse_file(
        "C.cs",
        "class C { void F() { var w = new Widget(); var e = new(); a.B().C(); o?.M(); Generic<int>(); } }\n",
    );
    assert!(
        has_relation(&parsed, RelationType::ConstructorCall, "Widget"),
        "new Widget() should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
    assert!(
        !parsed
            .raw_relations
            .iter()
            .any(|r| r.relation_type == RelationType::ConstructorCall && r.dst_name.is_empty()),
        "target-typed new() must not yield a phantom edge, got {:?}",
        relations_snapshot(&parsed)
    );
    for (relation, needle) in [
        (RelationType::StaticMethodCall, "a.B"),
        (RelationType::ChainedMethodCall, "a.B().C"),
        (RelationType::InstanceMethodCall, "M"),
        (RelationType::GenericCall, "Generic"),
    ] {
        assert!(
            has_relation(&parsed, relation, needle),
            "{relation:?} '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn csharp_ctor_base_initializer() {
    let parsed = parse_file(
        "C.cs",
        "class B { public B(int x) { } }\nclass C : B { public C() : base(1) { this.H(); } void H() { } }\n",
    );
    assert!(
        has_relation(&parsed, RelationType::Inheritance, "B"),
        "base list should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
    assert!(
        has_relation(&parsed, RelationType::InstanceMethodCall, "H"),
        "this.H() should resolve to H, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn go_embedding_and_goroutine_calls() {
    let parsed = parse_file(
        "a.go",
        "package p\ntype A struct {\n Inner\n *Ptr\n pkg.Q\n}\ntype I interface {\n Reader\n}\nfunc f() { go serve(); defer cleanup(); a.B().C(); }\n",
    );
    for needle in ["Inner", "Ptr", "Q"] {
        assert!(
            has_relation(&parsed, RelationType::Embedding, needle),
            "embedding '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        has_relation(&parsed, RelationType::TraitInheritance, "Reader"),
        "interface embedding should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
    for (relation, needle) in [
        (RelationType::GoroutineCall, "serve"),
        (RelationType::DeferredCall, "cleanup"),
    ] {
        assert!(
            has_relation(&parsed, relation, needle),
            "{relation:?} '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn ruby_mixin_inheritance_autoload_gem() {
    let parsed = parse_file(
        "a.rb",
        "autoload :Helper, \"helper\"\ngem \"rake\"\nclass Animal\nend\nclass Lone < Base\nend\nclass Dog < Animal\n include Fetch\n extend Tricks\n prepend Loud\nend\n",
    );
    for needle in ["Animal", "Base"] {
        assert!(
            has_relation(&parsed, RelationType::Inheritance, needle),
            "superclass '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        !parsed
            .raw_relations
            .iter()
            .any(|r| { r.relation_type == RelationType::Inheritance && r.dst_name == "Dog" }),
        "class name must not become its own superclass, got {:?}",
        relations_snapshot(&parsed)
    );
    for needle in ["Fetch", "Tricks", "Loud"] {
        assert!(
            has_relation(&parsed, RelationType::Mixin, needle),
            "mixin '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        !parsed.raw_relations.iter().any(|r| {
            r.relation_type == RelationType::IncludeLocal
                && ["Fetch", "Tricks", "Loud"].contains(&r.dst_name.as_str())
        }),
        "mixins must not masquerade as file inclusions, got {:?}",
        relations_snapshot(&parsed)
    );
    assert!(
        !parsed.raw_relations.iter().any(|r| {
            matches!(
                r.relation_type,
                RelationType::DirectCall | RelationType::InstanceMethodCall
            ) && ["include", "extend", "prepend"].contains(&r.dst_name.as_str())
        }),
        "mixin declarations must not double as call edges, got {:?}",
        relations_snapshot(&parsed)
    );
    for (relation, needle) in [
        (RelationType::ImportStandard, "helper"),
        (RelationType::ImportStandard, "rake"),
    ] {
        assert!(
            has_relation(&parsed, relation, needle),
            "{relation:?} '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    find(&parsed.entities, EntityKind::Class, "Dog");
    find(&parsed.entities, EntityKind::Class, "Lone");
}

#[test]
fn ruby_calls_super_safe_navigation_chain() {
    let parsed = parse_file(
        "a.rb",
        "class D < A\n def m\n super\n fetch.bone\n @leash&.clip\n foo.bar.baz\n puts \"woof\"\n end\nend\nD.new\n",
    );
    for needle in ["bone", "clip", "puts", "new"] {
        assert!(
            parsed.raw_relations.iter().any(|r| {
                matches!(
                    r.relation_type,
                    RelationType::DirectCall | RelationType::InstanceMethodCall
                ) && r.dst_name.contains(needle)
            }),
            "call '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        has_relation(&parsed, RelationType::ChainedMethodCall, "foo.bar.baz"),
        "chained call should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn php_use_function_const_and_single_alias() {
    let parsed = parse_file(
        "a.php",
        "<?php\nuse function Acme\\helper;\nuse const Acme\\VERSION;\nuse Vendor\\Package as VP;\n",
    );
    for needle in ["Acme\\helper", "Acme\\VERSION", "Vendor\\Package"] {
        assert!(
            has_relation(&parsed, RelationType::Use, needle),
            "use target '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        !parsed.raw_relations.iter().any(|r| r.dst_name == "VP"),
        "single-use alias must not become a target, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn php_trait_use_records_mixin() {
    let parsed = parse_file("a.php", "<?php\nclass C { use Loggable; use A, B; }\n");
    assert!(
        parsed
            .raw_relations
            .iter()
            .any(|r| r.relation_type == RelationType::Mixin),
        "trait use should record a mixin edge, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn php_calls_new_static_this_parent() {
    let parsed = parse_file(
        "a.php",
        "<?php\nclass C { function m() { $w = new Widget(); Vendor\\Package::create(); $this->handle(); parent::boot(); self::version(); $this->cache?->get(); $o->db->table()->first(); } }\n",
    );
    // Qualified static calls degrade to direct calls (receiver scope is
    // dropped); only the edge presence is pinned here.
    for (relation, needle) in [
        (RelationType::ConstructorCall, "Widget"),
        (RelationType::DirectCall, "create"),
        (RelationType::InstanceMethodCall, "handle"),
        (RelationType::InstanceMethodCall, "get"),
    ] {
        assert!(
            has_relation(&parsed, relation, needle),
            "{relation:?} '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        parsed.raw_relations.iter().any(|r| {
            r.relation_type == RelationType::ChainedMethodCall && r.dst_name.contains("first")
        }),
        "chained call should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn c_ifdef_include_and_calls() {
    let parsed = parse_file(
        "a.c",
        "#ifdef WITH_EXTRA\n#include \"extra.h\"\n#endif\nint apply(int (*fp)(int), int v) { return fp(v); }\nint m() { return MAX(1, 2); }\n",
    );
    assert!(
        has_relation(&parsed, RelationType::IncludeLocal, "extra.h"),
        "guarded include should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
    assert!(
        has_relation(&parsed, RelationType::DirectCall, "fp"),
        "function pointer call should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
    assert!(
        has_relation(&parsed, RelationType::DirectCall, "MAX"),
        "macro call should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn python_from_import_items_and_aliases() {
    let parsed = parse_file(
        "a.py",
        "import pkg.loader as loader\nfrom pkg import tool, other as renamed\nfrom pkg.sub import (alpha, beta)\nfrom os import path\n",
    );
    for needle in [
        "pkg.loader",
        "pkg",
        "tool",
        "other",
        "pkg.sub",
        "alpha",
        "beta",
    ] {
        assert!(
            has_relation(&parsed, RelationType::ImportStandard, needle)
                || has_relation(&parsed, RelationType::ImportNamed, needle),
            "import '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    for junk in ["loader", "renamed"] {
        assert!(
            !parsed.raw_relations.iter().any(|r| r.dst_name == junk),
            "local alias '{junk}' must not become a target, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    let dotted: Vec<&str> = parsed
        .raw_relations
        .iter()
        .filter(|r| r.relation_type.is_import() && r.dst_name.contains('.'))
        .map(|r| r.dst_name.as_str())
        .collect();
    assert!(
        dotted.iter().all(|t| *t == "pkg.loader" || *t == "pkg.sub"),
        "multi-segment modules must be single whole-path edges, got {dotted:?}"
    );
}

#[test]
fn python_wildcard_and_conditional_imports() {
    let parsed = parse_file(
        "a.py",
        "from pkg import *\nif TYPE_CHECKING:\n from typing import Optional\ntry:\n from fallback import impl\nexcept ImportError:\n impl = None\n",
    );
    assert!(
        has_relation(&parsed, RelationType::ImportNamespace, "pkg"),
        "star import should record its module, got {:?}",
        relations_snapshot(&parsed)
    );
    for needle in ["Optional", "impl"] {
        assert!(
            has_relation(&parsed, RelationType::ImportStandard, needle)
                || has_relation(&parsed, RelationType::ImportNamed, needle),
            "conditional import '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn python_calls_super_self_constructor() {
    let parsed = parse_file(
        "a.py",
        "class C:\n def m(self):\n  super().m()\n  self.n()\n  Cls()\n",
    );
    assert!(
        has_relation(&parsed, RelationType::ChainedMethodCall, "super().m"),
        "super call chain should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
    for (relation, needle) in [
        (RelationType::InstanceMethodCall, "n"),
        (RelationType::InstanceMethodCall, "Cls"),
    ] {
        assert!(
            has_relation(&parsed, relation, needle),
            "{relation:?} '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn scala_wildcard_exclude_and_constructor() {
    let parsed = parse_file(
        "A.scala",
        "import util._\nimport conf.{host, port => servicePort}\nimport store.{backend => _}\nobject O { val s = new Service\n o.start() }\n",
    );
    for needle in ["util", "conf", "host", "port", "store"] {
        assert!(
            has_relation(&parsed, RelationType::ImportStandard, needle),
            "import '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    for alias in ["servicePort", "backend"] {
        assert!(
            !parsed.raw_relations.iter().any(|r| r.dst_name == alias),
            "selector alias '{alias}' must not become a target, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        has_relation(&parsed, RelationType::ConstructorCall, "Service"),
        "new Service should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
    assert!(
        has_relation(&parsed, RelationType::InstanceMethodCall, "start"),
        "method call should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn cpp_using_namespace_and_type_targets() {
    let parsed = parse_file(
        "a.cpp",
        "using namespace mylib;\nusing mylib::Widget;\nint main() { }\n",
    );
    for needle in ["mylib", "mylib::Widget"] {
        assert!(
            has_relation(&parsed, RelationType::Using, needle),
            "using target '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        !parsed
            .raw_relations
            .iter()
            .any(|r| r.relation_type == RelationType::Using
                && (r.dst_name.contains("using ") || r.dst_name.contains(';'))),
        "using statements must not become raw targets, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn cpp_calls_new_and_template() {
    let parsed = parse_file(
        "a.cpp",
        "template<typename T>\nT identity(T x) { return x; }\nint main() { auto w = new Widget(); auto v = identity<int>(42); }\n",
    );
    for (relation, needle) in [
        (RelationType::ConstructorCall, "Widget"),
        (RelationType::GenericCall, "identity"),
    ] {
        assert!(
            has_relation(&parsed, relation, needle),
            "{relation:?} '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    find(&parsed.entities, EntityKind::Template, "identity");
    // Free-function operators match the operator pattern; member
    // declarations without bodies are not modeled (known limitation).
    let parsed = parse_file(
        "b.cpp",
        "struct S { };\nS operator+(const S& a, const S& b) { return a; }\n",
    );
    assert!(
        parsed.entities.iter().any(|e| e.name.contains("operator")),
        "free-function operator overload should be recorded, got {:?}",
        parsed.entities.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn java_imports_static_star_and_constructor() {
    let parsed = parse_file(
        "A.java",
        "import java.util.List;\nimport static java.lang.Math.max;\nimport java.util.*;\nclass A { void f() { var m = new Main(); var c = max(1, 2); m.run().finish(); Runnable r = Main::run; } String g() { return super.toString(); } }\n",
    );
    for needle in ["java.util.List", "java.lang.Math", "java.util"] {
        assert!(
            has_relation(&parsed, RelationType::ImportStandard, needle),
            "import '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        has_relation(&parsed, RelationType::ConstructorCall, "Main"),
        "new Main() should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
    for needle in ["max", "run", "finish"] {
        assert!(
            parsed
                .raw_relations
                .iter()
                .any(|r| r.dst_name.contains(needle)),
            "call '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn javascript_import_export_dynamic_forms() {
    let parsed = parse_file(
        "a.js",
        "import x from \"mod-default\";\nimport * as ns from \"mod-ns\";\nimport {a as b, c} from \"mod-named\";\nimport \"mod-side\";\nexport {x} from \"mod-reexport\";\nconst lazy = await import(\"mod-dynamic\");\nconst cjs = require(\"mod-cjs\");\n",
    );
    for (relation, needle) in [
        (RelationType::ImportDefault, "\"mod-default\""),
        (RelationType::ImportNamespace, "\"mod-ns\""),
        (RelationType::ImportNamed, "\"mod-named\""),
        (RelationType::ImportStandard, "\"mod-side\""),
        (RelationType::ModuleDependency, "\"mod-reexport\""),
        (RelationType::ImportDynamic, "\"mod-dynamic\""),
        (RelationType::ImportStandard, "\"mod-cjs\""),
    ] {
        assert!(
            has_relation(&parsed, relation, needle),
            "{relation:?} '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn javascript_calls_new_promise_optional() {
    let parsed = parse_file(
        "a.js",
        "const f = new Foo();\nf.run();\npromise.then(done).catch(fail);\nmaybe?.call();\n",
    );
    for (relation, needle) in [
        (RelationType::ConstructorCall, "Foo"),
        (RelationType::InstanceMethodCall, "call"),
    ] {
        assert!(
            has_relation(&parsed, relation, needle),
            "{relation:?} '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn typescript_import_type_and_heritage() {
    let parsed = parse_file(
        "a.ts",
        "import type {Config} from \"./config\";\nimport {serve} from \"./server\";\ninterface Named extends Base { }\nclass App extends Service implements Runnable { }\nenum Mode { Fast }\n",
    );
    find(&parsed.entities, EntityKind::Enum, "Mode");
    for needle in ["\"./config\"", "\"./server\""] {
        assert!(
            has_relation(&parsed, RelationType::ImportNamed, needle),
            "import '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    for (relation, needle) in [
        (RelationType::Inheritance, "Base"),
        (RelationType::Inheritance, "Service"),
        (RelationType::Implementation, "Runnable"),
    ] {
        assert!(
            has_relation(&parsed, relation, needle),
            "{relation:?} '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        !parsed.raw_relations.iter().any(|r| {
            matches!(
                r.relation_type,
                RelationType::Inheritance | RelationType::Implementation
            ) && (r.dst_name == "Named" || r.dst_name == "App")
        }),
        "declaration names must not become heritage targets, got {:?}",
        relations_snapshot(&parsed)
    );
    let impl_edges = parsed
        .raw_relations
        .iter()
        .filter(|r| r.relation_type == RelationType::Implementation && r.dst_name == "Runnable")
        .count();
    assert_eq!(
        impl_edges,
        1,
        "overlapping implement shapes must merge, got {:?}",
        relations_snapshot(&parsed)
    );
}

#[test]
fn kotlin_imports_and_calls() {
    let parsed = parse_file(
        "A.kt",
        "import shop.cart.Item\nimport shop.cart.*\nclass Store { fun open(): Item { val item = Item()\n cart.add(item)\n cart?.checkout()\n return item } }\nfun String.shout(): String = this.uppercase()\n",
    );
    for needle in ["shop.cart.Item", "shop.cart"] {
        assert!(
            has_relation(&parsed, RelationType::ImportStandard, needle),
            "import '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    for needle in ["Item", "add", "checkout"] {
        assert!(
            parsed
                .raw_relations
                .iter()
                .any(|r| r.dst_name.contains(needle)),
            "call '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    find(&parsed.entities, EntityKind::Function, "shout");
}

#[test]
fn lua_require_and_colon_calls() {
    let parsed = parse_file(
        "a.lua",
        "local util = require(\"util\")\nstore:save(data)\nrun(\"now\")\ndofile(\"extra.lua\")\n",
    );
    for (relation, needle) in [
        (RelationType::ImportStandard, "\"util\""),
        (RelationType::IncludeLocal, "\"extra.lua\""),
    ] {
        assert!(
            has_relation(&parsed, relation, needle),
            "{relation:?} '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    for needle in ["save", "run"] {
        assert!(
            parsed
                .raw_relations
                .iter()
                .any(|r| r.dst_name.contains(needle)),
            "call '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn dart_imports_and_constructor_style_calls() {
    let parsed = parse_file(
        "a.dart",
        "import \"util.dart\";\nexport \"api.dart\";\npart \"impl.dart\";\nclass App { void run() { var svc = Service();\n svc.start(); } }\n",
    );
    // Deferred imports (`import "x" deferred as y`) parse into a distinct
    // shape the scheme does not model yet; see the deferred list.
    for (relation, needle) in [
        (RelationType::ImportStandard, "\"util.dart\""),
        (RelationType::ModuleDependency, "\"api.dart\""),
        (RelationType::ModuleDependency, "\"impl.dart\""),
    ] {
        assert!(
            has_relation(&parsed, relation, needle),
            "{relation:?} '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    for needle in ["Service", "svc.start"] {
        assert!(
            parsed
                .raw_relations
                .iter()
                .any(|r| r.dst_name.contains(needle)),
            "call '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
}

#[test]
fn bash_source_dot_and_command_calls() {
    let parsed = parse_file(
        "run.sh",
        "source ./lib.sh\n. ./env.sh\nexport APP_PORT=8080\nstart_server --port \"$APP_PORT\"\n",
    );
    for needle in ["./lib.sh", "./env.sh"] {
        assert!(
            has_relation(&parsed, RelationType::ImportStandard, needle),
            "inclusion '{needle}' should be recorded, got {:?}",
            relations_snapshot(&parsed)
        );
    }
    assert!(
        !parsed.raw_relations.iter().any(|r| {
            r.relation_type == RelationType::ImportStandard && r.dst_name.contains("source ")
        }),
        "inclusion must not keep the command text, got {:?}",
        relations_snapshot(&parsed)
    );
    assert!(
        parsed
            .raw_relations
            .iter()
            .any(|r| r.relation_type == RelationType::DirectCall
                && r.dst_name.contains("start_server")),
        "command call should be recorded, got {:?}",
        relations_snapshot(&parsed)
    );
}
