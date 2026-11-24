# Fixtures Guide

This document describes the organization and purpose of the `fixtures/` directory in the cce-e2e-tests package.

## Overview

Fixtures form a **single shared tree** at the crate root `fixtures/`. Tests and examples load fixtures via `src/fixture.rs` loaders. There are no separate fixture copies in `tests/`.

## Design Principles

1. **Single source of truth** — One fixture tree shared by all tests and examples
2. **Language-organized** — Each language has its own subdirectory
3. **Category-organized** — Within each language, fixtures are grouped by purpose (`basic/`, `review/`, `type_inference/`)
4. **Self-contained** — Each fixture is a minimal but complete project (with manifest files like `Cargo.toml`, `package.json`)

## Top-Level Categories

| Directory | Purpose |
|-----------|---------|
| `rust/` | Rust language fixtures |
| `python/` | Python language fixtures |
| `typescript/` | TypeScript language fixtures |
| `javascript/` | JavaScript language fixtures |
| `go/` | Go language fixtures |
| `java/` | Java language fixtures |
| `kotlin/` | Kotlin language fixtures |
| `scala/` | Scala language fixtures |
| `ruby/` | Ruby language fixtures |
| `csharp/` | C# language fixtures |
| `c/` | C language fixtures |
| `cpp/` | C++ language fixtures |
| `php/` | PHP language fixtures |
| `dart/` | Dart language fixtures |
| `bash/` | Bash language fixtures |
| `lua/` | Lua language fixtures |
| `multi_language/` | Cross-language mixed project |
| `documents/` | Structured document format fixtures (JSON/XML/YAML/TOML) |
| `tools/` | Tool-specific fixtures (file-fold) |

## Fixture Categories

Each language directory typically contains three subcategories:

### `basic/`

**Purpose:** Minimal valid project for indexing workflow tests. Contains just enough code to exercise the full parse-group-chunk-embed pipeline.

**Typical contents:** Manifest file (`Cargo.toml`, `package.json`, etc.) + 1-3 source files

### `review/`

**Purpose:** Larger, realistic codebases for NL document export, chunk segmentation, and structured report generation. These are the primary inputs for the `export_{lang}` examples.

**Typical contents:** Full project trees with multiple source files, mimicking real-world crate/package structure

### `type_inference/`

**Purpose:** Focused test cases for type inference across different language patterns. Each subdirectory targets a specific type-inference scenario.

**Typical contents:** 1-2 source files demonstrating a specific pattern

## Language-Specific Fixture Details

### Rust (`fixtures/rust/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal lib + binary project (`Cargo.toml`, `src/lib.rs`, `src/main.rs`) |
| `distractor/` | Review | Code library (`matrix.rs`, `sorting.rs`) used as distractor in benchmarks |
| `review/once_cell/` | Review | once_cell crate — `src/{lib,imp_cs,imp_pl,imp_std,race}.rs` |
| `review/ripgrep/` | Review | Full ripgrep workspace (cli, core, globset, grep, ignore, matcher, pcre2, printer, regex, searcher) |
| `review/index_sidecar/` | Review | Index sidecar scenario (`Cargo.toml`, `src/lib.rs`) |
| `review/re_export/` | Review | Re-export chain (`lib.rs`, `main.rs`, `origin.rs`, `middle.rs`, `consumer.rs`) |
| `review/relation_demo/` | Review | Relationship indexing demo (`src/{main,model,repository,utils,workflow}.rs`) |
| `review/relation_diamond/` | Review | Diamond dependency graph (`src/{lib,main,repository,service_a,service_b}.rs`) |
| `type_inference/closure/` | TypeInference | Closure type capture patterns |
| `type_inference/control_flow/` | Type Inference | Control-flow based type narrowing |
| `type_inference/destructuring/` | Type Inference | Destructuring assignment types |
| `type_inference/generics/` | Type Inference | Generic type parameters |
| `type_inference/impl_self/` | Type Inference | `impl Self` type resolution |
| `type_inference/lifetime/` | Type Inference | Lifetime annotation inference |
| `type_inference/reference/` | Type Inference | Reference type inference |
| `type_inference/wildcard/` | Type Inference | Wildcard/underscore type patterns |

### Python (`fixtures/python/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal Python project |
| `review/flask/` | Review | Full Flask source tree (`src/flask/`, `docs/`, `examples/`, `tests/`) |
| `review/index_sidecar/` | Review | Index sidecar scenario |
| `review/re_export/` | Review | Re-export chain scenario |
| `review/wildcard/` | Review | Wildcard import scenario |
| `type_inference/control_flow/` | Type Inference | Control-flow narrowing |
| `type_inference/cross_file/` | Type Inference | Cross-file type resolution |
| `type_inference/destructuring/` | Type Inference | Destructuring patterns |
| `type_inference/discriminated_union/` | Type Inference | Discriminated union pattern matching |
| `type_inference/lambda/` | Type Inference | Lambda parameter types |
| `type_inference/negated_checks/` | Type Inference | Negated type narrowing |
| `type_inference/type_hints/` | Type Inference | Type hint propagation |
| `type_inference/visibility/` | Type Inference | Visibility modifier handling |

### TypeScript (`fixtures/typescript/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal TypeScript project |
| `review/index_sidecar/` | Review | Index sidecar scenario |
| `review/re_export/` | Review | Re-export chain scenario |
| `review/wildcard/` | Review | Wildcard import scenario |
| `type_inference/control_positions/` | Type Inference | Types at control-flow positions |
| `type_inference/cross_file/` | Type Inference | Cross-file type resolution |
| `type_inference/destructuring/` | Type Inference | Destructuring patterns |
| `type_inference/generics/` | Type Inference | Generic type parameters |
| `type_inference/lambda/` | Type Inference | Lambda parameter types |
| `type_inference/negated_checks/` | Type Inference | Negated narrowing |
| `type_inference/overloads/` | Type Inference | Function overload resolution |
| `type_inference/unions/` | Type Inference | Union type inference |
| `type_inference/visibility/` | Type Inference | Visibility modifier handling |

### JavaScript (`fixtures/javascript/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal JavaScript project |
| `review/express/` | Review | Full Express.js source with examples and tests |
| `review/re_export/` | Review | Re-export chain scenario |
| `type_inference/cross_file/` | Type Inference | Cross-file type resolution |
| `type_inference/narrowing/` | Type Inference | Type narrowing patterns |
| `type_inference/wildcard/` | Type Inference | Wildcard patterns |

### Go (`fixtures/go/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal Go project |
| `review/gin/` | Review | Full Gin framework source tree (binding, codec, internal, render, examples) |
| `type_inference/control_flow/` | Type Inference | Control-flow narrowing |
| `type_inference/cross_file/` | Type Inference | Cross-file type resolution |
| `type_inference/interfaces/` | Type Inference | Interface type inference |
| `type_inference/type_assertion/` | Type Inference | Type assertion patterns |
| `type_inference/visibility/` | Type Inference | Visibility/export handling |

### Java (`fixtures/java/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal Java project |
| `review/index_sidecar/` | Review | Index sidecar scenario |
| `review/springboot-minimal-demo/` | Review | Spring Boot minimal demo (controller, dto, entity, repository, service) |
| `review/jackson-core/` | Review | Jackson core library source tree |
| `type_inference/control_flow/` | Type Inference | Control-flow narrowing |
| `type_inference/control_positions/` | Type Inference | Types at control-flow positions |
| `type_inference/cross_file/` | Type Inference | Cross-file type resolution |
| `type_inference/discriminated_union/` | Type Inference | Sealed class/pattern matching |
| `type_inference/generics/` | Type Inference | Generic type parameters |
| `type_inference/lambda/` | Type Inference | Lambda parameter types |
| `type_inference/negated_checks/` | Type Inference | Negated narrowing |
| `type_inference/overloads/` | Type Inference | Method overload resolution |
| `type_inference/pattern_matching/` | Type Inference | Pattern matching types |
| `type_inference/var_inference/` | Type Inference | `var` keyword inference |
| `type_inference/visibility/` | Type Inference | Visibility modifier handling |

### Kotlin (`fixtures/kotlin/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal Kotlin project (`Main.kt`, `StringExt.kt`) |
| `review/coroutines/` | Review | Coroutine patterns (`main.kt`, `results.kt`) |
| `type_inference/control_flow/` | Type Inference | Control-flow narrowing |
| `type_inference/cross_file/` | Type Inference | Cross-file type resolution (`User.kt`, `Service.kt`) |
| `type_inference/discriminated_union/` | Type Inference | Sealed class inference |
| `type_inference/generics/` | Type Inference | Generic type parameters |
| `type_inference/lambda/` | Type Inference | Lambda types |
| `type_inference/negated_is/` | Type Inference | Negated `is` checks |
| `type_inference/null_safety/` | Type Inference | Null-safety type inference |
| `type_inference/overloads/` | Type Inference | Overload resolution |
| `type_inference/scope_functions/` | Type Inference | Scope function (`let`, `apply`, etc.) types |
| `type_inference/visibility/` | Type Inference | Visibility modifier handling |

### Scala (`fixtures/scala/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal Scala project |
| `review/case_class/` | Review | Case class patterns |
| `type_inference/control_flow/` | Type Inference | Control-flow narrowing |
| `type_inference/cross_file/` | Type Inference | Cross-file type resolution |
| `type_inference/declarations/` | Type Inference | Value/variable declarations |
| `type_inference/for_comprehension/` | Type Inference | For-comprehension type inference |
| `type_inference/overloads/` | Type Inference | Overload resolution |
| `type_inference/visibility/` | Type Inference | Visibility modifier handling |

### Ruby (`fixtures/ruby/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal Ruby project |
| `review/mixin/` | Review | Mixin module patterns |
| `type_inference/constructors/` | Type Inference | Constructor type inference |
| `type_inference/cross_file/` | Type Inference | Cross-file type resolution |

### C# (`fixtures/csharp/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal C# project (`Calculator.cs`, `Logger.cs`) |
| `review/MediatR/` | Review | Full MediatR library source (pipeline, wrappers, handlers, DI, samples, tests) |
| `type_inference/control_flow/` | Type Inference | Control-flow narrowing |
| `type_inference/control_positions/` | Type Inference | Types at control positions |
| `type_inference/cross_file/` | Type Inference | Cross-file resolution (`User.cs`, `Service.cs`) |
| `type_inference/discriminated_union/` | Type Inference | Discriminated union patterns |
| `type_inference/generics/` | Type Inference | Generic type parameters |
| `type_inference/is_not/` | Type Inference | `is not` pattern |
| `type_inference/lambda/` | Type Inference | Lambda types |
| `type_inference/overloads/` | Type Inference | Overload resolution |
| `type_inference/visibility/` | Type Inference | Visibility modifier handling |

### C (`fixtures/c/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal C project |
| `review/macros/` | Review | Macro-heavy C code |
| `type_inference/declarations/` | Type Inference | Declaration type inference |

### C++ (`fixtures/cpp/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal C++ project |
| `review/templates/` | Review | Template-heavy C++ code |
| `review/media_server/` | Review | Media server project (used in benchmarks) |

### PHP (`fixtures/php/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal PHP project |
| `review/namespace_trait/` | Review | Namespace and trait patterns |
| `type_inference/cross_file/` | Type Inference | Cross-file resolution |
| `type_inference/overloads/` | Type Inference | Overload resolution |
| `type_inference/phpdoc/` | Type Inference | PHPDoc-based inference |

### Dart (`fixtures/dart/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal Dart project (`bin/`, `lib/`) |
| `review/mixin_async/` | Review | Mixin async patterns |
| `type_inference/control_flow/` | Type Inference | Control-flow narrowing |
| `type_inference/control_positions/` | Type Inference | Types at control positions |
| `type_inference/cross_file/` | Type Inference | Cross-file resolution |
| `type_inference/declarations/` | Type Inference | Declaration inference |
| `type_inference/discriminated_union/` | Type Inference | Union type patterns |
| `type_inference/is_negated/` | Type Inference | Negated `is` checks |
| `type_inference/overloads/` | Type Inference | Overload resolution |

### Bash (`fixtures/bash/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal Bash scripts |
| `review/pipeline/` | Review | Bash pipeline patterns |
| `type_inference/variables/` | Type Inference | Variable type inference |

### Lua (`fixtures/lua/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `basic/` | Basic | Minimal Lua project |
| `review/closure/` | Review | Lua closure patterns |
| `type_inference/variables/` | Type Inference | Variable type inference |

### Multi-Language (`fixtures/multi_language/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `src/{csharp,go,java,kotlin,php,ruby}/` | Mixed | Multi-language project for cross-language indexing tests |

### Documents (`fixtures/documents/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `json/` | Document | JSON format test files |
| `xml/` | Document | XML format test files |
| `yaml/` | Document | YAML format test files |
| `toml/` | Document | TOML format test files |

### Tools (`fixtures/tools/`)

| Fixture | Category | Description |
|---------|----------|-------------|
| `file-fold/src/` | Tool | Multi-language files for `FileFoldTool` testing (`handler.ts`, `large.rs`, `main.rs`, `Order.java`, `server.go`, `service.py`) + `notes.unknown` + `README.md` |

## Fixture Loading

Fixtures are loaded via `src/fixture.rs` which provides:

- `FixtureCategory` — Enum identifying the fixture type (Rust, Python, TypeScript, etc.)
- `FixtureSpec` — Struct combining category + name for lookup
- `TestFixture` — Loaded fixture with path and metadata
- `all_type_inference_cases()` — Returns all type-inference fixture cases across languages

## How to Add a New Fixture

1. Create a subdirectory under the appropriate language in `fixtures/`
2. Add the minimal project files (manifest + source)
3. Register the fixture category in `src/fixture.rs` if it is a new category
4. Reference the fixture from examples or tests via `FixtureSpec`
